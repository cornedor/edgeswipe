use crate::config::{Binding, Direction, Edge, GestureConfig};
use evdev::AbsoluteAxisType;
use std::time::Instant;

/// Multitouch protocol supports up to 10 concurrent contact points
const MAX_SLOTS: usize = 10;

#[derive(Debug, Clone, Default)]
struct Slot {
    active: bool,
    x: i32,
    y: i32,
    start_x: i32,
    start_y: i32,
    start_time: Option<Instant>,
}

/// Idle -> Tracking once fingers land in an edge zone, Tracking -> Fired once
/// thresholds are met. Resets to Idle when all fingers lift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Tracking,
    Fired,
}

pub struct FireAction {
    pub command: Option<String>,
    pub grab: bool,
    pub ungrab: bool,
}

pub struct GestureDetector {
    config: GestureConfig,
    bindings: Vec<Binding>,
    x_min: i32,
    x_max: i32,
    slots: [Slot; MAX_SLOTS],
    current_slot: usize,
    state: State,
    last_fire: Option<Instant>,
    grabbed: bool,
    scrollable: bool,
}

pub enum GestureResult {
    None,
    Fire(FireAction),
}

impl GestureDetector {
    pub fn new(
        config: GestureConfig,
        bindings: Vec<Binding>,
        x_min: i32,
        x_max: i32,
    ) -> Self {
        Self {
            config,
            bindings,
            x_min,
            x_max,
            slots: Default::default(),
            current_slot: 0,
            state: State::Idle,
            last_fire: None,
            grabbed: false,
            scrollable: false,
        }
    }

    pub fn set_grabbed(&mut self, grabbed: bool) {
        self.grabbed = grabbed;
        // Reset cooldown on mode transitions so the close gesture isn't blocked
        self.last_fire = None;
        self.state = State::Idle;
    }

    pub fn set_scrollable(&mut self, scrollable: bool) {
        self.scrollable = scrollable;
    }

    pub fn is_scrollable(&self) -> bool {
        self.scrollable
    }

    /// Returns the number of currently active fingers.
    pub fn active_finger_count(&self) -> u32 {
        self.slots.iter().filter(|s| s.active).count() as u32
    }

    /// Feed raw evdev events here. Gesture evaluation happens on SYN events
    /// (after the kernel has delivered a complete input frame).
    pub fn process_event(&mut self, event: &evdev::InputEvent) -> GestureResult {
        use evdev::EventType;

        match event.event_type() {
            EventType::ABSOLUTE => self.handle_abs(event),
            EventType::SYNCHRONIZATION => return self.handle_syn(),
            _ => {}
        }
        GestureResult::None
    }

    fn handle_abs(&mut self, event: &evdev::InputEvent) {
        let code = AbsoluteAxisType(event.code());
        let val = event.value();

        match code {
            AbsoluteAxisType::ABS_MT_SLOT => {
                if (val as usize) < MAX_SLOTS {
                    self.current_slot = val as usize;
                }
            }
            AbsoluteAxisType::ABS_MT_TRACKING_ID => {
                let slot = &mut self.slots[self.current_slot];
                if val == -1 {
                    // Finger lifted
                    slot.active = false;
                    slot.start_time = None;
                } else {
                    // New finger
                    slot.active = true;
                    slot.start_time = Some(Instant::now());
                    // Sentinel: the actual start position arrives in subsequent
                    // X/Y events within this same input frame
                    slot.start_x = i32::MIN;
                    slot.start_y = i32::MIN;
                }
            }
            AbsoluteAxisType::ABS_MT_POSITION_X => {
                let slot = &mut self.slots[self.current_slot];
                slot.x = val;
                if slot.start_x == i32::MIN {
                    slot.start_x = val;
                }
            }
            AbsoluteAxisType::ABS_MT_POSITION_Y => {
                let slot = &mut self.slots[self.current_slot];
                slot.y = val;
                if slot.start_y == i32::MIN {
                    slot.start_y = val;
                }
            }
            _ => {}
        }
    }

    fn handle_syn(&mut self) -> GestureResult {
        let active_slots: Vec<usize> = self.slots.iter().enumerate()
            .filter(|(_, s)| s.active)
            .map(|(i, _)| i)
            .collect();

        let active_count = active_slots.len() as u32;

        // Reset state if no fingers
        if active_count == 0 {
            if self.state != State::Idle {
                log::debug!("All fingers lifted, resetting to Idle");
            }
            self.state = State::Idle;
            return GestureResult::None;
        }

        // Check cooldown
        if let Some(last) = self.last_fire {
            if last.elapsed().as_millis() < self.config.cooldown_ms as u128 {
                return GestureResult::None;
            }
        }

        if self.state == State::Fired {
            return GestureResult::None;
        }

        // When grabbed and scrollable, skip gesture detection for grabbed-mode
        // bindings — let the events pass through as scroll
        let skip_grabbed_bindings = self.grabbed && self.scrollable;

        // Determine edge zone for active fingers
        let x_range = (self.x_max - self.x_min) as f64;
        let edge_width = x_range * self.config.edge_zone;

        for binding in &self.bindings {
            // Filter bindings by mode
            match &binding.mode {
                Some(mode) if mode == "grabbed" => {
                    if !self.grabbed || skip_grabbed_bindings {
                        continue;
                    }
                }
                Some(_) => continue, // unknown mode, skip
                None => {
                    if self.grabbed {
                        continue; // normal bindings inactive while grabbed
                    }
                }
            }

            if active_count != binding.fingers {
                continue;
            }

            // Check if all fingers started in the edge zone (skip for Edge::Any)
            if binding.edge != Edge::Any {
                let all_in_edge = active_slots.iter().all(|&i| {
                    let slot = &self.slots[i];
                    match binding.edge {
                        Edge::Right => (slot.start_x as f64) > (self.x_max as f64 - edge_width),
                        Edge::Left => (slot.start_x as f64) < (self.x_min as f64 + edge_width),
                        Edge::Any => unreachable!(),
                    }
                });

                if !all_in_edge {
                    continue;
                }
            }

            if self.state == State::Idle {
                self.state = State::Tracking;
                log::debug!(
                    "Started tracking {} fingers from {:?} edge",
                    active_count,
                    binding.edge
                );
            }

            // Check distance and velocity for each finger
            let gesture_complete = active_slots.iter().all(|&i| {
                let slot = &self.slots[i];
                let dx = (slot.x - slot.start_x) as f64;
                let distance = dx.abs();

                let direction_ok = match binding.direction {
                    Direction::Left => dx < 0.0,
                    Direction::Right => dx > 0.0,
                };

                let velocity = slot.start_time.map_or(0.0, |t| {
                    let elapsed = t.elapsed().as_secs_f64();
                    if elapsed > 0.0 { distance / elapsed } else { 0.0 }
                });

                let ok = direction_ok
                    && distance >= self.config.distance_threshold
                    && velocity >= self.config.velocity_threshold;

                if ok {
                    log::debug!(
                        "Slot {i}: distance={distance:.0}, velocity={velocity:.0}, direction_ok={direction_ok}"
                    );
                }
                ok
            });

            if gesture_complete {
                log::info!(
                    "Gesture fired: {} fingers, {:?} edge, {:?} swipe → {:?}",
                    binding.fingers,
                    binding.edge,
                    binding.direction,
                    binding.command
                );
                self.state = State::Fired;
                self.last_fire = Some(Instant::now());
                return GestureResult::Fire(FireAction {
                    command: binding.command.clone(),
                    grab: binding.grab,
                    ungrab: binding.ungrab,
                });
            }
        }

        GestureResult::None
    }
}
