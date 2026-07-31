//! M3 spike: full VoiceMeeter topology headless.
//!
//!   apps -> [lm.strip.v1: gate->comp->eq] -> routing matrix -> [lm.bus.a1: limiter] -> hw sink
//!                                                          \-> [lm.bus.b1: limiter] -> virtual mic
//!
//! Run `cargo run --example graph`, then:
//!   route a1 0|1     toggle strip -> speakers
//!   route b1 0|1     toggle strip -> virtual mic
//!   links            print live lm.* links
//!   q                quit
//!
//! Verify: play into "LM Strip 1" (audible on default sink), record the mix
//! with `pw-record --target lm.bus.b1 out.wav`, toggle routes live.

use std::cell::RefCell;
use std::io::BufRead;
use std::rc::Rc;

use pipewire::context::ContextRc;
use pipewire::main_loop::MainLoopRc;
use pipewire::types::ObjectType;

use lm_engine::filterchain::{bus_args, hardware_strip_args, virtual_strip_args, LoadedModule};
use lm_engine::links::LinkManager;
use lm_engine::meter::MeterTap;
use lm_engine::registry::{parse, GraphModel};

const STRIP_OUT: &str = "lm.strip.v1.out";
const A1_IN: &str = "lm.bus.a1.in";
const B1_IN: &str = "lm.bus.b1.in";

