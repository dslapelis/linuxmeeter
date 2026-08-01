//! Integration tests against a real PipeWire daemon — real filter-chain
//! modules, real LSP plugins, real audio samples — with no hardware and no
//! human listening.
//!
//! These are `#[ignore]`d so a bare `cargo test` never touches an audio graph.
//! Run them with `make test-audio`, which starts a private daemon (a null sink,
//! no session manager, its own socket) and points this process at it.
//!
//! The rig deliberately uses the engine's own building blocks — `LoadedModule`,
//! `LinkManager`, `MeterTap`, `params` — so a regression in any of them shows
//! up here as silence or a wrong level, exactly as a user would hear it.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use lm_engine::filterchain::{bus_args, virtual_strip_args, LoadedModule};
use lm_engine::links::LinkManager;
use lm_engine::meter::{MeterAccum, MeterTap};
use lm_engine::params;
use lm_engine::registry::{parse, GraphModel, PortDirection};
use lm_protocol::{BusState, LimiterParams, StripKind, StripState};

use pipewire::context::ContextRc;
use pipewire::core::CoreRc;
use pipewire::main_loop::MainLoopRc;
use pipewire::node::Node;
use pipewire::properties::properties;
use pipewire::stream::{StreamFlags, StreamListener, StreamRc};
use pipewire::types::ObjectType;

const RATE: u32 = 48000;
const SINK: &str = "alsa_output.lm-test-sink";

/// Guard against ever running these against the user's live audio session,
/// where they would create real devices and rewire real links.
fn require_private_daemon() {
    assert_eq!(
        std::env::var("LM_TEST_DAEMON").ok().as_deref(),
        Some("1"),
        "audio tests must run against the private daemon — use `make test-audio`, \
         not a bare `cargo test -- --ignored`"
    );
}

struct State {
    model: GraphModel,
    links: LinkManager,
    /// Node names we want proxies for, so params can be written to them.
    wanted: HashSet<String>,
    nodes: HashMap<String, Node>,
}

/// A PipeWire client with a graph model, a link reconciler, and whatever
/// modules/taps/streams a test has created. Everything lives on one main loop.
struct Rig {
    mainloop: MainLoopRc,
    context: ContextRc,
    core: CoreRc,
    state: Rc<RefCell<State>>,
    modules: Vec<LoadedModule>,
    taps: Vec<MeterTap>,
    tones: Vec<Tone>,
    /// The registry proxy must outlive its listener — dropping it silently
    /// unsubscribes and the graph model stays empty forever.
    _registry: pipewire::registry::RegistryRc,
    _reg_listener: pipewire::registry::Listener,
}

impl Rig {
    fn new() -> Self {
        require_private_daemon();
        pipewire::init();

        let mainloop = MainLoopRc::new(None).expect("main loop");
        let context = ContextRc::new(&mainloop, None).expect("context");
        let core = context.connect_rc(None).expect("connect to the test daemon");
        let registry = core.get_registry_rc().expect("registry");

        let state = Rc::new(RefCell::new(State {
            model: GraphModel::default(),
            links: LinkManager::default(),
            wanted: HashSet::new(),
            nodes: HashMap::new(),
        }));

        // Mirror the engine: track globals, bind wanted control nodes, and
        // reconcile links whenever ports appear.
        let reg_listener = {
            let state = state.clone();
            let state_rm = state.clone();
            let core_cb = core.clone();
            let registry_weak = registry.downgrade();
            registry
                .add_listener_local()
                .global(move |global| {
                    let mut s = state.borrow_mut();
                    match global.type_ {
                        ObjectType::Node => {
                            let Some(props) = global.props else { return };
                            let Some(n) = parse::node(global.id, props) else { return };
                            let name = n.name.clone();
                            s.model.nodes.insert(global.id, n);
                            if s.wanted.contains(&name) && !s.nodes.contains_key(&name) {
                                if let Some(registry) = registry_weak.upgrade() {
                                    if let Ok(node) = registry.bind::<Node, _>(global) {
                                        s.nodes.insert(name, node);
                                    }
                                }
                            }
                        }
                        ObjectType::Port => {
                            let Some(props) = global.props else { return };
                            if let Some(p) = parse::port(global.id, props) {
                                s.model.ports.insert(global.id, p);
                                let State { links, model, .. } = &mut *s;
                                links.reconcile(&core_cb, model);
                            }
                        }
                        ObjectType::Link => {
                            let Some(props) = global.props else { return };
                            if let Some(l) = parse::link(global.id, props) {
                                s.model.links.insert(global.id, l);
                            }
                        }
                        _ => {}
                    }
                })
                .global_remove(move |id| {
                    let mut s = state_rm.borrow_mut();
                    if let Some(n) = s.model.nodes.get(&id) {
                        let name = n.name.clone();
                        s.nodes.remove(&name);
                    }
                    s.model.remove_global(id);
                    let State { links, model, .. } = &mut *s;
                    links.prune_dead(model);
                })
                .register()
        };

        let rig = Self {
            mainloop,
            context,
            core,
            state,
            modules: Vec::new(),
            taps: Vec::new(),
            tones: Vec::new(),
            _registry: registry,
            _reg_listener: reg_listener,
        };
        rig.settle(300); // initial enumeration
        rig
    }

