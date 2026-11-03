// ponytail: straightforward persistent app configuration with atomic writes and schema validation.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
            color_g: 255,
            color_b: 255,
            brightness: 100,
            powered: true,                  // Default to powered ON
            color_order: "RGB".to_string(), // Configured for MELK-OA10 native channel order
            effect_name: "Static".to_string(),
            hardware_pattern: None,
            power_limit: true,
            last_device_address: Some("BE:69:DF:01:46:00".to_string()),
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
        let mut config = Self::default();
        if let Some(path) = Self::config_path() {
            if path.exists() {
                match fs::read_to_string(&path) {
                    Ok(data) => match serde_json::from_str::<AppConfig>(&data) {
                        Ok(loaded) => {
                            config = loaded;
                        }
                        Err(e) => {
                            log::warn!(
                                "Failed to parse config file: {e}, using default configuration"
                            );
                        }
                    },
                    Err(e) => {
                        log::warn!("Failed to read config file: {e}, using default configuration");
                    }
                }
            }
        }

        config.validate_and_sanitize();
        config
    }

    pub fn validate_and_sanitize(&mut self) {
        if self.brightness > 100 {
            log::warn!(
                "Loaded brightness {} exceeds 100%, clamping to 100%",
                self.brightness
            );
            self.brightness = 100;
        }

        if crate::ble::ColorOrder::from_str_name(&self.color_order).is_none() {
            log::warn!(
                "Loaded invalid color order '{}', resetting to default RGB",
                self.color_order
            );
            self.color_order = "RGB".to_string();
        }

        if let Some(pat) = self.hardware_pattern {
            if pat > 210 {
                log::warn!(
                    "Loaded invalid hardware pattern index {}, resetting to None",
                    pat
                );
                self.hardware_pattern = None;
            }
        }

        if let Some(ref addr) = self.last_device_address {
            if !is_valid_mac(addr) {
                log::warn!("Loaded invalid Bluetooth MAC address '{}', clearing", addr);
                self.last_device_address = None;
            }
        }
    }

    /// Atomically writes the configuration to disk using a temporary file and atomic rename.
    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = match Self::config_path() {
            Some(p) => p,
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Unable to resolve home directory for config path",
                ))
            }
        };

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let tmp_path = path.with_extension("tmp");
        fs::write(&tmp_path, json)?;
        fs::rename(&tmp_path, &path)?;

        Ok(())
    }
}

fn is_valid_mac(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        return false;
    }
    parts
        .iter()
        .all(|p| p.len() == 2 && u8::from_str_radix(p, 16).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mac_validation() {
        assert!(is_valid_mac("AA:BB:CC:DD:EE:FF"));
        assert!(is_valid_mac("00:11:22:33:44:55"));
        assert!(is_valid_mac("a1:b2:c3:d4:e5:f6"));
        assert!(!is_valid_mac("invalid"));
        assert!(!is_valid_mac("00:11:22:33:44"));
        assert!(!is_valid_mac("00:11:22:33:44:55:66"));
        assert!(!is_valid_mac("00:11:22:33:44:ZZ"));
    }

    #[test]
    fn test_validate_and_sanitize_out_of_bounds() {
        let mut cfg = AppConfig {
            brightness: 150,
            color_order: "UNKNOWN".to_string(),
            hardware_pattern: Some(250),
            last_device_address: Some("bad-mac".to_string()),
            ..Default::default()
        };

        cfg.validate_and_sanitize();

        assert_eq!(cfg.brightness, 100);
        assert_eq!(cfg.color_order, "RGB");
        assert_eq!(cfg.hardware_pattern, None);
        assert_eq!(cfg.last_device_address, None);
    }

    #[test]
    fn test_validate_and_sanitize_valid_fields() {
        let mut cfg = AppConfig {
            brightness: 75,
            color_order: "GRB".to_string(),
            hardware_pattern: Some(42),
            last_device_address: Some("11:22:33:44:55:66".to_string()),
            ..Default::default()
        };

        cfg.validate_and_sanitize();

        assert_eq!(cfg.brightness, 75);
        assert_eq!(cfg.color_order, "GRB");
        assert_eq!(cfg.hardware_pattern, Some(42));
        assert_eq!(
            cfg.last_device_address,
            Some("11:22:33:44:55:66".to_string())
        );
    }
}
