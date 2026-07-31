//! PipeWire audio engine for linuxmeeter.
//!
//! Everything in this crate runs on (or is driven from) a dedicated engine
//! thread that owns the PipeWire main loop. No Tauri dependencies here so the
//! engine stays testable headless.

pub mod engine;
pub mod filterchain;
pub mod links;
pub mod meter;
pub mod params;
pub mod persist;
pub mod registry;
