use std::{
    fs,
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

pub(super) fn configure(
    files: &ApplianceFiles,
    git_token_file: Option<PathBuf>,
    github: bool,
    clear_git: bool,
) -> VmResult<()> {
    if git_token_file.is_none() && !github && !clear_git {
        return Err(VmError::validation(
            "Provide --github, a Git token file, or --clear",
            None::<String>,
        ));
    }
    let git_token = if github {
        Some(github_token()?)
    } else {
        credential(git_token_file, clear_git, "Git")?
    };
    if let Some(token) = git_token {
        files.set_git_token(&token)?;
        vm_success!("Package Git credential updated");
    }
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

fn credential(path: Option<PathBuf>, clear: bool, kind: &str) -> VmResult<Option<String>> {
    match (path, clear) {
        (Some(path), false) => fs::read_to_string(&path)
            .map(|token| Some(token.trim().to_string()))
            .map_err(|error| {
                VmError::filesystem(
                    error,
                    path.display().to_string(),
                    format!("read {kind} token"),
                )
            }),
        (None, true) => Ok(Some(String::new())),
        (None, false) => Ok(None),
        (Some(_), true) => Err(VmError::validation(
            format!("Cannot set and clear the {kind} token together"),
            None::<String>,
        )),
    }
}
