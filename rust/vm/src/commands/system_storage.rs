//! Exact inventory and conservative deletion of provider-owned storage.

use crate::cli::SystemStorageSubcommand;
use crate::error::{VmError, VmResult};
use serde_json::Value;
use std::collections::BTreeSet;
use std::process::Command;
use vm_config::GlobalConfig;
use vm_core::{vm_println, vm_success};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Resource {
    id: String,
    owner: String,
    reason: Option<String>,
}

pub(super) fn handle(command: &SystemStorageSubcommand) -> VmResult<()> {
    let config = GlobalConfig::load()?;
    let provider = config.container_provider();
    let engine = provider.as_str();
    match command {
        SystemStorageSubcommand::List => {
            let resources = inventory(engine)?;
            if resources.is_empty() {
                vm_println!("No verified VM-owned storage on {engine}");
            }
            for resource in resources {
                let status = resource.reason.as_deref().unwrap_or("removable");
                vm_println!("{}\t{}\t{}", resource.id, resource.owner, status);
            }
            Ok(())
        }
        SystemStorageSubcommand::Remove { resource_id, yes } => {
            let resource = inventory(engine)?
                .into_iter()
                .find(|item| item.id == *resource_id)
                .ok_or_else(|| {
                    VmError::validation(
                        format!(
                            "Resource '{resource_id}' is not in the verified {engine} inventory"
                        ),
                        Some("Run `vm system storage list` to see exact resource IDs".to_string()),
                    )
                })?;
            if let Some(reason) = resource.reason {
                return Err(VmError::validation(
                    format!("Cannot remove '{}': {reason}", resource.id),
                    None::<String>,
                ));
            }
            if !yes
                && !vm_core::prompts::confirm_select(
                    &format!(
                        "Permanently remove '{}' owned by {}?",
                        resource.id, resource.owner
                    ),
                    false,
                )?
            {
                vm_println!("Storage removal cancelled");
                return Ok(());
            }

            // Reinspect after confirmation. The provider's non-force removal is
            // the final reference check if state changes after this inspection.
            let current = inventory(engine)?
                .into_iter()
                .find(|item| item.id == *resource_id)
                .ok_or_else(|| {
                    VmError::validation("Resource changed during confirmation", None::<String>)
                })?;
            if current.reason.is_some() || current.owner != resource.owner {
                return Err(VmError::validation(
                    "Resource changed during confirmation; inspect storage again",
                    None::<String>,
                ));
            }
            let volume_prefix = format!("{engine}:volume:");
            let image_prefix = format!("{engine}:image:");
            if let Some(name) = resource_id.strip_prefix(&volume_prefix) {
                run(engine, &["volume", "rm", name])?;
            } else if let Some(id) = resource_id.strip_prefix(&image_prefix) {
                run(engine, &["image", "rm", id])?;
            } else {
                return Err(VmError::validation("Invalid resource ID", None::<String>));
            }
            vm_success!("Removed {}", resource_id);
            Ok(())
        }
    }
}

fn inventory(engine: &str) -> VmResult<Vec<Resource>> {
    let mut resources = Vec::new();
    let volumes = run(
        engine,
        &[
            "volume",
            "ls",
            "--filter",
            "label=com.vm.managed=true",
            "--format",
            "{{.Name}}",
        ],
    )?;
    for name in volumes.lines().filter(|line| !line.is_empty()) {
        let inspect = inspect(engine, &["volume", "inspect", name])?;
        if let Some(resource) = volume_resource(engine, &inspect)? {
            let refs = run(
                engine,
                &[
                    "ps",
                    "-a",
                    "--filter",
                    &format!("volume={name}"),
                    "--format",
                    "{{.ID}}",
                ],
            )?;
            resources.push(with_references(resource, &refs));
        }
    }

    let images = run(
        engine,
        &[
            "image",
            "ls",
            "-a",
            "--filter",
            "label=com.vm.managed=true",
            "--format",
            "{{.ID}}",
        ],
    )?;
    for id in images
        .lines()
        .filter(|line| !line.is_empty())
        .collect::<BTreeSet<_>>()
    {
        let inspect = inspect(engine, &["image", "inspect", id])?;
        if let Some(resource) = image_resource(engine, &inspect)? {
            let full_id = resource
                .id
                .strip_prefix(&format!("{engine}:image:"))
                .expect("image resource ID");
            let refs = run(
                engine,
                &[
                    "ps",
                    "-a",
                    "--filter",
                    &format!("ancestor={full_id}"),
                    "--format",
                    "{{.ID}}",
                ],
            )?;
            resources.push(with_references(resource, &refs));
        }
    }
    resources.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(resources)
}

fn with_references(mut resource: Resource, refs: &str) -> Resource {
    if refs.lines().any(|line| !line.trim().is_empty()) {
        resource.reason = Some("referenced by a container".to_string());
    }
    resource
}

