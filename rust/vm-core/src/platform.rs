// Standard library
use std::env;
use std::path::{Path, PathBuf};

// External crates
use crate::error::Result;
use anyhow::Context;

/// A cross-platform equivalent of `readlink -f`.
pub fn portable_readlink(path: &Path) -> Result<PathBuf> {
    let canonical_path = path
        .canonicalize()
        .with_context(|| format!("Failed to get canonical path for {:?}", path))?;
    Ok(canonical_path)
}

/// A cross-platform equivalent of `realpath --relative-to`.
pub fn portable_relative_path(base: &Path, target: &Path) -> Result<PathBuf> {
    let relative_path = pathdiff::diff_paths(target, base).ok_or_else(|| {
        anyhow::anyhow!(
            "Failed to calculate relative path from {:?} to {:?}",
            base,
            target
        )
    })?;
    Ok(relative_path)
}

/// Supported operating systems for platform detection.
///
/// This enum represents the different operating systems that the VM tool
/// can detect and operate on. It maps Rust's `std::env::consts::OS` values
/// to a more convenient enum representation.
#[derive(Debug, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOS,
    Windows,
    Unsupported,
}

/// Supported CPU architectures for platform detection.
///
/// This enum represents the different CPU architectures that the VM tool
/// can detect and operate on. It maps Rust's `std::env::consts::ARCH` values
/// to a more convenient enum representation.
#[derive(Debug, PartialEq, Eq)]
pub enum Arch {
    Amd64,
    Arm64,
    Unsupported,
}

pub struct Platform {
    pub os: Os,
    pub arch: Arch,
}

pub fn get_platform_info() -> Platform {
    let os = match env::consts::OS {
        "linux" => Os::Linux,
        "darwin" => Os::MacOS,
        "windows" => Os::Windows,
        _ => Os::Unsupported,
    };

    let arch = match env::consts::ARCH {
        "x86_64" => Arch::Amd64,
        "aarch64" => Arch::Arm64,
        _ => Arch::Unsupported,
    };

    Platform { os, arch }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let os_str = match self.os {
            Os::Linux => "linux",
            Os::MacOS => "darwin", // Keep consistency with shell script output 'darwin' for macOS
            Os::Windows => "windows",
            Os::Unsupported => "unsupported_os",
        };
        let arch_str = match self.arch {
            Arch::Amd64 => "amd64",
            Arch::Arm64 => "arm64",
            Arch::Unsupported => "unsupported_arch",
        };
        write!(f, "{}_{}", os_str, arch_str)
    }
}
