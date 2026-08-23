use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use vm_core::{vm_println, vm_success, vm_warning};
use vm_packages::{repository_urls_equivalent, SourceKind, ToolKind};

use crate::error::{VmError, VmResult};

use super::{appliance::configured_client, discovery, files::ApplianceFiles, registration};

#[derive(Default)]
pub(super) struct SourceReconcileOutcome {
    pub(super) quarantined: Vec<PathBuf>,
    pub(super) failures: Vec<String>,
}

impl SourceReconcileOutcome {
    pub(super) fn is_degraded(&self) -> bool {
        !self.quarantined.is_empty() || !self.failures.is_empty()
    }

    fn merge(&mut self, mut other: Self) {
        self.quarantined.append(&mut other.quarantined);
        self.failures.append(&mut other.failures);
    }
}

#[derive(Clone, Copy)]
enum SourcePolicy {
    ManagedShelf,
    Canonical,
}

pub(super) struct SourcePlan {
    source_count: usize,
    policy: SourcePolicy,
    pub(super) discovery: discovery::Discovery,
}

pub(super) fn prepare_source_roots(source_roots: &[String]) -> VmResult<SourcePlan> {
    let source_roots = validated_source_roots(source_roots)?;
    let discovery = if source_roots.is_empty() {
        discovery::Discovery::default()
    } else {
        discovery::discover_configured(&source_roots)?
    };
    Ok(SourcePlan {
        source_count: source_roots.len(),
        policy: SourcePolicy::ManagedShelf,
        discovery,
    })
}

pub(super) async fn prepare_canonical_sources(
    files: &ApplianceFiles,
    canonical_sources: &[String],
) -> VmResult<SourcePlan> {
    let discovery = if canonical_sources.is_empty() {
        discovery::Discovery::default()
    } else {
        let client = configured_client(files)?;
        let (packages, tools) = tokio::try_join!(client.package_definitions(), client.tools())?;
        discovery::discover_canonical(canonical_sources, &packages, &tools)
    };
    Ok(SourcePlan {
        source_count: canonical_sources.len(),
        policy: SourcePolicy::Canonical,
        discovery,
    })
}

pub(super) async fn prepare_sources(
    files: &ApplianceFiles,
    settings: &vm_config::PackageInfrastructureSettings,
) -> VmResult<[SourcePlan; 2]> {
    Ok([
        prepare_source_roots(&settings.source_roots)?,
        prepare_canonical_sources(files, &settings.canonical_sources).await?,
    ])
}

pub(super) async fn reconcile_source_plans(
    files: &ApplianceFiles,
    plans: [SourcePlan; 2],
) -> VmResult<SourceReconcileOutcome> {
    let mut outcome = SourceReconcileOutcome::default();
    for plan in plans {
        outcome.merge(reconcile_sources(files, plan).await?);
    }
    Ok(outcome)
}