fn volume_resource(engine: &str, inspect: &Value) -> VmResult<Option<Resource>> {
    let item = first_item(inspect)?;
    let labels = &item["Labels"];
    if labels["com.vm.managed"] != "true" {
        return Ok(None);
    }
    let name = required_string(&item["Name"], "volume name")?;
    let project = required_string(&labels["com.vm.project"], "project owner")?;
    let instance = required_string(&labels["com.vm.instance"], "instance owner")?;
    let reason = match labels["com.vm.retention"].as_str() {
        Some("disposable") => None,
        Some("keep") => Some("retained by its owner".to_string()),
        _ => Some("retention policy is unknown".to_string()),
    };
    Ok(Some(Resource {
        id: format!("{engine}:volume:{name}"),
        owner: format!("project {project}, environment {instance}"),
        reason,
    }))
}

fn image_resource(engine: &str, inspect: &Value) -> VmResult<Option<Resource>> {
    let item = first_item(inspect)?;
    if item["Config"]["Labels"]["com.vm.managed"] != "true" {
        return Ok(None);
    }
    let id = required_string(&item["Id"], "image ID")?;
    if !id.starts_with("sha256:") || !id[7..].bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(VmError::validation(
            "Provider returned an invalid image ID",
            None::<String>,
        ));
    }
    let has_tags = has_real_references(item.get("RepoTags"), "image tags")?;
    let has_digests = has_real_references(item.get("RepoDigests"), "image digests")?;
    Ok(Some(Resource {
        id: format!("{engine}:image:{id}"),
        owner: "vm-managed image".to_string(),
        reason: (has_tags || has_digests).then(|| "tagged or published image".to_string()),
    }))
}

fn has_real_references(value: Option<&Value>, field: &str) -> VmResult<bool> {
    let value = value.ok_or_else(|| {
        VmError::validation(
            format!("Provider inspection omitted {field}"),
            None::<String>,
        )
    })?;
    if value.is_null() {
        return Ok(false);
    }
    let values = value.as_array().ok_or_else(|| {
        VmError::validation(
            format!("Provider inspection returned invalid {field}"),
            None::<String>,
        )
    })?;
    Ok(values.iter().any(|item| {
        item.as_str()
            .map_or(true, |text| !text.starts_with("<none>"))
    }))
}

fn first_item(value: &Value) -> VmResult<&Value> {
    value
        .as_array()
        .and_then(|items| (items.len() == 1).then(|| &items[0]))
        .ok_or_else(|| {
            VmError::validation(
                "Provider inspection did not return exactly one resource",
                None::<String>,
            )
        })
}

fn required_string<'a>(value: &'a Value, field: &str) -> VmResult<&'a str> {
    value
        .as_str()
        .filter(|text| !text.is_empty())
        .ok_or_else(|| {
            VmError::validation(
                format!("Provider inspection omitted {field}"),
                None::<String>,
            )
        })
}

fn inspect(engine: &str, args: &[&str]) -> VmResult<Value> {
    serde_json::from_str(&run(engine, args)?)
        .map_err(|error| VmError::general(error, "Invalid provider inspection result"))
}

fn run(engine: &str, args: &[&str]) -> VmResult<String> {
    let output = Command::new(engine)
        .args(args)
        .output()
        .map_err(|error| VmError::general(error, format!("Failed to run {engine}")))?;
    if !output.status.success() {
        return Err(VmError::validation(
            format!(
                "{engine} {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            None::<String>,
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| VmError::general(error, "Invalid provider output"))
}

#[cfg(test)]
mod tests {
    use super::{image_resource, volume_resource, with_references};
    use serde_json::json;

    #[test]
    fn only_disposable_owned_unreferenced_volumes_are_removable() {
        let item = json!([{"Name":"demo_data","Labels":{
            "com.vm.managed":"true","com.vm.project":"demo",
            "com.vm.instance":"demo-dev","com.vm.retention":"disposable"
        }}]);
        let resource = volume_resource("docker", &item).unwrap().unwrap();
        assert_eq!(resource.id, "docker:volume:demo_data");
        assert_eq!(resource.reason, None);
        assert!(with_references(resource, "container123\n").reason.is_some());
        let mut retained = item.clone();
        retained[0]["Labels"]["com.vm.retention"] = "keep".into();
        assert!(volume_resource("docker", &retained)
            .unwrap()
            .unwrap()
            .reason
            .is_some());
        retained[0]["Labels"]["com.vm.managed"] = "false".into();
        assert!(volume_resource("docker", &retained).unwrap().is_none());
        let mut unowned = item;
        unowned[0]["Labels"]
            .as_object_mut()
            .unwrap()
            .remove("com.vm.project");
        assert!(volume_resource("docker", &unowned).is_err());
    }

    #[test]
    fn tagged_images_cannot_be_removed() {
        let item = json!([{"Id":format!("sha256:{}", "a".repeat(64)),"Config":{
            "Labels":{"com.vm.managed":"true"}},"RepoTags":["vm/demo:latest"],"RepoDigests":[]
        }]);
        assert!(image_resource("podman", &item)
            .unwrap()
            .unwrap()
            .reason
            .is_some());
        let mut dangling = item;
        dangling[0]["RepoTags"] = json!(["<none>:<none>"]);
        assert!(image_resource("podman", &dangling)
            .unwrap()
            .unwrap()
            .reason
            .is_none());
        dangling[0].as_object_mut().unwrap().remove("RepoDigests");
        assert!(image_resource("podman", &dangling).is_err());
    }
}
