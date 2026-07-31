//! Loading `libpipewire-module-filter-chain` into our own context, and
//! building the SPA-JSON module args for strip/bus DSP graphs.
//!
//! Modules are loaded in-process so every virtual device dies with the app —
//! `pw-dump` must show zero `lm.*` nodes after exit.

use std::ffi::CString;
use std::ptr::NonNull;

use pipewire::context::ContextRc;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("module args contained an interior NUL byte")]
    BadArgs(#[from] std::ffi::NulError),
    #[error("pw_context_load_module failed: {0}")]
    LoadFailed(std::io::Error),
}

/// An in-process PipeWire module. Destroying this unloads the module and
/// removes every node it created from the graph.
///
/// Must be dropped before the [`ContextRc`] it was loaded into; holding a
/// clone of the context here guarantees that ordering.
pub struct LoadedModule {
    ptr: NonNull<pipewire::sys::pw_impl_module>,
    _context: ContextRc,
}

impl LoadedModule {
    pub fn load(context: &ContextRc, name: &str, args: &str) -> Result<Self, ModuleError> {
        let cname = CString::new(name)?;
        let cargs = CString::new(args)?;
        // SAFETY: context pointer is valid for the lifetime of ContextRc, and
        // we keep a ContextRc clone alive for as long as the module exists.
        let ptr = unsafe {
            pipewire::sys::pw_context_load_module(
                context.as_raw_ptr(),
                cname.as_ptr(),
                cargs.as_ptr(),
                std::ptr::null_mut(),
            )
        };
        match NonNull::new(ptr) {
            Some(ptr) => Ok(Self { ptr, _context: context.clone() }),
            None => Err(ModuleError::LoadFailed(std::io::Error::last_os_error())),
        }
    }
}

impl Drop for LoadedModule {
    fn drop(&mut self) {
        // SAFETY: ptr came from pw_context_load_module and the context is
        // still alive (we hold an Rc on it).
        unsafe { pipewire::sys::pw_impl_module_destroy(self.ptr.as_ptr()) };
    }
}

const LV2_GATE: &str = "http://lsp-plug.in/plugins/lv2/gate_stereo";
const LV2_COMP: &str = "http://lsp-plug.in/plugins/lv2/compressor_stereo";
const LV2_EQ: &str = "http://lsp-plug.in/plugins/lv2/para_equalizer_x8_stereo";
const LV2_LIMITER: &str = "http://lsp-plug.in/plugins/lv2/limiter_stereo";

/// Module args for a virtual input strip: an `Audio/Sink` other apps play
/// into, processed through gate → comp → EQ.
///
/// `sink_name` becomes the visible device (e.g. `lm.strip.v1`); the processed
/// output appears as stream node `<sink_name>.out`.
///
/// When `autoconnect_output` is true the output stream connects to the default
/// sink (useful for testing a strip in isolation before buses exist). In the
/// real graph it stays false and the routing matrix links it to bus inputs.
pub fn virtual_strip_args(sink_name: &str, label: &str, autoconnect_output: bool) -> String {
    let mut playback_props = json!({
        "node.name": format!("{sink_name}.out"),
        "node.description": format!("{label} (out)"),
        "media.class": "Stream/Output/Audio",
        "node.virtual": true,
            "state.restore-props": false,
        "node.dont-reconnect": true,
    });
    if !autoconnect_output {
        playback_props["node.autoconnect"] = json!(false);
    }
    json!({
        "node.description": label,
        "media.name": label,
        "filter.graph": {
            "nodes": [
                { "type": "lv2", "name": "gate", "plugin": LV2_GATE },
                { "type": "lv2", "name": "comp", "plugin": LV2_COMP },
                { "type": "lv2", "name": "eq",   "plugin": LV2_EQ },
            ],
            "links": [
                { "output": "gate:out_l", "input": "comp:in_l" },
                { "output": "gate:out_r", "input": "comp:in_r" },
                { "output": "comp:out_l", "input": "eq:in_l" },
                { "output": "comp:out_r", "input": "eq:in_r" },
            ],
            "inputs":  [ "gate:in_l", "gate:in_r" ],
            "outputs": [ "eq:out_l", "eq:out_r" ],
        },
        "capture.props": {
            "node.name": sink_name,
            "node.description": label,
            "media.class": "Audio/Sink",
            "audio.position": [ "FL", "FR" ],
            "node.virtual": true,
            "state.restore-props": false,
        },
        "playback.props": playback_props,
    })
    .to_string()
}

