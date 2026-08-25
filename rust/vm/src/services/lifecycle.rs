use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use anyhow::Result;
use futures_util::future;
use tokio::time::sleep;
use tracing::{debug, info, warn};
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::{vm_progress, vm_success, vm_warning};

use super::{
    registry::{self, Services},
    state::{ServiceState, ServiceStateStore},
};

#[derive(Clone)]
pub(crate) struct ServiceLifecycle {
    state: ServiceStateStore,
    services: Arc<Services>,
    readiness_attempts: usize,
    readiness_interval: Duration,
}

impl ServiceLifecycle {
    pub(crate) fn new() -> Result<Self> {
        Self::from_services(
            registry::managed_services(),
            vm_core::user_paths::services_state_path()?,
        )
    }

    fn from_services(services: Services, state_path: std::path::PathBuf) -> Result<Self> {
        let lifecycle = Self {
            state: ServiceStateStore::new(state_path),
            services: Arc::new(services),
            readiness_attempts: 10,
            readiness_interval: Duration::from_secs(2),
        };
        if let Err(error) = lifecycle
            .state
            .load(|name| lifecycle.services.contains_key(name))
        {
            warn!(%error, "Failed to load service state");
            debug!("Starting with clean service state");
        }
        Ok(lifecycle)
    }

    pub(crate) async fn register_vm_services(
        &self,
        vm_name: &str,
        vm_config: &VmConfig,
        global_config: &GlobalConfig,
    ) -> Result<()> {
        info!(vm_name, "Registering services for environment");
        let selected = self.selected_services(vm_config, global_config);
        let mut services_needing_start = Vec::new();
        self.state.update(|states| {
            for service_name in selected {
                let port = self.service_port(service_name, global_config);
                let state =
                    states
                        .entry(service_name.to_string())
                        .or_insert_with(|| ServiceState {
                            port,
                            ..Default::default()
                        });
                if !state.registered_vms.iter().any(|name| name == vm_name) {
                    state.registered_vms.push(vm_name.to_string());
                    state.reference_count += 1;
                    info!(
                        vm_name,
                        service_name,
                        reference_count = state.reference_count,
                        "Environment registered for service"
                    );
                }
                if !state.is_running {
                    services_needing_start.push(service_name.to_string());
                }
            }
        })?;

        for service_name in services_needing_start {
            if let Err(error) = self.start_service(&service_name, global_config).await {
                warn!(service_name, %error, "Failed to start service");
                vm_warning!("Service '{service_name}' failed to start: {error}");
            }
        }
        self.state.save()
    }

    pub(crate) async fn unregister_vm_services(
        &self,
        vm_name: &str,
        global_config: &GlobalConfig,
    ) -> Result<()> {
        info!(vm_name, "Unregistering services for environment");
        let services_to_stop = self.state.update(|states| {
            let mut services_to_stop = Vec::new();
            for (service_name, state) in states {
                let Some(position) = state.registered_vms.iter().position(|name| name == vm_name)
                else {
                    continue;
                };
                state.registered_vms.remove(position);
                state.reference_count = state.reference_count.saturating_sub(1);
                info!(
                    vm_name,
                    service_name,
                    reference_count = state.reference_count,
                    "Environment unregistered from service"
                );
                if state.reference_count == 0 && state.is_running {
                    services_to_stop.push(service_name.clone());
                }
            }
            services_to_stop
        })?;

        future::join_all(services_to_stop.into_iter().map(|service_name| {
            let lifecycle = self.clone();
            async move {
                if let Err(error) = lifecycle.stop_service(&service_name, global_config).await {
                    warn!(service_name, %error, "Failed to stop service");
                }
            }
        }))
        .await;
        self.state.save()
    }

    pub(crate) fn service_status(&self, service_name: &str) -> Option<ServiceState> {
        self.state.get(service_name)
    }

