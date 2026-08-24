use std::fs;
use std::path::Path;

use vm_config::detector::{
    git::{detect_repository, RepositoryFacts},
    ProjectFacts,
};
use vm_packages::{
    PackageEcosystem, RegisterPackage, RegisterTool, SourceKind, ToolSourceManifest,
};

use crate::error::{VmError, VmResult};

pub(super) const TOOL_MANIFEST: &str = "vm-tool.yaml";

pub(in crate::commands::packages) fn source_identity(
    root: &Path,
) -> VmResult<(String, SourceKind)> {
    if is_tool_repository(root)? {
        let name = repository_name(
            root,
            Some("Rename the repository directory, then rerun `vm packages doctor --fix`"),
        )?;
        let kind = match tool_manifest(root)?.kind {
            vm_packages::ToolKind::Binary => SourceKind::ToolBinary,
            vm_packages::ToolKind::Collection => SourceKind::ToolCollection,
        };
        return Ok((name, kind));
    }
    let facts = ProjectFacts::detect(root);
    let ecosystem = resolve_ecosystem(root, &facts, None)?;
    Ok((package_name(root, ecosystem)?, SourceKind::Package))
}

pub(super) fn is_tool_repository(root: &Path) -> VmResult<bool> {
    let path = root.join(TOOL_MANIFEST);
    if !path.is_file() {
        return Ok(false);
    }
    tool_manifest(root)?;
    Ok(true)
}

pub(in crate::commands::packages) fn tool_manifest(root: &Path) -> VmResult<ToolSourceManifest> {
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

pub(super) fn discover_package(
    root: &Path,
    override_ecosystem: Option<PackageEcosystem>,
    branch: Option<&str>,
    workspace_release: bool,
) -> VmResult<RegisterPackage> {
    let repository = exact_repository(root)?;
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

pub(in crate::commands::packages) fn discover_tool(
    root: &Path,
    branch: Option<&str>,
    workspace_release: bool,
) -> VmResult<RegisterTool> {
    let manifest = tool_manifest(root)?;
    let repository = exact_repository(root)?;
    let request = RegisterTool {
        name: repository_name(root, None)?,
        kind: manifest.kind,
        repository: normalize_repository_url(&repository.origin_url)?,
        default_branch: branch
            .map(str::to_string)
            .or(repository.default_branch)
            .unwrap_or_else(|| "main".into()),
        build_sources: manifest
            .build_sources
            .into_iter()
            .map(|source| source.name)
            .collect(),
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

fn repository_name(root: &Path, hint: Option<&str>) -> VmResult<String> {
    root.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            VmError::validation(
                format!("Tool repository {} has no usable name", root.display()),
                hint.map(str::to_string),
            )
        })
}

pub(super) fn exact_repository(root: &Path) -> VmResult<RepositoryFacts> {
    let repository = detect_repository(root).map_err(VmError::from)?;
    if repository.root != root {
        return Err(VmError::validation(
            format!("{} is not a Git repository root", root.display()),
            Some(format!("Use {} instead", repository.root.display())),
        ));
    }
    Ok(repository)
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

pub(in crate::commands::packages) fn package_name(
    root: &Path,
    ecosystem: PackageEcosystem,
) -> VmResult<String> {
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

pub(in crate::commands::packages) fn normalize_repository_url(value: &str) -> VmResult<String> {
    vm_packages::normalize_remote_repository_url(value).map_err(|error| {
        VmError::validation(
            format!("Invalid Git origin '{value}': {error}"),
            Some("Set origin to an HTTPS or SSH repository URL"),
        )
    })
}
