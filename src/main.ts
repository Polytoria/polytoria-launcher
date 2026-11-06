import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { loadModel, type Pose } from './viewer';

const phases = {
  waiting: ['Waiting for a game', 'Idle'],
  checking: ['Getting ready', 'Idle'],
  downloading: ['Updating', 'Run'],
  installing: ['Almost there', 'Run'],
  launching: ['Have fun!', 'Wave'],
  error: ['', 'Sit'],
} as const;

type Phase = keyof typeof phases;
type Failure = { title: string; detail: string; retry: boolean; reinstall: string | null };

const $ = (sel: string) => document.querySelector(sel) as HTMLElement;
const shell = $('.shell');
const win = getCurrentWindow();

let phase: Phase = 'checking';
let progress = 0;
let fill = 0;
let failure: Failure | undefined;
let play: ((pose: Pose) => void) | undefined;

function show(next: Phase, title: string = phases[next][0], detail = '') {
  if (next !== phase) {
    $('.message').animate([{ opacity: 0, transform: 'translateY(6px)' }, { opacity: 1, transform: 'none' }], {
      duration: 350,
      easing: 'ease-out',
    });
  }
  phase = next;
  shell.dataset.state = next;
  $('.title .text').textContent = title;
  $('.detail').textContent = detail;
  $('.retry').hidden = !failure?.retry;
  play?.(phases[next][1]);
  render();
}

function setPhase(next: Phase) {
  failure = undefined;
  progress = 0;
  show(next);
}

function fail(f: Failure) {
  failure = f;
  show('error', f.title, f.detail);
}

function render() {
  const working = phase === 'downloading' || phase === 'installing';
  const p = progress / 100;
  if (phase === 'downloading') fill = 0.05 + p * 0.85;
  else if (phase === 'installing') fill = 0.9 + p * 0.1;
  else if (phase === 'launching') fill = 1.06;
  else if (phase === 'error') fill = Math.max(0, fill - 0.14);
  else fill = 0;

  $('.pct').textContent = working ? `${Math.floor(progress)}%` : '';
  shell.style.setProperty('--progress', `${phase === 'launching' ? 100 : working ? progress : 0}%`);
  shell.style.setProperty('--fill', `${fill}`);
}

async function loadLogo() {
  const svg = await fetch('/images/logo.svg').then((r) => r.text());
  const paths = svg.match(/<path[^>]+>/g)!.join('');
  const wave = `M-280 0 q70 -36 140 0 ${'t140 0 '.repeat(6)}V700H-280Z`;
  $('.logo').innerHTML =
    `<clipPath id="logo-shape">${paths}</clipPath><g class="pieces">${paths}</g>` +
    `<g clip-path="url(#logo-shape)"><g class="liquid"><path class="wave" d="${wave}"/></g></g>`;
}

shell.addEventListener('mousedown', (e) => {
  if (!(e.target as HTMLElement).closest('button')) win.startDragging();
});

$('.retry').addEventListener('click', () => {
  const reinstall = failure?.reinstall ?? null;
  setPhase('checking');
  invoke('retry', { reinstall });
});

$('.close').addEventListener('click', () => win.close());

async function main() {
  show('checking');
  loadLogo();

  await Promise.all([
    listen<string>('component', (e) => (document.documentElement.dataset.component = e.payload)),
    listen<Phase>('phase', (e) => setPhase(e.payload)),
    listen<number>('progress', (e) => {
      progress = e.payload;
      render();
    }),
    listen<Failure>('error', (e) => fail(e.payload)),
  ]);

  const canvas = $('#model-canvas') as HTMLCanvasElement;
  loadModel(canvas)
    .then((fn) => {
      play = fn;
      fn(phases[phase][1]);
    })
    .catch(() => canvas.parentElement!.remove());

  getVersion().then((v) => ($('.version').textContent = v));

  await invoke('ready');
}

main().catch((e) => {
  console.error(e);
  fail({ title: 'Something went wrong', detail: 'Close the launcher and open the game again.', retry: false, reinstall: null });
});
