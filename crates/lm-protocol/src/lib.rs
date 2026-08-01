//! Shared types between the audio engine, the Tauri shell, and (via serde)
//! the frontend. Serialization is camelCase to match the TypeScript mirror in
//! `src/lib/types.ts` — keep the two in sync.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum BusId {
    A1,
    A2,
    B1,
    B2,
}

impl BusId {
    pub const ALL: [BusId; 4] = [BusId::A1, BusId::A2, BusId::B1, BusId::B2];

    /// Lowercase node-name fragment: `lm.bus.a1`.
    pub fn node_base(&self) -> String {
        format!("lm.bus.{}", format!("{self:?}").to_lowercase())
    }
}

pub type StripId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StripKind {
    /// Appears as an Audio/Sink other apps can play into.
    Virtual,
    /// Captures from a hardware (or any existing) source node.
    Hardware,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateParams {
    pub enabled: bool,
    pub threshold_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub hold_ms: f32,
}

impl Default for GateParams {
    fn default() -> Self {
        Self { enabled: false, threshold_db: -40.0, attack_ms: 20.0, release_ms: 100.0, hold_ms: 50.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompParams {
    pub enabled: bool,
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_db: f32,
}

impl Default for CompParams {
    fn default() -> Self {
        Self { enabled: false, threshold_db: -18.0, ratio: 4.0, attack_ms: 20.0, release_ms: 100.0, makeup_db: 0.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EqBandKind {
    LowShelf,
    Peak,
    HighShelf,
}

impl EqBandKind {
    /// LSP para_equalizer `ft_N` filter-type value.
    pub fn lsp_filter_type(&self) -> f32 {
        match self {
            EqBandKind::Peak => 1.0,      // Bell
            EqBandKind::HighShelf => 3.0, // Hi-shelf
            EqBandKind::LowShelf => 5.0,  // Lo-shelf
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqBand {
    pub kind: EqBandKind,
    pub freq_hz: f32,
    pub gain_db: f32,
    pub q: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqParams {
    pub enabled: bool,
    pub bands: Vec<EqBand>,
}

impl EqParams {
    /// Bands 4..8 are reserved for the voice-color pad (voice-tuned corners),
    /// independent of the four user-facing EQ-panel bands.
    pub fn color_defaults() -> [EqBand; 4] {
        [
            EqBand { kind: EqBandKind::LowShelf, freq_hz: 300.0, gain_db: 0.0, q: 0.7 },
            EqBand { kind: EqBandKind::Peak, freq_hz: 400.0, gain_db: 0.0, q: 0.8 },
            EqBand { kind: EqBandKind::Peak, freq_hz: 3000.0, gain_db: 0.0, q: 0.8 },
            EqBand { kind: EqBandKind::HighShelf, freq_hz: 3500.0, gain_db: 0.0, q: 0.7 },
        ]
    }
}

impl Default for EqParams {
    fn default() -> Self {
        let mut bands = vec![
            EqBand { kind: EqBandKind::LowShelf, freq_hz: 100.0, gain_db: 0.0, q: 0.7 },
            EqBand { kind: EqBandKind::Peak, freq_hz: 400.0, gain_db: 0.0, q: 1.0 },
            EqBand { kind: EqBandKind::Peak, freq_hz: 2500.0, gain_db: 0.0, q: 1.0 },
            EqBand { kind: EqBandKind::HighShelf, freq_hz: 8000.0, gain_db: 0.0, q: 0.7 },
        ];
        bands.extend(Self::color_defaults());
        Self { enabled: false, bands }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LimiterParams {
    pub enabled: bool,
    pub threshold_db: f32,
    pub release_ms: f32,
}

impl Default for LimiterParams {
    fn default() -> Self {
        Self { enabled: true, threshold_db: -1.0, release_ms: 50.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StripState {
    pub id: StripId,
    pub kind: StripKind,
    pub label: String,
    /// Stable PipeWire identity (`node.name`) of the hardware source, for Hardware strips.
    pub hw_key: Option<String>,
    pub online: bool,
    pub gain_db: f32,
    pub mute: bool,
    pub solo: bool,
    pub routes: BTreeMap<BusId, bool>,
    pub gate: GateParams,
    pub comp: CompParams,
    /// `default` so profiles written before the EQ existed still load.
    #[serde(default)]
    pub eq: EqParams,
}

impl StripState {
    /// Base node name; virtual strips' sink node carries this name directly.
    pub fn node_base(&self) -> String {
        format!("lm.strip.{}", self.id)
    }

    /// Node that carries Props (volume/mute/filter params): the capture side.
    pub fn control_node(&self) -> String {
        match self.kind {
            StripKind::Virtual => self.node_base(),
            StripKind::Hardware => format!("{}.cap", self.node_base()),
        }
    }

    /// Node whose output ports feed the routing matrix.
    pub fn out_node(&self) -> String {
        format!("{}.out", self.node_base())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusState {
    pub id: BusId,
    pub label: String,
    /// `node.name` of the hardware sink this bus feeds (A buses). None = virtual source (B buses).
    pub target_hw_key: Option<String>,
    pub online: bool,
    pub gain_db: f32,
    pub mute: bool,
    pub limiter: LimiterParams,
}

impl BusState {
    pub fn node_base(&self) -> String {
        self.id.node_base()
    }

    /// Mix-input node (also carries Props for gain/mute/limiter).
    pub fn in_node(&self) -> String {
        format!("{}.in", self.node_base())
    }

    /// Node whose output ports carry the post-limiter signal (meter tap point).
    pub fn tap_node(&self) -> String {
        if self.target_hw_key.is_some() {
            format!("{}.out", self.node_base())
        } else {
            self.node_base()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub node_name: String,
    pub description: String,
    pub media_class: String,
    pub serial: u64,
    pub channels: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub strips: Vec<StripState>,
    pub buses: Vec<BusState>,
    pub devices: Vec<DeviceInfo>,
    pub profile_name: String,
    /// True while linuxmeeter's System strip is the system default output.
    pub take_default_output: bool,
}

/// One batched meter update, ~30/s. Channel layout: [peakL, peakR, rmsL, rmsR]
/// in dBFS; entries follow `AppState.strips` / `AppState.buses` order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterFrame {
    pub seq: u32,
    pub strips: Vec<[f32; 4]>,
    pub buses: Vec<[f32; 4]>,
}

/// A strip or a bus, as sent by the frontend: `{"strip": 3}` or `{"bus": "A1"}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Target {
    Strip { strip: StripId },
    Bus { bus: BusId },
}

/// Commands into the engine thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineCommand {
    SetGain { target: Target, db: f32 },
    SetMute { target: Target, mute: bool },
    SetSolo { strip: StripId, solo: bool },
    SetRoute { strip: StripId, bus: BusId, on: bool },
    SetGateParams { strip: StripId, params: GateParams },
    SetCompParams { strip: StripId, params: CompParams },
    SetEqParams { strip: StripId, params: EqParams },
    SetLimiterParams { bus: BusId, params: LimiterParams },
    /// Point a hardware strip at a different capture device (module reload).
    SetStripDevice { strip: StripId, hw_key: String },
    /// Point an A-bus at a different output device (module reload).
    SetBusTarget { bus: BusId, hw_key: String },
    /// Make the System strip the system-wide default output (VoiceMeeter
    /// style: all desktop audio flows through the mixer), or restore the
    /// previous hardware default.
    SetDefaultOutput { on: bool },
    Shutdown,
}

/// Events out of the engine thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineEvent {
    /// Full snapshot; sent on startup and after any state-affecting change.
    State(AppState),
    Meters(MeterFrame),
}
