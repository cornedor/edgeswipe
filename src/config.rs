use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub touchpad: TouchpadConfig,
    #[serde(default)]
    pub gesture: GestureConfig,
    #[serde(default)]
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TouchpadConfig {
    pub device: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GestureConfig {
    #[serde(default = "default_edge_zone")]
    pub edge_zone: f64,
    #[serde(default = "default_distance_threshold")]
    pub distance_threshold: f64,
    #[serde(default = "default_velocity_threshold")]
    pub velocity_threshold: f64,
    #[serde(default = "default_cooldown_ms")]
    pub cooldown_ms: u64,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            edge_zone: default_edge_zone(),
            distance_threshold: default_distance_threshold(),
            velocity_threshold: default_velocity_threshold(),
            cooldown_ms: default_cooldown_ms(),
        }
    }
}

fn default_edge_zone() -> f64 { 0.15 }              // fraction of touchpad width
fn default_distance_threshold() -> f64 { 200.0 }     // absolute axis units
fn default_velocity_threshold() -> f64 { 100.0 }     // axis units per second
fn default_cooldown_ms() -> u64 { 500 }

#[derive(Debug, Deserialize, Clone)]
pub struct Binding {
    pub edge: Edge,
    pub fingers: u32,
    pub direction: Direction,
    pub command: String,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Left,
    Right,
    Any,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Left,
    Right,
}

impl Config {
    pub fn load() -> Self {
        let config_path = config_path();
        if config_path.exists() {
            log::info!("Loading config from {}", config_path.display());
            let text = std::fs::read_to_string(&config_path).unwrap_or_else(|e| {
                log::warn!("Failed to read config: {e}, using defaults");
                String::new()
            });
            toml::from_str(&text).unwrap_or_else(|e| {
                log::warn!("Failed to parse config: {e}, using defaults");
                Self::default()
            })
        } else {
            log::info!("No config at {}, using defaults", config_path.display());
            Self::default()
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            touchpad: TouchpadConfig::default(),
            gesture: GestureConfig::default(),
            bindings: vec![Binding {
                edge: Edge::Right,
                fingers: 2,
                direction: Direction::Left,
                command: "hyprctl dispatch global quickshell:sidepanel".into(),
            }],
        }
    }
}

fn config_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("edgeswipe/config.toml")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config/edgeswipe/config.toml")
    } else {
        PathBuf::from("config.toml")
    }
}
