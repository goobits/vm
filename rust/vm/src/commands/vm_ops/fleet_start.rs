//! Frozen selection and creation of declared environments for fleet start.

use std::collections::BTreeSet;

use crate::cli::FleetArgs;
use crate::error::{VmError, VmResult};
use vm_config::{config::VmConfig, GlobalConfig};
use vm_provider::{InstanceInfo, ProviderContext};

use super::create::handle_create;
use super::fleet::{
    apply_lifecycle, configured_provider, configured_provider_for_instance,
    filter_project_instances, FleetAction, FleetProgress, FleetProject,
};
use super::target::canonical_instance_name;
use super::targets::match_pattern;

fn discover_start_instances(
    targets: &FleetArgs,
    project: &FleetProject,
) -> VmResult<Vec<InstanceInfo>> {
    let mut instances = Vec::new();
    for provider_name in vm_config::config::ProviderName::SUPPORTED {
        if targets
            .provider
            .as_deref()
            .is_some_and(|selected| selected != provider_name)
        {
            continue;
        }
        let required = provider_required(targets, project, provider_name);
        let provider = match configured_provider(&project.config, provider_name) {
            Ok(provider) => provider,
            Err(error) if required => return Err(error),
            Err(_) => continue,
        };
        match provider.list_instances() {
            Ok(discovered) => instances.extend(discovered.into_iter().filter(|instance| {
                targets
                    .pattern
                    .as_deref()
                    .map_or(true, |pattern| match_pattern(&instance.name, pattern))
            })),
            Err(error) if required => return Err(VmError::from(error)),
            Err(_) => {}
        }
    }
    filter_project_instances(instances, project)
}

fn provider_required(targets: &FleetArgs, project: &FleetProject, provider_name: &str) -> bool {
    targets.provider.as_deref() == Some(provider_name)
        || project
            .config
            .environments
            .values()
            .any(|declaration| declaration.provider.as_str() == provider_name)
}

enum StartTarget {
    Existing(InstanceInfo),
    Declared {
        name: String,
        provider: String,
        runtime: String,
        config: Box<VmConfig>,
    },
}

fn plan_start_targets(
    project: &FleetProject,
    targets: &FleetArgs,
    existing: Vec<InstanceInfo>,
) -> VmResult<Vec<StartTarget>> {
    let mut selected = Vec::new();
    let mut seen = BTreeSet::new();
    for instance in existing {
        seen.insert((instance.provider.clone(), instance.name.clone()));
        selected.push(StartTarget::Existing(instance));
    }
    for (name, declaration) in &project.config.environments {
        let provider = declaration.provider.as_str();
        let runtime = canonical_instance_name(provider, &project.name, Some(name));
        if targets
            .provider
            .as_deref()
            .is_some_and(|filter| filter != provider)
            || targets
                .pattern
                .as_deref()
                .is_some_and(|pattern| !match_pattern(&runtime, pattern))
            || seen.contains(&(provider.to_string(), runtime.clone()))
        {
            continue;
        }
        selected.push(StartTarget::Declared {
            name: name.clone(),
            provider: provider.to_string(),
            runtime,
            config: Box::new(declaration.apply_to(&project.config)),
        });
    }
    if selected.is_empty() {
        return Err(VmError::validation(
            format!(
                "No matching environments belong to project '{}'",
                project.name
            ),
            Some("Run `vm list` to inspect project environments"),
        ));
    }
    Ok(selected)
}

pub async fn handle_fleet_start(
    targets: &FleetArgs,
    project: &FleetProject,
    no_wait: bool,
) -> VmResult<()> {
    let existing = discover_start_instances(targets, project)?;
    let frozen = plan_start_targets(project, targets, existing)?;
    let global = GlobalConfig::load()?;
    let context = ProviderContext::default().with_config(global.clone());
    let mut progress = FleetProgress::default();
    for target in frozen {
        match target {
            StartTarget::Existing(instance) => {
                let outcome = async {
                    let provider = configured_provider_for_instance(project, &instance)?;
                    apply_lifecycle(
                        provider.as_ref(),
                        &context,
                        &instance.name,
                        FleetAction::Start,
                        no_wait,
                    )
                    .await
                }
                .await;
                match outcome {
                    Ok(()) => progress.success(&instance.name),
                    Err(error) => progress.failure(&instance.name, &error),
                }
            }
            StartTarget::Declared {
                name,
                provider,
                runtime,
                config,
            } => {
                let outcome = async {
                    let provider = configured_provider(&config, &provider)?;
                    handle_create(
                        provider.clone_box(),
                        *config,
                        global.clone(),
                        false,
                        Some(name),
                    )
                    .await?;
                    apply_lifecycle(
                        provider.as_ref(),
                        &context,
                        &runtime,
                        FleetAction::Start,
                        no_wait,
                    )
                    .await
                }
                .await;
                match outcome {
                    Ok(()) => progress.success(&runtime),
                    Err(error) => progress.failure(&runtime, &error),
                }
            }
        }
    }
    progress.finish()
}

#[cfg(test)]
mod tests {
    use super::{plan_start_targets, provider_required, StartTarget};
    use crate::cli::FleetArgs;
    use crate::commands::vm_ops::fleet::FleetProject;
    use vm_config::config::VmConfig;
    use vm_provider::InstanceInfo;

    #[test]
    fn start_plan_freezes_existing_and_missing_declarations_once() {
        let config: VmConfig = serde_yaml_ng::from_str(
            "project:\n  name: demo\nenvironments:\n  dev:\n    provider: docker\n    image: ubuntu:24.04\n  test:\n    provider: docker\n    image: ubuntu:24.04\n",
        ).unwrap();
        let project = FleetProject {
            name: "demo".into(),
            config_path: "/tmp/demo/vm.yaml".into(),
            config,
        };
        let existing = vec![InstanceInfo {
            name: "demo-dev-dev".into(),
            id: "id".into(),
            status: "running".into(),
            provider: "docker".into(),
            project: Some("demo".into()),
            uptime: None,
            created_at: None,
        }];
        let targets = FleetArgs {
            fleet: true,
            provider: None,
            pattern: None,
        };
        let plan = plan_start_targets(&project, &targets, existing).unwrap();
        assert!(provider_required(&targets, &project, "docker"));
        assert!(!provider_required(&targets, &project, "podman"));
        assert_eq!(plan.len(), 2);
        assert!(matches!(&plan[0], StartTarget::Existing(item) if item.name == "demo-dev-dev"));
        assert!(matches!(&plan[1], StartTarget::Declared { name, .. } if name == "test"));

        let filtered = FleetArgs {
            fleet: true,
            provider: Some("docker".into()),
            pattern: Some("*-test-dev".into()),
        };
        let plan = plan_start_targets(&project, &filtered, Vec::new()).unwrap();
        assert!(matches!(plan.as_slice(), [StartTarget::Declared { name, .. }] if name == "test"));

        let missing = FleetArgs {
            fleet: true,
            provider: Some("tart".into()),
            pattern: None,
        };
        assert!(plan_start_targets(&project, &missing, Vec::new()).is_err());
    }
}
