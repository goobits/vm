use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use vm_packages::{
    PackageDefinition, PackageEcosystem, RegisterPackage, RegisterTool, ToolDefinition,
};
use walkdir::{DirEntry, WalkDir};

use crate::error::{VmError, VmResult};

mod source_identity;

#[cfg(test)]
use source_identity::TOOL_MANIFEST;
use source_identity::{discover_package, exact_repository, is_tool_repository};
pub(super) use source_identity::{
    discover_tool, normalize_repository_url, package_name, source_identity, tool_manifest,
};

#[derive(Default)]
pub(super) struct Discovery {
    pub(super) packages: Vec<RegisterPackage>,
    pub(super) tools: Vec<RegisterTool>,
    pub(super) failures: Vec<DiscoveryFailure>,
}

#[derive(Debug)]
pub(super) struct DiscoveryFailure {
    pub(super) source_root: PathBuf,
    pub(super) repository: PathBuf,
    pub(super) message: String,
}

pub(super) struct LocalSource {
    pub(super) root: PathBuf,
    pub(super) request: SourceRequest,
}

pub(super) enum SourceRequest {
    Package(RegisterPackage),
    Tool(RegisterTool),
}

#[derive(Default)]
struct RepositoryRoots {
    packages: BTreeSet<PathBuf>,
    tools: BTreeSet<PathBuf>,
}

pub(super) fn discover(
    targets: &[String],
    recursive: bool,
    ecosystem: Option<PackageEcosystem>,
    branch: Option<&str>,
) -> VmResult<Discovery> {
    discover_with_policy(targets, recursive, ecosystem, branch, false)
}

pub(super) fn discover_configured(targets: &[String]) -> VmResult<Discovery> {
    let repositories = configured_repository_paths(targets)?;
    let mut discovery = Discovery::default();
    for (source_root, repository) in repositories {
        let result =
            discover_source(&repository, None, None, true).map(|source| discovery.push(source));
        if let Err(error) = result {
            discovery.failures.push(DiscoveryFailure {
                source_root,
                repository: repository.clone(),
                message: format!("{}: {error}", repository.display()),
            });
        }
    }
    Ok(discovery)
}

/// Discover explicit local registrations and retain their physical roots.
pub(super) fn discover_local(
    targets: &[String],
    recursive: bool,
    ecosystem: Option<PackageEcosystem>,
    branch: Option<&str>,
) -> VmResult<Vec<LocalSource>> {
    let roots = repository_roots(targets, recursive, false)?;
    roots
        .packages
        .into_iter()
        .map(|root| {
            discover_package(&root, ecosystem, branch, true).map(|request| LocalSource {
                root,
                request: SourceRequest::Package(request),
            })
        })
        .chain(roots.tools.into_iter().map(|root| {
            discover_tool(&root, branch, true).map(|request| LocalSource {
                root,
                request: SourceRequest::Tool(request),
            })
        }))
        .collect()
}

/// Inspect exact configured repositories independently without mutating them.
pub(super) fn discover_canonical(
    targets: &[String],
    packages: &[PackageDefinition],
    tools: &[ToolDefinition],
) -> Discovery {
    let mut discovery = Discovery::default();
    for target in targets {
        let configured = PathBuf::from(target);
        let result = if !configured.is_absolute() {
            Err(VmError::validation(
                format!("Canonical package source '{target}' is not an absolute host path"),
                Some("Re-register the repository with `vm packages register <local-path>`"),
            ))
        } else {
            fs::canonicalize(&configured)
                .map_err(|error| {
                    VmError::filesystem(error, target, "resolve canonical package source")
                })
                .and_then(|root| discover_registered_source(&root, packages, tools))
        };
        match result {
            Ok(source) => discovery.push(source),
            Err(error) => discovery.failures.push(DiscoveryFailure {
                source_root: configured.clone(),
                repository: configured,
                message: error.to_string(),
            }),
        }
    }
    discovery
}

fn discover_registered_source(
    root: &Path,
    packages: &[PackageDefinition],
    tools: &[ToolDefinition],
) -> VmResult<SourceRequest> {
    let repository = normalize_repository_url(&exact_repository(root)?.origin_url)?;
    let packages = packages
        .iter()
        .filter(|package| {
            package.workspace_release
                && vm_packages::repository_urls_equivalent(&package.repository, &repository)
        })
        .collect::<Vec<_>>();
    let tools = tools
        .iter()
        .filter(|tool| {
            tool.workspace_release
                && vm_packages::repository_urls_equivalent(&tool.repository, &repository)
        })
        .collect::<Vec<_>>();
    match (packages.as_slice(), tools.as_slice()) {
        ([package], []) => discover_package(
            root,
            Some(package.ecosystem),
            Some(&package.default_branch),
            true,
        )
        .map(SourceRequest::Package),
        ([], [tool]) => {
            discover_tool(root, Some(&tool.default_branch), true).map(SourceRequest::Tool)
        }
        ([], []) => discover_source(root, None, None, true),
        _ => Err(VmError::validation(
            format!(
                "Canonical source {} has more than one registered workspace-release identity",
                root.display()
            ),
            Some("Remove duplicate source registrations, then retry"),
        )),
    }
}

