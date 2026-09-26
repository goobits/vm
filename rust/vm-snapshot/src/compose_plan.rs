use std::collections::BTreeMap;

use serde_json::Value;
use vm_core::error::{Result, VmError};

use crate::metadata::ExcludedMount;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamedVolume {
    pub(crate) name: String,
    pub(crate) runtime_name: String,
    pub(crate) instance: String,
    pub(crate) owner_config_path: String,
}

pub(crate) struct ComposeCapturePlan {
    pub(crate) volumes: Vec<NamedVolume>,
    pub(crate) excluded_mounts: Vec<ExcludedMount>,
}

enum VolumeDisposition {
    Included(NamedVolume),
    Excluded(&'static str),
}

impl ComposeCapturePlan {
    pub(crate) fn parse(json: &str, owner_config_path: &str) -> Result<Self> {
        let config: Value = serde_json::from_str(json).map_err(|error| {
            VmError::general(error, "Cannot parse normalized Compose configuration")
        })?;
        let project = required_string(&config, "name", "Compose project")?;
        let services = config
            .get("services")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                VmError::validation("Compose configuration has no services", None::<String>)
            })?;
        let definitions = config.get("volumes").and_then(Value::as_object);
        let mut volumes = BTreeMap::new();
        let mut excluded_mounts = Vec::new();

        for (service, definition) in services {
            let Some(mounts) = definition.get("volumes") else {
                continue;
            };
            let mounts = mounts.as_array().ok_or_else(|| {
                VmError::validation(
                    format!("Service '{service}' has invalid volume declarations"),
                    None::<String>,
                )
            })?;
            for mount in mounts {
                let kind = required_string(mount, "type", "Compose mount")?;
                let target = required_string(mount, "target", "Compose mount")?;
                if mount
                    .get("source")
                    .is_some_and(|value| !value.is_null() && !value.is_string())
                {
                    return Err(VmError::validation(
                        format!("Service '{service}' has an uninspectable mount source"),
                        None::<String>,
                    ));
                }
                let source = mount
                    .get("source")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let disposition = volume_disposition(
                    kind,
                    source.as_deref(),
                    definitions,
                    project,
                    owner_config_path,
                )?;
                if let Some(VolumeDisposition::Included(ref volume)) = disposition {
                    volumes.insert(volume.name.clone(), volume.clone());
                    continue;
                }
                excluded_mounts.push(ExcludedMount {
                    service: service.clone(),
                    kind: if kind == "volume" && source.is_none() {
                        "anonymous-volume".to_string()
                    } else if let Some(VolumeDisposition::Excluded(reason)) = disposition {
                        reason.to_string()
                    } else {
                        kind.to_string()
                    },
                    source,
                    target: target.to_string(),
                });
            }
        }
        excluded_mounts.sort_by(|left, right| {
            (&left.service, &left.target, &left.kind, &left.source).cmp(&(
                &right.service,
                &right.target,
                &right.kind,
                &right.source,
            ))
        });
        Ok(Self {
            volumes: volumes.into_values().collect(),
            excluded_mounts,
        })
    }
}

fn volume_disposition(
    kind: &str,
    source: Option<&str>,
    definitions: Option<&serde_json::Map<String, Value>>,
    project: &str,
    owner_config_path: &str,
) -> Result<Option<VolumeDisposition>> {
    if kind != "volume" {
        return Ok(None);
    }
    source
        .map(|logical| named_volume(definitions, project, logical, owner_config_path))
        .transpose()
}

