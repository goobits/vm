use crate::error::VmError;
use std::process::Command;
use vm_core::{vm_error, vm_println, vm_success, vm_warning};

pub fn handle_update(version: Option<&str>, force: bool) -> Result<(), VmError> {
    // Get current version
    let current_version = env!("CARGO_PKG_VERSION");

    vm_println!("Current version: v{}", current_version);

    // Determine target version
    let target_version = version.unwrap_or("latest");
    vm_println!("Target version: {}", target_version);

    // Check if running from cargo or binary
    let is_cargo_install = std::env::current_exe()
        .ok()
        .and_then(|path| path.to_str().map(|s| s.to_string()))
        .map(|path| path.contains(".cargo"))
        .unwrap_or(false);

    if is_cargo_install && version.is_none() && !force {
        // For cargo installs without specific version, use cargo
        vm_println!("Updating via cargo...");

        let output = Command::new("cargo")
            .args(["install", "vm", "--force"])
            .output()?;

        if output.status.success() {
            vm_success!("Successfully updated vm via cargo");
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            vm_error!("Failed to update: {}", stderr);
            return Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::Other, "Update failed"),
                "Failed to update via cargo".to_string(),
            ));
        }
    } else {
        // Download binary from GitHub
        vm_println!("Downloading latest binary from GitHub releases...");

        // Detect platform
        let target = detect_target();

        // Construct download URL
        let repo_url = "https://github.com/goobits/vm";
        let api_url = if target_version == "latest" {
            "https://api.github.com/repos/goobits/vm/releases/latest".to_string()
        } else {
            format!(
                "https://api.github.com/repos/goobits/vm/releases/tags/{}",
                target_version
            )
        };

        // Create temporary directory
        let temp_dir = std::env::temp_dir().join("vm-update");
        std::fs::create_dir_all(&temp_dir)?;

        // Download release info
        vm_println!("Fetching release information...");
        let release_info = Command::new("curl")
            .args([
                "-sSL",
                "-H",
                "Accept: application/vnd.github.v3+json",
                &api_url,
            ])
            .output()?;

        if !release_info.status.success() {
            vm_error!("Failed to fetch release information");
            vm_warning!(
                "Check if version '{}' exists at {}/releases",
                target_version,
                repo_url
            );
            return Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::NotFound, "Release not found"),
                format!("Failed to fetch release info for {}", target_version),
            ));
        }

        // Parse JSON to find download URL
        let release_json = String::from_utf8_lossy(&release_info.stdout);

        // Find the asset URL for our platform
        let asset_pattern = format!("vm-{}.tar.gz", target);
        let asset_url = find_asset_url(&release_json, &asset_pattern);

        if asset_url.is_none() {
            vm_error!("Could not find download URL for platform: {}", target);
            return Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::NotFound, "Platform not supported"),
                format!("No binary available for {}", target),
            ));
        }

        let asset_url = asset_url.unwrap();
        let archive_path = temp_dir.join(&asset_pattern);

        // Download the archive
        vm_println!("Downloading vm binary...");
        let download_output = Command::new("curl")
            .args(["-sSL", "-o", archive_path.to_str().unwrap(), &asset_url])
            .output()?;

        if !download_output.status.success() {
            vm_error!("Failed to download binary");
            return Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::Other, "Download failed"),
                "Failed to download binary from GitHub".to_string(),
            ));
        }

        // Extract the archive
        vm_println!("Extracting binary...");
        let extract_output = Command::new("tar")
            .args([
                "-xzf",
                archive_path.to_str().unwrap(),
                "-C",
                temp_dir.to_str().unwrap(),
            ])
            .output()?;

        if !extract_output.status.success() {
            vm_error!("Failed to extract archive");
            return Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::Other, "Extraction failed"),
                "Failed to extract downloaded archive".to_string(),
            ));
        }

        // Find the vm binary
        let binary_name = format!("vm-{}", target);
        let temp_binary = temp_dir.join(&binary_name);

        if !temp_binary.exists() {
            // Try without the target suffix
            let temp_binary = temp_dir.join("vm");
            if !temp_binary.exists() {
                vm_error!("Binary not found in archive");
                return Err(VmError::general(
                    std::io::Error::new(std::io::ErrorKind::NotFound, "Binary not found"),
                    "Could not find vm binary in extracted archive".to_string(),
                ));
            }
        }

        // Get the current executable path
        let current_exe = std::env::current_exe()?;
        let backup_exe = current_exe.with_extension("backup");

        // Backup current binary
        vm_println!("Backing up current binary...");
        std::fs::rename(&current_exe, &backup_exe)?;

        // Install new binary
        vm_println!("Installing new binary...");
        std::fs::copy(&temp_binary, &current_exe)?;

        // Make it executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&current_exe)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&current_exe, perms)?;
        }

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_file(&backup_exe);

        vm_success!("Successfully updated vm to {}", target_version);
    }

    // Show new version
    let version_output = Command::new("vm").arg("--version").output()?;

    if version_output.status.success() {
        let version_str = String::from_utf8_lossy(&version_output.stdout);
        vm_println!("New version: {}", version_str.trim());
    }

    Ok(())
}

fn detect_target() -> String {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        panic!("Unsupported architecture");
    };

    let os = if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(target_os = "linux") {
        "unknown-linux-gnu"
    } else if cfg!(target_os = "windows") {
        "pc-windows-msvc"
    } else {
        panic!("Unsupported OS");
    };

    format!("{}-{}", arch, os)
}

fn find_asset_url(json: &str, pattern: &str) -> Option<String> {
    // Simple JSON parsing to find browser_download_url
    // In production, you'd use serde_json
    for line in json.lines() {
        if line.contains("browser_download_url") && line.contains(pattern) {
            // Extract URL from line like: "browser_download_url": "https://...",
            if let Some(start) = line.rfind("https://") {
                if let Some(end) = line[start..].find('"') {
                    return Some(line[start..start + end].to_string());
                }
            }
        }
    }
    None
}
