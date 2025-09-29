// Standard library
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

// External crates
use vm_cli::msg;
use vm_core::error::Result;
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;

// Internal imports
use crate::prompt::confirm_prompt;

pub fn ensure_path(bin_dir: &Path) -> Result<()> {
    let path_var = env::var("PATH").unwrap_or_default();
    let is_in_path = env::split_paths(&path_var).any(|p| p == bin_dir);

    if is_in_path {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.installer_path_already_configured,
                path = bin_dir.display().to_string()
            )
        );
        return Ok(());
    }

    vm_println!(
        "{}",
        msg!(
            MESSAGES.installer_path_not_configured,
            path = bin_dir.display().to_string()
        )
    );

    let shell_profile = get_shell_profile()?;
    let Some(profile_path) = shell_profile else {
        vm_println!(
            "Could not detect shell profile. Please add {} to your PATH manually.",
            bin_dir.display()
        );
        return Ok(());
    };
    let prompt = format!(
        "Add {} to your PATH in {:?}?",
        bin_dir.display(),
        profile_path
    );

    if confirm_prompt(&prompt)? {
        add_to_profile(&profile_path, bin_dir)?;
        vm_println!(
            "✅ Added PATH to {}. Please restart your shell.",
            profile_path.display()
        );
    } else {
        vm_println!("{}", msg!(MESSAGES.installer_manual_path_hint));
        vm_println!("  export PATH=\"{}:$PATH\"", bin_dir.display());
    }

    Ok(())
}

fn get_shell_profile() -> Result<Option<PathBuf>> {
    #[cfg(windows)]
    {
        // On Windows, check for PowerShell profile
        // First check if PROFILE env var is set (PowerShell sets this)
        if let Ok(profile) = env::var("PROFILE") {
            return Ok(Some(PathBuf::from(profile)));
        }

        // Fallback to standard PowerShell location
        if let Ok(docs) = vm_core::user_paths::documents_dir() {
            let ps_profile = docs
                .join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1");
            // Create the directory if it doesn't exist
            if let Some(parent) = ps_profile.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            return Ok(Some(ps_profile));
        }

        // If we can't find Documents, try WindowsPowerShell in Documents
        if let Ok(home) = vm_core::user_paths::home_dir() {
            let ps_profile = home
                .join("Documents")
                .join("WindowsPowerShell")
                .join("Microsoft.PowerShell_profile.ps1");
            if let Some(parent) = ps_profile.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            return Ok(Some(ps_profile));
        }

        return Ok(None);
    }

    #[cfg(not(windows))]
    {
        let shell = env::var("SHELL").unwrap_or_default();
        let home = vm_core::user_paths::home_dir()?;

        Ok(match shell.split('/').next_back() {
            Some("bash") => Some(home.join(".bashrc")),
            Some("zsh") => Some(home.join(".zshrc")),
            Some("fish") => Some(home.join(".config/fish/config.fish")),
            _ => None,
        })
    }
}

fn add_to_profile(profile_path: &Path, bin_dir: &Path) -> Result<()> {
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(profile_path)?;

    let line_to_add = if profile_path.ends_with("config.fish") {
        format!("\nfish_add_path -p \"{}\"", bin_dir.display())
    } else if cfg!(windows) {
        // PowerShell syntax for Windows
        format!(
            "\n# Added by VM tool installer\n$env:Path = \"{};$env:Path\"",
            bin_dir.display()
        )
    } else {
        // Unix shell syntax
        format!(
            "\n# Added by VM tool installer\nexport PATH=\"{}:$PATH\"",
            bin_dir.display()
        )
    };

    writeln!(file, "{}", line_to_add).map_err(|e| {
        vm_core::error::VmError::Internal(format!("Failed to write to shell profile: {}", e))
    })
}