    pub(crate) async fn ensure_service_running(
        &self,
        service_name: &str,
        global_config: &GlobalConfig,
    ) -> Result<()> {
        let healthy = self.check_health(service_name, global_config).await;
        let port = self.service_port(service_name, global_config);
        self.state.update(|states| {
            let state = states
                .entry(service_name.to_string())
                .or_insert_with(|| ServiceState {
                    port,
                    ..Default::default()
                });
            state.is_running = healthy;
            state.port = port;
        })?;
        if healthy {
            return self.state.save();
        }
        self.start_service(service_name, global_config).await?;
        self.state.save()
    }

    async fn start_service(&self, service_name: &str, global_config: &GlobalConfig) -> Result<()> {
        let service = self.service(service_name)?;
        let port = service.get_port(global_config);
        info!(service_name, port, "Starting service");
        vm_progress!("Starting {} on port {port}...", service.name());
        service.start(global_config).await?;

        for attempt in 1..=self.readiness_attempts {
            if service.check_health(global_config).await {
                self.mark_running(service_name)?;
                vm_success!("Service '{service_name}' started successfully");
                return Ok(());
            }
            debug!(
                service_name,
                attempt,
                attempts = self.readiness_attempts,
                "Service not ready"
            );
            if attempt < self.readiness_attempts {
                sleep(self.readiness_interval).await;
            }
        }
        if let Err(error) = service.stop(global_config).await {
            warn!(service_name, %error, "Failed to stop unhealthy service");
        }
        anyhow::bail!("Service '{service_name}' failed to start properly")
    }

    async fn stop_service(&self, service_name: &str, global_config: &GlobalConfig) -> Result<()> {
        let service = self.service(service_name)?;
        info!(service_name, "Stopping service");
        vm_progress!("Stopping {}...", service.name());
        service.stop(global_config).await?;
        self.state.update(|states| {
            if let Some(state) = states.get_mut(service_name) {
                state.is_running = false;
                state.pid = None;
            }
        })?;
        vm_success!("Service '{service_name}' stopped");
        Ok(())
    }

    fn selected_services<'a>(
        &'a self,
        vm_config: &'a VmConfig,
        global_config: &'a GlobalConfig,
    ) -> impl Iterator<Item = &'static str> + 'a {
        ["auth_proxy", "postgresql", "redis", "mongodb", "mysql"]
            .into_iter()
            .filter(|name| {
                vm_config
                    .services
                    .get(*name)
                    .is_some_and(|service| service.enabled)
                    || match *name {
                        "postgresql" => global_config.services.postgresql.enabled,
                        "redis" => global_config.services.redis.enabled,
                        "mongodb" => global_config.services.mongodb.enabled,
                        "mysql" => global_config.services.mysql.enabled,
                        "auth_proxy" => global_config.services.auth_proxy.enabled,
                        _ => false,
                    }
            })
    }

    fn service(&self, service_name: &str) -> Result<Arc<dyn super::ManagedService>> {
        self.services
            .get(service_name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Unknown service: {service_name}"))
    }

    fn service_port(&self, service_name: &str, global_config: &GlobalConfig) -> u16 {
        self.services
            .get(service_name)
            .map_or(0, |service| service.get_port(global_config))
    }

    fn mark_running(&self, service_name: &str) -> Result<()> {
        self.state.update(|states| {
            if let Some(state) = states.get_mut(service_name) {
                state.is_running = true;
            }
        })
    }

    async fn check_health(&self, service_name: &str, global_config: &GlobalConfig) -> bool {
        match self.services.get(service_name) {
            Some(service) => service.check_health(global_config).await,
            None => false,
        }
    }
}

impl Default for ServiceLifecycle {
    fn default() -> Self {
        Self::new().expect("Failed to create service lifecycle")
    }
}

static SERVICE_LIFECYCLE: OnceLock<Result<ServiceLifecycle, String>> = OnceLock::new();

