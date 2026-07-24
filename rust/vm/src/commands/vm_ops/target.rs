//! Selection of one existing environment for a command.

use dialoguer::{theme::ColorfulTheme, Select};
use std::io::IsTerminal;

use crate::error::{VmError, VmResult};
use vm_config::config::VmConfig;
use vm_provider::{InstanceInfo, Provider};

enum TargetChoice {
    Selected(String),
    Ambiguous(Vec<InstanceInfo>),
    Missing,
}

pub(super) fn canonical_instance_name(
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

pub fn resolve_runtime_target(
    provider: &dyn Provider,
    config: &VmConfig,
    requested: Option<&str>,
) -> VmResult<String> {
    let project = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");
    let canonical = provider
        .resolve_instance_name(None)
        .map_err(VmError::from)?;
    let instances = provider
        .list_instances()
        .map_err(VmError::from)?
        .into_iter()
        .filter(|instance| project_instance_matches(instance, project))
        .collect::<Vec<_>>();

    match choose_target(&instances, project, &canonical, requested) {
        TargetChoice::Selected(name) => Ok(name),
        TargetChoice::Ambiguous(candidates) => select_ambiguous_target(candidates),
        TargetChoice::Missing if requested.is_some() => Err(VmError::validation(
            format!("No environment matches '{}'", requested.unwrap_or_default()),
            Some("Run `vm list` and use an exact environment name"),
        )),
        TargetChoice::Missing => Err(VmError::validation(
            format!("No environment exists for project '{project}'"),
            Some("Create one with `vm run linux`"),
        )),
    }
}

fn choose_target(
    instances: &[InstanceInfo],
    project: &str,
    canonical: &str,
    requested: Option<&str>,
) -> TargetChoice {
    let Some(requested) = requested else {
        if instances.iter().any(|instance| instance.name == canonical) {
            return TargetChoice::Selected(canonical.to_string());
        }
        return match instances {
            [instance] => TargetChoice::Selected(instance.name.clone()),
            [] => TargetChoice::Missing,
            _ => TargetChoice::Ambiguous(instances.to_vec()),
        };
    };

    if let Some(instance) = instances.iter().find(|instance| instance.name == requested) {
        return TargetChoice::Selected(instance.name.clone());
    }

    let aliases = [
        (requested == project).then_some(canonical.to_string()),
        Some(format!("{requested}-dev")),
        Some(format!("{project}-{requested}")),
        Some(format!("{project}-{requested}-dev")),
    ];
    let alias_matches = instances
        .iter()
        .filter(|instance| {
            aliases
                .iter()
                .flatten()
                .any(|alias| instance.name == *alias)
        })
        .cloned()
        .collect::<Vec<_>>();
    match alias_matches.as_slice() {
        [instance] => return TargetChoice::Selected(instance.name.clone()),
        [] => {}
        _ => return TargetChoice::Ambiguous(alias_matches),
    }

    let id_matches = instances
        .iter()
        .filter(|instance| instance.id.starts_with(requested))
        .cloned()
        .collect::<Vec<_>>();
    match id_matches.as_slice() {
        [instance] => return TargetChoice::Selected(instance.name.clone()),
        [] => {}
        _ => return TargetChoice::Ambiguous(id_matches),
    }

    let name_matches = instances
        .iter()
        .filter(|instance| instance.name.contains(requested))
        .cloned()
        .collect::<Vec<_>>();
    match name_matches.as_slice() {
        [instance] => TargetChoice::Selected(instance.name.clone()),
        [] => TargetChoice::Missing,
        _ => TargetChoice::Ambiguous(name_matches),
    }
}

fn select_ambiguous_target(mut candidates: Vec<InstanceInfo>) -> VmResult<String> {
    candidates.sort_by(|left, right| left.name.cmp(&right.name));
    let names = candidates
        .iter()
        .map(|instance| instance.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(VmError::validation(
            "Multiple environments match",
            Some(format!("Specify one of: {names}")),
        ));
    }

    let labels = candidates
        .iter()
        .map(|instance| {
            format!(
                "{} ({}, {})",
                instance.name, instance.status, instance.provider
            )
        })
        .collect::<Vec<_>>();
    let selected = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Which environment?")
        .items(&labels)
        .default(0)
        .interact()
        .map_err(|error| VmError::general(error, "Failed to read environment selection"))?;
    Ok(candidates[selected].name.clone())
}

pub fn copy_target(source: &str, destination: &str) -> VmResult<Option<String>> {
    let source_target = remote_target(source);
    let destination_target = remote_target(destination);
    match (source_target, destination_target) {
        (Some(source), Some(destination)) if source != destination => Err(VmError::validation(
            "Source and destination reference different environments",
            Some("Copy through the host or use the same environment name"),
        )),
        (Some(target), _) | (_, Some(target)) => Ok(Some(target.to_string())),
        (None, None) => Ok(None),
    }
}

fn remote_target(path: &str) -> Option<&str> {
    let (prefix, remainder) = path.split_once(':')?;
    if prefix.len() == 1 && (remainder.starts_with('/') || remainder.starts_with('\\')) {
        return None;
    }
    (!prefix.is_empty() && !prefix.contains('/') && !prefix.contains('\\')).then_some(prefix)
}

pub fn project_instance_matches(instance: &InstanceInfo, project_name: &str) -> bool {
    instance.project.as_deref() == Some(project_name)
        || instance.name == project_name
        || instance.name == format!("{project_name}-dev")
        || instance.name.starts_with(&format!("{project_name}-"))
}

#[cfg(test)]
mod tests {
    use super::{canonical_instance_name, choose_target, copy_target, TargetChoice};
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
            TargetChoice::Selected(name) if name == "demo-dev"
        ));
    }

    #[test]
    fn sole_project_instance_is_the_default_when_canonical_is_absent() {
        let instances = vec![instance("demo-feature-dev")];
        assert!(matches!(
            choose_target(&instances, "demo", "demo-dev", None),
            TargetChoice::Selected(name) if name == "demo-feature-dev"
        ));
    }

    #[test]
    fn instance_suffix_resolves_to_canonical_provider_name() {
        let instances = vec![instance("demo-feature-dev")];
        assert!(matches!(
            choose_target(&instances, "demo", "demo-dev", Some("feature")),
            TargetChoice::Selected(name) if name == "demo-feature-dev"
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
    fn copy_target_reads_remote_prefix() {
        assert_eq!(
            copy_target("./local.txt", "feature:/tmp/remote.txt").unwrap(),
            Some("feature".to_string())
        );
        assert!(copy_target("one:/tmp/a", "two:/tmp/b").is_err());
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
