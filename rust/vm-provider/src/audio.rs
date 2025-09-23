#[cfg(target_os = "macos")]
use anyhow::{Context, Result};
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
#[cfg(target_os = "macos")]
use vm_common::vm_error;

#[cfg(target_os = "macos")]
/// Manages PulseAudio server on macOS for container audio.
pub struct MacOSAudioManager;

#[cfg(target_os = "macos")]
impl MacOSAudioManager {
    /// Ensures PulseAudio is installed and running.
    pub fn setup() -> Result<()> {
        if !is_pulseaudio_installed()? {
            println!("🎧 Installing PulseAudio via Homebrew...");
            install_pulseaudio()?;
        }
        start_pulseaudio_daemon()
    }

    /// Stops the PulseAudio daemon.
    pub fn cleanup() -> Result<()> {
        println!("⏹️ Stopping audio services...");
        Command::new("pulseaudio")
            .arg("-k")
            .status()
            .context("Failed to stop PulseAudio daemon.")?;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn is_pulseaudio_installed() -> Result<bool> {
    Ok(Command::new("brew")
        .args(["list", "pulseaudio"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?
        .success())
}

#[cfg(target_os = "macos")]
fn install_pulseaudio() -> Result<()> {
    let status = Command::new("brew")
        .args(["install", "pulseaudio"])
        .status()
        .context("Failed to execute 'brew install pulseaudio'. Make sure Homebrew is installed.")?;
    if !status.success() {
        vm_error!("'brew install pulseaudio' failed.");
        return Err(anyhow::anyhow!("brew install pulseaudio failed"));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn start_pulseaudio_daemon() -> Result<()> {
    println!("🎧 Starting audio services...");
    let status = Command::new("pulseaudio")
        .args([
            "--load=module-native-protocol-unix",
            "--exit-idle-time=-1",
            "--daemon",
        ])
        .status()
        .context("Failed to start PulseAudio daemon.")?;
    if !status.success() {
        vm_error!("Failed to start PulseAudio daemon.");
        return Err(anyhow::anyhow!("Failed to start PulseAudio daemon"));
    }
    Ok(())
}

// Stub implementation for non-macOS platforms to allow compilation.
#[cfg(not(target_os = "macos"))]
pub struct MacOSAudioManager;

#[cfg(not(target_os = "macos"))]
impl MacOSAudioManager {
    pub fn setup() {
        // Do nothing on non-macOS platforms.
    }

    pub fn cleanup() {
        // Do nothing on non-macOS platforms.
    }
}