    /// Run the loop for `ms`, letting globals arrive, links reconcile, and
    /// audio flow.
    fn settle(&self, ms: u64) {
        let ml = self.mainloop.clone();
        let timer = self.mainloop.loop_().add_timer(move |_| ml.quit());
        timer
            .update_timer(Some(Duration::from_millis(ms)), None)
            .into_result()
            .expect("arm timer");
        self.mainloop.run();
    }

    fn want_control(&mut self, node_name: &str) {
        self.state.borrow_mut().wanted.insert(node_name.to_string());
    }

    fn load(&mut self, args: &str) {
        let module = LoadedModule::load(&self.context, "libpipewire-module-filter-chain", args)
            .expect("filter-chain module must load (is lsp-plugins-lv2 installed?)");
        self.modules.push(module);
    }

    fn route(&mut self, from: &str, to: &str, on: bool) {
        let mut s = self.state.borrow_mut();
        s.links.set_route(from, to, on);
        let State { links, model, .. } = &mut *s;
        links.reconcile(&self.core, model);
    }

    /// Tap `target`'s output. Returns the accumulator the meter drains.
    fn tap(&mut self, key: &str, target: &str) -> Arc<MeterAccum> {
        let tap = MeterTap::new(self.core.clone(), key).expect("meter tap");
        let accum = tap.accum.clone();
        let node_name = tap.node_name.clone();
        self.taps.push(tap);
        self.route(target, &node_name, true);
        accum
    }

    fn tone(&mut self, name: &str, freq: f32, amplitude: f32) {
        self.tones.push(Tone::new(self.core.clone(), name, freq, amplitude));
    }

    fn set_props(&self, node_name: &str, bytes: &[u8]) {
        let s = self.state.borrow();
        let node = s
            .nodes
            .get(node_name)
            .unwrap_or_else(|| panic!("no proxy bound for {node_name}; call want_control first"));
        let pod = libspa::pod::Pod::from_bytes(bytes).expect("valid pod");
        node.set_param(libspa::param::ParamType::Props, 0, pod);
    }

    fn node_exists(&self, name: &str) -> bool {
        self.state.borrow().model.node_by_name(name).is_some()
    }

    fn node_media_class(&self, name: &str) -> Option<String> {
        self.state.borrow().model.node_by_name(name).map(|n| n.media_class.clone())
    }

    fn node_names_starting_with(&self, prefix: &str) -> Vec<String> {
        let mut v: Vec<String> = self
            .state
            .borrow()
            .model
            .nodes
            .values()
            .filter(|n| n.name.starts_with(prefix))
            .map(|n| n.name.clone())
            .collect();
        v.sort();
        v
    }

    /// How many links currently join these two nodes.
    fn link_count(&self, out_node: &str, in_node: &str) -> usize {
        let s = self.state.borrow();
        let (Some(o), Some(i)) =
            (s.model.node_by_name(out_node), s.model.node_by_name(in_node))
        else {
            return 0;
        };
        let outs: Vec<u32> = s.model.node_ports(o.id, PortDirection::Out).iter().map(|p| p.id).collect();
        let ins: Vec<u32> = s.model.node_ports(i.id, PortDirection::In).iter().map(|p| p.id).collect();
        s.model
            .links
            .values()
            .filter(|l| outs.contains(&l.output_port) && ins.contains(&l.input_port))
            .count()
    }

    /// Let audio flow, then read the meter. Drains once first so the reading
    /// covers only the measured window.
    fn measure(&self, accum: &MeterAccum, ms: u64) -> [f32; 4] {
        accum.drain();
        self.settle(ms);
        accum.drain()
    }
}

