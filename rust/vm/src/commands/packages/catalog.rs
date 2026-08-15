use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use vm_core::{vm_println, vm_success, vm_warning};
use vm_packages::{
    PackageEcosystem, PackageIdentity, PackageInventory, RegisterPackage, RegisterTool, SourceKind,
    ToolKind,
};

use crate::error::{VmError, VmResult};

use super::{appliance::configured_client, discovery, files::ApplianceFiles};

#[derive(Default)]
pub(super) struct SourceReconcileOutcome {
    pub(super) quarantined: Vec<PathBuf>,
    pub(super) failures: Vec<String>,
}

impl SourceReconcileOutcome {
    pub(super) fn is_degraded(&self) -> bool {
        !self.quarantined.is_empty() || !self.failures.is_empty()
    }
}

pub(super) struct RegistrationIntent {
    pub(super) targets: Vec<String>,
    pub(super) ecosystem: Option<String>,
    pub(super) repository: Option<String>,
    pub(super) branch: Option<String>,
    pub(super) recursive: bool,
}

pub(super) struct SourceRootPlan {
    root_count: usize,
    pub(super) discovery: discovery::Discovery,
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
        )?;
        (discovery.packages, discovery.tools)
    };

    let failures = apply_registration(files, requests, tool_repositories).await?;
    registration_result(failures)
}

async fn apply_registration(
    files: &ApplianceFiles,
    requests: Vec<RegisterPackage>,
    tools: Vec<RegisterTool>,
) -> VmResult<Vec<String>> {
    if requests.is_empty() && tools.is_empty() {
        vm_success!("Package source scan complete; no package or tool repositories found");
        return Ok(Vec::new());
    }
    let client = configured_client(files)?;
    let mut failures = Vec::new();
    for request in requests {
        match client.register_package(&request).await {
            Ok(package) => {
                vm_success!("Registered {} ({})", package.name, package.ecosystem);
                vm_println!("Repository: {}", package.repository);
            }
            Err(error) => {
                vm_warning!("Could not register package {}: {error}", request.name);
                failures.push(format!("package {}: {error}", request.name));
            }
        }
    }
    for request in tools {
        match client.register_tool(&request).await {
            Ok(tool) => {
                vm_success!("Registered tool {} ({:?})", tool.name, tool.kind);
                vm_println!("Repository: {}", tool.repository);
            }
            Err(error) => {
                vm_warning!("Could not register tool {}: {error}", request.name);
                failures.push(format!("tool {}: {error}", request.name));
            }
        }
    }
    Ok(failures)
}

fn registration_result(failures: Vec<String>) -> VmResult<()> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(VmError::validation(
            format!("{} source registration(s) failed", failures.len()),
            Some("Fix the reported repositories and retry"),
        ))
    }
}

pub(super) fn prepare_source_roots(source_roots: &[String]) -> VmResult<SourceRootPlan> {
    let source_roots = validated_source_roots(source_roots)?;
    let discovery = if source_roots.is_empty() {
        discovery::Discovery::default()
    } else {
        discovery::discover_configured(&source_roots)?
    };
    Ok(SourceRootPlan {
        root_count: source_roots.len(),
        discovery,
    })
}

pub(super) async fn reconcile_source_roots(
    files: &ApplianceFiles,
    plan: SourceRootPlan,
) -> VmResult<SourceReconcileOutcome> {
    if plan.root_count == 0 {
        return Ok(SourceReconcileOutcome::default());
    }
    vm_println!(
        "Reconciling package sources from {} configured root(s)",
        plan.root_count
    );
    let mut outcome = SourceReconcileOutcome::default();
    for failure in plan.discovery.failures {
        vm_warning!("Source discovery failed: {}", failure.message);
        match quarantine_repository(&failure.source_root, &failure.repository) {
            Ok(path) => {
                vm_warning!(
                    "Quarantined unhealthy repository {} at {}",
                    failure.repository.display(),
                    path.display()
                );
                outcome.quarantined.push(path);
            }
            Err(error) => outcome
                .failures
                .push(format!("{}: {error}", failure.repository.display())),
        }
    }
    outcome
        .failures
        .extend(apply_registration(files, plan.discovery.packages, plan.discovery.tools).await?);
    Ok(outcome)
}

