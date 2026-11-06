#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_deep_link::DeepLinkExt;

static LOG: OnceLock<PathBuf> = OnceLock::new();

macro_rules! log {
    ($($t:tt)*) => { crate::write_log(&format!($($t)*)) };
}

mod launch;
mod settings;
mod update;

use launch::{Component, Link};
use settings::Settings;

fn write_log(line: &str) {
    eprintln!("{line}");
    let Some(path) = LOG.get() else { return };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let secs = std::time::UNIX_EPOCH.elapsed().map_or(0, |d| d.as_secs());
        let _ = writeln!(file, "[{secs}] {line}");
    }
}

struct AppState {
    link: Mutex<Option<Link>>,
    data_dir: PathBuf,
    http: reqwest::Client,
    ready: AtomicBool,
    busy: AtomicBool,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum Phase {
    Waiting,
    Checking,
    Downloading,
    Installing,
    Launching,
}

#[derive(Clone, Serialize)]
struct Failure {
    title: String,
    detail: String,
    retry: bool,
    reinstall: Option<Component>,
}

impl Failure {
    fn new(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            retry: true,
            reinstall: None,
        }
    }

    fn no_retry(mut self) -> Self {
        self.retry = false;
        self
    }

    fn reinstall(mut self, component: Component) -> Self {
        self.reinstall = Some(component);
        self
    }

    fn log(self, cause: impl std::fmt::Display) -> Self {
        log!("{}: {cause}", self.title);
        self
    }
}

fn phase(app: &AppHandle, phase: Phase) {
    let _ = app.emit("phase", phase);
}

fn progress(app: &AppHandle, percent: f64) {
    let _ = app.emit("progress", percent.clamp(0.0, 100.0));
}

fn fail(app: &AppHandle, failure: Failure) {
    log!("error: {} ({})", failure.title, failure.detail);
    let _ = app.emit("error", failure);
}

#[tauri::command]
fn ready(app: AppHandle, state: tauri::State<AppState>) {
    state.ready.store(true, Ordering::SeqCst);
    if state.link.lock().unwrap().is_some() {
        start(&app, None);
    } else {
        phase(&app, Phase::Waiting);
    }
}

#[tauri::command]
fn retry(app: AppHandle, reinstall: Option<Component>) {
    start(&app, reinstall);
}

fn start(app: &AppHandle, reinstall: Option<Component>) {
    if app.state::<AppState>().busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match tauri::async_runtime::spawn(run(app.clone(), reinstall)).await {
            Ok(Ok(())) => {}
            Ok(Err(failure)) => fail(&app, failure),
            Err(e) => fail(
                &app,
                Failure::new(
                    "Something went wrong",
                    "Try again, or open the game from the website.",
                )
                .log(e),
            ),
        }
        app.state::<AppState>().busy.store(false, Ordering::SeqCst);
    });
}

async fn run(app: AppHandle, reinstall: Option<Component>) -> Result<(), Failure> {
    let state = app.state::<AppState>();
    let link = state.link.lock().unwrap().clone().ok_or_else(|| {
        Failure::new(
            "Nothing to launch",
            "Open a game from the Polytoria website.",
        )
        .no_retry()
    })?;
    let component = link.component;
    let release = link.release;
    let _ = app.emit("component", component);
    phase(&app, Phase::Checking);

    let path = state.data_dir.join("Settings.json");
    let mut settings = Settings::load(&path);
    let installed = settings.version(component, release);

    let updates = update::check(
        &state.http,
        &link.token,
        release,
        settings.version(Component::Client, release).as_deref(),
        settings.version(Component::Creator, release).as_deref(),
    )
    .await?;
    if updates.maintenance {
        return Err(Failure::new(
            "Down for maintenance",
            "Polytoria is being worked on. Try again in a bit.",
        ));
    }
    let update = match component {
        Component::Client => updates.client,
        Component::Creator => updates.creator,
    };

    let missing = || {
        Failure::new(
            format!("Couldn't find the {}", component.name().to_lowercase()),
            "Try again to reinstall it.",
        )
        .reinstall(component)
    };
    let version = match update {
        Some(update)
            if reinstall == Some(component) || installed.as_deref() != Some(&update.version) =>
        {
            update::install(
                &app,
                &state,
                component,
                &update,
                installed.as_deref(),
                &link.token,
            )
            .await
            .map_err(|e| {
                Failure::new("Couldn't update", "Check your connection and try again.")
                    .reinstall(component)
                    .log(e)
            })?;
            settings.set(component, release, &update.version);
            settings.save(&path).map_err(|e| {
                Failure::new(
                    "Couldn't update",
                    "The launcher couldn't save its settings.",
                )
                .log(e)
            })?;
            update.version
        }
        _ => installed.ok_or_else(missing)?,
    };

    let binary = launch::binary(&state.data_dir, component, &version);
    if !binary.exists() {
        return Err(missing().log(binary.display()));
    }
    let args = launch::args(&link).ok_or_else(|| {
        Failure::new(
            "Couldn't start the test",
            "The link has no map in it. Open it again from the website.",
        )
        .no_retry()
    })?;
    if component == Component::Client {
        launch::kill(&binary);
    }

    log!("launching {} {version}", component.name());
    phase(&app, Phase::Launching);
    tokio::time::sleep(Duration::from_secs(2)).await;
    launch::spawn(&binary, &args).map_err(|e| {
        Failure::new("Couldn't start the game", "Try again to reinstall it.")
            .reinstall(component)
            .log(e)
    })?;
    app.exit(0);
    Ok(())
}

fn main() {
    #[cfg(target_os = "linux")]
    {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland");
        if wayland && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_local_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("Polytoria"));
            let _ = LOG.set(data_dir.join("Launcher.log"));
            log!("---- launcher start ----");

            #[cfg(target_os = "linux")]
            if std::env::var_os("APPIMAGE").is_some() {
                if let Err(e) = app.deep_link().register_all() {
                    log!("couldn't register polytoria:// handler: {e}");
                }
            }

            let args: Vec<String> = std::env::args().collect();
            let link = launch::parse(&args)
                .inspect_err(|e| log!("no link in argv: {e}"))
                .ok();

            let Some(link) = link else {
                if launch::open_browser("https://polytoria.com/places").is_err() {
                    log!("couldn't open the browser");
                }
                app.handle().exit(0);
                return Ok(());
            };

            let window = tauri::WebviewWindowBuilder::from_config(
                app.handle(),
                &app.config().app.windows[0],
            )?
            .build()?;
            let _ = window.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));

            let http = reqwest::Client::builder()
                .user_agent(format!("PolytoriaLauncher/{}", env!("CARGO_PKG_VERSION")))
                .connect_timeout(Duration::from_secs(15))
                .read_timeout(Duration::from_secs(60))
                .build()?;

            app.manage(AppState {
                link: Mutex::new(Some(link)),
                data_dir,
                http,
                ready: AtomicBool::new(false),
                busy: AtomicBool::new(false),
            });

            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                let urls: Vec<String> = event.urls().iter().map(|u| u.to_string()).collect();
                match launch::parse(&urls) {
                    Ok(link) => {
                        let state = handle.state::<AppState>();
                        *state.link.lock().unwrap() = Some(link);
                        if state.ready.load(Ordering::SeqCst) {
                            start(&handle, None);
                        }
                    }
                    Err(e) => fail(
                        &handle,
                        Failure::new(
                            "That link didn't work",
                            "Open the game again from the Polytoria website.",
                        )
                        .no_retry()
                        .log(e),
                    ),
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![ready, retry])
        .run(tauri::generate_context!())
        .expect("failed to run launcher");
}