/// Module args for a hardware input strip: captures from an existing source
/// node (`hw_target` = its `node.name`), processed through the same
/// gate → comp → EQ graph. Output appears as `<strip_name>.out`.
pub fn hardware_strip_args(strip_name: &str, label: &str, hw_target: &str) -> String {
    json!({
        "node.description": label,
        "media.name": label,
        "filter.graph": {
            "nodes": [
                { "type": "lv2", "name": "gate", "plugin": LV2_GATE },
                { "type": "lv2", "name": "comp", "plugin": LV2_COMP },
                { "type": "lv2", "name": "eq",   "plugin": LV2_EQ },
            ],
            "links": [
                { "output": "gate:out_l", "input": "comp:in_l" },
                { "output": "gate:out_r", "input": "comp:in_r" },
                { "output": "comp:out_l", "input": "eq:in_l" },
                { "output": "comp:out_r", "input": "eq:in_r" },
            ],
            "inputs":  [ "gate:in_l", "gate:in_r" ],
            "outputs": [ "eq:out_l", "eq:out_r" ],
        },
        "capture.props": {
            "node.name": format!("{strip_name}.cap"),
            "node.description": format!("{label} (capture)"),
            "target.object": hw_target,
            "stream.capture.sink": false,
            "node.dont-reconnect": true,
            "node.virtual": true,
            "state.restore-props": false,
        },
        "playback.props": {
            "node.name": format!("{strip_name}.out"),
            "node.description": format!("{label} (out)"),
            "media.class": "Stream/Output/Audio",
            "node.autoconnect": false,
            "node.dont-reconnect": true,
            "node.virtual": true,
            "state.restore-props": false,
        },
    })
    .to_string()
}

/// Module args for an output bus.
///
/// `hw_target` = Some(node.name of a hardware sink) makes an A-bus whose
/// output plays to that device; None makes a B-bus that appears as a
/// microphone (`media.class = Audio/Source`) for Discord/OBS/etc.
pub fn bus_args(bus_name: &str, label: &str, hw_target: Option<&str>) -> String {
    let playback_props = match hw_target {
        Some(target) => json!({
            "node.name": format!("{bus_name}.out"),
            "node.description": format!("{label} (out)"),
            "target.object": target,
            "node.dont-reconnect": true,
            "node.virtual": true,
            "state.restore-props": false,
        }),
        None => json!({
            "node.name": bus_name,
            "node.description": label,
            "media.class": "Audio/Source",
            "audio.position": [ "FL", "FR" ],
            "node.virtual": true,
            "state.restore-props": false,
        }),
    };
    json!({
        "node.description": label,
        "media.name": label,
        "filter.graph": {
            "nodes": [
                { "type": "lv2", "name": "lim", "plugin": LV2_LIMITER },
            ],
            "inputs":  [ "lim:in_l", "lim:in_r" ],
            "outputs": [ "lim:out_l", "lim:out_r" ],
        },
        "capture.props": {
            "node.name": format!("{bus_name}.in"),
            "node.description": format!("{label} (in)"),
            "media.class": "Stream/Input/Audio",
            "node.autoconnect": false,
            "node.dont-reconnect": true,
            "node.virtual": true,
            "state.restore-props": false,
        },
        "playback.props": playback_props,
    })
    .to_string()
}
