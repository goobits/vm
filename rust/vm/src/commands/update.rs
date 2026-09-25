use crate::error::VmError;
use std::process::Command;
use vm_core::{vm_println, vm_progress, vm_success};

pub fn handle_update(version: Option<&str>) -> Result<(), VmError> {
    let current_exe = std::env::current_exe()?;
    vm_core::install_record::verified(&current_exe).map_err(|error| {
        VmError::validation(
            format!(
                "Cannot update unowned or changed installation {}: {error}",
                current_exe.display()
            ),
            Some("Install vm with the VM installer to enable managed updates"),
        )
    })?;

    let requested = version.unwrap_or("latest");
    if requested != "latest" {
        validate_version(requested)?;
    }
    let target = detect_target();
    let api_url = if requested == "latest" {
        "https://api.github.com/repos/goobits/vm/releases/latest".to_string()
    } else {
        format!("https://api.github.com/repos/goobits/vm/releases/tags/{requested}")
    };
    vm_progress!("Fetching release information...");
    let metadata = Command::new("curl")
        .args([
            "-fsSL",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "-H",
            "Accept: application/vnd.github.v3+json",
            &api_url,
        ])
        .output()?;
    if !metadata.status.success() {
        return Err(VmError::validation(
            format!("Release '{requested}' was not found"),
            None::<String>,
        ));
    }
    let release: GitHubRelease = serde_json::from_slice(&metadata.stdout)
        .map_err(|error| VmError::general(error, "Invalid GitHub release metadata"))?;
    let release_version = validate_version(&release.tag_name)?;
    if requested != "latest" && release::normalize_cargo_version(requested) != release_version {
        return Err(VmError::validation(
            "Release metadata does not match requested version",
            None::<String>,
        ));
    }
    if release_version == env!("CARGO_PKG_VERSION") {
        vm_println!("Already on vm v{release_version}");
        return Ok(());
    }

    let archive_extension = if cfg!(windows) { "zip" } else { "tar.gz" };
    let archive_name = format!("vm-{target}.{archive_extension}");
    let checksum_name = format!("{archive_name}.sha256");
    let archive_url = release.asset_url(&archive_name).ok_or_else(|| {
        VmError::validation(
            format!("No release binary is available for {target}"),
            None::<String>,
        )
    })?;
    let checksum_url = release.asset_url(&checksum_name).ok_or_else(|| {
        VmError::validation(
            format!("Release checksum is missing for {archive_name}"),
            None::<String>,
        )
    })?;
    let temp_dir = tempfile::Builder::new().prefix("vm-update-").tempdir()?;
    let archive_path = temp_dir.path().join(&archive_name);
    let checksum_path = temp_dir.path().join(&checksum_name);
    vm_progress!("Downloading and verifying vm release...");
    download_asset(archive_url, &archive_path, "release archive")?;
    download_asset(checksum_url, &checksum_path, "release checksum")?;
    verify_release_checksum(&archive_path, &checksum_path, &archive_name)?;

    let binary_name = format!("vm-{target}{}", std::env::consts::EXE_SUFFIX);
    validate_release_archive(&archive_path, &binary_name)?;
    let extract = Command::new("tar")
        .arg("-xf")
        .arg(&archive_path)
        .arg("-C")
        .arg(temp_dir.path())
        .output()?;
    if !extract.status.success() {
        return Err(VmError::validation(
            "Failed to extract verified release archive",
            None::<String>,
        ));
    }
    let binary = temp_dir.path().join(&binary_name);
    if !std::fs::symlink_metadata(&binary).is_ok_and(|metadata| metadata.file_type().is_file()) {
        return Err(VmError::validation(
            "Verified release archive has no regular vm binary",
            None::<String>,
        ));
    }
    verify_binary_version(&binary, &release_version)?;
    vm_progress!("Installing vm v{release_version}...");
    install_executable_update(&binary, &current_exe, &release_version)?;
    vm_success!("Updated vm to v{release_version}");
    Ok(())
}

mod install;
mod release;

use install::install_executable_update;
#[cfg(test)]
use install::replace_executable;
#[cfg(test)]
use release::{archive_entry_matches, normalize_cargo_version, parse_release_checksum};
use release::{
    detect_target, download_asset, validate_release_archive, validate_version,
    verify_binary_version, verify_release_checksum, GitHubRelease,
};

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
