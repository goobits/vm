use crate::error::VmError;
use std::io::{self, Write};
use std::path::PathBuf;
use vm_cli::msg;
use vm_core::{vm_println, vm_success, vm_warning};
use vm_messages::messages::MESSAGES;

pub fn handle_uninstall(keep_config: bool, yes: bool) -> Result<(), VmError> {
    // Get current executable path
    let current_exe = std::env::current_exe().map_err(|e| {
        VmError::general(e, "Failed to determine current executable path".to_string())
    })?;

    vm_println!("{}", MESSAGES.vm_uninstall_header);
    vm_println!("{}", MESSAGES.vm_uninstall_will_remove);
    vm_println!(
        "{}",
        msg!(
            MESSAGES.vm_uninstall_binary,
            path = current_exe.display().to_string()
        )
    );

    // Find config files to remove
    let mut config_files = Vec::new();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());

    // Common config locations
    let config_paths = vec![
        PathBuf::from(&home).join(".vm"),
        PathBuf::from(&home).join(".config/vm"),
        PathBuf::from(&home).join(".vm-install.log"),
    ];

    for path in &config_paths {
        if path.exists() {
            config_files.push(path.clone());
        }
    }

    if !keep_config && !config_files.is_empty() {
        vm_println!("{}", MESSAGES.vm_uninstall_config_files);
        for file in &config_files {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_uninstall_config_file_item,
                    path = file.display().to_string()
                )
            );
        }
    }

    // Find shell config entries
    let shell_configs = find_shell_configs(&home);
    if !shell_configs.is_empty() {
        vm_println!("{}", MESSAGES.vm_uninstall_path_entries);
        for config in &shell_configs {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_uninstall_path_entry_item,
                    path = config.display().to_string()
                )
            );
        }
    }

    vm_println!();

    // Confirm with user unless --yes flag is provided
    if !yes {
        vm_warning!("This action cannot be undone!");
        print!("Are you sure you want to uninstall vm? (y/N): ");
        io::stdout()
            .flush()
            .map_err(|e| VmError::general(e, "Failed to flush stdout"))?;

        let mut response = String::new();
        io::stdin()
            .read_line(&mut response)
            .map_err(|e| VmError::general(e, "Failed to read user input"))?;

        if !response.trim().eq_ignore_ascii_case("y") {
            vm_println!("{}", MESSAGES.vm_uninstall_cancelled);
            return Ok(());
        }
    }

    vm_println!("{}", MESSAGES.vm_uninstall_progress);

    // Remove configuration files if requested
    if !keep_config {
        for path in &config_files {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_uninstall_removing_file,
                    path = path.display().to_string()
                )
            );
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
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_uninstall_cleaned_path,
                    path = config_file.display().to_string()
                )
            );
        }
    }

    // Instructions for final removal
    vm_println!();
    vm_success!("VM has been uninstalled!");
    vm_println!("{}", MESSAGES.vm_uninstall_complete_instructions);

    // Provide the correct removal command based on location
    if current_exe.to_string_lossy().contains("cargo") {
        // Installed via cargo
        vm_println!("{}", MESSAGES.vm_uninstall_remove_cargo);
    } else if current_exe.parent().and_then(|p| p.file_name()) == Some(std::ffi::OsStr::new("bin"))
    {
        // Installed in a bin directory
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm_uninstall_remove_sudo,
                path = current_exe.display().to_string()
            )
        );
        vm_println!("{}", MESSAGES.vm_uninstall_remove_no_sudo_hint);
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm_uninstall_remove_no_sudo,
                path = current_exe.display().to_string()
            )
        );
    } else {
        // Generic removal
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm_uninstall_remove_generic,
                path = current_exe.display().to_string()
            )
        );
    }

    vm_println!("{}", MESSAGES.vm_uninstall_thank_you);

    Ok(())
}

fn find_shell_configs(home: &str) -> Vec<PathBuf> {
    let mut configs = Vec::new();

    let potential_configs = vec![
        PathBuf::from(home).join(".bashrc"),
        PathBuf::from(home).join(".bash_profile"),
        PathBuf::from(home).join(".zshrc"),
        PathBuf::from(home).join(".zprofile"),
        PathBuf::from(home).join(".profile"),
        PathBuf::from(home).join(".config/fish/config.fish"),
    ];

    for config in potential_configs {
        if config.exists() {
            // Check if file contains vm-related PATH entries
            if let Ok(contents) = std::fs::read_to_string(&config) {
                if contents.contains(".cargo/bin") || contents.contains("vm") {
                    configs.push(config);
                }
            }
        }
    }

    configs
}

fn clean_shell_config(config_file: &PathBuf) -> Result<(), std::io::Error> {
    let contents = std::fs::read_to_string(config_file)?;
    let mut new_lines = Vec::new();
    let mut skip_next = false;

    for line in contents.lines() {
        if skip_next {
            skip_next = false;
            continue;
        }

        // Skip lines added by VM installer
        if line.contains("# Added by VM installer") {
            skip_next = true;
            continue;
        }

        // Skip cargo/bin PATH additions that might be from VM
        if line.contains("export PATH=") && line.contains(".cargo/bin") {
            // Only skip if this looks like it was added for VM
            if let Ok(original) = std::fs::read_to_string(config_file) {
                if original.contains("# Added by VM installer") {
                    continue;
                }
            }
        }

        new_lines.push(line);
    }

    // Only write back if we actually removed something
    let new_contents = new_lines.join("\n");
    if new_contents != contents {
        std::fs::write(config_file, new_contents)?;
    }

    Ok(())
}
