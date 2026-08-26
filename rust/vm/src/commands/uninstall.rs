use crate::error::VmError;
use std::path::PathBuf;
use vm_core::{vm_hint, vm_println, vm_progress, vm_success, vm_warning};

const INSTALLER_MARKER: &str = "# Added by VM installer";

pub fn handle_uninstall(keep_config: bool, yes: bool) -> Result<(), VmError> {
    // Get current executable path
    let current_exe = std::env::current_exe().map_err(|e| {
        VmError::general(e, "Failed to determine current executable path".to_string())
    })?;

    vm_println!("Uninstall vm");
    vm_println!("  Binary: {}", current_exe.display());

    // Find config files to remove. Use the platform-aware home lookup so
    // uninstall works on Windows and so we don't silently scan `/tmp` for
    // config files when `$HOME` is unset.
    let mut config_files = Vec::new();
    let home = vm_core::user_paths::home_dir().map_err(|e| {
        VmError::general(
            e,
            "Failed to locate home directory while preparing uninstall".to_string(),
        )
    })?;

    // Common config locations
    let config_paths = vec![
        home.join(".vm"),
        home.join(".config/vm"),
        home.join(".vm-install.log"),
    ];

    for path in &config_paths {
        if path.exists() {
            config_files.push(path.clone());
        }
    }

    if !keep_config && !config_files.is_empty() {
        vm_println!("  Configuration:");
        for file in &config_files {
            vm_println!("    - {}", file.display());
        }
    }

    // Find shell config entries
    let shell_configs = find_shell_configs(&home);
    if !shell_configs.is_empty() {
        vm_println!("  Installer-managed shell entries:");
        for config in &shell_configs {
            vm_println!("    - {}", config.display());
        }
    }

    vm_println!();

    // Confirm with user unless --yes flag is provided
    if !yes {
        vm_warning!("This action cannot be undone!");
        if !vm_core::prompts::confirm_select("Uninstall vm?", false)? {
            vm_progress!("Uninstall cancelled");
            return Ok(());
        }
    }

    vm_progress!("Removing vm configuration...");

    if let Err(error) = crate::commands::tools::activation::remove_worker() {
        vm_warning!("Failed to stop the tool activation worker: {error}");
    }

    // Remove configuration files if requested
    if !keep_config {
        for path in &config_files {
            vm_progress!("Removing {}...", path.display());
            if path.is_dir() {
                if let Err(e) = std::fs::remove_dir_all(path) {
                    vm_warning!("Failed to remove {}: {}", path.display(), e);
                }
            } else if let Err(e) = std::fs::remove_file(path) {
                vm_warning!("Failed to remove {}: {}", path.display(), e);
            }
        }
    }

    // Clean shell configurations
    for config_file in &shell_configs {
        if let Err(e) = clean_shell_config(config_file) {
            vm_warning!("Failed to clean {}: {}", config_file.display(), e);
        } else {
            vm_success!("Cleaned {}", config_file.display());
        }
    }

    // Instructions for final removal
    vm_println!();
    vm_success!("Uninstall cleanup complete");
    vm_hint!("Remove the executable with:");

    // Provide the correct removal command based on location
    if current_exe.to_string_lossy().contains("cargo") {
        // Installed via cargo
        vm_println!("  cargo uninstall goobits-vm");
    } else if current_exe.parent().and_then(|p| p.file_name()) == Some(std::ffi::OsStr::new("bin"))
    {
        // Installed in a bin directory
        vm_println!("  sudo rm {}", current_exe.display());
        vm_println!("  rm {}", current_exe.display());
    } else {
        // Generic removal
        vm_println!("  rm {}", current_exe.display());
    }

    Ok(())
}

fn find_shell_configs(home: &std::path::Path) -> Vec<PathBuf> {
    let mut configs = Vec::new();

    let potential_configs = vec![
        home.join(".bashrc"),
        home.join(".bash_profile"),
        home.join(".zshrc"),
        home.join(".zprofile"),
        home.join(".profile"),
        home.join(".config/fish/config.fish"),
    ];

    for config in potential_configs {
        if config.exists() {
            if let Ok(contents) = std::fs::read_to_string(&config) {
                if contents.lines().any(is_installer_marker) {
                    configs.push(config);
                }
            }
        }
    }

    configs
}

fn clean_shell_config(config_file: &std::path::Path) -> Result<(), std::io::Error> {
    let contents = std::fs::read_to_string(config_file)?;
    let mut new_lines = Vec::new();
    let mut skip_next = false;

    for line in contents.lines() {
        if skip_next {
            skip_next = false;
            continue;
        }

        if is_installer_marker(line) {
            if new_lines.last().is_some_and(|line: &&str| line.is_empty()) {
                new_lines.pop();
            }
            skip_next = true;
            continue;
        }

        new_lines.push(line);
    }

    let mut new_contents = new_lines.join("\n");
    if contents.ends_with('\n') && !new_contents.is_empty() {
        new_contents.push('\n');
    }
    if new_contents != contents {
        std::fs::write(config_file, new_contents)?;
    }

    Ok(())
}

fn is_installer_marker(line: &str) -> bool {
    let line = line.trim();
    line == INSTALLER_MARKER
        || line
            .strip_prefix(INSTALLER_MARKER)
            .is_some_and(|suffix| suffix.starts_with(" v"))
}

#[cfg(test)]
mod tests {
    use super::{clean_shell_config, find_shell_configs};

    #[test]
    fn shell_cleanup_removes_only_installer_owned_lines() {
        let home = tempfile::tempdir().unwrap();
        let shell_config = home.path().join(".zshrc");
        std::fs::write(
            &shell_config,
            format!(
                "export KEEP=1\n\n# Added by VM installer v{}\nexport PATH=\"$PATH:/vm\"\necho '# Added by VM installer is documentation'\n",
                env!("CARGO_PKG_VERSION")
            ),
        )
        .unwrap();

        let configs = find_shell_configs(home.path());
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0], shell_config);
        clean_shell_config(&shell_config).unwrap();
        assert_eq!(
            std::fs::read_to_string(shell_config).unwrap(),
            "export KEEP=1\necho '# Added by VM installer is documentation'\n"
        );
    }
}
