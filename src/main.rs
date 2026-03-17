mod config;
mod device;
mod gesture;

use gesture::{GestureDetector, GestureResult};
use std::process::Command;
use std::thread;

fn main() {
    env_logger::init();

    let config = config::Config::load();
    log::debug!("Config: {config:#?}");

    let touchpad = device::find_touchpad(config.touchpad.device.as_deref());

    let mut detector = GestureDetector::new(
        config.gesture,
        config.bindings,
        touchpad.x_min,
        touchpad.x_max,
    );

    log::info!("Listening for gestures...");

    let mut device = touchpad.device;
    loop {
        let events = match device.fetch_events() {
            Ok(events) => events,
            Err(e) => {
                log::error!("Error reading events: {e}");
                continue;
            }
        };

        for event in events {
            if let GestureResult::Fire(cmd) = detector.process_event(&event) {
                match Command::new("sh").args(["-c", &cmd]).spawn() {
                    Ok(mut child) => {
                        log::debug!("Spawned: {cmd}");
                        thread::spawn(move || { let _ = child.wait(); });
                    }
                    Err(e) => log::error!("Failed to spawn command: {e}"),
                }
            }
        }
    }
}
