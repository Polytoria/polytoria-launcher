use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Component {
    Client,
    Creator,
}

impl Component {
    pub fn name(self) -> &'static str {
        match self {
            Self::Client => "Client",
            Self::Creator => "Creator",
        }
    }
}

#[derive(Clone)]
pub struct Link {
    pub component: Component,
    pub release: &'static str,
    pub test: bool,
    pub token: String,
    pub map: Option<String>,
}

pub fn parse(args: &[String]) -> anyhow::Result<Link> {
    let link = args
        .iter()
        .find_map(|a| a.strip_prefix("polytoria://"))
        .context("no polytoria:// link")?;

    let mut parts = link.splitn(3, '/');
    let kind = parts.next().unwrap_or_default().to_lowercase();
    let (base, release) = if let Some(base) = kind.strip_suffix("nightly") {
        (base, "Nightly")
    } else if let Some(base) = kind.strip_suffix("beta") {
        (base, "Beta")
    } else {
        (kind.as_str(), "Stable")
    };
    let (component, test) = match base {
        "client" => (Component::Client, false),
        "test" => (Component::Client, true),
        "creator" => (Component::Creator, false),
        _ => bail!("bad launch type: {kind}"),
    };

    let token = parts.next().unwrap_or_default().to_string();
    if token.is_empty() {
        bail!("no token");
    }
    let map = parts.next().filter(|m| !m.is_empty()).map(String::from);

    Ok(Link {
        component,
        release,
        test,
        token,
        map,
    })
}

pub fn binary(data_dir: &Path, component: Component, version: &str) -> PathBuf {
    let name = component.name();
    let binary = if cfg!(target_os = "windows") {
        format!("Polytoria {name}.exe")
    } else if cfg!(target_os = "macos") {
        format!("Polytoria {name}.app/Contents/MacOS/Polytoria {name}")
    } else {
        format!("Polytoria {name}.x86_64")
    };
    data_dir.join(name).join(version).join(binary)
}

pub fn args(link: &Link) -> Option<Vec<String>> {
    let token = link.token.clone();
    Some(match (link.component, link.test) {
        (Component::Client, true) => vec!["-solo".into(), link.map.clone()?],
        (Component::Client, false) => {
            vec!["-network".into(), "client".into(), "-token".into(), token]
        }
        (Component::Creator, _) => vec!["-token".into(), token],
    })
}

pub fn kill(binary: &Path) {
    let name = binary.file_name().unwrap();
    #[cfg(windows)]
    let _ = quiet(Command::new("taskkill").args(["/f", "/im"]).arg(name)).status();
    #[cfg(not(windows))]
    let _ = quiet(Command::new("pkill").args(["-9", "-f"]).arg(name)).status();
}

pub fn spawn(binary: &Path, args: &[String]) -> std::io::Result<()> {
    quiet(Command::new(binary).args(args)).spawn().map(drop)
}

pub fn open_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = Command::new("rundll32");
    #[cfg(target_os = "windows")]
    command.args(["url.dll,FileProtocolHandler", url]);
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "macos")]
    command.arg(url);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = Command::new("xdg-open");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    command.arg(url);
    quiet(&mut command).spawn().map(drop)
}

fn quiet(command: &mut Command) -> &mut Command {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
