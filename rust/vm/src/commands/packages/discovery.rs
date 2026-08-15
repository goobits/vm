use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use vm_config::detector::{git::detect_repository, ProjectFacts};
use vm_packages::SourceKind;
use vm_packages::{PackageEcosystem, RegisterPackage, RegisterTool, ToolSourceManifest};
use walkdir::{DirEntry, WalkDir};

use crate::error::{VmError, VmResult};

const TOOL_MANIFEST: &str = "vm-tool.yaml";

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
        let result = match is_tool_repository(&repository) {
            Ok(true) => {
                discover_tool(&repository, None, true).map(|tool| discovery.tools.push(tool))
            }
            Ok(false) => discover_one(&repository, None, None, true)
                .map(|package| discovery.packages.push(package)),
            Err(error) => Err(error),
        };
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

pub(super) fn source_identity(root: &Path) -> VmResult<(String, SourceKind)> {
    if is_tool_repository(root)? {
        let name = root
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.trim().is_empty())
            .ok_or_else(|| {
                VmError::validation(
                    format!("Tool repository {} has no usable name", root.display()),
                    Some("Rename the repository directory, then rerun `vm packages doctor --fix`"),
                )
            })?;
        let kind = match tool_manifest(root)?.kind {
            vm_packages::ToolKind::Binary => SourceKind::ToolBinary,
            vm_packages::ToolKind::Collection => SourceKind::ToolCollection,
        };
        return Ok((name.to_string(), kind));
    }
    let facts = ProjectFacts::detect(root);
    let ecosystem = resolve_ecosystem(root, &facts, None)?;
    Ok((package_name(root, ecosystem)?, SourceKind::Package))
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
        .map(|root| discover_one(root, ecosystem, branch, false))
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

fn is_tool_repository(root: &Path) -> VmResult<bool> {
    let path = root.join(TOOL_MANIFEST);
    if !path.is_file() {
        return Ok(false);
    }
    tool_manifest(root)?;
    Ok(true)
}

pub(super) fn tool_manifest(root: &Path) -> VmResult<ToolSourceManifest> {
    let path = root.join(TOOL_MANIFEST);
    let manifest =
        serde_yaml_ng::from_str::<ToolSourceManifest>(&read_manifest(&path)?).map_err(|error| {
            VmError::validation(
                format!("Invalid {}: {error}", path.display()),
                Some("Fix vm-tool.yaml, then rerun the same command"),
            )
        })?;
    manifest.validate().map_err(|error| {
        VmError::validation(
            format!("Invalid {}: {error}", path.display()),
            Some("Fix vm-tool.yaml, then rerun the same command"),
        )
    })?;
    Ok(manifest)
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

fn discover_one(
    root: &Path,
    override_ecosystem: Option<PackageEcosystem>,
    branch: Option<&str>,
    workspace_release: bool,
) -> VmResult<RegisterPackage> {
    let repository = detect_repository(root).map_err(VmError::from)?;
    if repository.root != root {
        return Err(VmError::validation(
            format!("{} is not a Git repository root", root.display()),
            Some(format!("Use {} instead", repository.root.display())),
        ));
    }

    let facts = ProjectFacts::detect(root);
    let ecosystem = resolve_ecosystem(root, &facts, override_ecosystem)?;
    let request = RegisterPackage {
        name: package_name(root, ecosystem)?,
        ecosystem,
        repository: normalize_repository_url(&repository.origin_url)?,
        default_branch: branch
            .map(str::to_string)
            .or(repository.default_branch)
            .unwrap_or_else(|| "main".into()),
        workspace_release,
    };
    request.validate().map_err(|error| {
        VmError::validation(
            format!("Invalid package metadata in {}: {error}", root.display()),
            None::<String>,
        )
    })?;
    Ok(request)
}

fn discover_tool(
    root: &Path,
    branch: Option<&str>,
    workspace_release: bool,
) -> VmResult<RegisterTool> {
    let manifest = tool_manifest(root)?;
    let repository = detect_repository(root).map_err(VmError::from)?;
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            VmError::validation(
                format!("Tool repository {} has no usable name", root.display()),
                None::<String>,
            )
        })?
        .to_string();
    let request = RegisterTool {
        name,
        kind: manifest.kind,
        repository: normalize_repository_url(&repository.origin_url)?,
        default_branch: branch
            .map(str::to_string)
            .or(repository.default_branch)
            .unwrap_or_else(|| "main".into()),
        workspace_release,
    };
    request.validate().map_err(|error| {
        VmError::validation(
            format!("Invalid tool metadata in {}: {error}", root.display()),
            None::<String>,
        )
    })?;
    Ok(request)
}

