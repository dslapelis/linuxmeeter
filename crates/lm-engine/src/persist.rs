//! Profile persistence: TOML under `$XDG_CONFIG_HOME/linuxmeeter/`.
//!
//! `config.toml` holds app-level settings (currently the last profile name);
//! `profiles/<name>.toml` holds everything needed to rebuild the mixer.
//! Identity is by `node.name` — never transient PipeWire ids.

use std::fs;
use std::io;
use std::path::PathBuf;

use lm_protocol::{BusState, StripKind, StripState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub take_default_output: bool,
    pub strips: Vec<StripState>,
    pub buses: Vec<BusState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub last_profile: String,
}

impl Default for Config {
    fn default() -> Self {
        Self { last_profile: "Default".into() }
    }
}

pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").expect("HOME not set")).join(".config"))
        .join("linuxmeeter")
}

pub fn load_config() -> Config {
    let path = config_dir().join("config.toml");
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| toml::from_str(&s).map_err(|e| tracing::warn!("bad config.toml: {e}")).ok())
        .unwrap_or_default()
}

pub fn save_config(config: &Config) -> io::Result<()> {
    let toml = toml::to_string_pretty(config).map_err(io::Error::other)?;
    atomic_write(&config_dir().join("config.toml"), &toml)
}

pub fn load_profile(name: &str) -> Option<Profile> {
    let path = config_dir().join("profiles").join(format!("{name}.toml"));
    let text = fs::read_to_string(&path).ok()?;
    match toml::from_str::<Profile>(&text) {
        Ok(mut p) => {
            sanitize(&mut p);
            tracing::info!("loaded profile {name} ({})", path.display());
            Some(p)
        }
        Err(e) => {
            tracing::warn!("profile {name} unreadable, using defaults: {e}");
            None
        }
    }
}

pub fn save_profile(name: &str, profile: &Profile) -> io::Result<()> {
    let dir = config_dir().join("profiles");
    fs::create_dir_all(&dir)?;
    let toml = toml::to_string_pretty(profile).map_err(io::Error::other)?;
    atomic_write(&dir.join(format!("{name}.toml")), &toml)
}

/// Reset transient runtime state that should never survive a restart.
fn sanitize(p: &mut Profile) {
    for s in &mut p.strips {
        s.solo = false;
        // `online` is recomputed against the live registry at build time.
        s.online = s.kind == StripKind::Virtual;
        // Migrate pre-color-pad profiles (4 bands) to 8 bands.
        while s.eq.bands.len() < 8 {
            let i = s.eq.bands.len() - 4;
            s.eq.bands.push(lm_protocol::EqParams::color_defaults()[i.min(3)]);
        }
    }
    for b in &mut p.buses {
        b.online = true;
    }
}

fn atomic_write(path: &std::path::Path, contents: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)
}
