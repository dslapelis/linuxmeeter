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

#[cfg(test)]
mod tests {
    use super::*;
    use lm_protocol::{BusId, BusState, EqBand, EqBandKind, EqParams, GateParams, LimiterParams, StripState};
    use std::collections::BTreeMap;

    fn routes(on: &[BusId]) -> BTreeMap<BusId, bool> {
        BusId::ALL.iter().map(|b| (*b, on.contains(b))).collect()
    }

    fn strip(id: u32, kind: StripKind) -> StripState {
        StripState {
            id,
            kind,
            label: "Test".into(),
            hw_key: (kind == StripKind::Hardware).then(|| "alsa_input.usb-foo".to_string()),
            online: true,
            gain_db: -6.0,
            mute: false,
            solo: false,
            routes: routes(&[BusId::A1]),
            gate: GateParams::default(),
            comp: Default::default(),
            eq: EqParams::default(),
        }
    }

    fn profile(strips: Vec<StripState>) -> Profile {
        Profile {
            take_default_output: false,
            strips,
            buses: vec![BusState {
                id: BusId::A1,
                label: "Speakers".into(),
                target_hw_key: Some("alsa_output.hdmi".into()),
                online: false,
                gain_db: 0.0,
                mute: false,
                limiter: LimiterParams::default(),
            }],
        }
    }

    /// Solo is a live performance state. Restoring it would leave the user
    /// silently muted on every other strip after a restart.
    #[test]
    fn sanitize_clears_solo() {
        let mut s = strip(1, StripKind::Virtual);
        s.solo = true;
        let mut p = profile(vec![s]);
        sanitize(&mut p);
        assert!(!p.strips[0].solo);
    }

    /// A hardware strip is only online if its device is present, which is
    /// recomputed against the live registry — never trusted from disk.
    #[test]
    fn sanitize_resets_hardware_online_but_trusts_virtual() {
        let mut p = profile(vec![strip(1, StripKind::Hardware), strip(2, StripKind::Virtual)]);
        sanitize(&mut p);
        assert!(!p.strips[0].online, "hardware strips start offline until seen");
        assert!(p.strips[1].online, "virtual strips always exist");
        assert!(p.buses[0].online);
    }

    /// Profiles written before the color pad existed have only the four
    /// EQ-panel bands; bands 4..7 must be filled with the voice-tuned corners
    /// or the pad would drive whatever indices happen to exist.
    #[test]
    fn sanitize_migrates_pre_color_pad_profiles_to_eight_bands() {
        let mut s = strip(1, StripKind::Virtual);
        s.eq.bands.truncate(4);
        let mut p = profile(vec![s]);
        sanitize(&mut p);

        let bands = &p.strips[0].eq.bands;
        assert_eq!(bands.len(), 8);
        assert_eq!(&bands[4..], &EqParams::color_defaults()[..], "color bands must be the voice-tuned defaults");
    }

    #[test]
    fn sanitize_preserves_user_eq_bands_while_migrating() {
        let mut s = strip(1, StripKind::Virtual);
        s.eq.bands.truncate(4);
        s.eq.bands[1] = EqBand { kind: EqBandKind::Peak, freq_hz: 900.0, gain_db: -4.5, q: 2.0 };
        let mut p = profile(vec![s]);
        sanitize(&mut p);
        assert_eq!(p.strips[0].eq.bands[1].freq_hz, 900.0);
        assert_eq!(p.strips[0].eq.bands[1].gain_db, -4.5);
    }

    #[test]
    fn sanitize_leaves_current_profiles_untouched() {
        let mut p = profile(vec![strip(1, StripKind::Virtual)]);
        let before = p.strips[0].eq.bands.clone();
        sanitize(&mut p);
        assert_eq!(p.strips[0].eq.bands, before);
    }

    /// Profiles are advertised as hand-editable, so the TOML has to survive a
    /// full round trip with everything the mixer needs to rebuild itself.
    #[test]
    fn profile_survives_a_toml_round_trip() {
        let mut p = profile(vec![strip(1, StripKind::Hardware), strip(2, StripKind::Virtual)]);
        p.take_default_output = true;
        p.strips[0].gain_db = -13.5;
        p.strips[0].routes.insert(BusId::B1, true);
        p.strips[0].gate.enabled = true;
        p.strips[0].gate.threshold_db = -33.0;

        let text = toml::to_string_pretty(&p).expect("serialize");
        let back: Profile = toml::from_str(&text).expect("deserialize");

        assert!(back.take_default_output);
        assert_eq!(back.strips[0].gain_db, -13.5);
        assert_eq!(back.strips[0].routes[&BusId::B1], true);
        assert_eq!(back.strips[0].gate.threshold_db, -33.0);
        assert_eq!(back.strips[0].hw_key.as_deref(), Some("alsa_input.usb-foo"));
        assert_eq!(back.buses[0].target_hw_key.as_deref(), Some("alsa_output.hdmi"));
        assert_eq!(back.strips[1].kind, StripKind::Virtual);
    }

    /// A profile written before the EQ existed has no `eq` key at all. It must
    /// load with defaults rather than being rejected, which would silently
    /// reset the user's whole mixer.
    #[test]
    fn profile_predating_the_eq_still_loads() {
        const OLD: &str = r#"
take_default_output = false

[[strips]]
id = 1
kind = "virtual"
label = "System"
online = true
gainDb = -6.0
mute = false
solo = false

[strips.routes]
A1 = true
A2 = false
B1 = false
B2 = false

[strips.gate]
enabled = false
thresholdDb = -40.0
attackMs = 20.0
releaseMs = 100.0
holdMs = 50.0

[strips.comp]
enabled = false
thresholdDb = -18.0
ratio = 4.0
attackMs = 20.0
releaseMs = 100.0
makeupDb = 0.0

[[buses]]
id = "A1"
label = "Speakers"
targetHwKey = "alsa_output.hdmi"
online = true
gainDb = 0.0
mute = false

[buses.limiter]
enabled = true
thresholdDb = -1.0
releaseMs = 50.0
"#;
        let mut back: Profile = toml::from_str(OLD).expect("eq must be #[serde(default)]");
        assert_eq!(back.strips[0].label, "System");
        assert_eq!(back.strips[0].gain_db, -6.0);
        sanitize(&mut back);
        assert_eq!(back.strips[0].eq.bands.len(), 8, "defaults then migration give a full band set");
    }

    #[test]
    fn config_defaults_to_the_default_profile() {
        assert_eq!(Config::default().last_profile, "Default");
    }

    #[test]
    fn config_dir_lives_under_xdg_config_home() {
        // Uses whatever the environment says; assert the shape, not the value.
        let dir = config_dir();
        assert!(dir.ends_with("linuxmeeter"), "got {}", dir.display());
    }
}
