//! Exact inventory and conservative deletion of provider-owned storage.

use crate::cli::SystemStorageSubcommand;
use crate::error::{VmError, VmResult};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::process::{Command, Output};
use vm_core::{vm_println, vm_success};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Resource {
    id: String,
    owner: String,
    reason: Option<String>,
}

#[derive(Serialize)]
struct StorageView<'a> {
    id: &'a str,
    removable: bool,
}

#[derive(Serialize)]
struct StorageRemoval<'a> {
    id: &'a str,
    removed: bool,
}

pub(super) fn handle(command: &SystemStorageSubcommand) -> VmResult<()> {
    match command {
        SystemStorageSubcommand::List { json } => {
            let resources = inventory_all()?;
            if *json {
                let views = resources
                    .iter()
                    .map(|resource| StorageView {
                        id: &resource.id,
                        removable: resource.reason.is_none(),
                    })
                    .collect::<Vec<_>>();
                return crate::presentation::success("system storage list", views);
            }
            if resources.is_empty() {
                vm_println!("No verified VM-owned storage");
            }
            for resource in resources {
                let status = resource.reason.as_deref().unwrap_or("removable");
                vm_println!("{}\t{}\t{}", resource.id, resource.owner, status);
            }
            Ok(())
        }
        SystemStorageSubcommand::Remove {
            resource_id,
            yes,
            json,
        } => {
            let resource = inventory_all()
                .map_err(|error| error.with_target(resource_id))?
                .into_iter()
                .find(|item| item.id == *resource_id)
                .ok_or_else(|| {
                    VmError::validation(
                        format!(
                            "Resource '{resource_id}' is not in the verified storage inventory"
                        ),
                        Some("Run `vm system storage list` to see exact resource IDs".to_string()),
                    )
                    .with_target(resource_id)
                })?;
            if let Some(reason) = resource.reason {
                return Err(VmError::validation(
                    format!("Cannot remove '{}': {reason}", resource.id),
                    None::<String>,
                )
                .with_target(resource_id));
            }
            if !crate::confirmation::destructive(
                &format!(
                    "Permanently remove '{}' owned by {}?",
                    resource.id, resource.owner
                ),
                *yes,
            )? {
                if *json {
                    return crate::presentation::success(
                        "system storage remove",
                        StorageRemoval {
                            id: resource_id,
                            removed: false,
                        },
                    );
                }
                vm_println!("Storage removal cancelled");
                return Ok(());
            }

            // Reinspect after confirmation. The provider's non-force removal is
            // the final reference check if state changes after this inspection.
            let current = inventory_all()
                .map_err(|error| error.with_target(resource_id))?
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
            match resource_id.split_once(':') {
                Some((engine @ ("docker" | "podman"), rest)) => {
                    if let Some(name) = rest.strip_prefix("volume:") {
                        run(engine, &["volume", "rm", name])?;
                    } else if let Some(id) = rest.strip_prefix("image:") {
                        run(engine, &["image", "rm", id])?;
                    } else {
                        return Err(VmError::validation("Invalid resource ID", None::<String>));
                    }
                }
                Some(("tart", _)) if resource_id.starts_with("tart:vm:") => {
                    remove_tart_storage(resource_id)?;
                }
                _ => return Err(VmError::validation("Invalid resource ID", None::<String>)),
            }
            if *json {
                return crate::presentation::success(
                    "system storage remove",
                    StorageRemoval {
                        id: resource_id,
                        removed: true,
                    },
                );
            }
            vm_success!("Removed {}", resource_id);
            Ok(())
        }
    }
}

fn inventory_all() -> VmResult<Vec<Resource>> {
    let mut resources = Vec::new();
    for engine in ["docker", "podman"] {
        if Command::new(engine)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            resources
                .extend(container_inventory(engine).map_err(|error| error.with_target(engine))?);
        }
    }
    resources.extend(tart_resources()?);
    resources.extend(snapshot_resources()?);
    resources.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(resources)
}

