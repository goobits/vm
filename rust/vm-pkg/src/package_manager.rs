use anyhow::Result;
use clap::ValueEnum;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq)]
pub enum PackageManager {
    /// Cargo (Rust) package manager
    Cargo,
    /// NPM (Node.js) package manager
    Npm,
    /// Pip/Pipx (Python) package manager
    Pip,
}

impl fmt::Display for PackageManager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PackageManager::Cargo => write!(f, "cargo"),
            PackageManager::Npm => write!(f, "npm"),
            PackageManager::Pip => write!(f, "pip"),
        }
    }
}

impl PackageManager {
    /// Get the links directory for this package manager
    pub fn links_dir(&self, user: &str) -> PathBuf {
        let base = PathBuf::from(format!("/home/{}/.links", user));
        match self {
            PackageManager::Cargo => base.join("cargo"),
            PackageManager::Npm => base.join("npm"),
            PackageManager::Pip => base.join("pip"),
        }
    }


    /// Check if the package manager is available
    pub fn is_available(&self) -> Result<bool> {
        match self {
            PackageManager::Cargo => Ok(which::which("cargo").is_ok()),
            PackageManager::Npm => Ok(which::which("npm").is_ok() || which::which("node").is_ok()),
            PackageManager::Pip => {
                Ok(which::which("python3").is_ok() || which::which("pip3").is_ok())
            }
        }
    }


}