pub(crate) fn service_lifecycle() -> Result<&'static ServiceLifecycle> {
    match SERVICE_LIFECYCLE
        .get_or_init(|| ServiceLifecycle::new().map_err(|error| error.to_string()))
    {
        Ok(lifecycle) => Ok(lifecycle),
        Err(error) => Err(anyhow::anyhow!(
            "Failed to initialize service lifecycle: {error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{ServiceLifecycle, Services};
    use crate::services::ManagedService;
    use anyhow::{bail, Result};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use vm_config::{config::ServiceConfig, config::VmConfig, GlobalConfig};

    struct TestService {
        fail_start: AtomicBool,
        healthy: AtomicBool,
        starts: AtomicUsize,
        stops: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl ManagedService for TestService {
        async fn start(&self, _global_config: &GlobalConfig) -> Result<()> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            if self.fail_start.load(Ordering::SeqCst) {
                bail!("start failed");
            }
            Ok(())
        }

        async fn stop(&self, _global_config: &GlobalConfig) -> Result<()> {
            self.stops.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn check_health(&self, _global_config: &GlobalConfig) -> bool {
            self.healthy.load(Ordering::SeqCst)
        }

        fn name(&self) -> &str {
            "postgresql"
        }

        fn get_port(&self, _global_config: &GlobalConfig) -> u16 {
            3739
        }
    }

    fn lifecycle(service: Arc<TestService>, state_file: std::path::PathBuf) -> ServiceLifecycle {
        let services: Services = [("postgresql".into(), service as Arc<dyn ManagedService>)]
            .into_iter()
            .collect();
        let mut lifecycle = ServiceLifecycle::from_services(services, state_file).unwrap();
        lifecycle.readiness_attempts = 1;
        lifecycle.readiness_interval = Duration::ZERO;
        lifecycle
    }

    fn enabled_config() -> VmConfig {
        let mut config = VmConfig::default();
        config.services.insert(
            "postgresql".into(),
            ServiceConfig {
                enabled: true,
                ..Default::default()
            },
        );
        config
    }

    fn test_service(fail_start: bool, healthy: bool) -> Arc<TestService> {
        Arc::new(TestService {
            fail_start: AtomicBool::new(fail_start),
            healthy: AtomicBool::new(healthy),
            starts: AtomicUsize::new(0),
            stops: AtomicUsize::new(0),
        })
    }

    #[tokio::test]
    async fn failed_start_remains_retryable_for_the_same_vm() {
        let directory = tempfile::tempdir().unwrap();
        let service = test_service(true, true);
        let lifecycle = lifecycle(service.clone(), directory.path().join("services.json"));
        let config = enabled_config();
        let global = GlobalConfig::default();

        lifecycle
            .register_vm_services("demo-dev", &config, &global)
            .await
            .unwrap();
        assert!(!lifecycle.service_status("postgresql").unwrap().is_running);
        service.fail_start.store(false, Ordering::SeqCst);
        lifecycle
            .register_vm_services("demo-dev", &config, &global)
            .await
            .unwrap();

        let state = lifecycle.service_status("postgresql").unwrap();
        assert!(state.is_running);
        assert_eq!(state.reference_count, 1);
        assert_eq!(service.starts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn unhealthy_start_is_stopped_and_remains_retryable() {
        let directory = tempfile::tempdir().unwrap();
        let service = test_service(false, false);
        let lifecycle = lifecycle(service.clone(), directory.path().join("services.json"));
        let config = enabled_config();
        let global = GlobalConfig::default();

        lifecycle
            .register_vm_services("demo-dev", &config, &global)
            .await
            .unwrap();
        assert!(!lifecycle.service_status("postgresql").unwrap().is_running);
        assert_eq!(service.stops.load(Ordering::SeqCst), 1);
        service.healthy.store(true, Ordering::SeqCst);
        lifecycle
            .register_vm_services("demo-dev", &config, &global)
            .await
            .unwrap();
        assert!(lifecycle.service_status("postgresql").unwrap().is_running);
        assert_eq!(service.starts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn healthy_direct_use_does_not_create_a_vm_reference() {
        let directory = tempfile::tempdir().unwrap();
        let service = test_service(false, true);
        let lifecycle = lifecycle(service.clone(), directory.path().join("services.json"));

        lifecycle
            .ensure_service_running("postgresql", &GlobalConfig::default())
            .await
            .unwrap();
        let state = lifecycle.service_status("postgresql").unwrap();
        assert!(state.is_running);
        assert_eq!(state.reference_count, 0);
        assert_eq!(service.starts.load(Ordering::SeqCst), 0);
    }
}
