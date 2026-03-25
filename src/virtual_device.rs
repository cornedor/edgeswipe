use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{AbsoluteAxisType, AttributeSet, Device, InputEvent, UinputAbsSetup};

/// Create a virtual touchpad that mirrors the real device's capabilities.
/// libinput will recognize this as a touchpad and route events to the compositor.
pub fn create_virtual_touchpad(real: &Device) -> VirtualDevice {
    let mut builder = VirtualDeviceBuilder::new()
        .expect("Failed to create VirtualDeviceBuilder")
        .name("edgeswipe virtual touchpad");

    // Copy supported event types (keys/buttons)
    if let Some(keys) = real.supported_keys() {
        builder = builder.with_keys(keys).expect("Failed to set keys");
    }

    // Copy relative axes if any
    if let Some(rel) = real.supported_relative_axes() {
        builder = builder.with_relative_axes(rel).expect("Failed to set relative axes");
    }

    // Copy absolute axes with their min/max/resolution info
    if let Some(abs_axes) = real.supported_absolute_axes() {
        let abs_state = real.get_abs_state().expect("Failed to get abs state");
        for axis in abs_axes.iter() {
            let info = &abs_state[axis.0 as usize];
            // Ensure no phantom touches: a positive tracking ID in the
            // initial state would make libinput think a finger is already down.
            let initial_value = if axis == AbsoluteAxisType::ABS_MT_TRACKING_ID {
                -1
            } else {
                info.value
            };
            let setup = UinputAbsSetup::new(
                axis,
                evdev::AbsInfo::new(
                    initial_value,
                    info.minimum,
                    info.maximum,
                    info.fuzz,
                    info.flat,
                    info.resolution,
                ),
            );
            builder = builder.with_absolute_axis(&setup).expect("Failed to set abs axis");
        }
    }

    // Copy properties (POINTER, etc.)
    let mut props = AttributeSet::new();
    for prop in real.properties().iter() {
        props.insert(prop);
    }
    builder = builder.with_properties(&props).expect("Failed to set properties");

    builder.build().expect("Failed to build virtual device")
}

/// Emit finger-lift events for all given slots on the virtual device.
/// This cleanly ends any active touch sessions so libinput doesn't get confused.
pub fn emit_lift_all(vdev: &mut VirtualDevice, active_slots: &[usize]) {
    let mut events = Vec::new();
    for &slot in active_slots {
        events.push(InputEvent::new(
            evdev::EventType::ABSOLUTE,
            AbsoluteAxisType::ABS_MT_SLOT.0,
            slot as i32,
        ));
        events.push(InputEvent::new(
            evdev::EventType::ABSOLUTE,
            AbsoluteAxisType::ABS_MT_TRACKING_ID.0,
            -1,
        ));
    }
    if !events.is_empty() {
        events.push(InputEvent::new(
            evdev::EventType::SYNCHRONIZATION,
            0, // SYN_REPORT
            0,
        ));
        if let Err(e) = vdev.emit(&events) {
            log::error!("Failed to emit lift events on virtual device: {e}");
        }
    }
}
