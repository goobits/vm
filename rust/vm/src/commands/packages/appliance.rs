use std::time::Duration;

use vm_core::{vm_println, vm_progress, vm_success};
use vm_packages::{
    ApplianceConfig, PackageInfrastructureClient, RegistryEndpoints, APPLIANCE_DEFINITION_REVISION,
};

use crate::cli::PackageInfrastructureEngine;
use crate::error::{VmError, VmResult};

use super::{container, files::ApplianceFiles, process, state::ApplianceState};
use vm_config::{config::ProviderName, AppConfig};

const HEALTH_TIMEOUT: Duration = Duration::from_secs(60);
const HEALTH_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum PackageHealth {
    Healthy,
    Degraded,
    ActionRequired,
}

impl PackageHealth {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::ActionRequired => "action required",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum MaintenanceTask<'a> {
    List,
    Backup(&'a str),
    Restore(&'a str),
}

impl<'a> MaintenanceTask<'a> {
    pub(super) fn action(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Backup(_) => "backup",
            Self::Restore(_) => "restore",
        }
    }

    pub(super) fn backup_id(self) -> Option<&'a str> {
        match self {
            Self::List => None,
            Self::Backup(id) | Self::Restore(id) => Some(id),
        }
    }

    pub(super) fn requires_pause(self) -> bool {
        !matches!(self, Self::List)
    }
}

pub(super) fn configured_client(files: &ApplianceFiles) -> VmResult<PackageInfrastructureClient> {
    configured_state_and_client(files).map(|(_, client)| client)
}

pub(super) fn configured_state_and_client(
    files: &ApplianceFiles,
) -> VmResult<(ApplianceState, PackageInfrastructureClient)> {
    let state = files.read_state()?.ok_or_else(|| {
        VmError::validation(
            "Package infrastructure is not configured",
            Some("Run `vm packages up` first"),
        )
    })?;
    let client = workflow_client(files, &state)?;
    Ok((state, client))
}

pub(super) async fn up(
    files: &ApplianceFiles,
    requested: PackageInfrastructureEngine,
    port: u16,
    registry_image: Option<String>,
    job_image: Option<String>,
) -> VmResult<()> {
    let previous = files.read_state()?;
    if let Some(previous) = previous.as_ref() {
        ensure_supported_definition(previous)?;
    }
    let engine_name = resolve_engine(
        requested,
        previous.as_ref().map(|state| state.engine.clone()),
    );
    let engine = vm_provider::ContainerEngine::detect(&engine_name)?;
    let image = resolve_image(
        registry_image,
        previous.as_ref().map(|state| {
            (
                state.controller_version.as_str(),
                state.registry_image.as_str(),
            )
        }),
        default_registry_image(),
    );
    let job_image = resolve_image(
        job_image,
        previous
            .as_ref()
            .map(|state| (state.controller_version.as_str(), state.job_image.as_str())),
        default_job_image(),
    );
    let config = ApplianceConfig::new("0.0.0.0", port, image, job_image).map_err(VmError::from)?;
    let allow_source_build = config.registry_image == default_registry_image()
        && config.job_image == default_job_image();
    files.materialize(&config)?;

    let gateway_url = container::up(engine, files, &config, allow_source_build)?;
    wait_for_gateway(&gateway_url).await?;
    let registry_image_identity = super::source_images::identity(engine, &config.registry_image)?;

    files.write_state(&ApplianceState {
        definition_revision: APPLIANCE_DEFINITION_REVISION,
        engine: engine_name,
        gateway_url: gateway_url.clone(),
        gateway_port: port,
        registry_image: config.registry_image,
        registry_image_identity,
        job_image: config.job_image,
        controller_version: env!("CARGO_PKG_VERSION").to_string(),
    })?;

    vm_success!("Package infrastructure is ready");
    vm_println!("Gateway: {gateway_url}");
    vm_println!("Engine: {}", engine.name());
    Ok(())
}

/// Upgrade an existing appliance whose controller files predate scoped guest
/// credentials. This is intentionally limited to already-configured
/// infrastructure and runs only when required credentials are absent.
pub(super) fn repair_client_access(
    files: &ApplianceFiles,
    fallback: ApplianceState,
) -> VmResult<ApplianceState> {
    ensure_supported_definition(&fallback)?;
    if state_client_access_is_current(files, &fallback)? {
        return Ok(fallback);
    }

    let _lifecycle_lock = files.acquire_lifecycle_lock()?;
    let mut state = files.read_state()?.unwrap_or(fallback);
    ensure_supported_definition(&state)?;
    if state_client_access_is_current(files, &state)? {
        return Ok(state);
    }

    vm_progress!("Repairing package infrastructure client credentials...");
    let registry_image = resolve_image(
        None,
        Some((&state.controller_version, &state.registry_image)),
        default_registry_image(),
    );
    let job_image = resolve_image(
        None,
        Some((&state.controller_version, &state.job_image)),
        default_job_image(),
    );
    let config = ApplianceConfig::new("0.0.0.0", state.gateway_port, registry_image, job_image)
        .map_err(VmError::from)?;
    let allow_source_build = config.registry_image == default_registry_image()
        && config.job_image == default_job_image();
    files.materialize(&config)?;
    let engine = state.container_engine()?;
    state.gateway_url = container::up(engine, files, &config, allow_source_build)?;
    state.registry_image_identity = super::source_images::identity(engine, &config.registry_image)?;
    state.registry_image = config.registry_image;
    state.job_image = config.job_image;
    state.controller_version = env!("CARGO_PKG_VERSION").to_string();
    state.definition_revision = APPLIANCE_DEFINITION_REVISION;
    files.write_state(&state)?;
    Ok(state)
}