/// A sine generator: the signal under test.
struct Tone {
    _stream: StreamRc,
    _listener: StreamListener<f32>,
}

impl Tone {
    fn new(core: CoreRc, name: &str, freq: f32, amplitude: f32) -> Self {
        let props = properties! {
            *pipewire::keys::MEDIA_TYPE => "Audio",
            *pipewire::keys::MEDIA_CATEGORY => "Playback",
            *pipewire::keys::NODE_NAME => name,
            *pipewire::keys::NODE_AUTOCONNECT => "false",
            "node.virtual" => "true",
        };
        let stream = StreamRc::new(core, name, props).expect("tone stream");

        let step = 2.0 * std::f32::consts::PI * freq / RATE as f32;
        // user data = running phase, so the waveform is continuous across buffers
        let listener = stream
            .add_local_listener_with_user_data::<f32>(0.0)
            .process(move |stream, phase| {
                let Some(mut buffer) = stream.dequeue_buffer() else { return };
                let datas = buffer.datas_mut();
                let Some(data) = datas.first_mut() else { return };
                const STRIDE: usize = 8; // 2 channels * f32
                let Some(slice) = data.data() else { return };
                let frames = slice.len() / STRIDE;
                for f in 0..frames {
                    let v = amplitude * phase.sin();
                    *phase += step;
                    if *phase > std::f32::consts::TAU {
                        *phase -= std::f32::consts::TAU;
                    }
                    let bytes = v.to_ne_bytes();
                    slice[f * STRIDE..f * STRIDE + 4].copy_from_slice(&bytes);
                    slice[f * STRIDE + 4..f * STRIDE + 8].copy_from_slice(&bytes);
                }
                let chunk = data.chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.stride_mut() = STRIDE as i32;
                *chunk.size_mut() = (frames * STRIDE) as u32;
            })
            .register()
            .expect("tone listener");

        let mut info = libspa::param::audio::AudioInfoRaw::new();
        info.set_format(libspa::param::audio::AudioFormat::F32LE);
        info.set_rate(RATE);
        info.set_channels(2);
        let mut position = [0u32; libspa::sys::SPA_AUDIO_MAX_CHANNELS as usize];
        position[0] = libspa::sys::SPA_AUDIO_CHANNEL_FL;
        position[1] = libspa::sys::SPA_AUDIO_CHANNEL_FR;
        info.set_position(position);

        let obj = libspa::pod::Object {
            type_: libspa::utils::SpaTypes::ObjectParamFormat.as_raw(),
            id: libspa::param::ParamType::EnumFormat.as_raw(),
            properties: info.into(),
        };
        let bytes = libspa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &libspa::pod::Value::Object(obj),
        )
        .expect("serialize format")
        .0
        .into_inner();
        let mut params = [libspa::pod::Pod::from_bytes(&bytes).expect("format pod")];

        stream
            .connect(libspa::utils::Direction::Output, None, StreamFlags::MAP_BUFFERS, &mut params)
            .expect("connect tone");

        Self { _stream: stream, _listener: listener }
    }
}

// ---- helpers ---------------------------------------------------------------

fn strip(id: u32) -> StripState {
    let mut s = StripState {
        id,
        kind: StripKind::Virtual,
        label: format!("Test Strip {id}"),
        hw_key: None,
        online: true,
        gain_db: 0.0,
        mute: false,
        solo: false,
        routes: Default::default(),
        gate: Default::default(),
        comp: Default::default(),
        eq: Default::default(),
    };
    s.gain_db = 0.0;
    s
}

fn bus_a1() -> BusState {
    BusState {
        id: lm_protocol::BusId::A1,
        label: "Test A1".into(),
        target_hw_key: Some(SINK.into()),
        online: true,
        gain_db: 0.0,
        mute: false,
        limiter: LimiterParams::default(),
    }
}

/// Put the DSP into a known, transparent state — the plugins' own defaults are
/// not necessarily unity gain.
fn make_transparent(rig: &Rig, control_node: &str) {
    rig.set_props(control_node, &params::filter_params(&[("gate:enabled", 0.0), ("comp:enabled", 0.0), ("eq:enabled", 0.0)]));
    rig.set_props(control_node, &params::channel_volumes(1.0));
    rig.set_props(control_node, &params::mute(false));
}

