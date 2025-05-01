use anyhow::{Context, Result};
use std::fs;

#[derive(Debug, Clone)]
pub struct DeviceConfig {
    pub mac_address: String,
    pub name: String,
    pub enabled: bool,
    pub auto_connect: bool,
}

#[derive(Debug, Clone)]
pub struct ThresholdsConfig {
    pub lock_threshold: i16,
    pub unlock_threshold: i16,
}

#[derive(Debug, Clone)]
pub struct TimingsConfig {
    pub lock_hold_seconds: u64,
    pub unlock_hold_seconds: u64,
    pub poll_interval: u64,
    pub reconnect_interval: u64,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub devices: Vec<DeviceConfig>,
    pub thresholds: ThresholdsConfig,
    pub timings: TimingsConfig,
}

impl Config {
    pub fn load() -> Result<Self> {
        // Determine if we're in release mode
        let is_release = cfg!(not(debug_assertions));
        
        // Try to load config from current directory first (for development/debug mode)
        let config_path = if !is_release {
            // In debug mode, try current directory first
            if let Ok(path) = std::env::current_dir() {
                let local_config = path.join("hyprproxlock.conf");
                if local_config.exists() {
                    local_config
                } else {
                    // Fall back to system config directory if local config doesn't exist
                    dirs::config_dir()
                        .context("Failed to get config directory")?
                        .join("hypr")
                        .join("hyprproxlock.conf")
                }
            } else {
                // Fall back to system config directory if current directory can't be determined
                dirs::config_dir()
                    .context("Failed to get config directory")?
                    .join("hypr")
                    .join("hyprproxlock.conf")
            }
        } else {
            // In release mode, only use system config directory
            dirs::config_dir()
                .context("Failed to get config directory")?
                .join("hypr")
                .join("hyprproxlock.conf")
        };

        if !config_path.exists() {
            return Err(anyhow::anyhow!(
                "Configuration file not found. Please create {} with the following format:\n\n\
                device {{\n\
                \tmac_address = \"XX:XX:XX:XX:XX:XX\"\n\
                \tname = \"My Device\"\n\
                \tenabled = true\n\
                \tauto_connect = true\n\
                }}\n\n\
                thresholds {{\n\
                \tlock_threshold = -25\n\
                \tunlock_threshold = -15\n\
                }}\n\n\
                timings {{\n\
                \tlock_hold_seconds = 3\n\
                \tunlock_hold_seconds = 3\n\
                \tpoll_interval = 1\n\
                \treconnect_interval = 20\n\
                }}",
                config_path.display()
            ));
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {config_path:?}"))?;

        let mut devices = Vec::new();
        let mut thresholds = ThresholdsConfig {
            lock_threshold: -20,
            unlock_threshold: -15,
        };
        let mut timings = TimingsConfig {
            lock_hold_seconds: 3,
            unlock_hold_seconds: 3,
            poll_interval: 1,
            reconnect_interval: 20,
        };

        let mut current_section = String::new();
        let mut current_device = None;
        
        for line in content.lines() {
            let line = line.trim();
            
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.ends_with('{') {
                let section = line.trim_end_matches('{').trim().to_string();
                current_section = section;
                if current_section == "device" {
                    current_device = Some(DeviceConfig {
                        mac_address: String::new(),
                        name: String::new(),
                        enabled: true,
                        auto_connect: false,
                    });
                }
                continue;
            }

            if line == "}" {
                if current_section == "device" {
                    if let Some(device) = current_device.take() {
                        devices.push(device);
                    }
                }
                current_section.clear();
                continue;
            }

            if let Some(idx) = line.find('=') {
                let key = line[..idx].trim();
                let value = line[idx + 1..].trim().trim_matches('"');

                match current_section.as_str() {
                    "device" => {
                        if let Some(ref mut device) = current_device {
                            match key {
                                "mac_address" => device.mac_address = value.to_string(),
                                "name" => device.name = value.to_string(),
                                "enabled" => device.enabled = value.parse().unwrap_or(true),
                                "auto_connect" => device.auto_connect = value.parse().unwrap_or(false),
                                _ => {}
                            }
                        }
                    }
                    "thresholds" => {
                        match key {
                            "lock_threshold" => thresholds.lock_threshold = value.parse().unwrap_or(-20),
                            "unlock_threshold" => thresholds.unlock_threshold = value.parse().unwrap_or(-15),
                            _ => {}
                        }
                    }
                    "timings" => {
                        match key {
                            "lock_hold_seconds" => timings.lock_hold_seconds = value.parse().unwrap_or(3),
                            "unlock_hold_seconds" => timings.unlock_hold_seconds = value.parse().unwrap_or(3),
                            "poll_interval" => timings.poll_interval = value.parse().unwrap_or(1),
                            "reconnect_interval" => timings.reconnect_interval = value.parse().unwrap_or(20),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(Self {
            devices,
            thresholds,
            timings,
        })
    }
} 