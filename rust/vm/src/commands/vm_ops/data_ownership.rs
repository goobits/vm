//! Verified ownership and reclamation of container environment data.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use vm_config::config::VmConfig;

use crate::error::{VmError, VmResult};

#[derive(Debug)]
pub(super) struct DataPlan {
    engine: String,
    project: String,
    instance: String,
    config_path: PathBuf,
    volumes: Vec<String>,
}

fn run_engine(engine: &str, args: &[&str]) -> VmResult<String> {
    let output = Command::new(engine)
        .args(args)
        .output()
        .map_err(VmError::from)?;
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
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn inspect_one(engine: &str, kind: &str, name: &str) -> VmResult<Value> {
    let output = run_engine(engine, &[kind, "inspect", name])?;
    let values: Vec<Value> = serde_json::from_str(&output).map_err(VmError::from)?;
    values.into_iter().next().ok_or_else(|| {
        VmError::validation(
            format!("{engine} returned no inspection for {kind} '{name}'"),
            None::<String>,
        )
    })
}

fn required_label<'a>(labels: &'a Value, key: &str) -> VmResult<&'a str> {
    labels[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            VmError::validation(
                format!("Storage ownership is missing '{key}'"),
                Some("Inspect the resource with `vm system storage list`"),
            )
        })
}

fn same_path(label: &str, expected: &Path) -> bool {
    Path::new(label).canonicalize().ok().as_deref() == Some(expected)
}

pub(super) fn plan_data_deletion(
    engine: &str,
    target: &str,
    config: &VmConfig,
) -> VmResult<DataPlan> {
    let config_path = config
        .owning_config_path()
        .and_then(|path| path.canonicalize().ok())
        .ok_or_else(|| {
            VmError::validation("Cannot verify owning project configuration", None::<String>)
        })?;
    let main = inspect_one(engine, "container", target)?;
    let main_labels = &main["Config"]["Labels"];
    if required_label(main_labels, "com.vm.managed")? != "true"
        || required_label(main_labels, "com.vm.role")? != "environment"
        || !same_path(
            required_label(main_labels, "com.vm.config-path")?,
            &config_path,
        )
    {
        return Err(VmError::validation(
            "Environment ownership could not be verified",
            None::<String>,
        ));
    }
    let project = required_label(main_labels, "com.vm.project")?;
    let instance = required_label(main_labels, "com.vm.instance")?;
    let names = run_engine(
        engine,
        &[
            "volume",
            "ls",
            "--filter",
            "label=com.vm.managed=true",
            "--filter",
            &format!("label=com.vm.project={project}"),
            "--filter",
            &format!("label=com.vm.instance={instance}"),
            "--format",
            "{{.Name}}",
        ],
    )?;
    let mut volumes = BTreeSet::new();
    for name in names.lines().filter(|name| !name.is_empty()) {
        let volume = inspect_one(engine, "volume", name)?;
        let labels = &volume["Labels"];
        if volume_is_exclusive(labels, name, project, instance, &config_path)? {
            volumes.insert(name.to_string());
        }
    }
    Ok(DataPlan {
        engine: engine.to_string(),
        project: project.to_string(),
        instance: instance.to_string(),
        config_path,
        volumes: volumes.into_iter().collect(),
    })
}

fn volume_is_exclusive(
    labels: &Value,
    name: &str,
    project: &str,
    instance: &str,
    config_path: &Path,
) -> VmResult<bool> {
    if labels["com.vm.managed"] != "true"
        || labels["com.vm.project"] != project
        || labels["com.vm.instance"] != instance
    {
        return Err(VmError::validation(
            format!("Volume '{name}' has conflicting ownership"),
            None::<String>,
        ));
    }
    match labels["com.vm.scope"].as_str() {
        Some("project" | "platform") => Ok(false),
        Some("instance") => {
            if !labels["com.vm.config-path"]
                .as_str()
                .is_some_and(|path| same_path(path, config_path))
            {
                return Err(VmError::validation(
                    format!("Volume '{name}' has conflicting project ownership"),
                    None::<String>,
                ));
            }
            Ok(true)
        }
        _ => Err(VmError::validation(
            format!("Volume '{name}' has unknown ownership scope"),
            Some("Inspect it with `vm system storage list`"),
        )),
    }
}

pub(super) fn remove_exclusive_volumes(plan: &DataPlan) -> VmResult<()> {
    let mut errors = Vec::new();
    for name in &plan.volumes {
        let current = inspect_one(&plan.engine, "volume", name)?;
        if !volume_is_exclusive(
            &current["Labels"],
            name,
            &plan.project,
            &plan.instance,
            &plan.config_path,
        )? {
            errors.push(format!("{name}: ownership scope changed"));
            continue;
        }
        let refs = run_engine(
            &plan.engine,
            &[
                "ps",
                "-a",
                "--filter",
                &format!("volume={name}"),
                "--format",
                "{{.ID}}",
            ],
        )?;
        if refs.lines().any(|line| !line.trim().is_empty()) {
            errors.push(format!("{name}: still referenced by a container"));
            continue;
        }
        if let Err(error) = run_engine(&plan.engine, &["volume", "rm", name]) {
            errors.push(format!("{name}: {error}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(VmError::validation(
            format!(
                "Runtime removed, but some data remains:\n{}",
                errors.join("\n")
            ),
            Some("Inspect retained volumes with `vm system storage list`"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::volume_is_exclusive;

    #[test]
    fn deletion_requires_exact_instance_scope_and_config_owner() {
        let root = tempfile::tempdir().unwrap();
        let selected = root.path().join("vm.yaml");
        std::fs::write(&selected, "project: demo\n").unwrap();
        // The deletion planner passes a canonical owner path, including on
        // hosts where the temporary directory itself is a symlink.
        let selected = selected.canonicalize().unwrap();
        let labels = serde_json::json!({
            "com.vm.managed": "true",
            "com.vm.project": "demo",
            "com.vm.instance": "demo-dev",
            "com.vm.scope": "instance",
            "com.vm.config-path": selected,
        });
        assert!(volume_is_exclusive(&labels, "data", "demo", "demo-dev", &selected).unwrap());
        assert!(volume_is_exclusive(&labels, "data", "demo", "other", &selected).is_err());
        let other = root.path().join("other.yaml");
        std::fs::write(&other, "project: demo\n").unwrap();
        assert!(volume_is_exclusive(&labels, "data", "demo", "demo-dev", &other).is_err());
        let mut shared = labels.clone();
        shared["com.vm.scope"] = "project".into();
        assert!(!volume_is_exclusive(&shared, "data", "demo", "demo-dev", &selected).unwrap());
        shared["com.vm.scope"] = serde_json::Value::Null;
        assert!(volume_is_exclusive(&shared, "data", "demo", "demo-dev", &selected).is_err());
    }
}
