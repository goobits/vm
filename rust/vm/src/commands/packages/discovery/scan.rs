use super::*;
use std::collections::BTreeSet;
use walkdir::{DirEntry, WalkDir};

#[derive(Default)]
pub(super) struct RepositoryRoots {
    pub(super) packages: BTreeSet<PathBuf>,
    pub(super) tools: BTreeSet<PathBuf>,
}

pub(super) fn discover_source(
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

pub(in crate::commands::packages) fn quarantined_repositories(
    source_root: &Path,
) -> VmResult<Vec<PathBuf>> {
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
                Some("Run `vm packages service doctor --fix`"),
            )
        })?;
        if entry.file_type().is_dir() && entry.path().join(".git").exists() {
            repositories.push(entry.into_path());
        }
    }
    Ok(repositories)
}

pub(super) fn configured_repository_paths(
    targets: &[String],
) -> VmResult<BTreeSet<(PathBuf, PathBuf)>> {
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

pub(super) fn discover_with_policy(
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
        conflicts: Vec::new(),
    })
}

pub(super) fn repository_roots(
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
    if entry.path().join(".vm-packages-ignore").is_file() {
        return false;
    }
    !entry
        .path()
        .parent()
        .is_some_and(|parent| parent.join(".git").exists())
}
