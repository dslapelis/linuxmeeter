#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, RwLock};
use std::time::Duration;

use lm_protocol::{
    AppState, BusId, CompParams, EngineCommand, EngineEvent, GateParams, LimiterParams, StripId, Target,
};
use tauri::{Emitter, Manager};

struct AppData {
    cmd_tx: pipewire::channel::Sender<EngineCommand>,
    state: Arc<RwLock<Option<AppState>>>,
}

fn send(data: &tauri::State<AppData>, cmd: EngineCommand) -> Result<(), String> {
    data.cmd_tx.send(cmd).map_err(|_| "engine unavailable".to_string())
}

#[tauri::command]
fn get_state(data: tauri::State<AppData>) -> Result<AppState, String> {
    // The engine builds its graph asynchronously at startup; wait briefly.
    for _ in 0..50 {
        if let Some(s) = data.state.read().expect("state lock").clone() {
            return Ok(s);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("engine not ready".into())
}

#[tauri::command]
fn set_gain(data: tauri::State<AppData>, target: Target, db: f64) -> Result<(), String> {
    send(&data, EngineCommand::SetGain { target, db: db as f32 })
}

#[tauri::command]
fn set_mute(data: tauri::State<AppData>, target: Target, mute: bool) -> Result<(), String> {
    send(&data, EngineCommand::SetMute { target, mute })
}

#[tauri::command]
fn set_solo(data: tauri::State<AppData>, strip_id: StripId, solo: bool) -> Result<(), String> {
    send(&data, EngineCommand::SetSolo { strip: strip_id, solo })
}

#[tauri::command]
fn set_route(data: tauri::State<AppData>, strip_id: StripId, bus: BusId, on: bool) -> Result<(), String> {
    send(&data, EngineCommand::SetRoute { strip: strip_id, bus, on })
}

#[tauri::command]
fn set_gate_params(data: tauri::State<AppData>, strip_id: StripId, params: GateParams) -> Result<(), String> {
    send(&data, EngineCommand::SetGateParams { strip: strip_id, params })
}

#[tauri::command]
fn set_comp_params(data: tauri::State<AppData>, strip_id: StripId, params: CompParams) -> Result<(), String> {
    send(&data, EngineCommand::SetCompParams { strip: strip_id, params })
}

#[tauri::command]
fn set_limiter_params(data: tauri::State<AppData>, bus: BusId, params: LimiterParams) -> Result<(), String> {
    send(&data, EngineCommand::SetLimiterParams { bus, params })
}

#[tauri::command]
fn set_strip_device(data: tauri::State<AppData>, strip_id: StripId, hw_key: String) -> Result<(), String> {
    send(&data, EngineCommand::SetStripDevice { strip: strip_id, hw_key })
}

#[tauri::command]
fn set_bus_target(data: tauri::State<AppData>, bus: BusId, hw_key: String) -> Result<(), String> {
    send(&data, EngineCommand::SetBusTarget { bus, hw_key })
}

#[tauri::command]
fn set_default_output(data: tauri::State<AppData>, on: bool) -> Result<(), String> {
    send(&data, EngineCommand::SetDefaultOutput { on })
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let engine = lm_engine::engine::spawn();
    let cmd_tx = engine.cmd_tx.clone();
    let state: Arc<RwLock<Option<AppState>>> = Arc::new(RwLock::new(None));

    tauri::Builder::default()
        .manage(AppData { cmd_tx: engine.cmd_tx.clone(), state: state.clone() })
        .invoke_handler(tauri::generate_handler![
            get_state,
            set_gain,
            set_mute,
            set_solo,
            set_route,
            set_gate_params,
            set_comp_params,
            set_limiter_params,
            set_strip_device,
            set_bus_target,
            set_default_output,
        ])
        .setup(move |app| {
            // Pump engine events into the webview.
            let handle = app.handle().clone();
            let evt_rx = engine.evt_rx;
            std::thread::Builder::new()
                .name("lm-event-pump".into())
                .spawn(move || {
                    while let Ok(evt) = evt_rx.recv() {
                        match evt {
                            EngineEvent::State(s) => {
                                *state.write().expect("state lock") = Some(s.clone());
                                let _ = handle.emit("state_changed", &s);
                            }
                            EngineEvent::Meters(f) => {
                                let _ = handle.emit("meters", &f);
                            }
                        }
                    }
                })
                .expect("spawn event pump");
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building linuxmeeter")
        .run(move |_app, event| {
            if let tauri::RunEvent::Exit = event {
                // Graceful engine shutdown -> modules unload -> zero lm.* nodes left.
                let _ = cmd_tx.send(EngineCommand::Shutdown);
                std::thread::sleep(Duration::from_millis(200));
            }
        });
}
