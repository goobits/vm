mod catalog;
mod checkout;
mod docker;
mod files;
mod integration;
mod process;
mod runtime;
mod submission;
mod tart;

use std::{path::PathBuf, time::Duration};

use crate::cli::{PackageInfrastructureRuntime, PackagesSubcommand};
use crate::error::{VmError, VmResult};
use vm_config::config::VmConfig;
use vm_core::{vm_println, vm_success};
use vm_packages::{
    ApplianceConfig, ApplianceState, ClientEnvironment, InfrastructureRuntime,
    PackageInfrastructureClient, RegistryEndpoints,
};

use files::ApplianceFiles;

const HEALTH_ATTEMPTS: usize = 30;

pub(super) fn apply_client_environment(config: &mut VmConfig) -> VmResult<()> {
    let files = ApplianceFiles::discover()?;
    let Some(state) = files.read_state()? else {
        return Ok(());
    };
    let provider = config.provider.as_deref().unwrap_or("docker");
    let client = client_environment(&state, files.read_token()?, provider)?;
    apply_environment(config, &client);
    Ok(())
}

fn apply_environment(config: &mut VmConfig, client: &ClientEnvironment) {
    for (key, value) in client.variables() {
        config.environment.insert(key, value);
    }
}

fn client_environment(
    state: &ApplianceState,
    read_token: String,
    provider: &str,
) -> VmResult<ClientEnvironment> {
    let gateway = gateway_for_provider(state, provider)?;
    let endpoints = RegistryEndpoints::new(gateway).map_err(VmError::from)?;
    ClientEnvironment::new(endpoints, read_token).map_err(VmError::from)
}

fn gateway_for_provider(state: &ApplianceState, provider: &str) -> VmResult<String> {
    match (state.runtime, provider) {
        (InfrastructureRuntime::Docker, "tart") => Err(VmError::validation(
            "A Docker-hosted package appliance is not reachable from Tart guests",
            Some("Run `vm packages up --runtime tart` so every environment can reach it"),
        )),
        (InfrastructureRuntime::Docker, "docker" | "podman") => Ok(format!(
            "http://{}:{}",
            vm_platform::platform::get_host_gateway(),
            state.gateway_port
        )),
        (InfrastructureRuntime::Docker, _) => Err(VmError::validation(
            format!("Provider '{provider}' cannot reach a Docker-hosted package appliance"),
            Some("Use the Tart package infrastructure runtime"),
        )),
        (InfrastructureRuntime::Tart, _) => Ok(state.gateway_url.clone()),
    }
}

pub(super) async fn handle(
    command: PackagesSubcommand,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let files = ApplianceFiles::discover()?;
    match command {
        PackagesSubcommand::Up {
            runtime,
            port,
            registry_image,
            review_image,
        } => up(&files, runtime, port, registry_image, review_image).await,
        PackagesSubcommand::Down { runtime } => down(&files, runtime),
        PackagesSubcommand::Status { runtime } => status(&files, runtime).await,
        PackagesSubcommand::Doctor { runtime } => doctor(&files, runtime).await,
        PackagesSubcommand::Register {
            name,
            ecosystem,
            repository,
            branch,
        } => catalog::register(&files, name, ecosystem, repository, branch).await,
        PackagesSubcommand::List => catalog::list(&files).await,
        PackagesSubcommand::Checkout {
            package,
            agent,
            consumer,
            task,
        } => {
            checkout::handle(
                &files,
                checkout::CheckoutIntent {
                    config_path,
                    profile,
                    package,
                    agent,
                    consumer,
                    task,
                },
            )
            .await
        }
        PackagesSubcommand::Show { checkout_id } => catalog::show(&files, &checkout_id).await,
        PackagesSubcommand::Submit {
            checkout_id,
            consumer,
        } => submission::handle(&files, config_path, profile, checkout_id, consumer).await,
        PackagesSubcommand::Integrate {
            submission_id,
            consumer,
            strategy,
        } => {
            integration::handle(
                &files,
                config_path,
                profile,
                submission_id,
                consumer,
                strategy,
            )
            .await
        }
        PackagesSubcommand::Auth { token_file, clear } => {
            catalog::configure_git_auth(&files, token_file, clear)
        }
    }
}

