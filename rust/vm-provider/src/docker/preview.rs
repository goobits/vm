use serde_yaml_ng::{Mapping, Value};
use vm_core::error::{Result, VmError};

pub(super) fn redact_compose(content: &str) -> Result<String> {
    let mut compose: Value = serde_yaml_ng::from_str(content).map_err(|error| {
        VmError::Internal(format!(
            "Failed to parse generated Compose configuration for redaction: {error}"
        ))
    })?;

    if let Some(services) = compose.get_mut("services").and_then(Value::as_mapping_mut) {
        for service in services.values_mut().filter_map(Value::as_mapping_mut) {
            redact_build_context(service);
            redact_environment(service);
            redact_bind_mounts(service);
            redact_host_labels(service);
        }
    }

    serde_yaml_ng::to_string(&compose).map_err(|error| {
        VmError::Internal(format!(
            "Failed to serialize redacted Compose configuration: {error}"
        ))
    })
}

fn redact_host_labels(service: &mut Mapping) {
    let Some(labels) = service.get_mut("labels").and_then(Value::as_mapping_mut) else {
        return;
    };
    if labels.contains_key("com.vm.config-path") {
        labels.insert(
            Value::String("com.vm.config-path".to_string()),
            Value::String("<host-path>".to_string()),
        );
    }
}

fn redact_build_context(service: &mut Mapping) {
    if let Some(build) = service.get_mut("build").and_then(Value::as_mapping_mut) {
        if build.contains_key("context") {
            build.insert(
                Value::String("context".to_string()),
                Value::String("<generated-build-context>".to_string()),
            );
        }
    }
}

fn redact_environment(service: &mut Mapping) {
    let Some(environment) = service.get_mut("environment") else {
        return;
    };

    if let Some(entries) = environment.as_sequence_mut() {
        for entry in entries {
            let Some(value) = entry.as_str() else {
                continue;
            };
            let name = value.split_once('=').map_or(value, |(name, _)| name);
            *entry = Value::String(format!("{name}=<redacted>"));
        }
    } else if let Some(entries) = environment.as_mapping_mut() {
        for value in entries.values_mut() {
            *value = Value::String("<redacted>".to_string());
        }
    }
}

fn redact_bind_mounts(service: &mut Mapping) {
    let Some(mounts) = service.get_mut("volumes").and_then(Value::as_sequence_mut) else {
        return;
    };

    for mount in mounts {
        if let Some(short_mount) = mount.as_str() {
            *mount = Value::String(redact_short_mount(short_mount));
            continue;
        }

        let Some(mapping) = mount.as_mapping_mut() else {
            continue;
        };
        if mapping.get("type").and_then(Value::as_str) == Some("bind") {
            mapping.insert(
                Value::String("source".to_string()),
                Value::String("<host-path>".to_string()),
            );
        }
    }
}

fn redact_short_mount(mount: &str) -> String {
    let mut parts = mount.rsplitn(3, ':');
    let mode = parts.next();
    let target = parts.next();
    let source = parts.next();
    match (source, target, mode) {
        (Some(source), Some(target), Some(mode)) => {
            let target = if source == target {
                "<host-path>"
            } else {
                target
            };
            format!("<host-path>:{target}:{mode}")
        }
        _ => "<host-path>".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_secrets_and_host_paths_without_hiding_mount_semantics() {
        let redacted = redact_compose(
            r#"
services:
  app:
    build:
      context: /Users/miko/.vm/generated/project/docker/build_context
    environment:
      - API_TOKEN=super-secret
      - DATABASE_URL=postgres://user:password@db/app
    labels:
      com.vm.config-path: /Users/miko/project/vm.yaml
    volumes:
      - /Users/miko/project:/workspace:rw
      - /Users/miko/.vm/worktrees/project:/Users/miko/.vm/worktrees/project:rw
      - type: volume
        source: node_modules
        target: /workspace/node_modules
"#,
        )
        .unwrap();

        assert!(!redacted.contains("super-secret"));
        assert!(!redacted.contains("password"));
        assert!(!redacted.contains("/Users/miko"));
        assert!(redacted.contains("API_TOKEN=<redacted>"));
        assert!(redacted.contains("com.vm.config-path: <host-path>"));
        assert!(redacted.contains("<host-path>:/workspace:rw"));
        assert!(redacted.contains("source: node_modules"));
        assert!(redacted.contains("target: /workspace/node_modules"));
    }
}