fn quarantine_repository(source_root: &Path, repository: &Path) -> VmResult<PathBuf> {
    let relative = repository.strip_prefix(source_root).map_err(|_| {
        VmError::validation(
            format!(
                "Repository {} is outside configured source root {}",
                repository.display(),
                source_root.display()
            ),
            Some("Run `vm packages doctor --fix`"),
        )
    })?;
    if relative.as_os_str().is_empty() {
        return Err(VmError::validation(
            format!(
                "Configured source root {} is itself an unhealthy repository",
                source_root.display()
            ),
            Some("Configure its parent as the package source root"),
        ));
    }
    let destination = source_root.join(".vm-quarantine").join(relative);
    if destination.exists() {
        return Err(VmError::validation(
            format!(
                "Quarantine destination {} already exists",
                destination.display()
            ),
            Some("Run `vm packages doctor --fix`"),
        ));
    }
    let parent = destination
        .parent()
        .expect("quarantined repository has a parent");
    fs::create_dir_all(parent).map_err(|error| {
        VmError::filesystem(
            error,
            parent.display().to_string(),
            "create package repository quarantine",
        )
    })?;
    fs::rename(repository, &destination).map_err(|error| {
        VmError::filesystem(
            error,
            repository.display().to_string(),
            "quarantine unhealthy package repository",
        )
    })?;
    Ok(destination)
}

pub(super) fn has_quarantined_sources(source_roots: &[String]) -> bool {
    source_roots
        .iter()
        .any(|root| Path::new(root).join(".vm-quarantine").is_dir())
}

pub(super) async fn repair_quarantined_sources(
    files: &ApplianceFiles,
    source_roots: &[String],
) -> VmResult<SourceReconcileOutcome> {
    let client = configured_client(files)?;
    let (packages, tools) = tokio::try_join!(client.package_definitions(), client.tools())?;
    let mut registered = std::collections::BTreeMap::new();
    for package in packages {
        registered.insert((package.name, "package"), package.repository);
    }
    for tool in tools
        .into_iter()
        .filter(|tool| tool.kind == ToolKind::Collection)
    {
        registered.insert((tool.name, "collection"), tool.repository);
    }

    let mut outcome = SourceReconcileOutcome::default();
    for root in validated_source_roots(source_roots)? {
        let source_root = PathBuf::from(&root);
        for repository in discovery::quarantined_repositories(&source_root)? {
            let result = (|| -> VmResult<PathBuf> {
                let (name, kind) = discovery::source_identity(&repository)?;
                let kind = match kind {
                    SourceKind::Package => "package",
                    SourceKind::ToolCollection => "collection",
                };
                let remote = registered.get(&(name.clone(), kind)).ok_or_else(|| {
                    VmError::validation(
                        format!(
                            "Quarantined source '{}' has no exact registered identity",
                            name
                        ),
                        Some(format!(
                            "Run `vm packages register {}` after repairing its Git metadata",
                            repository.display()
                        )),
                    )
                })?;
                repair_repository_git(&repository, remote)?;
                let target = restore_target(&source_root, &repository)?;
                let parent = target.parent().expect("restored source has a parent");
                fs::create_dir_all(parent).map_err(|error| {
                    VmError::filesystem(
                        error,
                        parent.display().to_string(),
                        "create restored package source parent",
                    )
                })?;
                fs::rename(&repository, &target).map_err(|error| {
                    VmError::filesystem(
                        error,
                        repository.display().to_string(),
                        "restore repaired package source",
                    )
                })?;
                Ok(target)
            })();
            match result {
                Ok(path) => {
                    vm_success!("Restored repaired source {}", path.display());
                }
                Err(error) => outcome.failures.push(error.to_string()),
            }
        }
    }
    Ok(outcome)
}

