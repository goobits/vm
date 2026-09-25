//! Selection of one existing environment for a command.

use crate::error::{VmError, VmResult};
use vm_config::config::VmConfig;
use vm_provider::{InstanceInfo, InstanceProvider};

enum TargetChoice {
    Selected(InstanceInfo),
    Ambiguous(Vec<InstanceInfo>),
    Missing,
}

pub(in crate::commands) fn canonical_instance_name(
    provider: &str,
    project: &str,
    instance: Option<&str>,
) -> String {
    match (provider, instance) {
        ("tart", Some(instance)) => format!("{project}-{instance}"),
        ("tart", None) => project.to_string(),
        (_, Some(instance)) => format!("{project}-{instance}-dev"),
        (_, None) => format!("{project}-dev"),
    }
}

pub(in crate::commands) fn resolve_runtime_instance(
    provider: &dyn InstanceProvider,
    config: &VmConfig,
    requested: Option<&str>,
) -> VmResult<InstanceInfo> {
    find_runtime_target(provider, config, requested)?.ok_or_else(|| match requested {
        Some(requested) => VmError::validation(
            format!("No environment matches '{requested}'"),
            Some("Run `vm list` and use an exact environment name"),
        ),
        None => {
            let project = config
                .project
                .as_ref()
                .and_then(|project| project.name.as_deref())
                .unwrap_or("vm-project");
            VmError::validation(
                format!("No environment exists for project '{project}'"),
                Some("Start a declared environment with `vm start`"),
            )
        }
    })
}

pub(in crate::commands) fn find_runtime_target(
    provider: &dyn InstanceProvider,
    config: &VmConfig,
    requested: Option<&str>,
) -> VmResult<Option<InstanceInfo>> {
    let project = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");
    let canonical = provider
        .resolve_instance_name(None)
        .map_err(VmError::from)?;
    let selected_owner = config
        .owning_config_path()
        .map(|path| path.canonicalize())
        .transpose()
        .map_err(VmError::from)?;
    let instances = provider
        .list_instances()
        .map_err(VmError::from)?
        .into_iter()
        .filter(|instance| instance.project.as_deref() == Some(project))
        .filter_map(|instance| {
            if let Some(owner) = selected_owner.as_deref() {
                match instance_owner_matches(provider, owner, &instance.name) {
                    Ok(true) => {}
                    Ok(false) => return None,
                    Err(error) => return Some(Err(error)),
                }
            }
            Some(Ok(instance))
        })
        .collect::<VmResult<Vec<_>>>()?;

    match choose_target(&instances, project, &canonical, requested) {
        TargetChoice::Selected(name) => Ok(Some(name)),
        TargetChoice::Ambiguous(candidates) => Err(ambiguous_target(candidates)),
        TargetChoice::Missing => Ok(None),
    }
}

pub(super) fn verify_runtime_owner(
    provider: &dyn InstanceProvider,
    config: &VmConfig,
    target: &str,
) -> VmResult<()> {
    let selected = config
        .owning_config_path()
        .ok_or_else(|| {
            VmError::validation("Cannot verify owning project configuration", None::<String>)
        })?
        .canonicalize()
        .map_err(VmError::from)?;
    if !instance_owner_matches(provider, &selected, target)? {
        return Err(VmError::validation(
            format!("Environment '{target}' is not owned by the selected project configuration"),
            Some("Run `vm list` and select the owning project"),
        ));
    }
    Ok(())
}

fn instance_owner_matches(
    provider: &dyn InstanceProvider,
    selected: &std::path::Path,
    target: &str,
) -> VmResult<bool> {
    let owner = provider
        .instance_config_path(target)
        .map_err(VmError::from)?
        .and_then(|path| path.canonicalize().ok());
    Ok(owner.as_deref() == Some(selected))
}

fn choose_target(
    instances: &[InstanceInfo],
    project: &str,
    canonical: &str,
    requested: Option<&str>,
) -> TargetChoice {
    let Some(requested) = requested else {
        if instances.iter().any(|instance| instance.name == canonical) {
            return TargetChoice::Selected(
                instances
                    .iter()
                    .find(|instance| instance.name == canonical)
                    .expect("canonical instance exists")
                    .clone(),
            );
        }
        return match instances {
            [instance] => TargetChoice::Selected(instance.clone()),
            [] => TargetChoice::Missing,
            _ => TargetChoice::Ambiguous(instances.to_vec()),
        };
    };

    requested_target_choice(instances, requested, project, canonical)
}