fn named_volume(
    definitions: Option<&serde_json::Map<String, Value>>,
    project: &str,
    logical: &str,
    owner_config_path: &str,
) -> Result<VolumeDisposition> {
    if !valid_volume_name(logical) {
        return Err(VmError::validation(
            format!("Invalid Compose volume key '{logical}'"),
            None::<String>,
        ));
    }
    let definition = definitions
        .and_then(|items| items.get(logical))
        .ok_or_else(|| {
            VmError::validation(
                format!("Volume '{logical}' is not declared in normalized Compose configuration"),
                None::<String>,
            )
        })?;
    if definition
        .get("external")
        .is_some_and(|external| external != &Value::Bool(false) && !external.is_null())
    {
        return Ok(VolumeDisposition::Excluded("external-volume"));
    }
    let runtime_name = definition
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("{project}_{logical}"));
    if !valid_volume_name(&runtime_name) {
        return Err(VmError::validation(
            format!("Invalid named volume '{runtime_name}'"),
            None::<String>,
        ));
    }
    let labels = &definition["labels"];
    if labels["com.vm.managed"] != "true"
        || labels["com.vm.instance"] != project
        || labels["com.vm.config-path"] != owner_config_path
        || labels["com.vm.scope"] != "instance"
    {
        return Ok(VolumeDisposition::Excluded("unscoped-volume"));
    }
    Ok(VolumeDisposition::Included(NamedVolume {
        name: logical.to_string(),
        runtime_name,
        instance: project.to_string(),
        owner_config_path: owner_config_path.to_string(),
    }))
}

pub(crate) fn valid_volume_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && chars.all(|character| character.is_ascii_alphanumeric() || "_.-".contains(character))
}

fn required_string<'a>(value: &'a Value, field: &str, context: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            VmError::validation(format!("{context} is missing '{field}'"), None::<String>)
        })
}

#[cfg(test)]
mod tests {
    use super::ComposeCapturePlan;

    #[test]
    fn distinguishes_named_volume_from_bind_anonymous_and_external_mounts() {
        let plan = ComposeCapturePlan::parse(
            r#"{
            "name": "demo", "volumes": {
                "data": {"name": "vm_demo_data", "labels":{"com.vm.managed":"true","com.vm.instance":"demo","com.vm.config-path":"/project/vm.yaml","com.vm.scope":"instance"}},
                "custom": {"name": "custom-data"},
                "shared": {"name": "shared", "external": true}
            }, "services": {"web": {"volumes": [
                {"type":"volume", "source":"data", "target":"/data"},
                {"type":"volume", "source":"custom", "target":"/custom"},
                {"type":"bind", "source":"/home/user/code", "target":"/app"},
                {"type":"volume", "target":"/cache"},
                {"type":"volume", "source":"shared", "target":"/shared"}
            ]}}
        }"#,
            "/project/vm.yaml",
        )
        .unwrap();
        assert_eq!(plan.volumes.len(), 1);
        assert_eq!(plan.volumes[0].runtime_name, "vm_demo_data");
        assert_eq!(plan.excluded_mounts.len(), 4);
        assert_eq!(
            plan.excluded_mounts
                .iter()
                .map(|mount| mount.kind.as_str())
                .collect::<Vec<_>>(),
            vec![
                "bind",
                "anonymous-volume",
                "unscoped-volume",
                "external-volume"
            ]
        );
        assert_eq!(
            plan.excluded_mounts[0].source.as_deref(),
            Some("/home/user/code")
        );
    }

    #[test]
    fn excludes_shared_volumes_and_other_configuration_owners() {
        let mut config = serde_json::json!({
            "name": "demo-dev",
            "services": {"dev": {"volumes": [{"type":"volume", "source":"data", "target":"/data"}]}},
            "volumes": {"data": {"name":"vm_demo-dev_data", "labels": {
                "com.vm.managed":"true", "com.vm.instance":"demo-dev",
                "com.vm.config-path":"/project/vm.yaml", "com.vm.scope":"instance"
            }}}
        });
        let parse = |config: &serde_json::Value, owner: &str| {
            ComposeCapturePlan::parse(&config.to_string(), owner).unwrap()
        };
        assert_eq!(parse(&config, "/project/vm.yaml").volumes.len(), 1);
        assert!(parse(&config, "/other/vm.yaml").volumes.is_empty());
        for scope in ["project", "platform"] {
            config["volumes"]["data"]["labels"]["com.vm.scope"] = scope.into();
            assert!(parse(&config, "/project/vm.yaml").volumes.is_empty());
        }
    }

    #[test]
    fn rejects_uninspectable_mounts_instead_of_reporting_incomplete_coverage() {
        let invalid = r#"{"name":"demo","services":{"web":{"volumes":["./data:/data"]}}}"#;
        assert!(ComposeCapturePlan::parse(invalid, "/project/vm.yaml").is_err());
    }
}
