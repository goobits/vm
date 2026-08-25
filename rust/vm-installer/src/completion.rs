use std::{
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use vm_core::error::{Result, VmError};
use vm_core::{user_paths, vm_success, vm_warning};

pub(super) fn install(bin_dir: &Path) -> Result<()> {
    let shell = env::var("SHELL").unwrap_or_default();
    let shell_name = shell.split('/').next_back().unwrap_or_default();
    if shell_name.is_empty() {
        vm_warning!("Shell completion not installed: could not detect current shell");
        return Ok(());
    }

    let vm_binary = bin_dir.join(vm_platform::platform::executable_name("vm"));
    if !vm_binary.exists() {
        vm_warning!(
            "Shell completion not installed: VM binary not found at {}",
            vm_binary.display()
        );
        return Ok(());
    }

    let home = user_paths::home_dir()?;
    let setup = match shell_name {
        "bash" => Some((
            "bash",
            home.join(".vm-completion.bash"),
            Some((home.join(".bashrc"), "source ~/.vm-completion.bash")),
        )),
        "zsh" => Some((
            "zsh",
            home.join(".vm-completion.zsh"),
            Some((home.join(".zshrc"), "source ~/.vm-completion.zsh")),
        )),
        "fish" => Some(("fish", home.join(".config/fish/completions/vm.fish"), None)),
        "pwsh" | "powershell" => {
            let profile =
                user_paths::documents_dir()?.join("PowerShell/Microsoft.PowerShell_profile.ps1");
            Some((
                "powershell",
                profile.with_file_name("vm-completion.ps1"),
                Some((
                    profile,
                    ". \"$HOME/Documents/PowerShell/vm-completion.ps1\"",
                )),
            ))
        }
        _ => None,
    };
    let Some((completion_shell, completion_path, profile)) = setup else {
        vm_warning!(
            "Shell completion not installed automatically for unsupported shell '{shell_name}'"
        );
        return Ok(());
    };

    if let Some(parent) = completion_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            VmError::Internal(format!(
                "Failed to create completion directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    let completion_file = fs::File::create(&completion_path).map_err(|error| {
        VmError::Internal(format!(
            "Failed to create completion file {}: {error}",
            completion_path.display()
        ))
    })?;
    let status = Command::new(&vm_binary)
        .args(["internal-completion", completion_shell])
        .stdout(Stdio::from(completion_file))
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| {
            VmError::Internal(format!(
                "Failed to generate shell completion for {completion_shell}: {error}"
            ))
        })?;
    if !status.success() {
        vm_warning!(
            "Shell completion generation exited with code {}",
            status.code().unwrap_or(-1)
        );
        return Ok(());
    }
    if let Some((profile_path, source_line)) = profile {
        append_line_if_missing(&profile_path, source_line)?;
    }
    vm_success!(
        "Shell completion installed for {shell_name} at {}",
        completion_path.display()
    );
    Ok(())
}

fn append_line_if_missing(profile_path: &Path, line: &str) -> Result<()> {
    if let Some(parent) = profile_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            VmError::Internal(format!(
                "Failed to create shell profile directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    if fs::read_to_string(profile_path)
        .unwrap_or_default()
        .contains(line)
    {
        return Ok(());
    }
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(profile_path)
        .map_err(|error| {
            VmError::Internal(format!(
                "Failed to open shell profile {}: {error}",
                profile_path.display()
            ))
        })?;
    writeln!(file, "\n# Added by VM tool installer\n{line}").map_err(|error| {
        VmError::Internal(format!(
            "Failed to update shell profile {}: {error}",
            profile_path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::append_line_if_missing;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn profile_source_line_is_idempotent() {
        let temp_dir = tempdir().expect("create temp directory");
        let profile = temp_dir.path().join(".bashrc");
        let line = "source ~/.vm-completion.bash";
        append_line_if_missing(&profile, line).expect("append source line");
        append_line_if_missing(&profile, line).expect("avoid duplicate source line");
        assert_eq!(
            fs::read_to_string(profile)
                .expect("read profile")
                .matches(line)
                .count(),
            1
        );
    }
}
