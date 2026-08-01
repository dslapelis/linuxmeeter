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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(args: &str) -> Value {
        serde_json::from_str(args).expect("module args must be valid JSON")
    }

    fn every_props_block(v: &Value) -> Vec<&Value> {
        ["capture.props", "playback.props"].iter().filter_map(|k| v.get(k)).collect()
    }

    fn all_args() -> Vec<(&'static str, String)> {
        vec![
            ("virtual strip", virtual_strip_args("lm.strip.1", "System", false)),
            ("virtual strip autoconnected", virtual_strip_args("lm.strip.1", "System", true)),
            ("hardware strip", hardware_strip_args("lm.strip.2", "Mic", "alsa_input.usb-foo")),
            ("A bus", bus_args("lm.bus.a1", "Speakers", Some("alsa_output.usb-foo"))),
            ("B bus", bus_args("lm.bus.b1", "Stream Mic", None)),
        ]
    }

    /// WirePlumber's restore-stream overwrites volume/mute on node appearance.
    /// Every node we create must opt out, or the mixer fights the session
    /// manager on every device change.
    #[test]
    fn every_node_opts_out_of_wireplumber_restore() {
        for (what, args) in all_args() {
            let v = parse(&args);
            let blocks = every_props_block(&v);
            assert_eq!(blocks.len(), 2, "{what}: expected capture + playback props");
            for props in blocks {
                assert_eq!(
                    props.get("state.restore-props"),
                    Some(&Value::Bool(false)),
                    "{what}: node {:?} would let WirePlumber restore our volume",
                    props.get("node.name")
                );
            }
        }
    }

    #[test]
    fn every_node_is_marked_virtual() {
        for (what, args) in all_args() {
            let v = parse(&args);
            for props in every_props_block(&v) {
                assert_eq!(props.get("node.virtual"), Some(&Value::Bool(true)), "{what}");
            }
        }
    }

    /// Node names are the identity the profile, routing matrix, and meter taps
    /// all key on — they must match the helpers in lm-protocol exactly.
    #[test]
    fn virtual_strip_names_sink_and_output() {
        let v = parse(&virtual_strip_args("lm.strip.3", "Music", false));
        assert_eq!(v["capture.props"]["node.name"], "lm.strip.3");
        assert_eq!(v["capture.props"]["media.class"], "Audio/Sink", "apps must be able to play into it");
        assert_eq!(v["playback.props"]["node.name"], "lm.strip.3.out");
        assert_eq!(v["playback.props"]["media.class"], "Stream/Output/Audio");
    }

    #[test]
    fn hardware_strip_captures_from_its_target() {
        let v = parse(&hardware_strip_args("lm.strip.1", "Mic", "alsa_input.usb-zoom"));
        assert_eq!(v["capture.props"]["node.name"], "lm.strip.1.cap");
        assert_eq!(v["capture.props"]["target.object"], "alsa_input.usb-zoom");
        assert_eq!(v["capture.props"]["stream.capture.sink"], Value::Bool(false), "capture the source, not its monitor");
        assert_eq!(v["playback.props"]["node.name"], "lm.strip.1.out");
    }

    /// The routing matrix owns every link; letting the session manager
    /// autoconnect strip outputs would duplicate audio paths.
    #[test]
    fn strip_outputs_do_not_autoconnect() {
        for args in [
            virtual_strip_args("lm.strip.1", "System", false),
            hardware_strip_args("lm.strip.2", "Mic", "alsa_input.usb-foo"),
        ] {
            let v = parse(&args);
            assert_eq!(v["playback.props"]["node.autoconnect"], Value::Bool(false));
        }
    }

    /// The isolation escape hatch used by the spike example.
    #[test]
    fn autoconnect_flag_opens_the_output() {
        let v = parse(&virtual_strip_args("lm.strip.1", "System", true));
        assert!(v["playback.props"].get("node.autoconnect").is_none());
    }

    #[test]
    fn a_bus_plays_to_hardware() {
        let v = parse(&bus_args("lm.bus.a1", "Speakers", Some("alsa_output.hdmi")));
        assert_eq!(v["capture.props"]["node.name"], "lm.bus.a1.in");
        assert_eq!(v["playback.props"]["node.name"], "lm.bus.a1.out");
        assert_eq!(v["playback.props"]["target.object"], "alsa_output.hdmi");
    }

    /// B buses are the virtual microphones Discord/OBS select; the node must
    /// carry the bus name itself, since that is what `tap_node` meters.
    #[test]
    fn b_bus_is_a_virtual_microphone() {
        let v = parse(&bus_args("lm.bus.b1", "Stream Mic", None));
        assert_eq!(v["playback.props"]["node.name"], "lm.bus.b1");
        assert_eq!(v["playback.props"]["media.class"], "Audio/Source");
        assert!(v["playback.props"].get("target.object").is_none());
    }

    #[test]
    fn strip_dsp_chain_is_gate_then_comp_then_eq() {
        let v = parse(&virtual_strip_args("lm.strip.1", "System", false));
        let names: Vec<&str> = v["filter.graph"]["nodes"]
            .as_array()
            .expect("nodes array")
            .iter()
            .map(|n| n["name"].as_str().expect("node name"))
            .collect();
        assert_eq!(names, ["gate", "comp", "eq"]);
        // The Props keys the engine writes are "<node>:<port>"; these prefixes
        // are the contract between filterchain.rs and engine.rs.
        assert_eq!(v["filter.graph"]["inputs"][0], "gate:in_l");
        assert_eq!(v["filter.graph"]["outputs"][0], "eq:out_l");
    }

    #[test]
    fn bus_dsp_is_a_limiter() {
        let v = parse(&bus_args("lm.bus.a1", "Speakers", None));
        assert_eq!(v["filter.graph"]["nodes"][0]["name"], "lim");
        assert_eq!(v["filter.graph"]["inputs"][0], "lim:in_l");
    }

    #[test]
    fn module_args_reject_interior_nul() {
        // LoadedModule::load builds a CString; a NUL must surface as BadArgs
        // rather than a panic. (Labels come from user-editable profiles.)
        let err = CString::new("has\0nul").unwrap_err();
        let module_err: ModuleError = err.into();
        assert!(matches!(module_err, ModuleError::BadArgs(_)));
    }
}
