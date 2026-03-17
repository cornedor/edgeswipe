use evdev::{AbsoluteAxisType, Device, PropType};

pub struct TouchpadInfo {
    pub device: Device,
    pub x_min: i32,
    pub x_max: i32,
}

pub fn find_touchpad(explicit_path: Option<&str>) -> TouchpadInfo {
    if let Some(path) = explicit_path {
        log::info!("Using configured device: {path}");
        let device = Device::open(path)
            .unwrap_or_else(|e| panic!("Failed to open {path}: {e}"));
        return touchpad_info(device);
    }

    log::info!("Auto-detecting touchpad...");
    let mut candidates = Vec::new();

    for entry in std::fs::read_dir("/dev/input").expect("Cannot read /dev/input") {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if !name.starts_with("event") {
            continue;
        }

        let device = match Device::open(&path) {
            Ok(d) => d,
            Err(e) => {
                log::debug!("Cannot open {}: {e}", path.display());
                continue;
            }
        };

        if is_touchpad(&device) {
            let dev_name = device.name().unwrap_or("unknown").to_string();
            log::info!("Found touchpad candidate: {} ({})", dev_name, path.display());
            candidates.push((path, device));
        }
    }

    if candidates.is_empty() {
        panic!("No touchpad found. Ensure you are in the 'input' group and a touchpad is present.");
    }

    // Prefer the first match sorted by path for determinism
    candidates.sort_by(|a, b| a.0.cmp(&b.0));
    let (path, device) = candidates.into_iter().next().unwrap();
    log::info!(
        "Selected touchpad: {} ({})",
        device.name().unwrap_or("unknown"),
        path.display()
    );
    touchpad_info(device)
}

/// A touchpad must advertise POINTER property and multitouch position axes.
/// This distinguishes it from mice, keyboards, and single-touch screens.
fn is_touchpad(device: &Device) -> bool {
    let has_pointer = device.properties().contains(PropType::POINTER);
    let abs = device.supported_absolute_axes();
    let has_mt_x = abs.map_or(false, |a| a.contains(AbsoluteAxisType::ABS_MT_POSITION_X));
    let has_mt_y = abs.map_or(false, |a| a.contains(AbsoluteAxisType::ABS_MT_POSITION_Y));

    has_pointer && has_mt_x && has_mt_y
}

fn touchpad_info(device: Device) -> TouchpadInfo {
    let abs_state = device
        .get_abs_state()
        .expect("Failed to get abs state");
    let x_info = abs_state
        .get(AbsoluteAxisType::ABS_MT_POSITION_X.0 as usize)
        .expect("No ABS_MT_POSITION_X info");
    let y_info = abs_state
        .get(AbsoluteAxisType::ABS_MT_POSITION_Y.0 as usize)
        .expect("No ABS_MT_POSITION_Y info");

    let x_min = x_info.minimum;
    let x_max = x_info.maximum;

    log::info!("Touchpad axis range: X [{x_min}, {x_max}], Y [{}, {}]", y_info.minimum, y_info.maximum);

    TouchpadInfo {
        device,
        x_min,
        x_max,
    }
}
