use std::{
    fs,
    io::{self, IsTerminal, Read},
    path::PathBuf,
    process::{Command, Stdio},
};

use vm_core::{vm_println, vm_success};

use crate::error::{VmError, VmResult};

use super::files::ApplianceFiles;

pub(super) fn repair_github(files: &ApplianceFiles) -> VmResult<bool> {
    if files.has_git_token()? {
        return Ok(false);
    }
    let Ok(token) = github_token() else {
        return Ok(false);
    };
    files.set_git_token(&token)?;
    vm_success!("Imported the active GitHub credential");
    Ok(true)
}

pub(super) fn login(
    files: &ApplianceFiles,
    token_stdin: bool,
    token_file: Option<PathBuf>,
) -> VmResult<()> {
    let token = if token_stdin {
        if io::stdin().is_terminal() {
            return Err(VmError::validation(
                "--token-stdin requires piped input",
                Some("Pipe the token into vm packages auth login --token-stdin"),
            ));
        }
        let mut token = String::new();
        io::stdin()
            .read_to_string(&mut token)
            .map_err(|error| VmError::general(error, "Could not read Git token from stdin"))?;
        token.trim().to_string()
    } else if let Some(path) = token_file {
        fs::read_to_string(&path)
            .map_err(|error| {
                VmError::filesystem(error, path.display().to_string(), "read Git token")
            })?
            .trim()
            .to_string()
    } else {
        github_token()?
    };
    if token.is_empty() {
        return Err(VmError::validation("Git token is empty", None::<String>));
    }
    files.set_git_token(&token)?;
    vm_success!("Package Git credential updated");
    vm_println!("Run `vm packages up` to apply it to the appliance");
    Ok(())
}

pub(super) fn status(files: &ApplianceFiles) -> VmResult<()> {
    vm_println!(
        "Package Git credential: {}",
        if files.has_git_token()? {
            "configured"
        } else {
            "not configured"
        }
    );
    Ok(())
}

pub(super) fn logout(files: &ApplianceFiles) -> VmResult<()> {
    files.set_git_token("")?;
    vm_success!("Package Git credential removed");
    vm_println!("Run `vm packages up` to apply it to the appliance");
    Ok(())
}

fn github_token() -> VmResult<String> {
    let status = Command::new("gh")
        .args(["auth", "status", "--hostname", "github.com"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| VmError::general(error, "Could not run the GitHub CLI"))?;
    if !status.success() {
        return Err(invalid_github_credential());
    }
    let output = Command::new("gh")
        .args(["auth", "token", "--hostname", "github.com"])
        .output()
        .map_err(|error| VmError::general(error, "Could not run the GitHub CLI"))?;
    if !output.status.success() {
        return Err(invalid_github_credential());
    }
    let token = String::from_utf8(output.stdout)
        .map_err(|error| VmError::general(error, "GitHub CLI returned an invalid credential"))?
        .trim()
        .to_string();
    if token.is_empty() {
        return Err(VmError::validation(
            "The GitHub CLI returned an empty credential",
            Some("Run `gh auth login --hostname github.com`, then retry"),
        ));
    }
    Ok(token)
}

fn invalid_github_credential() -> VmError {
    VmError::validation(
        "The GitHub CLI has no valid active credential",
        Some("Run `gh auth login --hostname github.com`, then retry"),
    )
}
