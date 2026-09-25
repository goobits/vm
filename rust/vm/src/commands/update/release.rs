use crate::error::VmError;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

pub(super) fn validate_version(version: &str) -> Result<String, VmError> {
    let normalized = normalize_cargo_version(version);
    if normalized.is_empty()
        || !normalized
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
        || !normalized
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
    {
        return Err(VmError::validation(
            "Invalid release version",
            Some("Use a version such as v1.2.3"),
        ));
    }
    Ok(normalized)
}

pub(super) fn verify_binary_version(binary: &Path, expected: &str) -> Result<(), VmError> {
    let output = Command::new(binary).arg("--version").output()?;
    let actual = std::str::from_utf8(&output.stdout)
        .unwrap_or_default()
        .trim();
    if !output.status.success() || actual != format!("vm {expected}") {
        return Err(VmError::validation(
            format!("Downloaded binary reports '{actual}', expected vm {expected}"),
            Some("The release was not installed"),
        ));
    }
    Ok(())
}

pub(super) fn validate_release_archive(
    archive: &Path,
    expected_binary: &str,
) -> Result<(), VmError> {
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

pub(super) fn archive_entry_matches(entry: &str, expected_binary: &str) -> bool {
    let entry = entry.strip_prefix("./").unwrap_or(entry);
    entry == expected_binary
        && !entry.starts_with('/')
        && !Path::new(entry)
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

pub(super) fn normalize_cargo_version(version: &str) -> String {
    version.strip_prefix('v').unwrap_or(version).to_string()
}

#[derive(Debug, Deserialize)]
pub(super) struct GitHubRelease {
    pub(super) tag_name: String,
    pub(super) assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitHubAsset {
    pub(super) name: String,
    pub(super) browser_download_url: String,
}

impl GitHubRelease {
    pub(super) fn asset_url(&self, name: &str) -> Option<&str> {
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

pub(super) fn download_asset(url: &str, destination: &Path, kind: &str) -> Result<(), VmError> {
    if !url.starts_with("https://github.com/goobits/vm/releases/download/") {
        return Err(VmError::validation(
            format!("Unexpected {kind} URL in release metadata"),
            None::<String>,
        ));
    }
    let destination = path_as_str(destination, kind)?;
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "--proto",
            "=https",
            "--proto-redir",
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

pub(super) fn verify_release_checksum(
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

pub(super) fn parse_release_checksum(
    contents: &str,
    archive_name: &str,
) -> Result<String, VmError> {
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

pub(super) fn detect_target() -> String {
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