impl Discovery {
    fn push(&mut self, source: SourceRequest) {
        match source {
            SourceRequest::Package(request) => self.packages.push(request),
            SourceRequest::Tool(request) => self.tools.push(request),
        }
    }
}

fn discover_source(
    root: &Path,
    ecosystem: Option<PackageEcosystem>,
    branch: Option<&str>,
    workspace_release: bool,
) -> VmResult<SourceRequest> {
    if is_tool_repository(root)? {
        discover_tool(root, branch, workspace_release).map(SourceRequest::Tool)
    } else {
        discover_package(root, ecosystem, branch, workspace_release).map(SourceRequest::Package)
    }
}

pub(super) fn quarantined_repositories(source_root: &Path) -> VmResult<Vec<PathBuf>> {
    let quarantine = source_root.join(".vm-quarantine");
    if !quarantine.is_dir() {
        return Ok(Vec::new());
    }
    let mut repositories = Vec::new();
    for entry in WalkDir::new(&quarantine)
        .follow_links(false)
        .into_iter()
        .filter_entry(should_visit)
    {
        let entry = entry.map_err(|error| {
            VmError::validation(
                format!("Failed to scan {}: {error}", quarantine.display()),
                Some("Run `vm packages doctor --fix`"),
            )
        })?;
        if entry.file_type().is_dir() && entry.path().join(".git").exists() {
            repositories.push(entry.into_path());
        }
    }
    Ok(repositories)
}

fn configured_repository_paths(targets: &[String]) -> VmResult<BTreeSet<(PathBuf, PathBuf)>> {
    let mut repositories = BTreeSet::new();
    for target in targets {
        let root = fs::canonicalize(target).map_err(|error| {
            VmError::filesystem(error, target, "resolve package registration path")
        })?;
        if !root.is_dir() {
            return Err(VmError::validation(
                format!("Package source root {} is not a directory", root.display()),
                None::<String>,
            ));
        }
        for entry in WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_entry(should_visit)
        {
            let entry = entry.map_err(|error| {
                VmError::validation(
                    format!("Failed to scan {}: {error}", root.display()),
                    None::<String>,
                )
            })?;
            if entry.file_type().is_dir() && entry.path().join(".git").exists() {
                repositories.insert((root.clone(), entry.into_path()));
            }
        }
    }
    Ok(repositories)
}

fn discover_with_policy(
    targets: &[String],
    recursive: bool,
    ecosystem: Option<PackageEcosystem>,
    branch: Option<&str>,
    allow_empty: bool,
) -> VmResult<Discovery> {
    let roots = repository_roots(targets, recursive, allow_empty)?;
    let packages = roots
        .packages
        .iter()
        .map(|root| discover_package(root, ecosystem, branch, false))
        .collect::<VmResult<_>>()?;
    let tools = roots
        .tools
        .iter()
        .map(|root| discover_tool(root, branch, false))
        .collect::<VmResult<_>>()?;
    Ok(Discovery {
        packages,
        tools,
        failures: Vec::new(),
    })
}

fn repository_roots(
    targets: &[String],
    recursive: bool,
    allow_empty: bool,
) -> VmResult<RepositoryRoots> {
    let mut roots = RepositoryRoots::default();
    for target in targets {
        let path = fs::canonicalize(target).map_err(|error| {
            VmError::filesystem(error, target, "resolve package registration path")
        })?;
        if !path.is_dir() {
            return Err(VmError::validation(
                format!(
                    "Package registration target {} is not a directory",
                    path.display()
                ),
                None::<String>,
            ));
        }
        if recursive {
            let discovered = find_repository_roots(&path)?;
            roots.packages.extend(discovered.packages);
            roots.tools.extend(discovered.tools);
        } else if is_tool_repository(&path)? {
            roots.tools.insert(path);
        } else {
            roots.packages.insert(path);
        }
    }
    if !allow_empty && roots.packages.is_empty() && roots.tools.is_empty() {
        return Err(VmError::validation(
            "No Git package repositories were found",
            Some("Pass package repository roots, or use --recursive on their parent directory"),
        ));
    }
    Ok(roots)
}

fn find_repository_roots(root: &Path) -> VmResult<RepositoryRoots> {
    let mut repositories = RepositoryRoots::default();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(should_visit)
    {
        let entry = entry.map_err(|error| {
            VmError::validation(
                format!("Failed to scan {}: {error}", root.display()),
                None::<String>,
            )
        })?;
        if entry.file_type().is_dir() && entry.path().join(".git").exists() {
            let path = entry.into_path();
            if is_tool_repository(&path)? {
                repositories.tools.insert(path);
            } else {
                repositories.packages.insert(path);
            }
        }
    }
    Ok(repositories)
}

