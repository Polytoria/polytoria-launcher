import type { AnimationAction } from 'three';

export type Pose = 'Idle' | 'Run' | 'Wave' | 'Sit';

const facings: Record<Pose, number> = { Idle: Math.PI / 2, Run: Math.PI / 2, Wave: 0.2, Sit: 0.9 };

export async function loadModel(canvas: HTMLCanvasElement) {
  const [THREE, { GLTFLoader }] = await Promise.all([
    import('three'),
    import('three/examples/jsm/loaders/GLTFLoader.js'),
  ]);

  const gltf = await new GLTFLoader().loadAsync('/models/PolytorianModel.glb');
  const model = gltf.scene;
  const material = new THREE.MeshBasicMaterial({ color: 0xffffff, side: THREE.DoubleSide });
  model.traverse((child) => {
    if (child instanceof THREE.Mesh) child.material = material;
  });

  const box = new THREE.Box3().setFromObject(model);
  const scale = 1 / box.getSize(new THREE.Vector3()).y;
  model.scale.setScalar(scale);
  model.position.y = -box.min.y * scale;
  const root = new THREE.Group();
  root.add(model);
  root.rotation.y = facings.Idle;

  const scene = new THREE.Scene();
  scene.add(root);

  const { clientWidth: width, clientHeight: height } = canvas;
  const renderer = new THREE.WebGLRenderer({ canvas, alpha: true, antialias: true, powerPreference: 'low-power' });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.setSize(width, height, false);
  const view = 1.25;
  const halfW = (view * width) / height / 2;
  const camera = new THREE.OrthographicCamera(-halfW, halfW, view / 2, -view / 2, 0.1, 20);
  camera.position.set(0, 0.56, 5);

  const mixer = new THREE.AnimationMixer(model);
  const actions = new Map<Pose, AnimationAction>();
  for (const pose of Object.keys(facings) as Pose[]) {
    const clip = THREE.AnimationClip.findByName(gltf.animations, pose);
    if (clip) actions.set(pose, mixer.clipAction(clip));
  }

  let current: AnimationAction | undefined;
  let facing = facings.Idle;
  const play = (pose: Pose) => {
    const next = actions.get(pose);
    facing = facings[pose];
    if (!next || next === current) return;
    next.reset().play();
    if (current) next.crossFadeFrom(current, 0.25, false);
    current = next;
  };
  play('Idle');

  const clock = new THREE.Clock();
  renderer.setAnimationLoop(() => {
    const dt = Math.min(clock.getDelta(), 0.1);
    mixer.update(dt);
    root.rotation.y += (facing - root.rotation.y) * Math.min(1, dt * 8);
    renderer.render(scene, camera);
  });

  return play;
}
