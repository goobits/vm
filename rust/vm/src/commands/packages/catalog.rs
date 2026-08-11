use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

use vm_core::{vm_println, vm_success};
use vm_packages::{PackageEcosystem, RegisterPackage};

use crate::error::{VmError, VmResult};

use super::{appliance::configured_client, discovery, files::ApplianceFiles};

pub(super) struct RegistrationIntent {
    pub(super) targets: Vec<String>,
    pub(super) ecosystem: Option<String>,
    pub(super) repository: Option<String>,
    pub(super) branch: Option<String>,
    pub(super) ci_registry: Option<String>,
    pub(super) recursive: bool,
}

pub(super) async fn register(files: &ApplianceFiles, intent: RegistrationIntent) -> VmResult<()> {
    let ecosystem = intent
        .ecosystem
        .as_deref()
        .map(str::parse::<PackageEcosystem>)
        .transpose()
        .map_err(|error| VmError::validation(error.to_string(), None::<String>))?;
    let (requests, tool_repositories) = if let Some(repository) = intent.repository {
        if intent.recursive || intent.targets.len() != 1 {
            return Err(VmError::validation(
                "Explicit registration accepts exactly one package name and cannot be recursive",
                None::<String>,
            ));
        }
        let ecosystem = ecosystem.ok_or_else(|| {
            VmError::validation(
                "Explicit registration requires --ecosystem",
                Some("Use npm, cargo, or python"),
            )
        })?;
        let request = RegisterPackage {
            name: intent
                .targets
                .into_iter()
                .next()
                .expect("one target checked"),
            ecosystem,
            repository,
            default_branch: intent.branch.unwrap_or_else(|| "main".into()),
            ci_registry: intent.ci_registry,
        };
        request
            .validate()
            .map_err(|error| VmError::validation(error.to_string(), None::<String>))?;
        (vec![request], Vec::new())
    } else {
        let discovery = discovery::discover(
            &intent.targets,
            intent.recursive,
            ecosystem,
            intent.branch.as_deref(),
            intent.ci_registry.as_deref(),
        )?;
        (discovery.packages, discovery.tools)
    };

    for repository in tool_repositories {
        vm_println!(
            "Tool source: {} (managed by `vm tools`)",
            repository.display()
        );
    }
    if requests.is_empty() {
        vm_success!("Package source scan complete; no language packages found");
        return Ok(());
    }
    let client = configured_client(files)?;
    for request in requests {
        let package = client.register_package(&request).await?;
        vm_success!("Registered {} ({})", package.name, package.ecosystem);
        vm_println!("Repository: {}", package.repository);
        if let Some(registry) = package.ci_registry {
            vm_println!("CI registry: {registry}");
        }
    }
    Ok(())
}

pub(super) async fn list(files: &ApplianceFiles) -> VmResult<()> {
    let packages = configured_client(files)?.package_definitions().await?;
    if packages.is_empty() {
        vm_println!("No shared packages are registered");
    }
    for package in packages {
        vm_println!(
            "{}\t{}\t{}#{}",
            package.name,
            package.ecosystem,
            package.repository,
            package.default_branch
        );
    }
    Ok(())
}

pub(super) async fn show(files: &ApplianceFiles, checkout_id: &str) -> VmResult<()> {
    let checkout = configured_client(files)?.checkout(checkout_id).await?;
    let json = serde_json::to_string_pretty(&checkout)
        .map_err(|error| VmError::general(error, "Failed to render checkout"))?;
    vm_println!("{json}");
    Ok(())
}

pub(super) fn configure_auth(
    files: &ApplianceFiles,
    git_token_file: Option<PathBuf>,
    github: bool,
    ci_token_file: Option<PathBuf>,
    clear_git: bool,
    clear_ci: bool,
) -> VmResult<()> {
    if git_token_file.is_none() && !github && ci_token_file.is_none() && !clear_git && !clear_ci {
        return Err(VmError::validation(
            "Provide --github, a Git/CI token file, or a clear flag",
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
    if let Some(token) = credential(ci_token_file, clear_ci, "CI registry")? {
        files.set_ci_publish_token(&token)?;
        vm_success!("Package CI registry credential updated");
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
