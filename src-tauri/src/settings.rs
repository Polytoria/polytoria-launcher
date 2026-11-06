use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::launch::Component;

#[derive(Serialize, Deserialize)]
struct Entry {
    #[serde(rename = "Release")]
    release: String,
    #[serde(rename = "Version")]
    version: String,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(rename = "Manifest", default)]
    manifest: HashMap<String, Vec<Entry>>,
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_else(|e| {
            log!("Settings.json is broken ({e}), starting fresh");
            Self::default()
        })
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn version(&self, component: Component, release: &str) -> Option<String> {
        self.manifest
            .get(component.name())?
            .iter()
            .find(|e| e.release == release)
            .map(|e| e.version.clone())
    }

    pub fn set(&mut self, component: Component, release: &str, version: &str) {
        let entries = self.manifest.entry(component.name().into()).or_default();
        entries.retain(|e| e.release != release);
        entries.push(Entry {
            release: release.into(),
            version: version.into(),
        });
    }
}