pub(super) fn state_client_access_is_current(
    files: &ApplianceFiles,
    state: &ApplianceState,
) -> VmResult<bool> {
    ensure_supported_definition(state)?;
    Ok(state.definition_revision == APPLIANCE_DEFINITION_REVISION
        && files.runtime_credentials_ready()?)
}

fn ensure_supported_definition(state: &ApplianceState) -> VmResult<()> {
    if state.definition_revision > APPLIANCE_DEFINITION_REVISION {
        return Err(VmError::validation(
            format!(
                "Package infrastructure uses newer definition revision {}; this VM CLI supports {}",
                state.definition_revision, APPLIANCE_DEFINITION_REVISION
            ),
            Some("Run `vm update`, then retry"),
        ));
    }
    Ok(())
}

pub(super) fn down(files: &ApplianceFiles) -> VmResult<()> {
    let Some(state) = files.read_state()? else {
        vm_println!("Package infrastructure is not configured");
        return Ok(());
    };
    container::down(state.container_engine()?, files)?;
    vm_success!("Package infrastructure stopped; named volumes were preserved");
    Ok(())
}

pub(super) async fn status(files: &ApplianceFiles) -> VmResult<PackageHealth> {
    let Some(state) = files.read_state()? else {
        return Ok(PackageHealth::ActionRequired);
    };
    let engine_status = container::status(state.container_engine()?, files)?;
    let healthy = engine_status == "running" && gateway_is_healthy(&state.gateway_url).await;

    Ok(if healthy && files.runtime_credentials_ready()? {
        PackageHealth::Healthy
    } else {
        PackageHealth::ActionRequired
    })
}

pub(super) async fn doctor(files: &ApplianceFiles) -> VmResult<()> {
    let state = files.read_state()?;
    let engine_name = state
        .as_ref()
        .map(|state| state.engine.clone())
        .unwrap_or_else(first_run_engine);

    files.validate_definition()?;
    let engine = vm_provider::ContainerEngine::detect(&engine_name)?;
    container::doctor(engine, files)?;

    if let Some(state) = state {
        if !gateway_is_healthy(&state.gateway_url).await {
            return Err(VmError::validation(
                "Package gateway is not healthy",
                Some("Run `vm packages up` and inspect the appliance logs"),
            ));
        }
        workflow_client(files, &state)?.checkouts().await?;
    }
    vm_success!("Package infrastructure checks passed");
    Ok(())
}

pub(super) fn list_backups(files: &ApplianceFiles) -> VmResult<()> {
    maintenance(files, MaintenanceTask::List)
}

pub(super) fn backup(files: &ApplianceFiles) -> VmResult<()> {
    let backup_id = format!(
        "backup-{}-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        vm_core::secrets::generate_random_password(8)
    );
    maintenance(files, MaintenanceTask::Backup(&backup_id))?;
    vm_success!("Package infrastructure backup created");
    vm_println!("Backup: {backup_id}");
    Ok(())
}

pub(super) fn restore(files: &ApplianceFiles, backup_id: &str) -> VmResult<()> {
    maintenance(files, MaintenanceTask::Restore(backup_id))?;
    vm_success!("Package infrastructure restored from {backup_id}");
    Ok(())
}

fn maintenance(files: &ApplianceFiles, task: MaintenanceTask<'_>) -> VmResult<()> {
    let _maintenance_lock = files.acquire_maintenance_lock()?;
    let state = files.read_state()?.ok_or_else(|| {
        VmError::validation(
            "Package infrastructure is not configured",
            Some("Run `vm packages up` first"),
        )
    })?;
    if let Some(backup_id) = task.backup_id() {
        process::validate_job_id(backup_id)?;
    }
    let output = container::maintenance(state.container_engine()?, files, task)?;
    if matches!(task, MaintenanceTask::List) {
        if output.trim().is_empty() {
            vm_println!("No package infrastructure backups");
        } else {
            vm_println!("{output}");
        }
    }
    Ok(())
}

fn resolve_engine(
    requested: PackageInfrastructureEngine,
    previous: Option<ProviderName>,
) -> ProviderName {
    match requested {
        PackageInfrastructureEngine::Auto => previous.unwrap_or_else(first_run_engine),
        PackageInfrastructureEngine::Docker => ProviderName::Docker,
        PackageInfrastructureEngine::Podman => ProviderName::Podman,
    }
}

