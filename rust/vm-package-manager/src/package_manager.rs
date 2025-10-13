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
        let base = Self::get_user_links_dir(user);
        match self {
            PackageManager::Cargo => base.join("cargo"),
            PackageManager::Npm => base.join("npm"),
            PackageManager::Pip => base.join("pip"),
        }
    }

    /// Get the base .links directory for a user in a cross-platform way
    fn get_user_links_dir(user: &str) -> PathBuf {
        #[cfg(windows)]
        {
            PathBuf::from(format!("C:\\Users\\{}", user)).join(".links")
        }
        #[cfg(not(windows))]
        {
            PathBuf::from(format!("/home/{user}")).join(".links")
        }
    }

    /// Check if the package manager is available
    pub fn is_available(&self) -> bool {
        match self {
            PackageManager::Cargo => which::which("cargo").is_ok(),
            PackageManager::Npm => which::which("npm").is_ok() || which::which("node").is_ok(),
            PackageManager::Pip => which::which("python3").is_ok() || which::which("pip3").is_ok(),
        }
    }
}