async fn reconcile_sources(
    files: &ApplianceFiles,
    plan: SourcePlan,
) -> VmResult<SourceReconcileOutcome> {
    if plan.source_count == 0 {
        return Ok(SourceReconcileOutcome::default());
    }
    let source_kind = match (plan.policy, plan.source_count) {
        (SourcePolicy::ManagedShelf, 1) => "managed package shelf",
        (SourcePolicy::ManagedShelf, _) => "managed package shelves",
        (SourcePolicy::Canonical, 1) => "read-only canonical source",
        (SourcePolicy::Canonical, _) => "read-only canonical sources",
    };
    vm_println!("Reconciling {} {source_kind}", plan.source_count);
    let mut outcome = SourceReconcileOutcome::default();
    for failure in plan.discovery.failures {
        vm_warning!("Source discovery failed: {}", failure.message);
        match plan.policy {
            SourcePolicy::ManagedShelf => {
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
            SourcePolicy::Canonical => outcome.failures.push(format!(
                "{}; repair {} manually or remove it from packages.canonical_sources",
                failure.message,
                failure.repository.display()
            )),
        }
    }
    outcome.failures.extend(
        registration::apply_registration(files, plan.discovery.packages, plan.discovery.tools)
            .await?,
    );
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
    if !repository_is_clean_and_committed(repository) {
        return Err(VmError::validation(
            format!(
                "Refusing to quarantine {} because it has no committed clean state",
                repository.display()
            ),
            Some("Commit or move the repository outside the managed package shelf, then retry"),
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

fn repository_is_clean_and_committed(repository: &Path) -> bool {
    let Ok(repository) = git2::Repository::open(repository) else {
        return false;
    };
    if repository
        .head()
        .and_then(|head| head.peel_to_commit())
        .is_err()
    {
        return false;
    }
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    repository
        .statuses(Some(&mut options))
        .is_ok_and(|statuses| statuses.is_empty())
}

pub(super) fn has_quarantined_sources(source_roots: &[String]) -> bool {
    source_roots
        .iter()
        .any(|root| quarantine_has_content(&Path::new(root).join(".vm-quarantine")))
}

fn quarantine_has_content(path: &Path) -> bool {
    fs::read_dir(path).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            let path = entry.path();
            !path.is_dir() || path.join(".git").exists() || quarantine_has_content(&path)
        })
    })
}

fn prune_restored_quarantine(source_root: &Path, repository: &Path) -> VmResult<()> {
    let quarantine = source_root.join(".vm-quarantine");
    let Some(mut directory) = repository.parent() else {
        return Ok(());
    };
    while directory.starts_with(&quarantine) {
        match fs::remove_dir(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                if fs::read_dir(directory).is_ok_and(|mut entries| entries.next().is_some()) {
                    break;
                }
                return Err(VmError::filesystem(
                    error,
                    directory.display().to_string(),
                    "prune empty package quarantine",
                ));
            }
        }
        if directory == quarantine {
            break;
        }
        let Some(parent) = directory.parent() else {
            break;
        };
        directory = parent;
    }
    Ok(())
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
    for tool in tools {
        registered.insert(
            (
                tool.name,
                match tool.kind {
                    ToolKind::Binary => "binary",
                    ToolKind::Collection => "collection",
                },
            ),
            tool.repository,
        );
    }

    let mut outcome = SourceReconcileOutcome::default();
    for root in validated_source_roots(source_roots)? {
        let source_root = PathBuf::from(&root);
        for repository in discovery::quarantined_repositories(&source_root)? {
            let result = (|| -> VmResult<PathBuf> {
                let (name, kind) = discovery::source_identity(&repository)?;
                let kind = match kind {
                    SourceKind::Package => "package",
                    SourceKind::ToolBinary => "binary",
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
                    if let Err(error) = prune_restored_quarantine(&source_root, &repository) {
                        vm_warning!("Could not prune repaired source quarantine: {error}");
                    }
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
        let normalized_current = discovery::normalize_repository_url(&current)?;
        let normalized_expected = discovery::normalize_repository_url(expected_remote)?;
        if !repository_urls_equivalent(&normalized_current, &normalized_expected) {
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        has_quarantined_sources, prune_restored_quarantine, quarantine_repository,
        repair_repository_git, validated_source_roots,
    };

    fn committed_repository(path: &std::path::Path) {
        let repository = git2::Repository::init(path).unwrap();
        fs::write(path.join("keep.txt"), "preserved").unwrap();
        let mut index = repository.index().unwrap();
        index.add_path(std::path::Path::new("keep.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repository.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("VM Test", "vm@example.invalid").unwrap();
        repository
            .commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
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
        fs::create_dir_all(&repository).unwrap();
        committed_repository(&repository);

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
    fn uncommitted_repository_is_never_moved_to_quarantine() {
        let directory = tempfile::tempdir().unwrap();
        let source_root = directory.path().join("packages");
        let repository = source_root.join("access");
        fs::create_dir_all(&repository).unwrap();
        git2::Repository::init(&repository).unwrap();
        fs::write(repository.join("package.json"), r#"{"name":"access"}"#).unwrap();

        let error = quarantine_repository(&source_root, &repository).unwrap_err();

        assert!(error.to_string().contains("no committed clean state"));
        assert!(repository.join("package.json").is_file());
        assert!(!source_root.join(".vm-quarantine/access").exists());
    }

    #[test]
    fn restored_quarantine_is_pruned_and_empty_scaffolding_is_healthy() {
        let directory = tempfile::tempdir().unwrap();
        let source_root = directory.path().join("packages");
        let repository = source_root.join(".vm-quarantine/nested/repaired");
        fs::create_dir_all(&repository).unwrap();
        fs::write(repository.join("keep.txt"), "preserved").unwrap();
        assert!(has_quarantined_sources(&[source_root
            .to_string_lossy()
            .into_owned()]));

        let restored = source_root.join("nested/repaired");
        fs::create_dir_all(restored.parent().unwrap()).unwrap();
        fs::rename(&repository, &restored).unwrap();
        prune_restored_quarantine(&source_root, &repository).unwrap();

        assert!(!source_root.join(".vm-quarantine").exists());
        assert!(!has_quarantined_sources(&[source_root
            .to_string_lossy()
            .into_owned()]));
        assert_eq!(
            fs::read_to_string(restored.join("keep.txt")).unwrap(),
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

    #[test]
    fn deterministic_git_repair_accepts_equivalent_github_transports() {
        let directory = tempfile::tempdir().unwrap();
        let repository = git2::Repository::init(directory.path()).unwrap();
        fs::write(directory.path().join("package.json"), r#"{"name":"demo"}"#).unwrap();
        repository
            .remote("origin", "git@github.com:goobits/demo.git")
            .unwrap();

        repair_repository_git(directory.path(), "https://github.com/goobits/demo.git").unwrap();

        assert_eq!(
            repository.find_remote("origin").unwrap().url(),
            Ok("git@github.com:goobits/demo.git")
        );
    }
}