/// Detect platform string for use in build target directories
pub fn detect_platform_string() -> String {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;
    format!("{os}-{arch}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::tempdir;

    #[test]
    fn test_detect_platform_string() {
        let platform = detect_platform_string();

        // Platform string should contain OS and architecture
        assert!(platform.contains('-'));

        // Should match expected patterns
        let parts: Vec<&str> = platform.split('-').collect();
        assert_eq!(parts.len(), 2);

        // First part should be a known OS
        let os = parts[0];
        assert!(matches!(os, "linux" | "macos" | "windows"));

        // Second part should be a known architecture
        let arch = parts[1];
        assert!(matches!(arch, "x86_64" | "aarch64" | "arm64"));
    }

    #[test]
    fn test_get_shell_profile_detection() {
        // Test bash detection
        env::set_var("SHELL", "/bin/bash");
        let result = get_shell_profile().expect("Should detect shell profile");
        assert!(result.is_some());
        let profile = result.expect("Profile should exist for bash shell");
        assert!(profile.to_string_lossy().ends_with(".bashrc"));

        // Test zsh detection
        env::set_var("SHELL", "/usr/bin/zsh");
        let result = get_shell_profile().expect("Should detect shell profile");
        assert!(result.is_some());
        let profile = result.expect("Profile should exist for zsh shell");
        assert!(profile.to_string_lossy().ends_with(".zshrc"));

        // Test fish detection
        env::set_var("SHELL", "/usr/local/bin/fish");
        let result = get_shell_profile().expect("Should detect shell profile");
        assert!(result.is_some());
        let profile = result.expect("Profile should exist for fish shell");
        assert!(profile.to_string_lossy().ends_with("config.fish"));

        // Test unknown shell
        env::set_var("SHELL", "/bin/unknown-shell");
        let result = get_shell_profile().expect("Should handle unknown shell");
        assert!(result.is_none());

        // Test empty shell
        env::set_var("SHELL", "");
        let result = get_shell_profile().expect("Should handle empty shell");
        assert!(result.is_none());
    }

    #[test]
    fn test_path_modification_strings() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let bin_dir = temp_dir.path().join("bin");

        // Test bash/zsh format
        let bash_profile = temp_dir.path().join(".bashrc");
        std::fs::write(&bash_profile, "").expect("Failed to create test profile");

        add_to_profile(&bash_profile, &bin_dir).expect("Failed to add to bash profile");
        let content = std::fs::read_to_string(&bash_profile).expect("Failed to read profile");

        assert!(content.contains("# Added by VM tool installer"));
        assert!(content.contains(&format!("export PATH=\"{}:$PATH\"", bin_dir.display())));

        // Test fish format
        let fish_profile = temp_dir.path().join("config.fish");
        std::fs::write(&fish_profile, "").expect("Failed to create fish profile");

        add_to_profile(&fish_profile, &bin_dir).expect("Failed to add to fish profile");
        let fish_content =
            std::fs::read_to_string(&fish_profile).expect("Failed to read fish profile");

        assert!(fish_content.contains(&format!("fish_add_path -p \"{}\"", bin_dir.display())));
    }

    #[test]
    fn test_ensure_path_logic() {
        let temp_dir = tempdir().expect("Failed to create temp directory");
        let bin_dir = temp_dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).expect("Failed to create bin directory");

        // Test PATH checking with directory in PATH
        let current_path = env::var("PATH").unwrap_or_default();
        let test_path = format!("{}:{}", bin_dir.display(), current_path);
        env::set_var("PATH", &test_path);

        // Should detect that path is already in PATH
        let path_var = env::var("PATH").unwrap_or_default();
        let is_in_path = path_var.split(':').any(|p| Path::new(p) == bin_dir);
        assert!(is_in_path);

        // Test PATH checking with directory NOT in PATH
        env::set_var("PATH", &current_path);
        let path_var = env::var("PATH").unwrap_or_default();
        let is_in_path = path_var.split(':').any(|p| Path::new(p) == bin_dir);
        assert!(!is_in_path);

        // Restore original PATH
        env::set_var("PATH", &current_path);
    }

    #[test]
    fn test_shell_profile_path_validation() {
        let temp_dir = tempdir().expect("Failed to create temp directory");

        // Test that shell profile detection returns proper paths
        let original_home = env::var("HOME").ok();
        env::set_var("HOME", temp_dir.path());

        env::set_var("SHELL", "/bin/bash");
        let result = get_shell_profile().expect("Should detect bash profile");
        assert!(result.is_some());
        let bash_path = result.unwrap();
        assert_eq!(bash_path, temp_dir.path().join(".bashrc"));

        env::set_var("SHELL", "/bin/zsh");
        let result = get_shell_profile().expect("Should detect zsh profile");
        assert!(result.is_some());
        let zsh_path = result.unwrap();
        assert_eq!(zsh_path, temp_dir.path().join(".zshrc"));

        env::set_var("SHELL", "/usr/bin/fish");
        let result = get_shell_profile().expect("Should detect fish profile");
        assert!(result.is_some());
        let fish_path = result.unwrap();
        assert_eq!(fish_path, temp_dir.path().join(".config/fish/config.fish"));

        // Restore original HOME
        match original_home {
            Some(home) => env::set_var("HOME", home),
            None => env::remove_var("HOME"),
        }
    }
}
