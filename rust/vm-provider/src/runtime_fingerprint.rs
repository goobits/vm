//! Stable runtime-configuration identity shared by providers.

use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use vm_config::config::VmConfig;
use vm_core::error::Result;

pub(crate) fn runtime_fingerprint(config: &VmConfig) -> Result<String> {
    let project = config.project.as_ref();
    let user_environment = config
        .environment
        .iter()
        .filter(|(name, _)| !MANAGED_PACKAGE_ENV.contains(&name.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let runtime = serde_json::json!({
        "provider": config.provider,
        "tart": config.provider.as_ref().filter(|provider| provider.as_str() == "tart").and(config.tart.as_ref()),
        "os": config.os,
        "vm": config.vm,
        "storage": config.storage,
        "mounts": config.mounts,
        "ports": config.ports,
        "networking": config.networking,
        "services": config.services,
        "service_plugins": vm_config::config::resolve_service_plugins(config)?,
        "security": config.security,
        "environment": user_environment,
        "host_sync": config.host_sync,
        "workspace_path": project.and_then(|value| value.workspace_path.as_deref()).unwrap_or("/workspace"),
        "workspace_access": project.map(|value| value.workspace_access).unwrap_or_default(),
    });
    let bytes = serde_json::to_vec(&runtime)?;
    let digest = Sha256::digest(bytes);
    let mut fingerprint = String::with_capacity(64);
    for byte in digest {
        write!(&mut fingerprint, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(fingerprint)
}

const MANAGED_PACKAGE_ENV: &[&str] = &[
    "NPM_CONFIG_REGISTRY",
    "PIP_INDEX_URL",
    "UV_DEFAULT_INDEX",
    "CARGO_REGISTRIES_VM_INDEX",
    "CARGO_REGISTRIES_VM_TOKEN",
    "CARGO_SOURCE_CRATES_IO_REPLACE_WITH",
    "CARGO_SOURCE_VM_REGISTRY",
    "CARGO_REGISTRY_GLOBAL_CREDENTIAL_PROVIDERS",
    "VM_OCI_MIRROR",
    "NPM_CONFIG_USERCONFIG",
    "PIP_CONFIG_FILE",
    "VM_PACKAGES_CLIENT_URL",
    "VM_PACKAGES_WORK_GATEWAY",
    "VM_PACKAGES_AGENT_TOKEN",
    "VM_PACKAGES_CONSUMER",
    "VM_PACKAGES_CANONICAL_WORKSPACE",
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_fingerprint_tracks_runtime_settings_but_not_project_selection() {
        let mut config = VmConfig::default();
        let original = runtime_fingerprint(&config).unwrap();
        config.project = Some(vm_config::config::ProjectConfig {
            name: Some("demo".into()),
            default_environment: Some("dev".into()),
            ..Default::default()
        });
        assert_eq!(original, runtime_fingerprint(&config).unwrap());
        config
            .environment
            .insert("MODE".into(), "production".into());
        assert_ne!(original, runtime_fingerprint(&config).unwrap());
        config.environment.clear();
        config
            .environment
            .insert("VM_PACKAGES_AGENT_TOKEN".into(), "rotated".into());
        assert_eq!(original, runtime_fingerprint(&config).unwrap());
    }
}