fn first_run_engine() -> ProviderName {
    AppConfig::load(None, None, None)
        .map(|config| config.container_provider())
        .unwrap_or_default()
}

fn default_registry_image() -> String {
    format!(
        "ghcr.io/goobits/vm-package-server:{}",
        env!("CARGO_PKG_VERSION")
    )
}

fn default_job_image() -> String {
    format!(
        "ghcr.io/goobits/vm-package-jobs:{}",
        env!("CARGO_PKG_VERSION")
    )
}

fn resolve_image(
    requested: Option<String>,
    previous: Option<(&str, &str)>,
    default: String,
) -> String {
    requested
        .or_else(|| {
            previous
                .filter(|(version, image)| {
                    *version == env!("CARGO_PKG_VERSION") && !image.trim().is_empty()
                })
                .map(|(_, image)| image.to_string())
        })
        .unwrap_or(default)
}

async fn wait_for_gateway(gateway_url: &str) -> VmResult<()> {
    let endpoints = RegistryEndpoints::new(gateway_url).map_err(VmError::from)?;
    let client = PackageInfrastructureClient::new(endpoints);
    let deadline = tokio::time::Instant::now() + HEALTH_TIMEOUT;
    loop {
        if client.is_fully_healthy().await {
            return Ok(());
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        tokio::time::sleep(HEALTH_INTERVAL.min(deadline - now)).await;
    }
    Err(VmError::validation(
        format!("Package gateway did not become healthy at {gateway_url}"),
        Some("Run `vm packages doctor` and inspect the appliance logs"),
    ))
}

async fn gateway_is_healthy(gateway_url: &str) -> bool {
    let Ok(endpoints) = RegistryEndpoints::new(gateway_url) else {
        return false;
    };
    PackageInfrastructureClient::new(endpoints)
        .is_fully_healthy()
        .await
}

fn workflow_client(
    files: &ApplianceFiles,
    state: &ApplianceState,
) -> VmResult<PackageInfrastructureClient> {
    let endpoints = RegistryEndpoints::new(&state.gateway_url).map_err(VmError::from)?;
    Ok(PackageInfrastructureClient::new(endpoints)
        .with_read_token(files.read_token()?)
        .with_controller_token(files.controller_token()?))
}

#[cfg(test)]
mod tests {
    use super::{
        default_registry_image, ensure_supported_definition, resolve_engine, resolve_image,
    };
    use crate::cli::PackageInfrastructureEngine;
    use crate::commands::packages::state::ApplianceState;
    use vm_config::config::ProviderName;
    use vm_packages::APPLIANCE_DEFINITION_REVISION;

    fn state_with_revision(definition_revision: u32) -> ApplianceState {
        ApplianceState {
            definition_revision,
            engine: ProviderName::Docker,
            gateway_url: "http://127.0.0.1:3080".into(),
            gateway_port: 3080,
            registry_image: "registry:1".into(),
            registry_image_identity: "sha256:registry".into(),
            job_image: "jobs:1".into(),
            controller_version: "1".into(),
        }
    }

    #[test]
    fn auto_engine_reuses_state_before_platform_default() {
        assert_eq!(
            resolve_engine(
                PackageInfrastructureEngine::Auto,
                Some(ProviderName::Podman)
            ),
            ProviderName::Podman
        );
    }

    #[test]
    fn explicit_engine_overrides_saved_state() {
        assert_eq!(
            resolve_engine(
                PackageInfrastructureEngine::Docker,
                Some(ProviderName::Podman)
            ),
            ProviderName::Docker
        );
    }

    #[test]
    fn default_image_is_versioned() {
        assert!(default_registry_image().ends_with(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn same_version_reuses_image_overrides_until_the_controller_changes() {
        let local = resolve_image(
            None,
            Some((env!("CARGO_PKG_VERSION"), "registry.local/packages:dev")),
            "registry.example/packages:current".into(),
        );
        assert_eq!(local, "registry.local/packages:dev");

        let upgraded = resolve_image(
            None,
            Some(("0.0.1", "registry.local/packages:dev")),
            "registry.example/packages:current".into(),
        );
        assert_eq!(upgraded, "registry.example/packages:current");
    }

    #[test]
    fn explicit_image_override_wins() {
        let image = resolve_image(
            Some("registry.local/packages:new".into()),
            Some((env!("CARGO_PKG_VERSION"), "registry.local/packages:old")),
            "registry.example/packages:current".into(),
        );
        assert_eq!(image, "registry.local/packages:new");
    }

    #[test]
    fn older_cli_refuses_to_downgrade_newer_appliance_definition() {
        assert!(
            ensure_supported_definition(&state_with_revision(APPLIANCE_DEFINITION_REVISION))
                .is_ok()
        );

        let error =
            ensure_supported_definition(&state_with_revision(APPLIANCE_DEFINITION_REVISION + 1))
                .unwrap_err();
        assert!(error.to_string().contains("newer definition revision"));
        assert_eq!(error.hint(), Some("Run `vm update`, then retry"));
    }
}
