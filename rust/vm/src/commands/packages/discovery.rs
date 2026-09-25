use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use vm_packages::{
    repository_urls_equivalent, PackageDefinition, PackageEcosystem, RegisterPackage, RegisterTool,
    SourceKind, ToolDefinition, ToolKind,
};

use crate::error::{VmError, VmResult};

mod scan;
mod source_identity;

pub(super) use scan::quarantined_repositories;
use scan::{configured_repository_paths, discover_source, discover_with_policy, repository_roots};

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
    pub(super) conflicts: Vec<DiscoveryConflict>,
}

#[derive(Debug)]
pub(super) struct DiscoveryFailure {
    pub(super) source_root: PathBuf,
    pub(super) repository: PathBuf,
    pub(super) message: String,
}

#[derive(Debug)]
pub(super) struct DiscoveryConflict {
    pub(super) identity: String,
    pub(super) sources: Vec<(PathBuf, SourceRequest)>,
}

pub(super) struct LocalSource {
    pub(super) root: PathBuf,
    pub(super) request: SourceRequest,
}

#[derive(Debug)]
pub(super) enum SourceRequest {
    Package(RegisterPackage),
    Tool(RegisterTool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RegisteredSource {
    pub(super) name: String,
    pub(super) kind: SourceKind,
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
    let mut candidates = BTreeMap::<String, Vec<(PathBuf, SourceRequest)>>::new();
    for (source_root, repository) in repositories {
        match discover_source(&repository, None, None, true) {
            Ok(source) => candidates
                .entry(source.identity_key())
                .or_default()
                .push((repository, source)),
            Err(error) => discovery.failures.push(DiscoveryFailure {
                source_root,
                repository: repository.clone(),
                message: format!("{}: {error}", repository.display()),
            }),
        }
    }
    for (identity, mut sources) in candidates {
        sources.sort_by(|left, right| left.0.cmp(&right.0));
        let first_repository = sources[0].1.repository();
        if sources
            .iter()
            .all(|(_, source)| repository_urls_equivalent(first_repository, source.repository()))
        {
            discovery.push(sources.remove(0).1);
        } else {
            discovery
                .conflicts
                .push(DiscoveryConflict { identity, sources });
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
                .and_then(|root| discover_catalog_source(&root, packages, tools))
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

fn discover_catalog_source(
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

pub(super) fn resolve_registered_source_at(
    root: &Path,
    packages: &[PackageDefinition],
    tools: &[ToolDefinition],
) -> VmResult<RegisteredSource> {
    let repository = normalize_repository_url(&exact_repository(root)?.origin_url)?;
    resolve_registered_source(root, &repository, packages, tools)
}

pub(super) fn resolve_registered_source(
    root: &Path,
    repository: &str,
    packages: &[PackageDefinition],
    tools: &[ToolDefinition],
) -> VmResult<RegisteredSource> {
    let packages = packages
        .iter()
        .filter(|package| repository_urls_equivalent(&package.repository, repository))
        .collect::<Vec<_>>();
    let tools = tools
        .iter()
        .filter(|tool| repository_urls_equivalent(&tool.repository, repository))
        .collect::<Vec<_>>();
    if packages.len() + tools.len() != 1 {
        let message = if packages.is_empty() && tools.is_empty() {
            "Workspace Git origin is not registered in the package catalog".to_string()
        } else {
            "Workspace Git origin is ambiguous in the package catalog".to_string()
        };
        return Err(VmError::validation(
            message,
            Some("Run `vm packages service doctor --fix` on the controller host"),
        ));
    }
    if let Some(package) = packages.first() {
        if !package.workspace_release {
            return Err(unattested_workspace());
        }
        let actual = package_name(root, package.ecosystem)?;
        if actual != package.name {
            return Err(VmError::validation(
                format!(
                    "Workspace package identity '{actual}' does not match registered source '{}'",
                    package.name
                ),
                Some("Run `vm packages service doctor --fix` on the controller host"),
            ));
        }
        return Ok(RegisteredSource {
            name: package.name.clone(),
            kind: SourceKind::Package,
        });
    }
    let tool = tools[0];
    if !tool.workspace_release {
        return Err(unattested_workspace());
    }
    let manifest = tool_manifest(root)?;
    if manifest.kind != tool.kind {
        return Err(VmError::validation(
            "Workspace tool kind does not match its registered catalog identity",
            Some("Run `vm packages service doctor --fix` on the controller host"),
        ));
    }
    Ok(RegisteredSource {
        name: tool.name.clone(),
        kind: match tool.kind {
            ToolKind::Binary => SourceKind::ToolBinary,
            ToolKind::Collection => SourceKind::ToolCollection,
        },
    })
}

fn unattested_workspace() -> VmError {
    VmError::validation(
        "Workspace source is not registered as a read-only canonical workspace",
        Some("Run `vm packages register <local-path>` on the controller host"),
    )
}

impl Discovery {
    fn push(&mut self, source: SourceRequest) {
        match source {
            SourceRequest::Package(request) => self.packages.push(request),
            SourceRequest::Tool(request) => self.tools.push(request),
        }
    }

    pub(super) fn prefer_registered(
        &mut self,
        packages: &[PackageDefinition],
        tools: &[ToolDefinition],
    ) {
        let conflicts = std::mem::take(&mut self.conflicts);
        for mut conflict in conflicts {
            let selected = conflict
                .sources
                .iter()
                .position(|(_, source)| source.matches_registered(packages, tools));
            if let Some(selected) = selected {
                self.push(conflict.sources.remove(selected).1);
            } else {
                self.conflicts.push(conflict);
            }
        }
    }

    pub(super) fn is_degraded(&self) -> bool {
        !self.failures.is_empty() || !self.conflicts.is_empty()
    }

    pub(super) fn diagnostics(&self) -> Vec<String> {
        self.failures
            .iter()
            .map(|failure| failure.message.clone())
            .chain(self.conflicts.iter().map(DiscoveryConflict::message))
            .collect()
    }
}

impl DiscoveryConflict {
    pub(super) fn message(&self) -> String {
        let choices = self
            .sources
            .iter()
            .map(|(path, source)| format!("{} -> {}", path.display(), source.repository()))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "Conflicting source '{}': {choices}. Keep the intended source and place an empty .vm-packages-ignore file in each archived or unwanted repository subtree, then rerun `vm packages up`",
            self.identity
        )
    }
}

impl SourceRequest {
    fn identity_key(&self) -> String {
        match self {
            Self::Package(request) => {
                format!("package:{}:{}", request.ecosystem, request.name)
            }
            Self::Tool(request) => format!("tool:{:?}:{}", request.kind, request.name),
        }
    }

    pub(super) fn repository(&self) -> &str {
        match self {
            Self::Package(request) => &request.repository,
            Self::Tool(request) => &request.repository,
        }
    }

    fn matches_registered(&self, packages: &[PackageDefinition], tools: &[ToolDefinition]) -> bool {
        match self {
            Self::Package(request) => packages.iter().any(|package| {
                package.name == request.name
                    && package.ecosystem == request.ecosystem
                    && repository_urls_equivalent(&package.repository, &request.repository)
            }),
            Self::Tool(request) => tools.iter().any(|tool| {
                tool.name == request.name
                    && tool.kind == request.kind
                    && repository_urls_equivalent(&tool.repository, &request.repository)
            }),
        }
    }
}

#[cfg(test)]
mod tests;
