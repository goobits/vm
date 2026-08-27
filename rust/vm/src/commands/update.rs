use crate::error::VmError;
use serde::Deserialize;
use std::io::Write;
use std::path::Path;
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
        let archive_extension = if cfg!(windows) { "zip" } else { "tar.gz" };
        let asset_pattern = format!("vm-{target}.{archive_extension}");
        let checksum_pattern = format!("{asset_pattern}.sha256");
        let asset_url = release.asset_url(&asset_pattern).ok_or_else(|| {
            VmError::validation(
                format!("No release binary is available for {target}"),
                None::<String>,
            )
        })?;
        let checksum_url = release.asset_url(&checksum_pattern).ok_or_else(|| {
            VmError::validation(
                format!("Release checksum is missing for {asset_pattern}"),
                None::<String>,
            )
        })?;
        let archive_path = temp_dir.path().join(&asset_pattern);
        let checksum_path = temp_dir.path().join(&checksum_pattern);

        vm_progress!("Downloading vm binary...");
        download_asset(asset_url, &archive_path, "release archive")?;
        download_asset(checksum_url, &checksum_path, "release checksum")?;
        verify_release_checksum(&archive_path, &checksum_path, &asset_pattern)?;

        // Extract the archive
        vm_progress!("Extracting vm binary...");
        let binary_name = format!("vm-{target}{}", std::env::consts::EXE_SUFFIX);
        validate_release_archive(&archive_path, &binary_name)?;
        let archive_path_str = path_as_str(&archive_path, "Archive")?;
        let temp_dir_str = temp_dir.path().to_str().ok_or_else(|| {
            VmError::general(
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid path"),
                "Temp directory path is not valid UTF-8",
            )
        })?;
        let extract_output = Command::new("tar")
            .args(["-xf", archive_path_str, "-C", temp_dir_str])
            .output()?;

        if !extract_output.status.success() {
            return Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::Other, "Extraction failed"),
                "Failed to extract downloaded archive".to_string(),
            ));
        }

        // The release archive is required to contain exactly one regular file
        // with the platform-specific name validated above.
        let temp_binary = temp_dir.path().join(&binary_name);
        if !std::fs::symlink_metadata(&temp_binary).is_ok_and(|metadata| {
            metadata.file_type().is_file() && !metadata.file_type().is_symlink()
        }) {
            return Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::NotFound, "Binary not found"),
                "Could not find vm binary in extracted archive".to_string(),
            ));
        }

        // Get the current executable path
        let current_exe = std::env::current_exe()?;
        vm_progress!("Installing vm update...");
        install_executable_update(&temp_binary, &current_exe)?;

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

fn validate_release_archive(archive: &Path, expected_binary: &str) -> Result<(), VmError> {
    let archive = path_as_str(archive, "Archive")?;
    let listing = Command::new("tar").args(["-tf", archive]).output()?;
    if !listing.status.success() {
        return Err(VmError::validation(
            "Release archive could not be inspected",
            Some("The downloaded release was not installed"),
        ));
    }
    let listing = std::str::from_utf8(&listing.stdout)
        .map_err(|error| VmError::general(error, "Release archive listing is not UTF-8"))?;
    let mut entries = listing.lines();
    let entry = entries.next().unwrap_or_default();
    if !archive_entry_matches(entry, expected_binary) || entries.next().is_some() {
        return Err(VmError::validation(
            "Release archive must contain exactly the expected vm binary",
            Some("The downloaded release was not installed"),
        ));
    }
    Ok(())
}

fn archive_entry_matches(entry: &str, expected_binary: &str) -> bool {
    let entry = entry.strip_prefix("./").unwrap_or(entry);
    entry == expected_binary
        && !entry.starts_with('/')
        && !Path::new(entry)
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn install_executable_update(source: &Path, current: &Path) -> Result<(), VmError> {
    let parent = current.parent().ok_or_else(|| {
        VmError::validation("Current executable has no parent directory", None::<String>)
    })?;
    let mut staged = tempfile::Builder::new()
        .prefix(".vm-update-")
        .tempfile_in(parent)?;
    let mut source = std::fs::File::open(source)?;
    std::io::copy(&mut source, staged.as_file_mut())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        staged
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o755))?;
    }
    staged.as_file_mut().flush()?;
    staged.as_file().sync_all()?;
    let staged = staged.into_temp_path();

    // Windows cannot replace the running executable directly. Reserve a
    // random sibling path for its short-lived rollback file instead of using a
    // predictable `.backup` name. Unix ignores this path and renames atomically.
    let backup = tempfile::Builder::new()
        .prefix(".vm-backup-")
        .tempfile_in(parent)?
        .into_temp_path();
    replace_executable(staged.as_ref(), current, backup.as_ref())?;
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