fn repair_repository_git(repository: &Path, expected_remote: &str) -> VmResult<()> {
    if !git_succeeds(repository, ["rev-parse", "--show-toplevel"])
        && (!repository.join(".git").is_file()
            || !git_succeeds(repository, ["worktree", "repair"])
            || !git_succeeds(repository, ["rev-parse", "--show-toplevel"]))
    {
        return Err(VmError::validation(
            format!(
                "Git worktree {} cannot be repaired safely",
                repository.display()
            ),
            Some(format!(
                "Run `git -C {} worktree repair`",
                repository.display()
            )),
        ));
    }

    let current = Command::new("git")
        .args(["-C"])
        .arg(repository)
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|error| VmError::general(error, "Failed to inspect package source remote"))?;
    if current.status.success() {
        let current = String::from_utf8_lossy(&current.stdout).trim().to_string();
        if current != expected_remote {
            return Err(VmError::validation(
                format!(
                    "Repository {} has origin '{current}', expected '{expected_remote}'",
                    repository.display()
                ),
                Some(format!(
                    "Run `git -C {} remote set-url origin {}`",
                    repository.display(),
                    expected_remote
                )),
            ));
        }
    } else {
        let status = Command::new("git")
            .args(["-C"])
            .arg(repository)
            .args(["remote", "add", "origin", expected_remote])
            .status()
            .map_err(|error| VmError::general(error, "Failed to restore package source remote"))?;
        if !status.success() {
            return Err(VmError::validation(
                format!("Could not restore origin for {}", repository.display()),
                Some(format!(
                    "Run `git -C {} remote add origin {}`",
                    repository.display(),
                    expected_remote
                )),
            ));
        }
    }
    discovery::discover(
        &[repository.to_string_lossy().into_owned()],
        false,
        None,
        None,
    )?;
    Ok(())
}

