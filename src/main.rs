mod config;
mod device;
mod gesture;
mod ipc;
mod virtual_device;

use evdev::uinput::VirtualDevice;
use evdev::EventType;
use gesture::{GestureDetector, GestureResult};
use std::path::PathBuf;
use std::process::Command;
use std::thread;

fn main() {
    env_logger::init();

    let config_path = parse_config_arg();
    let config = config::Config::load(config_path);
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
    let mut ipc = ipc::IpcServer::new();
    let mut vdev: Option<VirtualDevice> = None;
    let mut grabbed = false;
    let mut forwarding_multi = false;
    // When true, we've sent the close but keep the grab until all fingers lift
    let mut ungrab_pending = false;

    loop {
        // Poll IPC for Quickshell state updates
        if ipc.poll() {
            detector.set_scrollable(ipc.scrollable);
        }

        let events: Vec<evdev::InputEvent> = match device.fetch_events() {
            Ok(events) => events.collect(),
            Err(e) => {
                log::error!("Error reading events: {e}");
                continue;
            }
        };

        // Collect pending actions to execute after processing all events in a frame
        let mut pending_actions: Vec<gesture::FireAction> = Vec::new();
        // Buffer events per input frame for consistent MT protocol forwarding
        let mut frame_buf: Vec<evdev::InputEvent> = Vec::new();

        for event in &events {
            // Always feed to gesture detector
            let result = detector.process_event(event);

            if let GestureResult::Fire(action) = result {
                pending_actions.push(action);
            }

            // Buffer events; flush on SYN_REPORT
            if grabbed {
                if event.event_type() == EventType::SYNCHRONIZATION {
                    let active = detector.active_finger_count();

                    // If ungrab is pending and all fingers lifted, release now
                    if ungrab_pending && active == 0 {
                        log::info!("All fingers lifted, releasing grab");
                        vdev = None;
                        if let Err(e) = device.ungrab() {
                            log::error!("EVIOCUNGRAB failed: {e}");
                        }
                        detector.set_grabbed(false);
                        detector.set_scrollable(false);
                        grabbed = false;
                        forwarding_multi = false;
                        ungrab_pending = false;
                        frame_buf.clear();
                        continue;
                    }

                    // Don't forward anything while ungrab is pending — just swallow
                    if ungrab_pending {
                        frame_buf.clear();
                        continue;
                    }

                    // End of frame — decide whether to forward the whole frame
                    if let Some(ref mut vd) = vdev {
                        let should_forward = active <= 1 || detector.is_scrollable();

                        if should_forward {
                            if !frame_buf.is_empty() {
                                let _ = vd.emit(&frame_buf);
                            }
                            let _ = vd.emit(&[*event]); // SYN_REPORT
                        } else if forwarding_multi {
                            // Transition: was forwarding, now swallowing
                            let slots: Vec<usize> = (0..active as usize).collect();
                            virtual_device::emit_lift_all(vd, &slots);
                        }

                        forwarding_multi = active > 1 && should_forward;
                    }
                    frame_buf.clear();
                } else {
                    frame_buf.push(*event);
                }
            }

            // Handle actions
            for action in pending_actions.drain(..) {
                if let Some(ref cmd) = action.command {
                    spawn_command(cmd);
                }

                if action.grab && !grabbed {
                    log::info!("Entering grab mode");
                    if let Err(e) = device.grab() {
                        log::error!("EVIOCGRAB failed: {e}");
                        continue;
                    }
                    vdev = Some(virtual_device::create_virtual_touchpad(&device));
                    detector.set_grabbed(true);
                    grabbed = true;
                    forwarding_multi = false;
                }

                if action.ungrab && grabbed {
                    log::info!("Close gesture fired, waiting for fingers to lift");
                    ipc.broadcast_close();
                    ungrab_pending = true;
                }
            }
        }
    }
}

fn spawn_command(cmd: &str) {
    match Command::new("sh").args(["-c", cmd]).spawn() {
        Ok(mut child) => {
            log::debug!("Spawned: {cmd}");
            thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => log::error!("Failed to spawn command: {e}"),
    }
}

fn parse_config_arg() -> Option<PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-c" | "--config" => {
                let path = args.next().unwrap_or_else(|| {
                    eprintln!("error: --config requires a path argument");
                    std::process::exit(1);
                });
                return Some(PathBuf::from(path));
            }
            _ => {
                eprintln!("error: unknown argument: {arg}");
                eprintln!("usage: edgeswipe [--config <path>]");
                std::process::exit(1);
            }
        }
    }
    None
}