fn resolve_ecosystem(
    root: &Path,
    facts: &ProjectFacts,
    requested: Option<PackageEcosystem>,
) -> VmResult<PackageEcosystem> {
    if let Some(ecosystem) = requested {
        if has_manifest(facts, ecosystem) {
            return Ok(ecosystem);
        }
        return Err(VmError::validation(
            format!("{} has no {} package manifest", root.display(), ecosystem),
            None::<String>,
        ));
    }

    let detected = PackageEcosystem::ALL
        .into_iter()
        .filter(|ecosystem| has_manifest(facts, *ecosystem))
        .collect::<Vec<_>>();
    match detected.as_slice() {
        [ecosystem] => Ok(*ecosystem),
        [] => Err(VmError::validation(
            format!("{} has no supported package manifest", root.display()),
            Some("Expected package.json, Cargo.toml, or pyproject.toml"),
        )),
        _ => Err(VmError::validation(
            format!("{} contains multiple package ecosystems", root.display()),
            Some("Pass --ecosystem npm, cargo, or python to select one"),
        )),
    }
}

fn has_manifest(facts: &ProjectFacts, ecosystem: PackageEcosystem) -> bool {
    match ecosystem {
        PackageEcosystem::Npm => facts.package_json,
        PackageEcosystem::Cargo => facts.cargo_toml,
        PackageEcosystem::Python => facts.pyproject_toml,
    }
}

pub(super) fn package_name(root: &Path, ecosystem: PackageEcosystem) -> VmResult<String> {
    let (manifest, name) = match ecosystem {
        PackageEcosystem::Npm => {
            let path = root.join("package.json");
            let value: serde_json::Value =
                serde_json::from_str(&read_manifest(&path)?).map_err(|error| {
                    VmError::validation(
                        format!("Invalid {}: {error}", path.display()),
                        None::<String>,
                    )
                })?;
            (
                path,
                value
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            )
        }
        PackageEcosystem::Cargo => {
            let path = root.join("Cargo.toml");
            let value: toml::Value = toml::from_str(&read_manifest(&path)?).map_err(|error| {
                VmError::validation(
                    format!("Invalid {}: {error}", path.display()),
                    None::<String>,
                )
            })?;
            let name = value
                .get("package")
                .and_then(|package| package.get("name"))
                .and_then(toml::Value::as_str)
                .map(str::to_string);
            (path, name)
        }
        PackageEcosystem::Python => {
            let path = root.join("pyproject.toml");
            let value: toml::Value = toml::from_str(&read_manifest(&path)?).map_err(|error| {
                VmError::validation(
                    format!("Invalid {}: {error}", path.display()),
                    None::<String>,
                )
            })?;
            let name = value
                .get("project")
                .and_then(|project| project.get("name"))
                .or_else(|| {
                    value
                        .get("tool")
                        .and_then(|tool| tool.get("poetry"))
                        .and_then(|poetry| poetry.get("name"))
                })
                .and_then(toml::Value::as_str)
                .map(str::to_string);
            (path, name)
        }
    };
    name.filter(|name| !name.trim().is_empty()).ok_or_else(|| {
        VmError::validation(
            format!("{} does not declare a package name", manifest.display()),
            None::<String>,
        )
    })
}

fn read_manifest(path: &Path) -> VmResult<String> {
    fs::read_to_string(path)
        .map_err(|error| VmError::filesystem(error, path.display().to_string(), "read manifest"))
}

pub(super) fn normalize_repository_url(value: &str) -> VmResult<String> {
    vm_packages::normalize_remote_repository_url(value).map_err(|error| {
        VmError::validation(
            format!("Invalid Git origin '{value}': {error}"),
            Some("Set origin to an HTTPS or SSH repository URL"),
        )
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use git2::Repository;
    use vm_packages::PackageEcosystem;

    use super::{discover, discover_configured, normalize_repository_url, TOOL_MANIFEST};

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
        fs::write(tool.join(TOOL_MANIFEST), "kind: collection\n").unwrap();

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