fn git_succeeds<const N: usize>(repository: &Path, arguments: [&str; N]) -> bool {
    Command::new("git")
        .args(["-C"])
        .arg(repository)
        .args(arguments)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn restore_target(source_root: &Path, repository: &Path) -> VmResult<PathBuf> {
    let quarantine = source_root.join(".vm-quarantine");
    let relative = repository.strip_prefix(&quarantine).map_err(|_| {
        VmError::validation(
            format!(
                "Quarantined source {} escaped its source root",
                repository.display()
            ),
            Some("Run `vm packages doctor --fix`"),
        )
    })?;
    let target = source_root.join(relative);
    if target.exists() {
        return Err(VmError::validation(
            format!(
                "Cannot restore {}; destination already exists",
                target.display()
            ),
            Some(format!(
                "Move {} aside, then run `vm packages doctor --fix`",
                target.display()
            )),
        ));
    }
    Ok(target)
}

pub(super) fn repair_github_credential(files: &ApplianceFiles) -> VmResult<bool> {
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

pub(super) async fn list(files: &ApplianceFiles) -> VmResult<()> {
    let client = configured_client(files)?;
    let (packages, inventory) = tokio::try_join!(client.package_definitions(), client.packages())?;
    if packages.is_empty() {
        vm_println!("No shared packages are registered");
        return Ok(());
    }
    vm_println!("NAME\tECOSYSTEM\tREGISTERED\tPUBLISHED\tINSTALLED\tCONSUMABLE\tSOURCE");
    for package in packages {
        let published = package_is_published(&inventory, package.ecosystem, &package.name);
        vm_println!(
            "{}\t{}\tyes\t{}\tn/a\t{}\t{}#{}",
            package.name,
            package.ecosystem,
            yes_no(published),
            yes_no(published),
            package.repository,
            package.default_branch
        );
    }
    Ok(())
}

fn package_is_published(
    inventory: &PackageInventory,
    ecosystem: PackageEcosystem,
    name: &str,
) -> bool {
    let registry = match ecosystem {
        PackageEcosystem::Npm => "npm",
        PackageEcosystem::Cargo => "cargo",
        PackageEcosystem::Python => "pypi",
    };
    let Ok(package) = PackageIdentity::new(ecosystem, name) else {
        return false;
    };
    inventory.get(registry).is_some_and(|packages| {
        packages
            .iter()
            .any(|candidate| package.matches_name(candidate))
    })
}

fn validated_source_roots(source_roots: &[String]) -> VmResult<Vec<String>> {
    source_roots
        .iter()
        .map(|root| {
            if std::path::Path::new(root).is_absolute() {
                Ok(root.clone())
            } else {
                Err(VmError::validation(
                    format!("Package source root '{root}' is not an absolute host path"),
                    Some("Run `vm config set packages.source_roots <absolute-path>... --global`"),
                ))
            }
        })
        .collect()
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

pub(super) async fn show(files: &ApplianceFiles, checkout_id: &str) -> VmResult<()> {
    let checkout = configured_client(files)?.checkout(checkout_id).await?;
    let json = serde_json::to_string_pretty(&checkout)
        .map_err(|error| VmError::general(error, "Failed to render checkout"))?;
    vm_println!("{json}");
    Ok(())
}

pub(super) async fn show_guest(checkout_id: &str) -> VmResult<()> {
    let checkout = super::runtime::GuestRuntime::discover()?
        .client()?
        .checkout(checkout_id)
        .await?;
    let json = serde_json::to_string_pretty(&checkout)
        .map_err(|error| VmError::general(error, "Failed to render checkout"))?;
    vm_println!("{json}");
    Ok(())
}

pub(super) async fn status_guest() -> VmResult<()> {
    let runtime = super::runtime::GuestRuntime::discover()?;
    let client = runtime.client()?;
    let (_packages, _tools) = tokio::try_join!(client.package_definitions(), client.tools())?;

    let _consumer = runtime.consumer();
    vm_println!("Package infrastructure: healthy");
    Ok(())
}

pub(super) fn configure_auth(
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use vm_packages::PackageEcosystem;

    use super::{
        package_is_published, quarantine_repository, repair_repository_git, validated_source_roots,
    };

    #[test]
    fn publication_state_uses_native_registry_name_normalization() {
        let inventory = BTreeMap::from([
            ("npm".to_string(), vec!["@scope/shared".to_string()]),
            ("pypi".to_string(), vec!["shared_auth".to_string()]),
            ("cargo".to_string(), vec!["shared-core".to_string()]),
        ]);

        assert!(package_is_published(
            &inventory,
            PackageEcosystem::Npm,
            "@scope/shared"
        ));
        assert!(!package_is_published(
            &inventory,
            PackageEcosystem::Npm,
            "@scope/shared_other"
        ));
        assert!(package_is_published(
            &inventory,
            PackageEcosystem::Python,
            "shared-auth"
        ));
        assert!(package_is_published(
            &inventory,
            PackageEcosystem::Cargo,
            "shared_core"
        ));
        assert!(!package_is_published(
            &inventory,
            PackageEcosystem::Cargo,
            "unpublished"
        ));
    }

    #[test]
    fn configured_source_roots_must_be_absolute() {
        assert_eq!(
            validated_source_roots(&["/srv/packages".to_string()]).unwrap(),
            ["/srv/packages"]
        );
        let error = validated_source_roots(&["../packages".to_string()]).unwrap_err();
        assert!(error.to_string().contains("absolute host path"));
        assert!(error.hint().unwrap().contains("packages.source_roots"));
    }

    #[test]
    fn unhealthy_repository_quarantine_is_reversible_and_ignored() {
        let directory = tempfile::tempdir().unwrap();
        let source_root = directory.path().join("packages");
        let repository = source_root.join("nested/broken");
        fs::create_dir_all(repository.join(".git")).unwrap();
        fs::write(repository.join("keep.txt"), "preserved").unwrap();

        let quarantined = quarantine_repository(&source_root, &repository).unwrap();

        assert!(!repository.exists());
        assert_eq!(
            quarantined,
            source_root.join(".vm-quarantine/nested/broken")
        );
        assert_eq!(
            fs::read_to_string(quarantined.join("keep.txt")).unwrap(),
            "preserved"
        );
    }

    #[test]
    fn deterministic_git_repair_restores_only_a_missing_origin() {
        let directory = tempfile::tempdir().unwrap();
        git2::Repository::init(directory.path()).unwrap();
        fs::write(directory.path().join("package.json"), r#"{"name":"demo"}"#).unwrap();

        repair_repository_git(directory.path(), "git@example.com:team/demo.git").unwrap();

        let repository = git2::Repository::open(directory.path()).unwrap();
        assert_eq!(
            repository.find_remote("origin").unwrap().url(),
            Ok("git@example.com:team/demo.git")
        );
    }

    #[test]
    fn deterministic_git_repair_refuses_to_replace_an_origin() {
        let directory = tempfile::tempdir().unwrap();
        let repository = git2::Repository::init(directory.path()).unwrap();
        repository
            .remote("origin", "git@example.com:team/other.git")
            .unwrap();

        let error =
            repair_repository_git(directory.path(), "git@example.com:team/demo.git").unwrap_err();

        assert!(error.to_string().contains("expected"));
        assert_eq!(
            repository.find_remote("origin").unwrap().url(),
            Ok("git@example.com:team/other.git")
        );
    }
}
