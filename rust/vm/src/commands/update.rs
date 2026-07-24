use crate::error::VmError;
use serde::Deserialize;
use std::process::Command;
use vm_core::{vm_hint, vm_println, vm_progress, vm_success};

const CARGO_PACKAGE_NAME: &str = "goobits-vm";

pub fn handle_update(version: Option<&str>, force: bool) -> Result<(), VmError> {
    // Get current version
    let current_version = env!("CARGO_PKG_VERSION");
    let normalized_current_version = normalize_cargo_version(current_version);

    vm_println!("Current version: v{current_version}");

    // Determine target version
    let target_version = version.unwrap_or("latest");
    vm_println!("Target version: {target_version}");

    // Check if running from cargo or binary
    let is_cargo_install = std::env::current_exe()
        .ok()
        .and_then(|path| path.to_str().map(|s| s.to_string()))
        .map(|path| path.contains(".cargo"))
        .unwrap_or(false);

    if is_cargo_install {
        // Cargo installs should update through cargo so the installed package stays consistent.
        if !force
            && version
                .map(normalize_cargo_version)
                .is_some_and(|requested| requested == normalized_current_version)
        {
            vm_println!("Already on requested cargo version v{}", current_version);
            return Ok(());
        }

        vm_progress!("Updating with Cargo...");

        let mut cargo_args = vec!["install".to_string(), CARGO_PACKAGE_NAME.to_string()];
        if let Some(version) = version {
            cargo_args.push("--version".to_string());
            cargo_args.push(normalize_cargo_version(version));
        }
        cargo_args.push("--locked".to_string());
        cargo_args.push("--force".to_string());

        let output = Command::new("cargo").args(&cargo_args).output()?;

        if output.status.success() {
            vm_success!("Updated vm with Cargo");
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VmError::validation(
                format!("Cargo update failed: {}", stderr.trim()),
                None::<String>,
            ));
        }
    } else {
        // Download binary from GitHub
        vm_progress!("Downloading a release from GitHub...");

        // Detect platform
        let target = detect_target();

        // Construct download URL
        let api_url = if target_version == "latest" {
            "https://api.github.com/repos/goobits/vm/releases/latest".to_string()
        } else {
            format!("https://api.github.com/repos/goobits/vm/releases/tags/{target_version}")
        };

        // Create temporary directory
        let temp_dir = tempfile::Builder::new().prefix("vm-update-").tempdir()?;

        // Download release info
        vm_progress!("Fetching release information...");
        let release_info = Command::new("curl")
            .args([
                "-fsSL",
                "-H",
                "Accept: application/vnd.github.v3+json",
                &api_url,
            ])
            .output()?;

        if !release_info.status.success() {
            return Err(VmError::validation(
                format!("Release '{target_version}' was not found"),
                Some(format!(
                    "Check https://github.com/goobits/vm/releases/tag/{target_version}"
                )),
            ));
        }

        let release: GitHubRelease = serde_json::from_slice(&release_info.stdout)
            .map_err(|error| VmError::general(error, "Invalid GitHub release metadata"))?;
        let normalized_release_tag = normalize_cargo_version(&release.tag_name);

        if !force && normalized_release_tag == normalized_current_version {
            vm_println!("Already on latest binary release {}", release.tag_name);
            return Ok(());
        }

        // Find the asset URL for our platform
        let asset_pattern = format!("vm-{target}.tar.gz");
        let asset_url = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_pattern)
            .map(|asset| asset.browser_download_url.as_str())
            .ok_or_else(|| {
                VmError::validation(
                    format!("No release binary is available for {target}"),
                    None::<String>,
                )
            })?;
        let archive_path = temp_dir.path().join(&asset_pattern);

        // Download the archive
        vm_progress!("Downloading vm binary...");
        let archive_path_str = archive_path.to_str().ok_or_else(|| {
            VmError::general(
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid path"),
                "Archive path is not valid UTF-8",
            )
        })?;
        let download_output = Command::new("curl")
            .args(["-fsSL", "-o", archive_path_str, asset_url])
            .output()?;

        if !download_output.status.success() {
            return Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::Other, "Download failed"),
                "Failed to download binary from GitHub".to_string(),
            ));
        }

        // Extract the archive
        vm_progress!("Extracting vm binary...");
        let temp_dir_str = temp_dir.path().to_str().ok_or_else(|| {
            VmError::general(
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid path"),
                "Temp directory path is not valid UTF-8",
            )
        })?;
        let extract_output = Command::new("tar")
            .args(["-xzf", archive_path_str, "-C", temp_dir_str])
            .output()?;

        if !extract_output.status.success() {
            return Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::Other, "Extraction failed"),
                "Failed to extract downloaded archive".to_string(),
            ));
        }

        // Find the vm binary
        let binary_name = format!("vm-{target}");
        let temp_binary = if temp_dir.path().join(&binary_name).exists() {
            temp_dir.path().join(&binary_name)
        } else if temp_dir.path().join("vm").exists() {
            temp_dir.path().join("vm")
        } else {
            return Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::NotFound, "Binary not found"),
                "Could not find vm binary in extracted archive".to_string(),
            ));
        };

        // Get the current executable path
        let current_exe = std::env::current_exe()?;
        let backup_exe = current_exe.with_extension("backup");
        let staged_exe = current_exe.with_extension(format!("update-{}", std::process::id()));

        vm_progress!("Installing vm update...");
        #[cfg(unix)]
        std::fs::copy(&current_exe, &backup_exe)?;
        std::fs::copy(&temp_binary, &staged_exe)?;

        // Make it executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&staged_exe)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&staged_exe, perms)?;
        }

        replace_executable(&staged_exe, &current_exe, &backup_exe)?;
        let _ = std::fs::remove_file(&backup_exe);

        vm_success!("Updated vm to {}", release.tag_name);
    }

    // Show new version
    let version_output = Command::new(std::env::current_exe()?)
        .arg("--version")
        .output()?;

    if version_output.status.success() {
        let version_str = String::from_utf8_lossy(&version_output.stdout);
        vm_hint!("Installed version: {}", version_str.trim());
    }

    Ok(())
}

