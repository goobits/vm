use crate::error::VmError;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use vm_core::{vm_println, vm_success};

const INSTALLER_MARKER: &str = "# Added by VM tool installer";

pub fn handle_uninstall(delete_config: bool, delete_data: bool, yes: bool) -> Result<(), VmError> {
    let executable = std::env::current_exe()?;
    let record = vm_core::install_record::verified(&executable).map_err(|error| {
        VmError::validation(
            format!(
                "Cannot uninstall unowned or changed installation {}: {error}",
                executable.display()
            ),
            Some("Install vm with the VM installer before using managed uninstall"),
        )
    })?;
    let paths = UninstallPaths::discover(&record.executable)?;
    vm_println!("Uninstall vm v{}", record.version);
    vm_println!("  Executable: {}", paths.executable.display());
    if delete_config {
        vm_println!(
            "  Configuration: {}, {}",
            paths.state.join("config.yaml").display(),
            paths.config.join("projects.json").display()
        );
    }
    if delete_data {
        vm_println!(
            "  Data: {}, {}, {}",
            paths.state.display(),
            paths.config.join("snapshots").display(),
            paths.data.display()
        );
    }
    if !yes {
        if !std::io::stdin().is_terminal() {
            return Err(VmError::validation(
                "Uninstall requires confirmation on a terminal",
                Some("Pass --yes after reviewing the paths above"),
            ));
        }
        if !vm_core::prompts::confirm_select("Uninstall vm?", false)? {
            return Ok(());
        }
    }

    crate::commands::tools::activation::remove_worker()?;
    if delete_config {
        remove_owned(&paths.state.join("config.yaml"))?;
        remove_owned(&paths.config.join("projects.json"))?;
    }
    if delete_data {
        for name in [
            "secrets",
            "ports.json",
            "services.json",
            "temp-vms.json",
            "plugins",
            "tart",
            "ssh",
            "home-repair",
            "infrastructure",
            "runtime-reconciliation",
            "generated",
        ] {
            remove_owned(&paths.state.join(name))?;
        }
        remove_owned(&paths.config.join("snapshots"))?;
        remove_owned(&paths.config.join("vm/tunnels"))?;
        remove_owned(&paths.data)?;
    }
    remove_owned(&paths.cache)?;
    clean_shell_configs(&paths.home, &paths.executable)?;
    remove_owned(
        &paths
            .executable
            .with_file_name(vm_core::SOURCE_WORKSPACE_MARKER),
    )?;
    remove_owned(&paths.executable)?;
    remove_owned(&vm_core::install_record::path_for(&paths.executable)?)?;
    vm_success!("Uninstalled vm");
    Ok(())
}

struct UninstallPaths {
    executable: PathBuf,
    home: PathBuf,
    state: PathBuf,
    config: PathBuf,
    data: PathBuf,
    cache: PathBuf,
}

impl UninstallPaths {
    fn discover(executable: &Path) -> Result<Self, VmError> {
        use vm_core::user_paths as paths;
        let selected = Self {
            executable: executable.to_path_buf(),
            home: paths::home_dir()?,
            state: paths::vm_state_dir()?,
            config: paths::user_config_dir()?,
            data: paths::user_data_dir()?,
            cache: paths::user_cache_dir()?,
        };
        for root in [
            &selected.state,
            &selected.config,
            &selected.data,
            &selected.cache,
        ] {
            let name = root.file_name().and_then(|name| name.to_str());
            if !matches!(name, Some("vm" | ".vm")) {
                return Err(VmError::validation(
                    format!("Refusing to delete non-VM directory {}", root.display()),
                    None::<String>,
                ));
            }
            if fs::symlink_metadata(root).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
                return Err(VmError::validation(
                    format!("Refusing to delete through symlink {}", root.display()),
                    None::<String>,
                ));
            }
        }
        Ok(selected)
    }
}

fn remove_owned(path: &Path) -> Result<(), VmError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        return Err(VmError::validation(
            format!("Refusing to delete symlink {}", path.display()),
            Some("Remove it manually after checking its target"),
        ));
    }
    if metadata.is_dir() {
        fs::remove_dir_all(path)?;
    } else if metadata.is_file() {
        fs::remove_file(path)?;
    } else {
        return Err(VmError::validation(
            format!("Unsupported path type {}", path.display()),
            None::<String>,
        ));
    }
    Ok(())
}

fn clean_shell_configs(home: &Path, executable: &Path) -> Result<(), VmError> {
    let mut profiles = vec![
        home.join(".bashrc"),
        home.join(".bash_profile"),
        home.join(".zshrc"),
        home.join(".zprofile"),
        home.join(".profile"),
        home.join(".config/fish/config.fish"),
    ];
    if let Ok(documents) = vm_core::user_paths::documents_dir() {
        profiles.push(documents.join("PowerShell/Microsoft.PowerShell_profile.ps1"));
    }
    for path in profiles {
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        let edited =
            remove_installer_lines(&contents, executable.parent().unwrap_or(Path::new("")));
        if edited != contents {
            fs::write(&path, edited)?;
        }
    }
    Ok(())
}

fn remove_installer_lines(contents: &str, bin_dir: &Path) -> String {
    let mut lines = contents.lines().peekable();
    let mut retained = Vec::new();
    let expected_path = format!("export PATH=\"{}:$PATH\"", bin_dir.display());
    let expected_fish_path = format!("fish_add_path -p \"{}\"", bin_dir.display());
    let expected_powershell_path = format!("$env:Path = \"{};$env:Path\"", bin_dir.display());
    while let Some(line) = lines.next() {
        if line.trim() == INSTALLER_MARKER
            && lines.peek().is_some_and(|next| {
                *next == expected_path
                    || *next == expected_fish_path
                    || *next == expected_powershell_path
                    || *next == "source ~/.vm-completion.bash"
                    || *next == "source ~/.vm-completion.zsh"
                    || *next == ". \"$HOME/Documents/PowerShell/vm-completion.ps1\""
            })
        {
            lines.next();
            if retained.last() == Some(&"") {
                retained.pop();
            }
        } else {
            retained.push(line);
        }
    }
    let mut edited = retained.join("\n");
    if contents.ends_with('\n') && !edited.is_empty() {
        edited.push('\n');
    }
    edited
}

#[cfg(test)]
mod tests {
    use super::{remove_installer_lines, remove_owned};
    use std::fs;

    #[test]
    fn cleanup_requires_exact_owned_marker_and_line() {
        let input = "keep\n\n# Added by VM tool installer\nexport PATH=\"/tmp/bin:$PATH\"\n# Added by VM tool installer\ncustom\n";
        let actual = remove_installer_lines(input, std::path::Path::new("/tmp/bin"));
        assert_eq!(actual, "keep\n# Added by VM tool installer\ncustom\n");
    }

    #[cfg(unix)]
    #[test]
    fn deletion_rejects_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("outside");
        fs::write(&outside, "keep").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert!(remove_owned(&link).is_err());
        assert_eq!(fs::read_to_string(outside).unwrap(), "keep");
    }
}