fn configured_client(files: &ApplianceFiles) -> VmResult<PackageInfrastructureClient> {
    configured_state_and_client(files).map(|(_, client)| client)
}

fn configured_state_and_client(
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

fn launch_review(
    files: &ApplianceFiles,
    state: &ApplianceState,
    submission_id: &str,
) -> VmResult<()> {
    if state.review_image.is_empty() {
        return Err(VmError::validation(
            "Package appliance state predates integration review support",
            Some("Run `vm packages up` to refresh it"),
        ));
    }
    match state.runtime {
        InfrastructureRuntime::Docker => docker::review(files, submission_id),
        InfrastructureRuntime::Tart => tart::review(files, submission_id),
    }
}

async fn up(
    files: &ApplianceFiles,
    requested: PackageInfrastructureRuntime,
    port: u16,
    registry_image: Option<String>,
    review_image: Option<String>,
) -> VmResult<()> {
    let runtime = resolve_runtime(requested, files)?;
    let image = registry_image.unwrap_or_else(default_registry_image);
    let review_image = review_image.unwrap_or_else(default_review_image);
    let bind = match runtime {
        InfrastructureRuntime::Docker => "127.0.0.1",
        InfrastructureRuntime::Tart => "0.0.0.0",
    };
    let config = ApplianceConfig::new(bind, port, image, review_image).map_err(VmError::from)?;
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
        review_image: config.review_image,
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
        if state.runtime == runtime {
            if !gateway_is_healthy(&state.gateway_url).await {
                return Err(VmError::validation(
                    "Package gateway is not healthy",
                    Some("Run `vm packages up` and inspect the appliance logs"),
                ));
            }
            workflow_client(files, &state)?.checkouts().await?;
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

fn default_review_image() -> String {
    format!(
        "ghcr.io/goobits/vm-package-review:{}",
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
    use super::{apply_environment, client_environment, default_registry_image, map_runtime};
    use crate::cli::PackageInfrastructureRuntime;
    use vm_config::config::VmConfig;
    use vm_packages::{ApplianceState, InfrastructureRuntime};

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

    #[test]
    fn tart_appliance_has_one_provider_neutral_client_shape() {
        let state = ApplianceState {
            runtime: InfrastructureRuntime::Tart,
            gateway_url: "http://192.0.2.8:3080".into(),
            gateway_port: 3080,
            registry_image: "registry/image:1".into(),
            review_image: "review/image:1".into(),
            controller_version: "1".into(),
        };
        let docker = client_environment(&state, "read-token".into(), "docker").unwrap();
        let tart = client_environment(&state, "read-token".into(), "tart").unwrap();
        assert_eq!(docker.variables(), tart.variables());
    }

    #[test]
    fn tart_guests_reject_a_loopback_only_docker_appliance() {
        let state = ApplianceState {
            runtime: InfrastructureRuntime::Docker,
            gateway_url: "http://127.0.0.1:3080".into(),
            gateway_port: 3080,
            registry_image: "registry/image:1".into(),
            review_image: "review/image:1".into(),
            controller_version: "1".into(),
        };
        assert!(client_environment(&state, "read-token".into(), "tart").is_err());
    }

    #[test]
    fn managed_package_settings_override_project_redirects() {
        let endpoints =
            vm_packages::RegistryEndpoints::new("http://packages.internal:3080").unwrap();
        let client = vm_packages::ClientEnvironment::new(endpoints, "read-token").unwrap();
        let mut config = VmConfig::default();
        config.environment.insert(
            "NPM_CONFIG_REGISTRY".into(),
            "https://untrusted.invalid".into(),
        );

        apply_environment(&mut config, &client);

        assert!(config.environment["NPM_CONFIG_REGISTRY"].contains("packages.internal"));
        assert_eq!(
            config.environment["CARGO_SOURCE_CRATES_IO_REPLACE_WITH"],
            "vm"
        );
    }
}
