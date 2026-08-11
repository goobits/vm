use std::time::Duration;

use vm_core::{vm_println, vm_success};
use vm_packages::{
    ApplianceConfig, ApplianceState, InfrastructureRuntime, PackageInfrastructureClient,
    RegistryEndpoints,
};

use crate::cli::PackageInfrastructureRuntime;
use crate::error::{VmError, VmResult};

use super::{docker, files::ApplianceFiles, process, tart};

const HEALTH_TIMEOUT: Duration = Duration::from_secs(60);
const HEALTH_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy)]
pub(super) enum PackageJob<'a> {
    Review(&'a str),
    Release(&'a str),
    Rollout(&'a str),
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

impl<'a> PackageJob<'a> {
    pub(super) fn service(self) -> &'static str {
        match self {
            Self::Review(_) => "reviewer",
            Self::Release(_) => "releaser",
            Self::Rollout(_) => "rollout",
        }
    }

    pub(super) fn variable(self) -> &'static str {
        match self {
            Self::Review(_) | Self::Release(_) => "SUBMISSION_ID",
            Self::Rollout(_) => "ROLLOUT_ID",
        }
    }

    pub(super) fn id(self) -> &'a str {
        match self {
            Self::Review(id) | Self::Release(id) | Self::Rollout(id) => id,
        }
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

pub(super) fn launch_job(
    files: &ApplianceFiles,
    state: &ApplianceState,
    job: PackageJob<'_>,
) -> VmResult<()> {
    if state.job_image.is_empty() {
        return Err(VmError::validation(
            "Package appliance state predates integration review support",
            Some("Run `vm packages up` to refresh it"),
        ));
    }
    match state.runtime {
        InfrastructureRuntime::Docker => docker::run_job(files, job),
        InfrastructureRuntime::Tart => tart::run_job(files, job),
    }
}

pub(super) fn launch_review(
    files: &ApplianceFiles,
    state: &ApplianceState,
    submission_id: &str,
) -> VmResult<()> {
    launch_job(files, state, PackageJob::Review(submission_id))
}

pub(super) async fn up(
    files: &ApplianceFiles,
    requested: PackageInfrastructureRuntime,
    port: u16,
    registry_image: Option<String>,
    job_image: Option<String>,
) -> VmResult<()> {
    let runtime = resolve_runtime(requested, files.read_state()?.map(|state| state.runtime));
    let image = registry_image.unwrap_or_else(default_registry_image);
    let job_image = job_image.unwrap_or_else(default_job_image);
    let bind = match runtime {
        InfrastructureRuntime::Docker => "127.0.0.1",
        InfrastructureRuntime::Tart => "0.0.0.0",
    };
    let config = ApplianceConfig::new(bind, port, image, job_image).map_err(VmError::from)?;
    files.materialize(&config)?;

    let gateway_url = match runtime {
        InfrastructureRuntime::Docker => docker::up(files, port)?,
        InfrastructureRuntime::Tart => tart::up(files, port)?,
    };
    wait_for_gateway(&gateway_url).await?;

    files.write_state(&ApplianceState {
        runtime,
        gateway_url: gateway_url.clone(),
        gateway_port: port,
        registry_image: config.registry_image,
        job_image: config.job_image,
        controller_version: env!("CARGO_PKG_VERSION").to_string(),
    })?;

    vm_success!("Package infrastructure is ready");
    vm_println!("Gateway: {gateway_url}");
    vm_println!("Runtime: {}", runtime.as_str());
    Ok(())
}

pub(super) fn down(
    files: &ApplianceFiles,
    requested: PackageInfrastructureRuntime,
) -> VmResult<()> {
    let Some(state) = files.read_state()? else {
        vm_println!("Package infrastructure is not configured");
        return Ok(());
    };
    match resolve_runtime(requested, Some(state.runtime)) {
        InfrastructureRuntime::Docker => docker::down(files)?,
        InfrastructureRuntime::Tart => tart::down(files)?,
    }
    vm_success!("Package infrastructure stopped; named volumes were preserved");
    Ok(())
}

pub(super) async fn status(
    files: &ApplianceFiles,
    requested: PackageInfrastructureRuntime,
) -> VmResult<()> {
    let Some(state) = files.read_state()? else {
        vm_println!("Package infrastructure: not configured");
        return Ok(());
    };
    let runtime = resolve_runtime(requested, Some(state.runtime));
    let runtime_status = match runtime {
        InfrastructureRuntime::Docker => docker::status(files)?,
        InfrastructureRuntime::Tart => tart::status(files)?,
    };
    let gateway_url = match runtime {
        InfrastructureRuntime::Docker => state.gateway_url.clone(),
        InfrastructureRuntime::Tart if runtime_status == "running" => {
            tart::gateway_url(state.gateway_port).unwrap_or_else(|_| state.gateway_url.clone())
        }
        InfrastructureRuntime::Tart => state.gateway_url.clone(),
    };
    let healthy = runtime_status == "running" && gateway_is_healthy(&gateway_url).await;

    vm_println!("Package infrastructure");
    vm_println!("  Runtime: {}", runtime.as_str());
    vm_println!("  State: {runtime_status}");
    vm_println!("  Gateway: {gateway_url}");
    vm_println!(
        "  Health: {}",
        if healthy { "healthy" } else { "unavailable" }
    );
    vm_println!("  Storage: persistent named volumes");
    Ok(())
}

pub(super) async fn doctor(
    files: &ApplianceFiles,
    requested: PackageInfrastructureRuntime,
) -> VmResult<()> {
    let state = files.read_state()?;
    let runtime = resolve_runtime(requested, state.as_ref().map(|state| state.runtime));

    files.validate_definition()?;
    match runtime {
        InfrastructureRuntime::Docker => docker::doctor(files)?,
        InfrastructureRuntime::Tart => tart::doctor(files)?,
    }

    if let Some(state) = state.filter(|state| state.runtime == runtime) {
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

pub(super) fn list_backups(
    files: &ApplianceFiles,
    runtime: PackageInfrastructureRuntime,
) -> VmResult<()> {
    maintenance(files, runtime, MaintenanceTask::List)
}

pub(super) fn backup(
    files: &ApplianceFiles,
    runtime: PackageInfrastructureRuntime,
) -> VmResult<()> {
    let backup_id = format!(
        "backup-{}-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        vm_core::secrets::generate_random_password(8)
    );
    maintenance(files, runtime, MaintenanceTask::Backup(&backup_id))?;
    vm_success!("Package infrastructure backup created");
    vm_println!("Backup: {backup_id}");
    Ok(())
}

pub(super) fn restore(
    files: &ApplianceFiles,
    runtime: PackageInfrastructureRuntime,
    backup_id: &str,
) -> VmResult<()> {
    maintenance(files, runtime, MaintenanceTask::Restore(backup_id))?;
    vm_success!("Package infrastructure restored from {backup_id}");
    Ok(())
}

fn maintenance(
    files: &ApplianceFiles,
    requested: PackageInfrastructureRuntime,
    task: MaintenanceTask<'_>,
) -> VmResult<()> {
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
    let output = match resolve_runtime(requested, Some(state.runtime)) {
        InfrastructureRuntime::Docker => docker::maintenance(files, task)?,
        InfrastructureRuntime::Tart => tart::maintenance(files, task)?,
    };
    if matches!(task, MaintenanceTask::List) {
        if output.trim().is_empty() {
            vm_println!("No package infrastructure backups");
        } else {
            vm_println!("{output}");
        }
    }
    Ok(())
}

fn resolve_runtime(
    requested: PackageInfrastructureRuntime,
    previous: Option<InfrastructureRuntime>,
) -> InfrastructureRuntime {
    match requested {
        PackageInfrastructureRuntime::Auto => previous.unwrap_or_else(first_run_runtime),
        PackageInfrastructureRuntime::Docker => InfrastructureRuntime::Docker,
        PackageInfrastructureRuntime::Tart => InfrastructureRuntime::Tart,
    }
}

fn first_run_runtime() -> InfrastructureRuntime {
    first_run_runtime_for(cfg!(target_os = "macos"))
}

const fn first_run_runtime_for(macos: bool) -> InfrastructureRuntime {
    if macos {
        InfrastructureRuntime::Tart
    } else {
        InfrastructureRuntime::Docker
    }
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
    use super::{default_registry_image, first_run_runtime_for, resolve_runtime};
    use crate::cli::PackageInfrastructureRuntime;
    use vm_packages::InfrastructureRuntime;

    #[test]
    fn auto_runtime_reuses_state_before_platform_default() {
        assert_eq!(
            resolve_runtime(
                PackageInfrastructureRuntime::Auto,
                Some(InfrastructureRuntime::Docker)
            ),
            InfrastructureRuntime::Docker
        );
        assert_eq!(
            resolve_runtime(
                PackageInfrastructureRuntime::Auto,
                Some(InfrastructureRuntime::Tart)
            ),
            InfrastructureRuntime::Tart
        );
    }

    #[test]
    fn first_run_prefers_tart_only_on_macos() {
        assert_eq!(first_run_runtime_for(true), InfrastructureRuntime::Tart);
        assert_eq!(first_run_runtime_for(false), InfrastructureRuntime::Docker);
    }

    #[test]
    fn explicit_runtime_overrides_saved_state() {
        assert_eq!(
            resolve_runtime(
                PackageInfrastructureRuntime::Tart,
                Some(InfrastructureRuntime::Docker)
            ),
            InfrastructureRuntime::Tart
        );
    }

    #[test]
    fn default_image_is_versioned() {
        assert!(default_registry_image().ends_with(env!("CARGO_PKG_VERSION")));
    }
}
