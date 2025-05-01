use anyhow::{Context, Result};
use std::{process::Command, thread, time::Duration};
use tracing::{info, warn};

pub struct LockManager {
    locked: bool,
    lock_timer: u64,
    unlock_timer: u64,
}

impl LockManager {
    pub fn new() -> Self {
        let locked = Self::is_hyprlock_running();
        Self {
            locked,
            lock_timer: 0,
            unlock_timer: 0,
        }
    }

    fn is_hyprlock_running() -> bool {
        Command::new("pgrep")
            .arg("-x")
            .arg("hyprlock")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn get_lock_timer(&self) -> u64 {
        self.lock_timer
    }

    pub fn get_unlock_timer(&self) -> u64 {
        self.unlock_timer
    }

    pub fn update_timers(
        &mut self,
        all_devices_weak: bool,
        any_device_in_range: bool,
        poll_interval: u64,
    ) {
        // Sync with actual system state
        self.locked = Self::is_hyprlock_running();

        if all_devices_weak && !self.locked {
            self.lock_timer += poll_interval;
        } else {
            self.lock_timer = 0;
        }

        if any_device_in_range && self.locked {
            self.unlock_timer += poll_interval;
        } else {
            self.unlock_timer = 0;
        }
    }

    pub fn lock_screen(&mut self) -> Result<()> {
        if !self.locked {
            info!("Locking screen");
            Command::new("hyprlock").spawn()?;
            self.locked = true;
            self.lock_timer = 0;
        }
        Ok(())
    }

    pub fn unlock_screen(&mut self) -> Result<()> {
        if self.locked {
            info!("Unlocking screen");

            // Send unlock signal to hyprlock
            Command::new("pkill")
                .args(["-USR1", "hyprlock"])
                .spawn()
                .context("Failed to send unlock signal to hyprlock")?;

            // Small delay before enabling display
            thread::sleep(Duration::from_millis(500));

            // Enable display using hyprctl
            let output = Command::new("hyprctl")
                .args(["dispatch", "dpms", "on"])
                .output()
                .context("Failed to execute hyprctl command")?;

            if !output.status.success() {
                let error_msg = String::from_utf8_lossy(&output.stderr);
                warn!("Failed to enable display: {}", error_msg);
            }

            self.locked = false;
            self.unlock_timer = 0;
        }
        Ok(())
    }
}
