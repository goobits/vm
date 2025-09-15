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

    /// Get the environment setup file path if needed
    #[allow(dead_code)]
    pub fn env_file(&self, user: &str) -> Option<PathBuf> {
        match self {
            PackageManager::Cargo => Some(PathBuf::from(format!("/home/{}/.cargo/env", user))),
            PackageManager::Npm => Some(PathBuf::from(format!("/home/{}/.nvm/nvm.sh", user))),
            PackageManager::Pip => None,
        }
    }

    /// Check if the package manager is available
    pub fn is_available(&self) -> Result<bool> {
        match self {
            PackageManager::Cargo => Ok(which::which("cargo").is_ok()),
            PackageManager::Npm => Ok(which::which("npm").is_ok() || which::which("node").is_ok()),
            PackageManager::Pip => Ok(which::which("python3").is_ok() || which::which("pip3").is_ok()),
        }
    }

    /// Get the install command for registry packages
    #[allow(dead_code)]
    pub fn install_command(&self) -> Vec<String> {
        match self {
            PackageManager::Cargo => vec!["cargo".to_string(), "install".to_string()],
            PackageManager::Npm => vec!["npm".to_string(), "install".to_string(), "-g".to_string()],
            PackageManager::Pip => vec!["python3".to_string(), "-m".to_string(), "pip".to_string(),
                                       "install".to_string(), "--user".to_string(),
                                       "--break-system-packages".to_string()],
        }
    }

    /// Get the command to install a linked package
    #[allow(dead_code)]
    pub fn link_install_command(&self, package_path: &PathBuf) -> Vec<String> {
        match self {
            PackageManager::Cargo => vec![
                "cargo".to_string(),
                "install".to_string(),
                "--path".to_string(),
                package_path.to_string_lossy().to_string()
            ],
            PackageManager::Npm => vec![
                "npm".to_string(),
                "link".to_string()
            ],
            PackageManager::Pip => vec![
                "python3".to_string(),
                "-m".to_string(),
                "pip".to_string(),
                "install".to_string(),
                "--user".to_string(),
                "--break-system-packages".to_string(),
                "-e".to_string(),
                package_path.to_string_lossy().to_string()
            ],
        }
    }
}