/// Build the production signal path: tone -> strip (gate/comp/eq) -> A1
/// (limiter) -> null sink, with meter taps on the strip and bus outputs.
fn full_chain(amplitude: f32) -> (Rig, Arc<MeterAccum>, Arc<MeterAccum>, StripState, BusState) {
    let mut rig = Rig::new();
    let s = strip(1);
    let b = bus_a1();

    rig.want_control(&s.control_node());
    rig.want_control(&b.in_node());
    rig.load(&virtual_strip_args(&s.node_base(), &s.label, false));
    rig.load(&bus_args(&b.node_base(), &b.label, b.target_hw_key.as_deref()));
    rig.settle(400);

    rig.tone("lm.test.tone", 1000.0, amplitude);
    rig.settle(200);

    rig.route("lm.test.tone", &s.node_base(), true);
    rig.route(&s.out_node(), &b.in_node(), true);
    rig.route(&b.tap_node(), SINK, true);

    let strip_meter = rig.tap("test-strip", &s.out_node());
    let bus_meter = rig.tap("test-bus", &b.tap_node());
    rig.settle(500);

    make_transparent(&rig, &s.control_node());
    rig.set_props(&b.in_node(), &params::filter_params(&[("lim:enabled", 0.0)]));
    rig.set_props(&b.in_node(), &params::channel_volumes(1.0));
    rig.settle(300);

    (rig, strip_meter, bus_meter, s, b)
}

// ---- tests -----------------------------------------------------------------

/// The cleanliness invariant: every node we create must vanish with the module
/// that made it. A leak here means stale devices in the user's session.
#[test]
#[ignore = "requires a PipeWire daemon; run with `make test-audio`"]
fn strip_module_appears_and_vanishes() {
    let mut rig = Rig::new();
    let s = strip(1);

    assert!(!rig.node_exists(&s.node_base()), "graph must start clean");

    rig.load(&virtual_strip_args(&s.node_base(), &s.label, false));
    rig.settle(400);

    assert!(rig.node_exists(&s.node_base()), "strip sink did not appear");
    assert!(rig.node_exists(&s.out_node()), "strip output did not appear");
    assert_eq!(
        rig.node_media_class(&s.node_base()).as_deref(),
        Some("Audio/Sink"),
        "apps must see the strip as a playback device"
    );

    rig.modules.clear(); // drop unloads the module
    rig.settle(400);

    assert_eq!(
        rig.node_names_starting_with("lm."),
        Vec::<String>::new(),
        "unloading must leave zero lm.* nodes behind"
    );
}

/// B buses are the virtual microphones other apps select.
#[test]
#[ignore = "requires a PipeWire daemon; run with `make test-audio`"]
fn b_bus_appears_as_a_microphone() {
    let mut rig = Rig::new();
    let b = BusState {
        id: lm_protocol::BusId::B1,
        label: "Test B1".into(),
        target_hw_key: None,
        online: true,
        gain_db: 0.0,
        mute: false,
        limiter: LimiterParams::default(),
    };

    rig.load(&bus_args(&b.node_base(), &b.label, None));
    rig.settle(400);

    assert_eq!(
        rig.node_media_class(&b.node_base()).as_deref(),
        Some("Audio/Source"),
        "a B bus must be selectable as a microphone"
    );
    assert!(rig.node_exists(&b.in_node()), "bus mix input did not appear");
}

/// Toggling a route must create and destroy real links, not just bookkeeping.
#[test]
#[ignore = "requires a PipeWire daemon; run with `make test-audio`"]
fn routing_matrix_creates_and_destroys_links() {
    let mut rig = Rig::new();
    let s = strip(1);
    let b = bus_a1();

    rig.load(&virtual_strip_args(&s.node_base(), &s.label, false));
    rig.load(&bus_args(&b.node_base(), &b.label, b.target_hw_key.as_deref()));
    rig.settle(500);

    assert_eq!(rig.link_count(&s.out_node(), &b.in_node()), 0, "no links before routing");

    rig.route(&s.out_node(), &b.in_node(), true);
    rig.settle(400);
    assert_eq!(
        rig.link_count(&s.out_node(), &b.in_node()),
        2,
        "a stereo route is exactly two links: FL->FL and FR->FR"
    );

    rig.route(&s.out_node(), &b.in_node(), false);
    rig.settle(400);
    assert_eq!(rig.link_count(&s.out_node(), &b.in_node()), 0, "route off must tear the links down");
}