fn normalize_cargo_version(version: &str) -> String {
    version.strip_prefix('v').unwrap_or(version).to_string()
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

fn replace_executable(
    staged: &std::path::Path,
    current: &std::path::Path,
    _backup: &std::path::Path,
) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::rename(staged, current)
    }

    #[cfg(windows)]
    {
        let _ = std::fs::remove_file(_backup);
        std::fs::rename(current, _backup)?;
        if let Err(error) = std::fs::rename(staged, current) {
            let _ = std::fs::rename(_backup, current);
            return Err(error);
        }
        Ok(())
    }
}

fn detect_target() -> String {
    // Use compile_error! for truly unsupported platforms (compile-time check)
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    compile_error!("Unsupported architecture - only x86_64 and aarch64 are supported");

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    compile_error!("Unsupported OS - only macOS, Linux, and Windows are supported");

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        unreachable!("Architecture already checked at compile time")
    };

    let os = if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(target_os = "linux") {
        "unknown-linux-gnu"
    } else if cfg!(target_os = "windows") {
        "pc-windows-msvc"
    } else {
        unreachable!("OS already checked at compile time")
    };

    format!("{arch}-{os}")
}

#[cfg(test)]
mod tests {
    use super::{normalize_cargo_version, GitHubRelease};

    #[test]
    fn release_metadata_is_parsed_structurally() {
        let release: GitHubRelease = serde_json::from_str(
            r#"{
                "tag_name": "v5.1.0",
                "assets": [{
                    "name": "vm-aarch64-apple-darwin.tar.gz",
                    "browser_download_url": "https://example.invalid/vm.tar.gz"
                }]
            }"#,
        )
        .unwrap();

        assert_eq!(normalize_cargo_version(&release.tag_name), "5.1.0");
        assert_eq!(release.assets[0].name, "vm-aarch64-apple-darwin.tar.gz");
    }

    #[cfg(unix)]
    #[test]
    fn staged_executable_replaces_current_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let current = temp_dir.path().join("vm");
        let staged = temp_dir.path().join("vm.update");
        let backup = temp_dir.path().join("vm.backup");
        std::fs::write(&current, "old").unwrap();
        std::fs::write(&staged, "new").unwrap();

        super::replace_executable(&staged, &current, &backup).unwrap();

        assert_eq!(std::fs::read_to_string(current).unwrap(), "new");
        assert!(!staged.exists());
    }
}