#[derive(Debug)]
enum Cmd {
    Route(&'static str, bool),
    Links,
    Meters(bool),
    Quit,
}

#[derive(Default)]
struct App {
    model: GraphModel,
    links: LinkManager,
    modules: Vec<LoadedModule>,
    taps: Vec<MeterTap>,
    loaded: bool,
    meters_on: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "debug".into()))
        .init();
    pipewire::init();

    let mainloop = MainLoopRc::new(None)?;
    let context = ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let app = Rc::new(RefCell::new(App::default()));

    // --- registry -> model, reconcile on every relevant change ---
    let _reg_listener = {
        let app = app.clone();
        let app_rm = app.clone();
        let core = core.clone();
        registry
            .add_listener_local()
            .global(move |global| {
                let mut a = app.borrow_mut();
                match global.type_ {
                    ObjectType::Node => {
                        if let Some(props) = global.props {
                            if let Some(n) = parse::node(global.id, props) {
                                a.model.nodes.insert(global.id, n);
                            }
                        }
                    }
                    ObjectType::Port => {
                        if let Some(props) = global.props {
                            if let Some(p) = parse::port(global.id, props) {
                                a.model.ports.insert(global.id, p);
                                if a.loaded {
                                    let App { model, links, .. } = &mut *a;
                                    links.reconcile(&core, model);
                                }
                            }
                        }
                    }
                    ObjectType::Link => {
                        if let Some(props) = global.props {
                            if let Some(l) = parse::link(global.id, props) {
                                a.model.links.insert(global.id, l);
                            }
                        }
                    }
                    _ => {}
                }
            })
            .global_remove({
                let app = app_rm;
                move |id| {
                    let mut a = app.borrow_mut();
                    a.model.remove_global(id);
                    let App { model, links, .. } = &mut *a;
                    links.prune_dead(model);
                }
            })
            .register()
    };

    // --- after initial enumeration: pick hw sink, build the graph ---
    let _core_listener = {
        let app = app.clone();
        let context = context.clone();
        let core2 = core.clone();
        core.add_listener_local()
            .done(move |_id, _seq| {
                let mut a = app.borrow_mut();
                if a.loaded {
                    return;
                }
                let Some(hw_sink) = a
                    .model
                    .nodes
                    .values()
                    .find(|n| n.media_class == "Audio/Sink" && n.name.starts_with("alsa_output"))
                    .map(|n| n.name.clone())
                else {
                    tracing::warn!("no hardware sink found yet");
                    return;
                };
                tracing::info!("A1 target: {hw_sink}");

                // Hardware strip from the first real capture device, if any.
                let hw_source = a
                    .model
                    .nodes
                    .values()
                    .find(|n| n.media_class == "Audio/Source" && n.name.starts_with("alsa_input"))
                    .map(|n| n.name.clone());

                let load = |args: &str| LoadedModule::load(&context, "libpipewire-module-filter-chain", args);
                let strip = load(&virtual_strip_args("lm.strip.v1", "LM Strip 1", false)).expect("strip");
                let a1 = load(&bus_args("lm.bus.a1", "LM A1 Speakers", Some(&hw_sink))).expect("a1");
                let b1 = load(&bus_args("lm.bus.b1", "LM B1 Stream Mic", None)).expect("b1");
                a.modules.extend([strip, a1, b1]);

                if let Some(src) = hw_source {
                    tracing::info!("hardware strip capturing from {src}");
                    let hw = load(&hardware_strip_args("lm.strip.hw1", "LM Mic", &src)).expect("hw strip");
                    a.modules.push(hw);
                    a.links.set_route("lm.strip.hw1.out", B1_IN, true);
                }

                a.links.set_route(STRIP_OUT, A1_IN, true);
                a.links.set_route(STRIP_OUT, B1_IN, true);

                // Meter taps: strip output + both bus outputs, linked through
                // the same routing machinery as everything else.
                for (key, target) in [("s1", STRIP_OUT), ("a1", "lm.bus.a1.out"), ("b1", "lm.bus.b1")] {
                    match MeterTap::new(core2.clone(), key) {
                        Ok(tap) => {
                            a.links.set_route(target, &tap.node_name.clone(), true);
                            a.taps.push(tap);
                        }
                        Err(e) => tracing::warn!("meter tap {key} failed: {e}"),
                    }
                }
                a.loaded = true;
                tracing::info!("graph loaded — strip routed to A1 + B1");
                let App { model, links, .. } = &mut *a;
                links.reconcile(&core2, model);
            })
            .register()
    };
    core.sync(0)?;

    // --- stdin commands ---
    let (sender, receiver) = pipewire::channel::channel::<Cmd>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            let t: Vec<&str> = line.split_whitespace().collect();
            let cmd = match t.as_slice() {
                ["q"] | ["quit"] => Some(Cmd::Quit),
                ["links"] => Some(Cmd::Links),
                ["meters", v] => Some(Cmd::Meters(*v == "1")),
                ["route", "a1", v] => Some(Cmd::Route(A1_IN, *v == "1")),
                ["route", "b1", v] => Some(Cmd::Route(B1_IN, *v == "1")),
                [] => None,
                _ => {
                    eprintln!("commands: route <a1|b1> <0|1> | links | q");
                    None
                }
            };
            if let Some(cmd) = cmd {
                let quit = matches!(cmd, Cmd::Quit);
                if sender.send(cmd).is_err() || quit {
                    break;
                }
            }
        }
    });

    let _attached = {
        let app = app.clone();
        let core = core.clone();
        let ml = mainloop.clone();
        receiver.attach(mainloop.loop_(), move |cmd| {
            let mut a = app.borrow_mut();
            match cmd {
                Cmd::Quit => ml.quit(),
                Cmd::Route(input, on) => {
                    tracing::info!("route {} -> {}", input, on);
                    a.links.set_route(STRIP_OUT, input, on);
                    let App { model, links, .. } = &mut *a;
                    links.reconcile(&core, model);
                }
                Cmd::Links => {
                    let a = &*a;
                    for l in a.model.links.values() {
                        let name = |id: u32| a.model.nodes.get(&id).map(|n| n.name.as_str()).unwrap_or("?");
                        let (on, inn) = (name(l.output_node), name(l.input_node));
                        if on.starts_with("lm.") || inn.starts_with("lm.") {
                            println!("  {} -> {}", on, inn);
                        }
                    }
                }
                Cmd::Meters(on) => a.meters_on = on,
            }
        })
    };

    // 30 Hz meter drain; prints one line per tick while `meters 1` is active.
    let timer = {
        let app = app.clone();
        mainloop.loop_().add_timer(move |_| {
            let a = app.borrow();
            if !a.meters_on || a.taps.is_empty() {
                return;
            }
            let line: Vec<String> = a
                .taps
                .iter()
                .map(|t| {
                    let [pl, pr, rl, rr] = t.accum.drain();
                    format!("{}: peak {pl:6.1}/{pr:6.1} rms {rl:6.1}/{rr:6.1}", t.node_name)
                })
                .collect();
            println!("[meters] {}", line.join(" | "));
        })
    };
    timer
        .update_timer(Some(std::time::Duration::from_millis(33)), Some(std::time::Duration::from_millis(33)))
        .into_result()?;

    tracing::info!("running — play into \"LM Strip 1\", record lm.bus.b1, `meters 1` to stream meters");
    mainloop.run();
    Ok(())
}
