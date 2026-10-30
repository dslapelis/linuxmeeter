//! Production engine/socket under the private test daemon; never user audio.
use lm_engine::control::{socket_path, ControlBridge, ControlServer};
use lm_protocol::{EngineCommand, EngineEvent};
use std::io::Read;

fn main() {
    assert_eq!(
        std::env::var("LM_TEST_DAEMON").ok().as_deref(),
        Some("1"),
        "run through scripts/with-test-daemon.sh"
    );
    let engine = lm_engine::engine::spawn();
    let _ = engine
        .cmd_tx
        .send(EngineCommand::SetMetersEnabled { on: false });
    let bridge = ControlBridge::new(engine.cmd_tx.clone());
    let receiver = bridge.clone();
    let server = ControlServer::start(socket_path().unwrap(), move |request| {
        receiver.request(request)
    })
    .unwrap();
    let pump = bridge.clone();
    std::thread::spawn(move || {
        while let Ok(event) = engine.evt_rx.recv() {
            match event {
                EngineEvent::ControlReply(response) => pump.complete(response),
                EngineEvent::State(state) => println!("{}", serde_json::json!({"state": state})),
                EngineEvent::Meters(_) => (),
            }
        }
    });
    // Closing stdin performs a clean shutdown, just like the Tauri shell.
    let _ = std::io::stdin().read(&mut [0u8]);
    bridge.stop();
    server.stop();
    let _ = engine.cmd_tx.send(EngineCommand::Shutdown);
    std::thread::sleep(std::time::Duration::from_millis(200));
}
