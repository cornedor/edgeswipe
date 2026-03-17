# edgeswipe

A Linux touchpad edge-swipe gesture daemon. Detects swipe gestures that start from the edge of your touchpad and executes configurable commands.

## Features

- Auto-detects your touchpad device
- Configurable edge zones, swipe directions, and finger counts
- Adjustable distance, velocity, and cooldown thresholds
- Runs arbitrary shell commands on gesture match

## Building

```sh
cargo build --release
```

## Configuration

Configuration is read from `~/.config/edgeswipe/config.toml`.

```toml
[touchpad]
# device = "/dev/input/event0"  # optional, auto-detected if omitted

[thresholds]
min_distance = 200
min_velocity = 0.5
edge_zone_width = 200
cooldown_ms = 500

[[gestures]]
name = "right-edge-swipe-left"
edge = "right"
direction = "left"
fingers = 2
command = "echo 'gesture triggered'"
```

## Usage

```sh
./target/release/edgeswipe
```

Enable debug logging with `RUST_LOG=debug`.

## License

MIT