fn should_visit(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    if matches!(
        name.as_ref(),
        ".git"
            | ".vm-quarantine"
            | "node_modules"
            | "target"
            | ".venv"
            | "venv"
            | ".tox"
            | "dist"
            | "build"
    ) {
        return false;
    }
    !entry
        .path()
        .parent()
        .is_some_and(|parent| parent.join(".git").exists())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use git2::Repository;
    use vm_packages::{PackageDefinition, PackageEcosystem};

    use super::{
        discover, discover_canonical, discover_configured, discover_local,
        normalize_repository_url, SourceRequest, TOOL_MANIFEST,
    };

    fn package(root: &Path, directory: &str, manifest: &str, content: &str) -> PathBuf {
        let path = root.join(directory);
        fs::create_dir_all(&path).unwrap();
        let repository = Repository::init(&path).unwrap();
        repository
            .remote("origin", &format!("git@example.com:shared/{directory}.git"))
            .unwrap();
        repository
            .reference_symbolic(
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
                true,
                "test default branch",
            )
            .unwrap();
        fs::write(path.join(manifest), content).unwrap();
        path
    }

    #[test]
    fn recursively_discovers_each_supported_ecosystem() {
        let directory = tempfile::tempdir().unwrap();
        package(
            directory.path(),
            "auth-js",
            "package.json",
            r#"{"name":"@shared/auth"}"#,
        );
        package(
            directory.path(),
            "auth-rs",
            "Cargo.toml",
            "[package]\nname = \"shared-auth\"\nversion = \"1.0.0\"\n",
        );
        package(
            directory.path(),
            "auth-py",
            "pyproject.toml",
            "[project]\nname = \"shared_auth\"\nversion = \"1.0.0\"\n",
        );

        let discovery = discover(
            &[directory.path().to_string_lossy().into_owned()],
            true,
            None,
            None,
        )
        .unwrap();
        let packages = discovery.packages;

        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].name, "@shared/auth");
        assert_eq!(packages[0].ecosystem, PackageEcosystem::Npm);
        assert_eq!(
            packages[0].repository,
            "ssh://git@example.com/shared/auth-js.git"
        );
        assert_eq!(packages[1].name, "shared_auth");
        assert_eq!(packages[1].ecosystem, PackageEcosystem::Python);
        assert_eq!(packages[2].name, "shared-auth");
        assert_eq!(packages[2].ecosystem, PackageEcosystem::Cargo);
        assert!(packages
            .iter()
            .all(|package| package.default_branch == "main"));
    }

    #[test]
    fn ecosystem_override_resolves_an_ambiguous_repository() {
        let directory = tempfile::tempdir().unwrap();
        let path = package(
            directory.path(),
            "mixed",
            "package.json",
            r#"{"name":"mixed-js"}"#,
        );
        fs::write(
            path.join("Cargo.toml"),
            "[package]\nname = \"mixed-rs\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        let target = path.to_string_lossy().into_owned();

        assert!(discover(std::slice::from_ref(&target), false, None, None).is_err());
        let discovery = discover(
            &[target],
            false,
            Some(PackageEcosystem::Cargo),
            Some("stable"),
        )
        .unwrap();
        let packages = discovery.packages;

        assert_eq!(packages[0].name, "mixed-rs");
        assert_eq!(packages[0].default_branch, "stable");
        assert_eq!(
            normalize_repository_url("github.com:shared/mixed.git").unwrap(),
            "ssh://github.com/shared/mixed.git"
        );
    }

    #[test]
    fn recursive_discovery_separates_tool_repositories() {
        let directory = tempfile::tempdir().unwrap();
        let tool = package(
            directory.path(),
            "agent-skills",
            "package.json",
            r#"{"name":"@shared/agent-skills"}"#,
        );
        fs::write(tool.join(TOOL_MANIFEST), "schema: 1\nkind: collection\n").unwrap();

        let discovery = discover(
            &[directory.path().to_string_lossy().into_owned()],
            true,
            None,
            None,
        )
        .unwrap();

        assert!(discovery.packages.is_empty());
        assert_eq!(discovery.tools.len(), 1);
        assert_eq!(discovery.tools[0].name, "agent-skills");
        assert_eq!(discovery.tools[0].kind, vm_packages::ToolKind::Collection);
    }

    #[test]
    fn only_configured_source_shelves_may_be_empty() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().to_string_lossy().into_owned();

        assert!(discover(std::slice::from_ref(&target), true, None, None).is_err());
        let configured = discover_configured(&[target]).unwrap();

        assert!(configured.packages.is_empty());
        assert!(configured.tools.is_empty());
        assert!(configured.failures.is_empty());
    }

    #[test]
    fn configured_discovery_isolates_invalid_repositories() {
        let directory = tempfile::tempdir().unwrap();
        package(
            directory.path(),
            "auth",
            "package.json",
            r#"{"name":"@shared/auth","version":"1.0.0"}"#,
        );
        let broken = directory.path().join("broken");
        fs::create_dir(&broken).unwrap();
        let repository = Repository::init(&broken).unwrap();
        repository
            .remote("origin", "git@example.com:shared/broken.git")
            .unwrap();

        let configured =
            discover_configured(&[directory.path().to_string_lossy().into_owned()]).unwrap();

        assert_eq!(configured.packages.len(), 1);
        assert_eq!(configured.failures.len(), 1);
        assert!(configured.failures[0].message.contains("broken"));
    }

    #[test]
    fn exact_discovery_retains_physical_roots_and_release_attestation() {
        let directory = tempfile::tempdir().unwrap();
        let source = package(
            directory.path(),
            "typemill",
            "package.json",
            r#"{"name":"typemill","version":"1.0.0"}"#,
        );
        let discovered =
            discover_local(&[source.to_string_lossy().into_owned()], false, None, None).unwrap();

        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].root, source.canonicalize().unwrap());
        let SourceRequest::Package(request) = &discovered[0].request else {
            panic!("expected package registration");
        };
        assert!(request.workspace_release);
    }

    #[test]
    fn exact_tool_registration_requires_the_git_root() {
        let directory = tempfile::tempdir().unwrap();
        let source = package(
            directory.path(),
            "codeatlas",
            "package.json",
            r#"{"name":"codeatlas","version":"1.0.0"}"#,
        );
        let nested = source.join("packages/codeatlas");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join(TOOL_MANIFEST), "schema: 1\nkind: collection\n").unwrap();

        let error = discover_local(&[nested.to_string_lossy().into_owned()], false, None, None)
            .err()
            .unwrap();

        assert!(error.to_string().contains("not a Git repository root"));
        assert_eq!(
            error.hint().unwrap(),
            format!("Use {} instead", source.display())
        );
    }

    #[test]
    fn canonical_discovery_keeps_healthy_sources_when_one_is_missing() {
        let directory = tempfile::tempdir().unwrap();
        let source = package(
            directory.path(),
            "typemill",
            "package.json",
            r#"{"name":"typemill","version":"1.0.0"}"#,
        );
        let missing = directory.path().join("missing");

        let discovery = discover_canonical(
            &[
                source.to_string_lossy().into_owned(),
                missing.to_string_lossy().into_owned(),
            ],
            &[],
            &[],
        );

        assert_eq!(discovery.packages.len(), 1);
        assert!(discovery.packages[0].workspace_release);
        assert_eq!(discovery.failures.len(), 1);
        assert!(!missing.exists());
    }

    #[test]
    fn canonical_discovery_reuses_registered_branch_and_ecosystem() {
        let directory = tempfile::tempdir().unwrap();
        let source = package(
            directory.path(),
            "multi-package",
            "Cargo.toml",
            "[package]\nname = \"multi-package\"\nversion = \"1.0.0\"\n",
        );
        fs::write(
            source.join("package.json"),
            r#"{"name":"multi-package-js","version":"1.0.0"}"#,
        )
        .unwrap();
        let registered = PackageDefinition {
            name: "multi-package".into(),
            ecosystem: PackageEcosystem::Cargo,
            repository: "https://example.com/shared/multi-package.git".into(),
            default_branch: "release".into(),
            workspace_release: true,
            registered_at: chrono::Utc::now(),
        };
        let repository = git2::Repository::open(&source).unwrap();
        repository
            .remote_set_url("origin", &registered.repository)
            .unwrap();

        let discovery =
            discover_canonical(&[source.to_string_lossy().into_owned()], &[registered], &[]);

        assert!(discovery.failures.is_empty());
        assert_eq!(discovery.packages.len(), 1);
        assert_eq!(discovery.packages[0].ecosystem, PackageEcosystem::Cargo);
        assert_eq!(discovery.packages[0].default_branch, "release");
    }

    #[test]
    fn invalid_tool_manifest_fails_discovery() {
        let directory = tempfile::tempdir().unwrap();
        let tool = package(
            directory.path(),
            "broken-tool",
            "package.json",
            r#"{"name":"broken-tool"}"#,
        );
        fs::write(tool.join(TOOL_MANIFEST), "kind: plugin\n").unwrap();

        let error = discover(
            &[directory.path().to_string_lossy().into_owned()],
            true,
            None,
            None,
        )
        .err()
        .unwrap()
        .to_string();

        assert!(error.contains("Invalid"));
        assert!(error.contains(TOOL_MANIFEST));
    }
}