fn snapshot_resources() -> VmResult<Vec<Resource>> {
    let snapshots = vm_snapshot::SnapshotManager::new()?.storage_inventory()?;
    Ok(snapshots
        .into_iter()
        .map(|snapshot| {
            let command = snapshot.owner_config_path.as_ref().filter(|path| path.is_file())
                .and_then(|path| vm_config::config::VmConfig::load(Some(path.clone())).ok().map(|config| (path, config)))
                .filter(|(_, config)| config.project.as_ref().and_then(|project| project.name.as_deref()) == Some(snapshot.project.as_str()))
                .map_or_else(
                    || "owner configuration unavailable; select the project before removing this snapshot".to_string(),
                    |(path, _)| format!(
                        "reclaim with vm --config {} snapshots remove {}",
                        shell_quote(&path.display().to_string()),
                        shell_quote(&snapshot.name),
                    ),
                );
            Resource {
                id: snapshot.id,
                owner: format!("saved snapshot for project {}", snapshot.project),
                reason: Some(command),
            }
        })
        .collect())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn container_inventory(engine: &str) -> VmResult<Vec<Resource>> {
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
        let Some(inspect) = inspect(engine, "volume", name)? else {
            continue;
        };
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
        let Some(inspect) = inspect(engine, "image", id)? else {
            continue;
        };
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

#[cfg(any(feature = "tart", target_os = "macos"))]
fn tart_resources() -> VmResult<Vec<Resource>> {
    Ok(vm_provider::tart_storage_inventory()?
        .into_iter()
        .map(|item| Resource {
            id: item.id,
            owner: item.owner,
            reason: item.reason,
        })
        .collect())
}

#[cfg(not(any(feature = "tart", target_os = "macos")))]
fn tart_resources() -> VmResult<Vec<Resource>> {
    Ok(Vec::new())
}

#[cfg(any(feature = "tart", target_os = "macos"))]
fn remove_tart_storage(id: &str) -> VmResult<()> {
    vm_provider::remove_tart_storage(id).map_err(Into::into)
}

#[cfg(not(any(feature = "tart", target_os = "macos")))]
fn remove_tart_storage(_id: &str) -> VmResult<()> {
    Err(VmError::validation(
        "Tart provider support is not enabled in this build",
        None::<String>,
    ))
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
    let scope = labels["com.vm.scope"].as_str().unwrap_or("unknown");
    let config = labels["com.vm.config-path"].as_str();
    let reason = match labels["com.vm.retention"].as_str() {
        Some("disposable") => None,
        Some("keep") => Some("retained by its owner".to_string()),
        _ => Some("retention policy is unknown".to_string()),
    };
    let reason = if matches!(scope, "project" | "instance") {
        reason
    } else {
        Some("storage scope is unknown".to_string())
    };
    let config_description = config.map_or_else(
        || "config unknown".to_string(),
        |path| format!("config {path}"),
    );
    Ok(Some(Resource {
        id: format!("{engine}:volume:{name}"),
        owner: format!(
            "project {project}, environment {instance}, {scope} scope, {config_description}"
        ),
        reason,
    }))
}

fn image_resource(engine: &str, inspect: &Value) -> VmResult<Option<Resource>> {
    let item = first_item(inspect)?;
    if item["Config"]["Labels"]["com.vm.managed"] != "true" {
        return Ok(None);
    }
    let id = required_string(&item["Id"], "image ID")?;
    // Docker reports a digest; Podman's native image inspection reports a bare
    // full SHA-256. Keep each runtime's identifier for later reference checks.
    let hash = id
        .strip_prefix("sha256:")
        .or_else(|| (engine == "podman").then_some(id));
    if !hash
        .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
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

fn inspect(engine: &str, kind: &str, id: &str) -> VmResult<Option<Value>> {
    let args = [kind, "inspect", id];
    let output = command_output(engine, &args)?;
    // Independent cleanup can remove a listed resource before inspection. Only
    // a precise missing-resource response is safe to omit from the inventory.
    if !output.status.success() && missing_resource(engine, kind, id, &output.stderr) {
        return Ok(None);
    }
    serde_json::from_str(&output_text(engine, &args, output)?)
        .map(Some)
        .map_err(|error| VmError::general(error, "Invalid provider inspection result"))
}

fn missing_resource(engine: &str, kind: &str, id: &str, stderr: &[u8]) -> bool {
    if engine != "docker" {
        return false;
    }
    let expected = match kind {
        "volume" => format!("Error response from daemon: get {id}: no such volume"),
        "image" => format!("Error response from daemon: No such image: {id}"),
        _ => return false,
    };
    String::from_utf8_lossy(stderr).trim() == expected
}

fn command_output(engine: &str, args: &[&str]) -> VmResult<Output> {
    Command::new(engine)
        .args(args)
        .output()
        .map_err(|error| VmError::general(error, format!("Failed to run {engine}")))
}

fn run(engine: &str, args: &[&str]) -> VmResult<String> {
    output_text(engine, args, command_output(engine, args)?)
}

fn output_text(engine: &str, args: &[&str], output: Output) -> VmResult<String> {
    if !output.status.success() {
        return Err(VmError::operation(
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
    use super::{image_resource, shell_quote, volume_resource, with_references};
    use serde_json::json;

    #[test]
    fn only_disposable_owned_unreferenced_volumes_are_removable() {
        let item = json!([{"Name":"demo_data","Labels":{
            "com.vm.managed":"true","com.vm.project":"demo",
            "com.vm.instance":"demo-dev","com.vm.retention":"disposable",
            "com.vm.scope":"instance","com.vm.config-path":"/work/demo/vm.yaml"
        }}]);
        let resource = volume_resource("docker", &item).unwrap().unwrap();
        assert_eq!(resource.id, "docker:volume:demo_data");
        assert!(resource
            .owner
            .contains("instance scope, config /work/demo/vm.yaml"));
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

    #[test]
    fn image_ids_accept_native_provider_forms_and_reject_malformed_hashes() {
        let hash = "a".repeat(64);
        for engine in ["docker", "podman"] {
            for id in [
                hash.clone(),
                format!("sha256:{hash}"),
                "sha256:".to_string(),
                format!("sha256:{}", "a".repeat(63)),
                format!("sha256:{}", "g".repeat(64)),
                format!("sha512:{hash}"),
                "a".repeat(65),
                "--all".to_string(),
            ] {
                let item = json!([{"Id": id, "Config": {
                    "Labels": {"com.vm.managed": "true"}},
                    "RepoTags": [], "RepoDigests": []}]);
                let accepted = id == format!("sha256:{hash}") || (engine == "podman" && id == hash);
                let result = image_resource(engine, &item);
                assert_eq!(result.is_ok(), accepted, "{engine}: {id}");
                if accepted {
                    assert_eq!(result.unwrap().unwrap().id, format!("{engine}:image:{id}"));
                }
            }
        }
    }

    #[test]
    fn generated_snapshot_reclaim_commands_quote_paths() {
        assert_eq!(
            shell_quote("/work/team's demo"),
            "'/work/team'\"'\"'s demo'"
        );
    }
}
