use std::path::{Path, PathBuf};

use vm_config::config::{MountAccess, VmConfig};
use vm_core::{vm_println, vm_success};
use vm_packages::{PackageDefinition, ToolDefinition};

use crate::{
    commands::{command_context::load_runtime_subject, vm_ops},
    error::{VmError, VmResult},
};

use super::{appliance::configured_client, discovery, files::ApplianceFiles};

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceOwner {
    source: String,
    root: PathBuf,
    config: PathBuf,
}

pub(super) async fn open(
    files: &ApplianceFiles,
    source: String,
    profile: Option<String>,
) -> VmResult<()> {
    let global = vm_config::GlobalConfig::load()?;
    let client = configured_client(files)?;
    let (packages, tools) = tokio::try_join!(client.package_definitions(), client.tools())?;
    let owner = resolve_workspace_owner(
        &source,
        &global.packages.canonical_sources,
        &packages,
        &tools,
    )?;
    let subject = load_runtime_subject(Some(owner.config), profile, None)?;
    validate_owner_environment(&subject.config, subject.provider.name(), &owner.root)?;

    vm_success!("Opening original workspace for {}", owner.source);
    vm_println!("Host source: {}", owner.root.display());
    vm_println!("Mode: direct workspace (no checkout)");
    vm_ops::handle_ssh(
        subject.provider,
        Some(subject.target.as_str()),
        Some(PathBuf::from(".")),
        subject.config,
        subject.global_config,
    )
    .await
}

fn resolve_workspace_owner(
    source: &str,
    canonical_sources: &[String],
    packages: &[PackageDefinition],
    tools: &[ToolDefinition],
) -> VmResult<WorkspaceOwner> {
    let mut matches = Vec::new();
    for configured in canonical_sources {
        let Ok(root) = std::fs::canonicalize(configured) else {
            continue;
        };
        let Ok(registered) = discovery::resolve_registered_source_at(&root, packages, tools) else {
            continue;
        };
        if registered.name == source {
            matches.push((root, registered.name));
        }
    }

    let (root, source) = match matches.as_slice() {
        [(root, source)] => (root.clone(), source.clone()),
        [] => {
            return Err(VmError::validation(
                format!("Source '{source}' has no attested canonical workspace"),
                Some(format!(
                    "Run `vm packages checkout {source}` inside a managed VM, or register its local Git root"
                )),
            ))
        }
        _ => {
            return Err(VmError::validation(
                format!("Source '{source}' has more than one attested canonical workspace"),
                Some("Remove duplicate canonical source registrations, then retry"),
            ))
        }
    };
    let config = root.join("vm.yaml");
    if !config.is_file() {
        return Err(VmError::validation(
            format!(
                "Canonical workspace {} has no owning vm.yaml",
                root.display()
            ),
            Some(format!(
                "Use `vm packages checkout {source}` inside a managed VM for an isolated copy"
            )),
        ));
    }
    Ok(WorkspaceOwner {
        source,
        root,
        config,
    })
}

fn validate_owner_environment(config: &VmConfig, provider: &str, root: &Path) -> VmResult<()> {
    if provider != "docker" {
        return Err(VmError::validation(
            format!("Canonical workspace owner uses the '{provider}' provider, not Docker"),
            Some("Use `vm packages checkout <source>` inside a managed VM for an isolated copy"),
        ));
    }
    let project_root = config
        .project_dir()
        .map_err(VmError::from)?
        .canonicalize()
        .map_err(|error| {
            VmError::filesystem(
                error,
                root.display().to_string(),
                "resolve owning project workspace",
            )
        })?;
    if project_root != root {
        return Err(VmError::validation(
            "Owning Docker configuration does not bind the canonical source root",
            Some("Place vm.yaml at the registered Git root, then retry"),
        ));
    }
    if config
        .project
        .as_ref()
        .is_some_and(|project| project.workspace_access == MountAccess::ReadOnly)
    {
        return Err(VmError::validation(
            "Owning Docker workspace is read-only",
            Some("Use a read-write owning workspace or an isolated package checkout"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use git2::Repository;
    use vm_config::config::{MountAccess, ProjectConfig};
    use vm_packages::{PackageEcosystem, SourceKind};

    use super::*;

    fn package(root: &Path, directory: &str, repository_url: &str) -> PathBuf {
        let source = root.join(directory);
        std::fs::create_dir_all(&source).unwrap();
        let repository = Repository::init(&source).unwrap();
        repository.remote("origin", repository_url).unwrap();
        std::fs::write(
            source.join("package.json"),
            format!(r#"{{"name":"{directory}"}}"#),
        )
        .unwrap();
        std::fs::write(
            source.join("vm.yaml"),
            format!("provider: docker\nproject:\n  name: {directory}\n"),
        )
        .unwrap();
        source
    }

    fn definition(name: &str, repository: &str) -> PackageDefinition {
        PackageDefinition {
            name: name.into(),
            ecosystem: PackageEcosystem::Npm,
            repository: repository.into(),
            default_branch: "main".into(),
            workspace_release: true,
            registered_at: Utc::now(),
        }
    }

    #[test]
    fn registered_package_routes_to_its_exact_owner() {
        let directory = tempfile::tempdir().unwrap();
        let repository = "https://example.com/team/auth.git";
        let source = package(directory.path(), "auth", repository);

        let owner = resolve_workspace_owner(
            "auth",
            &[source.to_string_lossy().into_owned()],
            &[definition("auth", repository)],
            &[],
        )
        .unwrap();

        assert_eq!(owner.source, "auth");
        assert_eq!(owner.root, source.canonicalize().unwrap());
        assert_eq!(owner.config, source.join("vm.yaml").canonicalize().unwrap());
        assert_eq!(
            discovery::resolve_registered_source_at(
                &owner.root,
                &[definition("auth", repository)],
                &[],
            )
            .unwrap()
            .kind,
            SourceKind::Package
        );
    }

    #[test]
    fn missing_direct_workspace_suggests_the_explicit_checkout_path() {
        let error = resolve_workspace_owner("auth", &[], &[], &[]).unwrap_err();

        assert!(error
            .to_string()
            .contains("no attested canonical workspace"));
        assert!(error.hint().unwrap().contains("vm packages checkout auth"));
    }

    #[test]
    fn duplicate_direct_workspace_owners_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let repository = "https://example.com/team/auth.git";
        let first = package(directory.path(), "auth", repository);
        let second = package(directory.path(), "auth-copy", repository);
        std::fs::write(second.join("package.json"), r#"{"name":"auth"}"#).unwrap();

        let error = resolve_workspace_owner(
            "auth",
            &[
                first.to_string_lossy().into_owned(),
                second.to_string_lossy().into_owned(),
            ],
            &[definition("auth", repository)],
            &[],
        )
        .unwrap_err();

        assert!(error.to_string().contains("more than one"));
    }

    #[test]
    fn direct_workspace_requires_a_writable_docker_owner() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let config_path = root.join("vm.yaml");
        let mut config = VmConfig {
            source_path: Some(config_path),
            project: Some(ProjectConfig::default()),
            ..Default::default()
        };

        assert!(validate_owner_environment(&config, "tart", &root).is_err());
        config.project.as_mut().unwrap().workspace_access = MountAccess::ReadOnly;
        assert!(validate_owner_environment(&config, "docker", &root).is_err());
        config.project.as_mut().unwrap().workspace_access = MountAccess::ReadWrite;
        assert!(validate_owner_environment(&config, "docker", &root).is_ok());
    }
}
