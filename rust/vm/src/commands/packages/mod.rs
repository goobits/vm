mod docker;
mod files;
mod process;
mod tart;

use std::time::Duration;

use crate::cli::{PackageInfrastructureRuntime, PackagesSubcommand};
use crate::error::{VmError, VmResult};
use vm_core::{vm_println, vm_success};
use vm_packages::{
    ApplianceConfig, ApplianceState, InfrastructureRuntime, PackageInfrastructureClient,
    RegistryEndpoints,
};

use files::ApplianceFiles;

const HEALTH_ATTEMPTS: usize = 30;

pub(super) async fn handle(command: PackagesSubcommand) -> VmResult<()> {
    let files = ApplianceFiles::discover()?;
    match command {
        PackagesSubcommand::Up {
            runtime,
            port,
            registry_image,
        } => up(&files, runtime, port, registry_image).await,
        PackagesSubcommand::Down { runtime } => down(&files, runtime),
        PackagesSubcommand::Status { runtime } => status(&files, runtime).await,
        PackagesSubcommand::Doctor { runtime } => doctor(&files, runtime).await,
    }
}

async fn up(
    files: &ApplianceFiles,
    requested: PackageInfrastructureRuntime,
    port: u16,
    registry_image: Option<String>,
) -> VmResult<()> {
    let runtime = resolve_runtime(requested, files)?;
    let image = registry_image.unwrap_or_else(default_registry_image);
    let bind = match runtime {
        InfrastructureRuntime::Docker => "127.0.0.1",
        InfrastructureRuntime::Tart => "0.0.0.0",
    };
    let config = ApplianceConfig::new(bind, port, image).map_err(VmError::from)?;
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
        controller_version: env!("CARGO_PKG_VERSION").to_string(),
    })?;

    vm_success!("Package infrastructure is ready");
    vm_println!("Gateway: {gateway_url}");
    vm_println!("Runtime: {}", runtime.as_str());
    Ok(())
}

fn down(files: &ApplianceFiles, requested: PackageInfrastructureRuntime) -> VmResult<()> {
    let Some(state) = files.read_state()? else {
        vm_println!("Package infrastructure is not configured");
        return Ok(());
    };
    let runtime = explicit_or_state_runtime(requested, state.runtime);
    match runtime {
        InfrastructureRuntime::Docker => docker::down(files)?,
        InfrastructureRuntime::Tart => tart::down(files)?,
    }
    vm_success!("Package infrastructure stopped; named volumes were preserved");
    Ok(())
}

async fn status(files: &ApplianceFiles, requested: PackageInfrastructureRuntime) -> VmResult<()> {
    let Some(state) = files.read_state()? else {
        vm_println!("Package infrastructure: not configured");
        return Ok(());
    };
    let runtime = explicit_or_state_runtime(requested, state.runtime);
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

async fn doctor(files: &ApplianceFiles, requested: PackageInfrastructureRuntime) -> VmResult<()> {
    let state = files.read_state()?;
    let runtime = match (requested, state.as_ref()) {
        (PackageInfrastructureRuntime::Auto, Some(state)) => state.runtime,
        (PackageInfrastructureRuntime::Auto, None) => InfrastructureRuntime::Docker,
        _ => map_runtime(requested).expect("explicit runtime maps to a value"),
    };

    files.validate_definition()?;
    match runtime {
        InfrastructureRuntime::Docker => docker::doctor(files)?,
        InfrastructureRuntime::Tart => tart::doctor(files)?,
    }

    if let Some(state) = state {
        if state.runtime == runtime && !gateway_is_healthy(&state.gateway_url).await {
            return Err(VmError::validation(
                "Package gateway is not healthy",
                Some("Run `vm packages up` and inspect the appliance logs"),
            ));
        }
    }
    vm_success!("Package infrastructure checks passed");
    Ok(())
}

fn resolve_runtime(
    requested: PackageInfrastructureRuntime,
    files: &ApplianceFiles,
) -> VmResult<InfrastructureRuntime> {
    if let Some(runtime) = map_runtime(requested) {
        return Ok(runtime);
    }
    Ok(files
        .read_state()?
        .map_or(InfrastructureRuntime::Docker, |state| state.runtime))
}

fn explicit_or_state_runtime(
    requested: PackageInfrastructureRuntime,
    state_runtime: InfrastructureRuntime,
) -> InfrastructureRuntime {
    map_runtime(requested).unwrap_or(state_runtime)
}

fn map_runtime(requested: PackageInfrastructureRuntime) -> Option<InfrastructureRuntime> {
    match requested {
        PackageInfrastructureRuntime::Auto => None,
        PackageInfrastructureRuntime::Docker => Some(InfrastructureRuntime::Docker),
        PackageInfrastructureRuntime::Tart => Some(InfrastructureRuntime::Tart),
    }
}

fn default_registry_image() -> String {
    format!(
        "ghcr.io/goobits/vm-package-server:{}",
        env!("CARGO_PKG_VERSION")
    )
}

async fn wait_for_gateway(gateway_url: &str) -> VmResult<()> {
    for _ in 0..HEALTH_ATTEMPTS {
        if gateway_is_healthy(gateway_url).await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
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
        .is_healthy()
        .await
}

#[cfg(test)]
mod tests {
    use super::{default_registry_image, map_runtime};
    use crate::cli::PackageInfrastructureRuntime;
    use vm_packages::InfrastructureRuntime;

    #[test]
    fn explicit_runtime_mapping_is_stable() {
        assert_eq!(
            map_runtime(PackageInfrastructureRuntime::Docker),
            Some(InfrastructureRuntime::Docker)
        );
        assert_eq!(map_runtime(PackageInfrastructureRuntime::Auto), None);
    }

    #[test]
    fn default_image_is_versioned() {
        assert!(default_registry_image().ends_with(env!("CARGO_PKG_VERSION")));
    }
}
