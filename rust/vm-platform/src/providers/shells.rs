//! Shell provider implementations for different shell types.

use crate::traits::ShellProvider;
use anyhow::Result;
use std::env;
use std::fs;
use std::path::PathBuf;

/// Bash shell provider
pub struct BashShell;

impl ShellProvider for BashShell {
    fn name(&self) -> &'static str {
        "bash"
    }

    fn profile_path(&self) -> Option<PathBuf> {
        dirs::home_dir().map(|home| home.join(".bashrc"))
    }

    fn path_export_syntax(&self, path: &std::path::Path) -> String {
        format!("export PATH=\"{}:$PATH\"", path.display())
    }

    fn create_profile_if_missing(&self) -> Result<()> {
        if let Some(profile) = self.profile_path() {
            if !profile.exists() {
                if let Some(parent) = profile.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&profile, "# Bash profile\n")?;
            }
        }
        Ok(())
    }

    fn is_active(&self) -> bool {
        env::var("SHELL")
            .map(|shell| shell.contains("bash"))
            .unwrap_or(false)
    }
}

/// Zsh shell provider
pub struct ZshShell;

impl ShellProvider for ZshShell {
    fn name(&self) -> &'static str {
        "zsh"
    }

    fn profile_path(&self) -> Option<PathBuf> {
        dirs::home_dir().map(|home| home.join(".zshrc"))
    }

    fn path_export_syntax(&self, path: &std::path::Path) -> String {
        format!("export PATH=\"{}:$PATH\"", path.display())
    }

    fn create_profile_if_missing(&self) -> Result<()> {
        if let Some(profile) = self.profile_path() {
            if !profile.exists() {
                if let Some(parent) = profile.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&profile, "# Zsh profile\n")?;
            }
        }
        Ok(())
    }

    fn is_active(&self) -> bool {
        env::var("SHELL")
            .map(|shell| shell.contains("zsh"))
            .unwrap_or(false)
    }
}

/// Fish shell provider
pub struct FishShell;

impl ShellProvider for FishShell {
    fn name(&self) -> &'static str {
        "fish"
    }

    fn profile_path(&self) -> Option<PathBuf> {
        dirs::home_dir().map(|home| home.join(".config").join("fish").join("config.fish"))
    }

    fn path_export_syntax(&self, path: &std::path::Path) -> String {
        format!("fish_add_path -p \"{}\"", path.display())
    }

    fn create_profile_if_missing(&self) -> Result<()> {
        if let Some(profile) = self.profile_path() {
            if !profile.exists() {
                if let Some(parent) = profile.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&profile, "# Fish shell config\n")?;
            }
        }
        Ok(())
    }

    fn is_active(&self) -> bool {
        env::var("SHELL")
            .map(|shell| shell.contains("fish"))
            .unwrap_or(false)
    }
}

/// PowerShell provider (Windows)
pub struct PowerShell;

impl ShellProvider for PowerShell {
    fn name(&self) -> &'static str {
        "powershell"
    }

    fn profile_path(&self) -> Option<PathBuf> {
        // Try $PROFILE environment variable first
        env::var("PROFILE")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                // Fallback to standard PowerShell profile location
                dirs::document_dir().map(|docs| {
                    docs.join("PowerShell")
                        .join("Microsoft.PowerShell_profile.ps1")
                })
            })
            .or_else(|| {
                // Legacy WindowsPowerShell location
                dirs::document_dir().map(|docs| {
                    docs.join("WindowsPowerShell")
                        .join("Microsoft.PowerShell_profile.ps1")
                })
            })
    }

    fn path_export_syntax(&self, path: &std::path::Path) -> String {
        format!("$env:Path = \"{};$env:Path\"", path.display())
    }

    fn create_profile_if_missing(&self) -> Result<()> {
        if let Some(profile) = self.profile_path() {
            if !profile.exists() {
                if let Some(parent) = profile.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&profile, "# PowerShell profile\n")?;
            }
        }
        Ok(())
    }

    fn is_active(&self) -> bool {
        // Check if PowerShell environment variables are present
        env::var("PSModulePath").is_ok() || env::var("PSVERSION").is_ok()
    }
}

/// Command Prompt provider (Windows)
pub struct CmdShell;

impl ShellProvider for CmdShell {
    fn name(&self) -> &'static str {
        "cmd"
    }

    fn profile_path(&self) -> Option<PathBuf> {
        // CMD doesn't have a standard profile file
        None
    }

    fn path_export_syntax(&self, path: &std::path::Path) -> String {
        format!("set PATH={};%PATH%", path.display())
    }

    fn create_profile_if_missing(&self) -> Result<()> {
        // CMD doesn't have a profile file to create
        Ok(())
    }

    fn is_active(&self) -> bool {
        // Check if we're in a CMD environment (no PowerShell variables)
        cfg!(windows) && env::var("PSModulePath").is_err()
    }
}