fn requested_target_choice(
    instances: &[InstanceInfo],
    requested: &str,
    project: &str,
    canonical: &str,
) -> TargetChoice {
    if let Some(instance) = instances.iter().find(|instance| instance.name == requested) {
        return TargetChoice::Selected(instance.clone());
    }
    let alias_matches = instances
        .iter()
        .filter(|instance| {
            instance.name == format!("{requested}-dev")
                || instance.project.as_deref().is_some_and(|project| {
                    instance.name == format!("{project}-{requested}")
                        || instance.name == format!("{project}-{requested}-dev")
                })
                || requested == project && instance.name == canonical
                || instance.name == format!("{project}-{requested}")
                || instance.name == format!("{project}-{requested}-dev")
        })
        .cloned()
        .collect::<Vec<_>>();
    match alias_matches.as_slice() {
        [instance] => return TargetChoice::Selected(instance.clone()),
        [] => {}
        _ => return TargetChoice::Ambiguous(alias_matches),
    }

    TargetChoice::Missing
}

fn ambiguous_target(mut candidates: Vec<InstanceInfo>) -> VmError {
    candidates.sort_by(|left, right| left.name.cmp(&right.name));
    let names = candidates
        .iter()
        .map(|instance| instance.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    VmError::validation(
        "Multiple environments match",
        Some(format!("Specify one of: {names}")),
    )
}

#[cfg(test)]
mod tests {
    use super::{canonical_instance_name, choose_target, TargetChoice};
    use vm_provider::InstanceInfo;

    fn instance(name: &str) -> InstanceInfo {
        InstanceInfo {
            name: name.to_string(),
            id: format!("id-{name}"),
            status: "stopped".to_string(),
            provider: "docker".to_string(),
            project: Some("demo".to_string()),
            uptime: None,
            created_at: None,
        }
    }

    #[test]
    fn canonical_target_wins_when_project_has_multiple_instances() {
        let instances = vec![instance("demo-feature-dev"), instance("demo-dev")];
        assert!(matches!(
            choose_target(&instances, "demo", "demo-dev", None),
            TargetChoice::Selected(instance) if instance.name == "demo-dev"
        ));
    }

    #[test]
    fn substring_and_id_prefixes_do_not_select_an_environment() {
        let instances = vec![instance("demo-feature-dev")];
        assert!(matches!(
            choose_target(&instances, "demo", "demo-dev", Some("feat")),
            TargetChoice::Missing
        ));
        assert!(matches!(
            choose_target(&instances, "demo", "demo-dev", Some("id-demo")),
            TargetChoice::Missing
        ));
    }

    #[test]
    fn sole_project_instance_is_the_default_when_canonical_is_absent() {
        let instances = vec![instance("demo-feature-dev")];
        assert!(matches!(
            choose_target(&instances, "demo", "demo-dev", None),
            TargetChoice::Selected(instance) if instance.name == "demo-feature-dev"
        ));
    }

    #[test]
    fn instance_suffix_resolves_to_canonical_provider_name() {
        let instances = vec![instance("demo-feature-dev")];
        assert!(matches!(
            choose_target(&instances, "demo", "demo-dev", Some("feature")),
            TargetChoice::Selected(instance) if instance.name == "demo-feature-dev"
        ));
    }

    #[test]
    fn multiple_noncanonical_instances_remain_ambiguous() {
        let instances = vec![instance("demo-one-dev"), instance("demo-two-dev")];
        assert!(matches!(
            choose_target(&instances, "demo", "demo-dev", None),
            TargetChoice::Ambiguous(_)
        ));
    }

    #[test]
    fn canonical_names_follow_provider_conventions() {
        assert_eq!(
            canonical_instance_name("docker", "demo", Some("feature")),
            "demo-feature-dev"
        );
        assert_eq!(
            canonical_instance_name("tart", "demo", Some("feature")),
            "demo-feature"
        );
        assert_eq!(canonical_instance_name("docker", "demo", None), "demo-dev");
        assert_eq!(canonical_instance_name("tart", "demo", None), "demo");
    }
}