/// The end-to-end proof: a known sine played into a strip comes out of the bus
/// at the level it went in, having passed through both LSP filter chains.
#[test]
#[ignore = "requires a PipeWire daemon; run with `make test-audio`"]
fn tone_travels_the_full_chain_at_the_expected_level() {
    // -6 dBFS peak, -9 dBFS RMS for a sine.
    let (rig, strip_meter, bus_meter, _s, _b) = full_chain(0.5);

    let [strip_peak, _, strip_rms, _] = rig.measure(&strip_meter, 500);
    assert!(
        (strip_peak - -6.02).abs() < 1.5,
        "strip peak {strip_peak} dBFS, expected about -6; is audio reaching the strip at all?"
    );
    assert!((strip_rms - -9.03).abs() < 1.5, "strip rms {strip_rms} dBFS, expected about -9");

    let [bus_peak, _, bus_rms, _] = rig.measure(&bus_meter, 500);
    assert!(
        (bus_peak - -6.02).abs() < 1.5,
        "bus peak {bus_peak} dBFS, expected about -6; the routing matrix or limiter changed the level"
    );
    assert!((bus_rms - -9.03).abs() < 1.5, "bus rms {bus_rms} dBFS, expected about -9");
}

/// Volume is applied as a Props pod carrying *linear* gain. If `db_to_linear`
/// were wrong — or the pod went to the wrong node — the level would not move
/// by the amount asked for.
#[test]
#[ignore = "requires a PipeWire daemon; run with `make test-audio`"]
fn strip_gain_moves_the_metered_level_by_the_requested_db() {
    let (rig, _strip_meter, bus_meter, s, _b) = full_chain(0.5);

    let before = rig.measure(&bus_meter, 500)[0];
    assert!(before > -20.0, "no signal to attenuate (peak {before} dBFS)");

    rig.set_props(&s.control_node(), &params::channel_volumes(params::db_to_linear(-12.0)));
    rig.settle(300);

    let after = rig.measure(&bus_meter, 500)[0];
    let delta = before - after;
    assert!(
        (delta - 12.0).abs() < 1.5,
        "asked for -12 dB, meter moved {delta:.2} dB ({before:.2} -> {after:.2})"
    );
}

/// Mute must actually silence the audio, not just grey out a button.
#[test]
#[ignore = "requires a PipeWire daemon; run with `make test-audio`"]
fn mute_silences_the_strip() {
    let (rig, _strip_meter, bus_meter, s, _b) = full_chain(0.5);

    let before = rig.measure(&bus_meter, 400)[0];
    assert!(before > -20.0, "no signal to mute (peak {before} dBFS)");

    rig.set_props(&s.control_node(), &params::mute(true));
    rig.settle(300);
    let muted = rig.measure(&bus_meter, 400)[0];
    assert!(muted < -60.0, "muted strip still metering {muted} dBFS");

    rig.set_props(&s.control_node(), &params::mute(false));
    rig.settle(300);
    let unmuted = rig.measure(&bus_meter, 400)[0];
    assert!(unmuted > -20.0, "unmute did not restore audio (peak {unmuted} dBFS)");
}

/// The gate threshold is written as linear gain. A gate set well above the
/// signal must close; set well below, it must pass. This is the DSP-control
/// path end to end: dB -> linear -> Props -> LSP control port -> audible.
#[test]
#[ignore = "requires a PipeWire daemon; run with `make test-audio`"]
fn gate_threshold_opens_and_closes_the_signal() {
    let (rig, _strip_meter, bus_meter, s, _b) = full_chain(0.5); // -6 dBFS

    // Threshold far above the signal: the gate should shut it out.
    rig.set_props(
        &s.control_node(),
        &params::filter_params(&[
            ("gate:enabled", 1.0),
            ("gate:gt", params::db_to_linear(-1.0)),
            ("gate:at", 1.0),
            ("gate:rt", 10.0),
            ("gate:hold", 0.0),
        ]),
    );
    rig.settle(400);
    let closed = rig.measure(&bus_meter, 500)[0];

    // Threshold far below the signal: it should pass again.
    rig.set_props(
        &s.control_node(),
        &params::filter_params(&[("gate:gt", params::db_to_linear(-60.0))]),
    );
    rig.settle(400);
    let open = rig.measure(&bus_meter, 500)[0];

    assert!(
        open - closed > 12.0,
        "gate made little difference: closed {closed:.1} dBFS vs open {open:.1} dBFS"
    );
    assert!(open > -20.0, "gate never reopened (peak {open} dBFS)");
}
