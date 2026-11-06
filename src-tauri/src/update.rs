use std::fs::{self, File};
use std::io::Write;
use std::path::{Component::Normal, Path};

use anyhow::bail;
use reqwest::StatusCode;
use serde::Deserialize;

use crate::launch::Component;
use crate::{phase, progress, AppState, Failure, Phase};

#[derive(Deserialize)]
pub struct Update {
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "Download")]
    pub download: String,
}

#[derive(Deserialize)]
pub struct Updates {
    #[serde(rename = "Maintenance", default)]
    pub maintenance: bool,
    #[serde(rename = "Client")]
    pub client: Option<Update>,
    #[serde(rename = "Creator")]
    pub creator: Option<Update>,
}

fn offline(cause: impl std::fmt::Display) -> Failure {
    Failure::new(
        "Couldn't check for updates",
        "Check your connection and try again.",
    )
    .log(cause)
}

pub async fn check(
    http: &reqwest::Client,
    token: &str,
    release: &str,
    client_version: Option<&str>,
    creator_version: Option<&str>,
) -> Result<Updates, Failure> {
    let release = release.to_lowercase();
    let mut query = vec![("os", std::env::consts::OS), ("release", release.as_str())];
    if let Some(v) = client_version {
        query.push(("client_version", v));
    }
    if let Some(v) = creator_version {
        query.push(("creator_version", v));
    }

    let res = http
        .get("https://api.polytoria.com/v1/launcher/updates")
        .query(&query)
        .header("Authorization", token)
        .send()
        .await
        .map_err(offline)?;
    let status = res.status();
    let body = res.text().await.map_err(offline)?;
    let snippet: String = body.chars().take(300).collect();

    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        log!("updates api {status}: {snippet}");
        let reason = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v["Message"].as_str().map(String::from))
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| "Open the game again from the Polytoria website.".into());
        return Err(Failure::new("Couldn't get in", reason).no_retry());
    }
    if !status.is_success() {
        return Err(offline(format!("updates api {status}: {snippet}")));
    }
    serde_json::from_str(&body).map_err(|e| offline(format!("{e}: {snippet}")))
}

pub async fn install(
    app: &tauri::AppHandle,
    state: &AppState,
    component: Component,
    update: &Update,
    old: Option<&str>,
    token: &str,
) -> anyhow::Result<()> {
    let name = component.name();
    phase(app, Phase::Downloading);

    let tmp = std::env::temp_dir().join("Polytoria");
    fs::create_dir_all(&tmp)?;
    let archive = tmp.join(format!("{name}.download"));

    let mut attempt = 1;
    while let Err(e) = download(app, state, &update.download, token, &archive).await {
        if attempt == 3 {
            let _ = fs::remove_file(&archive);
            return Err(e);
        }
        attempt += 1;
        log!("download of {name} failed: {e}, retrying");
    }

    let dir = state.data_dir.join(name).join(&update.version);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    phase(app, Phase::Installing);
    progress(app, 0.0);
    let result = unpack(app, &archive, &dir);
    let _ = fs::remove_file(&archive);
    if result.is_err() {
        let _ = fs::remove_dir_all(&dir);
    }
    result?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let binary = crate::launch::binary(&state.data_dir, component, &update.version);
        if binary.exists() {
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))?;
        }
    }

    if let Some(old) = old.filter(|old| *old != update.version) {
        let _ = fs::remove_dir_all(state.data_dir.join(name).join(old));
    }
    Ok(())
}

async fn download(
    app: &tauri::AppHandle,
    state: &AppState,
    url: &str,
    token: &str,
    path: &Path,
) -> anyhow::Result<()> {
    let mut req = state.http.get(url);
    if url.contains("api.polytoria.com") {
        req = req.header("Authorization", token);
    }
    let mut res = req.send().await?;
    if !res.status().is_success() {
        bail!("download returned {}", res.status());
    }

    progress(app, 0.0);
    let total = res.content_length().filter(|&n| n > 0);
    let mut file = File::create(path)?;
    let mut done = 0u64;
    let mut last = 0;
    while let Some(chunk) = res.chunk().await? {
        file.write_all(&chunk)?;
        done += chunk.len() as u64;
        if let Some(total) = total {
            let percent = done * 100 / total;
            if percent > last {
                last = percent;
                progress(app, percent as f64);
            }
        }
    }
    Ok(())
}

fn unpack(app: &tauri::AppHandle, archive: &Path, dest: &Path) -> anyhow::Result<()> {
    let mut reader = sevenz_rust::SevenZReader::open(archive, sevenz_rust::Password::empty())?;
    let files = reader
        .archive()
        .files
        .iter()
        .filter(|f| !f.is_directory())
        .count()
        .max(1);
    let mut done = 0;
    reader.for_each_entries(|entry, data| {
        let name = entry.name().replace('\\', "/");
        let rel = Path::new(&name);
        if !rel.components().all(|c| matches!(c, Normal(_))) {
            return Ok(true);
        }
        let out = dest.join(rel);
        if entry.is_directory() {
            fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            std::io::copy(data, &mut File::create(&out)?)?;
            done += 1;
            progress(app, done as f64 / files as f64 * 100.0);
        }
        Ok(true)
    })?;
    Ok(())
}
