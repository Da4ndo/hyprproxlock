mod bluetooth;
mod config;
mod lock;

use anyhow::{Context, Result};
use tokio::time;
use tracing::{debug, error, info};
use tracing_subscriber::fmt::time::LocalTime;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{filter::LevelFilter, fmt};

use bluetooth::BluetoothManager;
use config::Config;
use lock::LockManager;

struct ProxLock {
    config: Config,
    bluetooth: BluetoothManager,
    lock: LockManager,
}

impl ProxLock {
    async fn new() -> Result<Self> {
        let config = Config::load()?;
        let bluetooth = BluetoothManager::new().await?;
        let lock = LockManager::new();

        Ok(Self {
            config,
            bluetooth,
            lock,
        })
    }

    async fn run(&mut self) -> Result<()> {
        // Track auto-reconnect timing
        let mut reconnect_counter: u64 = 0;

        loop {
            let mut any_device_in_range = false;
            let mut all_devices_weak = true;
            let mut any_device_connected = false;

            // Collect potentially disconnected devices
            let mut potential_disconnects = Vec::new();

            // Check all configured devices
            for device in &self.config.devices {
                if !device.enabled {
                    continue;
                }

                // Attempt to get RSSI value
                let rssi = self
                    .bluetooth
                    .check_device_rssi(&device.mac_address)
                    .await?;

                if rssi > -255 {
                    // Device is connected with valid RSSI
                    any_device_connected = true;
                    debug!("Device {} RSSI: {} dBm", device.name, rssi);

                    if rssi >= self.config.thresholds.lock_threshold {
                        all_devices_weak = false;
                        debug!(
                            "Device {} signal strong enough to prevent locking",
                            device.name
                        );
                    }

                    if self.lock.is_locked() && rssi > self.config.thresholds.unlock_threshold {
                        any_device_in_range = true;
                        info!("Device {} signal strong enough for unlocking", device.name);
                    }
                } else if device.auto_connect {
                    // Failed to get RSSI, assume disconnected
                    debug!(
                        "Device {} appears disconnected (no valid RSSI)",
                        device.name
                    );
                    potential_disconnects.push(device);
                }
            }

            if !any_device_connected {
                all_devices_weak = false;
                info!("No devices connected, not locking");
            }

            // Handle auto-connect for potentially disconnected devices
            reconnect_counter += self.config.timings.poll_interval;
            if reconnect_counter >= self.config.timings.reconnect_interval
                && !potential_disconnects.is_empty()
            {
                reconnect_counter = 0;

                info!(
                    "Attempting reconnection for {} device(s)",
                    potential_disconnects.len()
                );
                for device in potential_disconnects {
                    // Try to connect to potentially disconnected devices
                    let _ = self
                        .bluetooth
                        .try_connect_device(
                            &device.mac_address,
                            &device.name,
                            self.config.timings.reconnect_interval,
                        )
                        .await;
                }
            }

            // Update timers and handle lock/unlock
            self.lock.update_timers(
                all_devices_weak,
                any_device_in_range,
                self.config.timings.poll_interval,
            );

            if all_devices_weak
                && !self.lock.is_locked()
                && self.lock.get_lock_timer() >= self.config.timings.lock_hold_seconds
            {
                self.lock.lock_screen()?;
            }

            if any_device_in_range
                && self.lock.is_locked()
                && self.lock.get_unlock_timer() >= self.config.timings.unlock_hold_seconds
            {
                self.lock.unlock_screen()?;
            }

            time::sleep(time::Duration::from_secs(self.config.timings.poll_interval)).await;
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Get the data directory for logs
    let data_dir = dirs::state_dir()
        .context("Failed to get state directory")?
        .join("hyprproxlock")
        .join("logs");

    // Create the log directory if it doesn't exist
    std::fs::create_dir_all(&data_dir).context("Failed to create log directory")?;

    // Get current date for log file
    let current_date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let current_log_file = data_dir.join(format!("hyprproxlock.log.{current_date}"));

    // Create a file appender with daily rotation
    let file_appender = tracing_appender_localtime::rolling::RollingFileAppender::new(
        tracing_appender_localtime::rolling::Rotation::DAILY,
        &data_dir,
        "hyprproxlock.log",
    );

    // Create a registry
    let registry = tracing_subscriber::registry();

    // File subscriber - only logs at INFO level and above
    let file_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .with_timer(LocalTime::rfc_3339())
        .with_target(true)
        .with_level(true)
        .with_filter(LevelFilter::INFO);

    // Console subscriber - logs at DEBUG level and above with more details
    let stdout_layer = fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true)
        .with_timer(LocalTime::rfc_3339())
        .with_target(true)
        .with_level(true)
        .with_filter(LevelFilter::DEBUG);

    // Register both subscribers with the registry
    registry.with(file_layer).with(stdout_layer).init();

    info!("Starting hyprproxlock");
    info!("Log file: {}", current_log_file.display());
    match ProxLock::new().await {
        Ok(mut proxlock) => proxlock.run().await,
        Err(e) => {
            error!("{}", e);
            eprintln!(
                "\nProgram closed due to an error. Please check the logs at:\n{}\n",
                current_log_file.display()
            );
            std::process::exit(1);
        }
    }
}