impl GitHubRelease {
    fn asset_url(&self, name: &str) -> Option<&str> {
        self.assets
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| asset.browser_download_url.as_str())
    }
}

fn path_as_str<'a>(path: &'a Path, kind: &str) -> Result<&'a str, VmError> {
    path.to_str().ok_or_else(|| {
        VmError::general(
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid path"),
            format!("{kind} path is not valid UTF-8"),
        )
    })
}

fn download_asset(url: &str, destination: &Path, kind: &str) -> Result<(), VmError> {
    let destination = path_as_str(destination, kind)?;
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "--proto",
            "=https",
            "--tlsv1.2",
            "-o",
            destination,
            url,
        ])
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, "Download failed"),
            format!("Failed to download {kind} from GitHub"),
        ))
    }
}

fn verify_release_checksum(
    archive_path: &Path,
    checksum_path: &Path,
    archive_name: &str,
) -> Result<(), VmError> {
    let checksum = std::fs::read_to_string(checksum_path)
        .map_err(|error| VmError::general(error, "Failed to read release checksum"))?;
    let expected = parse_release_checksum(&checksum, archive_name)?;
    let archive = std::fs::File::open(archive_path)
        .map_err(|error| VmError::general(error, "Failed to read downloaded release"))?;
    let (actual, _) = vm_packages::sha256_reader(std::io::BufReader::new(archive))
        .map_err(|error| VmError::general(error, "Failed to hash downloaded release"))?;
    if actual == expected {
        Ok(())
    } else {
        Err(VmError::validation(
            format!("Checksum verification failed for {archive_name}"),
            Some("The downloaded release was not installed"),
        ))
    }
}

fn parse_release_checksum(contents: &str, archive_name: &str) -> Result<String, VmError> {
    let mut fields = contents.trim_start_matches('\u{feff}').split_whitespace();
    let digest = fields.next().unwrap_or_default().to_ascii_lowercase();
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(VmError::validation(
            "Release checksum is not a valid SHA-256 digest",
            None::<String>,
        ));
    }
    if let Some(filename) = fields.next() {
        if filename.trim_start_matches('*') != archive_name {
            return Err(VmError::validation(
                format!("Release checksum names unexpected asset '{filename}'"),
                None::<String>,
            ));
        }
    }
    if fields.next().is_some() {
        return Err(VmError::validation(
            "Release checksum contains unexpected trailing fields",
            None::<String>,
        ));
    }
    Ok(digest)
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
    use super::{
        archive_entry_matches, normalize_cargo_version, parse_release_checksum,
        verify_release_checksum, GitHubRelease,
    };

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

    #[test]
    fn release_checksum_must_match_asset_name_and_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("vm-aarch64-apple-darwin.tar.gz");
        let checksum = temp.path().join("vm-aarch64-apple-darwin.tar.gz.sha256");
        std::fs::write(&archive, "vm").unwrap();
        std::fs::write(
            &checksum,
            "5bce98f73f3ed0c837f2729ed9509b38ea66a156db7f653356cb6fe37b366e85  vm-aarch64-apple-darwin.tar.gz\n",
        )
        .unwrap();

        verify_release_checksum(&archive, &checksum, "vm-aarch64-apple-darwin.tar.gz").unwrap();
        assert!(parse_release_checksum(
            "5bce98f73f3ed0c837f2729ed9509b38ea66a156db7f653356cb6fe37b366e85  other.tar.gz",
            "vm-aarch64-apple-darwin.tar.gz",
        )
        .is_err());
        std::fs::write(&archive, "tampered").unwrap();
        assert!(
            verify_release_checksum(&archive, &checksum, "vm-aarch64-apple-darwin.tar.gz",)
                .is_err()
        );
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

    #[test]
    fn release_archive_entry_must_be_the_exact_binary() {
        assert!(archive_entry_matches(
            "vm-x86_64-unknown-linux-gnu",
            "vm-x86_64-unknown-linux-gnu"
        ));
        assert!(archive_entry_matches(
            "./vm-x86_64-unknown-linux-gnu",
            "vm-x86_64-unknown-linux-gnu"
        ));
        assert!(!archive_entry_matches(
            "../vm-x86_64-unknown-linux-gnu",
            "vm-x86_64-unknown-linux-gnu"
        ));
        assert!(!archive_entry_matches(
            "bin/vm-x86_64-unknown-linux-gnu",
            "vm-x86_64-unknown-linux-gnu"
        ));
    }
}
