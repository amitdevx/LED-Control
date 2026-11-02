// ponytail: straightforward persistent app configuration using serde_json and standard fs.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub color_r: u8,
    pub color_g: u8,
    pub color_b: u8,
    pub brightness: u8,
    pub powered: bool,
    pub color_order: String,
    pub effect_name: String,
    pub hardware_pattern: Option<u8>,
    pub power_limit: bool,
    pub last_device_address: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            color_r: 255,
            color_g: 0,
            color_b: 128,
            brightness: 100,
            powered: true,
            color_order: "RBG".to_string(), // Default One94Store channel order
            effect_name: "Static".to_string(),
            hardware_pattern: None,
            power_limit: true,
            last_device_address: None,
        }
    }
}

impl AppConfig {
    fn config_path() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        let mut path = PathBuf::from(home);
        path.push(".config");
        path.push("led-controller");
        path.push("config.json");
        Some(path)
    }

    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if path.exists() {
                if let Ok(data) = fs::read_to_string(&path) {
                    if let Ok(config) = serde_json::from_str::<AppConfig>(&data) {
                        return config;
                    }
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = fs::write(path, json);
            }
        }
    }
}
