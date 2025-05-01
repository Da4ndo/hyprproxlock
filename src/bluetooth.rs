use anyhow::{Context, Result};
use std::collections::HashMap;
use std::process::Command;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

pub struct BluetoothManager {
    last_connect_attempts: HashMap<String, Instant>,
}

impl BluetoothManager {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            last_connect_attempts: HashMap::new(),
        })
    }

    pub async fn check_device_rssi(&self, mac: &str) -> Result<i16> {
        let output = Command::new("hcitool")
            .args(["rssi", mac])
            .output()
            .context("Failed to run hcitool rssi")?;

        if !output.status.success() {
            debug!("Failed to get RSSI for device {}", mac);
            return Ok(-255);
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        let rssi = output_str
            .lines()
            .find(|line| line.contains("RSSI return value:"))
            .and_then(|line| line.split(':').next_back())
            .and_then(|value| value.trim().parse::<i16>().ok())
            .context("Failed to parse RSSI value")?;

        Ok(rssi)
    }

    // Simple check if a device is actually connected
    pub async fn confirm_device_connected(&self, mac: &str) -> Result<bool> {
        let output = Command::new("bluetoothctl")
            .args(["info", mac])
            .output()
            .context("Failed to run bluetoothctl info command")?;

        let output_str = String::from_utf8_lossy(&output.stdout);

        // Check if the output contains "Connected: yes" and not "not available"
        let connected =
            output_str.contains("Connected: yes") && !output_str.contains("not available");

        Ok(connected)
    }

    pub async fn try_connect_device(
        &mut self,
        mac: &str,
        name: &str,
        reconnect_interval: u64,
    ) -> Result<bool> {
        let now = Instant::now();

        // Check if we've attempted to connect to this device recently
        if let Some(last_attempt) = self.last_connect_attempts.get(mac) {
            if now.duration_since(*last_attempt) < Duration::from_secs(reconnect_interval) {
                return Ok(false);
            }
        }

        // Double-check to make sure the device is really disconnected
        // This is a slower operation, so we only do it after the time check
        if self.confirm_device_connected(mac).await? {
            debug!(
                "Device {} is already connected, skipping connect attempt",
                name
            );
            return Ok(true);
        }

        // Update the last connect attempt time
        self.last_connect_attempts.insert(mac.to_string(), now);

        info!("Attempting to connect to device: {} ({})", name, mac);

        // Try to connect using bluetoothctl and capture the output
        let output = Command::new("bluetoothctl")
            .args(["connect", mac])
            .output()
            .context("Failed to run bluetoothctl connect command")?;

        let output_str = String::from_utf8_lossy(&output.stdout);

        // Check for success markers in the output
        let success = output_str.contains("Connection successful")
            || (output_str.contains("Connected: yes") && !output_str.contains("not available"));

        if !success {
            if output_str.contains("not available") {
                warn!("Device {} is not available for connection", name);
            } else {
                warn!("Failed to connect to device: {}", name);
            }
            return Ok(false);
        }

        info!("Successfully connected to device: {}", name);
        Ok(true)
    }
}
