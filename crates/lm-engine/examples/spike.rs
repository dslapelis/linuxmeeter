//! M1 spike: prove the three highest-risk mechanisms end to end.
//!
//! 1. `pw_context_load_module` from Rust — a filter-chain sink (`lm.strip.v1`)
//!    appears while running and vanishes on exit.
//! 2. LSP LV2 plugins (gate → comp → EQ) instantiate inside filter-chain.
//! 3. Runtime param control via Props pods built with libspa.
//!
//! Run with `cargo run --example spike`, play music into the "LM Strip 1"
//! sink (pick it in pavucontrol or `pactl play-sample`), then type commands:
//!
//!   setdb gate:gt -30     gate threshold to -30 dB (heavy gating: -10)
//!   set   comp:cr 8       compression ratio 8:1
//!   set   gate:enabled 0  bypass the gate
//!   vol   0.5             sink volume 50%
//!   mute  1 / mute 0
//!   q                     quit (sink must disappear from pactl)

use std::cell::RefCell;
use std::io::BufRead;
use std::rc::Rc;

use libspa::param::ParamType;
use libspa::pod::Pod;
use pipewire::context::ContextRc;
use pipewire::main_loop::MainLoopRc;
use pipewire::node::Node;
use pipewire::types::ObjectType;

use lm_engine::filterchain::{virtual_strip_args, LoadedModule};
use lm_engine::params;

const SINK_NAME: &str = "lm.strip.v1";

#[derive(Debug)]
enum Cmd {
    Set(String, f32),
    SetDb(String, f32),
    Vol(f32),
    Mute(bool),
    Quit,
}

#[derive(Default)]
struct Bound {
    /// The Audio/Sink side (capture stream) of our filter-chain.
    sink: Option<(u32, Node)>,
    /// The processed-output stream side.
    out: Option<(u32, Node)>,
}

fn set_props(label: &str, id: u32, node: &Node, bytes: &[u8]) {
    match Pod::from_bytes(bytes) {
        Some(pod) => {
            node.set_param(ParamType::Props, 0, pod);
            println!("  -> set Props on {label} node (id {id})");
        }
        None => eprintln!("  !! built an invalid pod ({} bytes)", bytes.len()),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pipewire::init();
    let mainloop = MainLoopRc::new(None)?;
    let context = ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    // Output autoconnects to the default sink so the DSP is audible.
    let args = virtual_strip_args(SINK_NAME, "LM Strip 1", true);
    println!("loading filter-chain module...");
    let _module = LoadedModule::load(&context, "libpipewire-module-filter-chain", &args)?;
    println!("module loaded — waiting for nodes to appear\n");

    let bound = Rc::new(RefCell::new(Bound::default()));

    let _reg_listener = {
        let bound = bound.clone();
        let registry_weak = registry.downgrade();
        registry
            .add_listener_local()
            .global(move |global| {
                if global.type_ != ObjectType::Node {
                    return;
                }
                let Some(props) = global.props else { return };
                let name = props.get("node.name").unwrap_or("?");
                let class = props.get("media.class").unwrap_or("-");
                println!("[registry] node {:>3}  {:<40} {}", global.id, name, class);

                let slot = match name {
                    n if n == SINK_NAME => 0,
                    n if n == format!("{SINK_NAME}.out") => 1,
                    _ => return,
                };
                let Some(registry) = registry_weak.upgrade() else { return };
                match registry.bind::<Node, _>(global) {
                    Ok(node) => {
                        let mut b = bound.borrow_mut();
                        if slot == 0 {
                            println!("\n*** bound sink node id {} — inspect with: pw-cli enum-params {} Props\n", global.id, global.id);
                            b.sink = Some((global.id, node));
                        } else {
                            println!("\n*** bound output node id {}\n", global.id);
                            b.out = Some((global.id, node));
                        }
                    }
                    Err(e) => eprintln!("bind failed for {name}: {e}"),
                }
            })
            .global_remove(|id| println!("[registry] removed {id}"))
            .register()
    };

    // stdin → engine loop, via the loop-attached channel (no polling).
    let (sender, receiver) = pipewire::channel::channel::<Cmd>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            match parse(&line) {
                Some(cmd) => {
                    let quit = matches!(cmd, Cmd::Quit);
                    if sender.send(cmd).is_err() || quit {
                        break;
                    }
                }
                None => {
                    if !line.trim().is_empty() {
                        eprintln!("commands: set <plugin:port> <val> | setdb <plugin:port> <dB> | vol <0..1.5> | mute <0|1> | q");
                    }
                }
            }
        }
    });

    let _attached = {
        let bound = bound.clone();
        let ml = mainloop.clone();
        receiver.attach(mainloop.loop_(), move |cmd| {
            let b = bound.borrow();
            match cmd {
                Cmd::Quit => {
                    println!("shutting down — verify the sink is gone: pactl list sinks short");
                    ml.quit();
                }
                Cmd::Set(key, v) | Cmd::SetDb(key, v) => {
                    println!("setting {key} = {v}");
                    let bytes = params::filter_params(&[(key.as_str(), v)]);
                    // The plugin controls live on one of the two stream nodes;
                    // set on both to find out empirically (extra set is a no-op).
                    if let Some((id, node)) = &b.sink {
                        set_props("sink", *id, node, &bytes);
                    }
                    if let Some((id, node)) = &b.out {
                        set_props("out", *id, node, &bytes);
                    }
                }
                Cmd::Vol(v) => {
                    if let Some((id, node)) = &b.sink {
                        set_props("sink", *id, node, &params::channel_volumes(v));
                    }
                }
                Cmd::Mute(m) => {
                    if let Some((id, node)) = &b.sink {
                        set_props("sink", *id, node, &params::mute(m));
                    }
                }
            }
        })
    };

    println!("engine loop running — play audio into the \"LM Strip 1\" sink");
    mainloop.run();
    println!("bye");
    Ok(())
}

fn parse(line: &str) -> Option<Cmd> {
    let mut t = line.split_whitespace();
    let cmd = t.next()?;
    match cmd {
        "q" | "quit" => Some(Cmd::Quit),
        "set" => Some(Cmd::Set(t.next()?.to_string(), t.next()?.parse().ok()?)),
        "setdb" => {
            let key = t.next()?.to_string();
            let db: f32 = t.next()?.parse().ok()?;
            let lin = params::db_to_linear(db);
            println!("({db} dB -> linear {lin:.5})");
            Some(Cmd::SetDb(key, lin))
        }
        "vol" => Some(Cmd::Vol(t.next()?.parse().ok()?)),
        "mute" => Some(Cmd::Mute(matches!(t.next()?, "1" | "true" | "on"))),
        _ => None,
    }
}
