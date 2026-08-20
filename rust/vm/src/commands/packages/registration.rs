use std::path::PathBuf;

use vm_core::{vm_println, vm_success, vm_warning};
use vm_packages::{PackageEcosystem, RegisterPackage, RegisterTool};

use crate::error::{VmError, VmResult};

use super::{appliance::configured_client, discovery, files::ApplianceFiles};

pub(super) struct RegistrationIntent {
    pub(super) targets: Vec<String>,
    pub(super) ecosystem: Option<String>,
    pub(super) repository: Option<String>,
    pub(super) branch: Option<String>,
    pub(super) recursive: bool,
}

pub(super) async fn register(files: &ApplianceFiles, intent: RegistrationIntent) -> VmResult<()> {
    let ecosystem = intent
        .ecosystem
        .as_deref()
        .map(str::parse::<PackageEcosystem>)
        .transpose()
        .map_err(|error| VmError::validation(error.to_string(), None::<String>))?;
    if let Some(repository) = intent.repository {
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
            workspace_release: false,
        };
        request
            .validate()
            .map_err(|error| VmError::validation(error.to_string(), None::<String>))?;
        return registration_result(apply_registration(files, vec![request], Vec::new()).await?);
    }

    let sources = discovery::discover_local(
        &intent.targets,
        intent.recursive,
        ecosystem,
        intent.branch.as_deref(),
    )?;
    let client = configured_client(files)?;
    let mut failures = Vec::new();
    let mut registered = Vec::new();
    for source in sources {
        match apply_source(&client, source.request).await {
            Ok(()) => registered.push(source.root),
            Err(failure) => failures.push(failure),
        }
    }
    remember_canonical_sources(&registered)?;
    registration_result(failures)
}

pub(super) async fn apply_registration(
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
        if let Err(failure) =
            apply_source(&client, discovery::SourceRequest::Package(request)).await
        {
            failures.push(failure);
        }
    }
    for request in tools {
        if let Err(failure) = apply_source(&client, discovery::SourceRequest::Tool(request)).await {
            failures.push(failure);
        }
    }
    Ok(failures)
}

async fn apply_source(
    client: &vm_packages::PackageInfrastructureClient,
    request: discovery::SourceRequest,
) -> Result<(), String> {
    match request {
        discovery::SourceRequest::Package(request) => {
            let name = request.name.clone();
            match client.register_package(&request).await {
                Ok(package) => {
                    vm_success!("Registered {} ({})", package.name, package.ecosystem);
                    vm_println!("Repository: {}", package.repository);
                    Ok(())
                }
                Err(error) => {
                    vm_warning!("Could not register package {name}: {error}");
                    Err(format!("package {name}: {error}"))
                }
            }
        }
        discovery::SourceRequest::Tool(request) => {
            let name = request.name.clone();
            match client.register_tool(&request).await {
                Ok(tool) => {
                    vm_success!("Registered tool {} ({:?})", tool.name, tool.kind);
                    vm_println!("Repository: {}", tool.repository);
                    Ok(())
                }
                Err(error) => {
                    vm_warning!("Could not register tool {name}: {error}");
                    Err(format!("tool {name}: {error}"))
                }
            }
        }
    }
}

fn remember_canonical_sources(paths: &[PathBuf]) -> VmResult<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut global = vm_config::GlobalConfig::load()?;
    merge_canonical_sources(&mut global.packages.canonical_sources, paths);
    global.save().map_err(VmError::from)
}

fn merge_canonical_sources(existing: &mut Vec<String>, paths: &[PathBuf]) {
    existing.extend(paths.iter().map(|path| path.to_string_lossy().into_owned()));
    existing.sort();
    existing.dedup();
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

#[cfg(test)]
mod tests {
    use super::merge_canonical_sources;

    #[test]
    fn canonical_sources_are_persisted_in_stable_deduplicated_order() {
        let mut existing = vec!["/srv/projects/typemill".into()];
        merge_canonical_sources(
            &mut existing,
            &[
                "/srv/projects/typemill".into(),
                "/srv/projects/codeatlas".into(),
            ],
        );

        assert_eq!(
            existing,
            ["/srv/projects/codeatlas", "/srv/projects/typemill"]
        );
    }
}
