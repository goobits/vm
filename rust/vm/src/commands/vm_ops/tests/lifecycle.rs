use std::path::Path;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use super::{ensure_running, wait_until_ready_for, ReadyFor, StartOutcome};
use crate::commands::vm_ops::interaction::{handle_exec, handle_ssh};
use crate::commands::vm_ops::resolve_or_create_target;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_provider::{
    CommandProvider, InstanceInfo, InstanceProvider, InstanceState, Provider, ProviderContext,
    VmError as ProviderError, VmResult as ProviderResult, VmStatusReport,
};

#[derive(Clone)]
struct FakeProvider {
    state: Arc<Mutex<Option<InstanceState>>>,
    instance_name: Arc<Mutex<String>>,
    starts: Arc<AtomicUsize>,
    creates: Arc<AtomicUsize>,
    shells: Arc<AtomicUsize>,
    execs: Arc<AtomicUsize>,
    runtime_reconciliations: Arc<AtomicUsize>,
    command_ready_checks: Arc<AtomicUsize>,
    shell_ready_checks: Arc<AtomicUsize>,
    fail_start_after_transition: bool,
    ready: bool,
}

impl FakeProvider {
    fn new(state: Option<InstanceState>) -> Self {
        Self {
            state: Arc::new(Mutex::new(state)),
            instance_name: Arc::new(Mutex::new("demo-dev".to_string())),
            starts: Arc::new(AtomicUsize::new(0)),
            creates: Arc::new(AtomicUsize::new(0)),
            shells: Arc::new(AtomicUsize::new(0)),
            execs: Arc::new(AtomicUsize::new(0)),
            runtime_reconciliations: Arc::new(AtomicUsize::new(0)),
            command_ready_checks: Arc::new(AtomicUsize::new(0)),
            shell_ready_checks: Arc::new(AtomicUsize::new(0)),
            fail_start_after_transition: false,
            ready: true,
        }
    }

    fn with_start_race(mut self) -> Self {
        self.fail_start_after_transition = true;
        self
    }

    fn never_ready(mut self) -> Self {
        self.ready = false;
        self
    }
}

fn project_config() -> VmConfig {
    serde_yaml_ng::from_str(
        r#"
provider: mock
project:
  name: demo
vm:
  user: developer
"#,
    )
    .unwrap()
}

impl CommandProvider for FakeProvider {}

impl InstanceProvider for FakeProvider {
    fn create_instance(
        &self,
        instance_name: &str,
        _context: &ProviderContext,
    ) -> ProviderResult<()> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        *self.instance_name.lock().unwrap() = format!("demo-{instance_name}-dev");
        *self.state.lock().unwrap() = Some(InstanceState::Running);
        Ok(())
    }

    fn list_instances(&self) -> ProviderResult<Vec<InstanceInfo>> {
        let Some(state) = self.state.lock().unwrap().clone() else {
            return Ok(Vec::new());
        };
        Ok(vec![InstanceInfo {
            name: self.instance_name.lock().unwrap().clone(),
            id: "demo-id".to_string(),
            status: state.to_string(),
            provider: "fake".to_string(),
            project: Some("demo".to_string()),
            uptime: None,
            created_at: None,
        }])
    }

    fn supports_multi_instance(&self) -> bool {
        true
    }
}

impl Provider for FakeProvider {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn create(&self, _context: &ProviderContext) -> ProviderResult<()> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        *self.instance_name.lock().unwrap() = "demo-dev".to_string();
        *self.state.lock().unwrap() = Some(InstanceState::Running);
        Ok(())
    }

    fn start(&self, _container: Option<&str>, _context: &ProviderContext) -> ProviderResult<()> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        *self.state.lock().unwrap() = Some(InstanceState::Running);
        if self.fail_start_after_transition {
            return Err(ProviderError::Provider(
                "another process completed startup".to_string(),
            ));
        }
        Ok(())
    }

    fn stop(&self, _container: Option<&str>) -> ProviderResult<()> {
        Ok(())
    }

    fn destroy(&self, _container: Option<&str>, _context: &ProviderContext) -> ProviderResult<()> {
        Ok(())
    }

    fn ssh(&self, _container: Option<&str>, _relative_path: &Path) -> ProviderResult<()> {
        self.shells.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn exec(&self, _container: Option<&str>, _cmd: &[String]) -> ProviderResult<()> {
        self.execs.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn logs(&self, _container: Option<&str>) -> ProviderResult<()> {
        Ok(())
    }

    fn copy(
        &self,
        _source: &str,
        _destination: &str,
        _container: Option<&str>,
    ) -> ProviderResult<()> {
        Ok(())
    }

    fn status(&self, container: Option<&str>) -> ProviderResult<VmStatusReport> {
        let state = self.instance_state(container)?;
        Ok(VmStatusReport {
            name: self.instance_name.lock().unwrap().clone(),
            provider: "fake".to_string(),
            is_running: state.is_running(),
            state,
            ..Default::default()
        })
    }

    fn instance_state(&self, _container: Option<&str>) -> ProviderResult<InstanceState> {
        self.state.lock().unwrap().clone().ok_or_else(|| {
            ProviderError::NotFound("Environment 'demo-dev' does not exist".to_string())
        })
    }

    fn is_ready(&self, container: Option<&str>) -> ProviderResult<bool> {
        self.command_ready_checks.fetch_add(1, Ordering::SeqCst);
        Ok(self.ready && self.instance_state(container)?.is_running())
    }

    fn is_shell_ready(&self, container: Option<&str>) -> ProviderResult<bool> {
        self.shell_ready_checks.fetch_add(1, Ordering::SeqCst);
        Ok(self.ready && self.instance_state(container)?.is_running())
    }

    fn provision(&self, _container: Option<&str>) -> ProviderResult<()> {
        Ok(())
    }

    fn reconcile_runtime(
        &self,
        _container: Option<&str>,
        _context: &ProviderContext,
    ) -> ProviderResult<()> {
        self.runtime_reconciliations.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn get_sync_directory(&self) -> String {
        "/workspace".to_string()
    }

    fn clone_box(&self) -> Box<dyn Provider> {
        Box::new(self.clone())
    }
}

#[tokio::test]
async fn stopped_environment_starts_without_creation() {
    let provider = FakeProvider::new(Some(InstanceState::Stopped));

    let outcome = ensure_running(
        &provider,
        Some("demo-dev"),
        &VmConfig::default(),
        &GlobalConfig::default(),
        true,
    )
    .await
    .unwrap();

    assert_eq!(outcome, StartOutcome::Started);
    assert_eq!(provider.starts.load(Ordering::SeqCst), 1);
    assert_eq!(provider.creates.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn running_environment_is_not_started_again() {
    let provider = FakeProvider::new(Some(InstanceState::Running));

    let outcome = ensure_running(
        &provider,
        Some("demo-dev"),
        &VmConfig::default(),
        &GlobalConfig::default(),
        true,
    )
    .await
    .unwrap();

    assert_eq!(outcome, StartOutcome::AlreadyRunning);
    assert_eq!(provider.starts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn ensure_running_does_not_create_a_missing_environment() {
    let provider = FakeProvider::new(None);

    let result = ensure_running(
        &provider,
        Some("demo-dev"),
        &VmConfig::default(),
        &GlobalConfig::default(),
        true,
    )
    .await;

    assert!(result.is_err());
    assert_eq!(provider.starts.load(Ordering::SeqCst), 0);
    assert_eq!(provider.creates.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn shell_target_is_created_from_config_when_missing() {
    let provider = FakeProvider::new(None);
    let config = project_config();

    let target = resolve_or_create_target(&provider, &config, &GlobalConfig::default(), None)
        .await
        .unwrap();

    handle_ssh(
        Box::new(provider.clone()),
        Some(&target),
        Some(PathBuf::from(".")),
        config,
        GlobalConfig::default(),
    )
    .await
    .unwrap();

    assert_eq!(target, "demo-dev");
    assert_eq!(provider.creates.load(Ordering::SeqCst), 1);
    assert_eq!(provider.shells.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn named_shell_target_is_created_when_missing() {
    let provider = FakeProvider::new(None);
    let target = resolve_or_create_target(
        &provider,
        &project_config(),
        &GlobalConfig::default(),
        Some("backend"),
    )
    .await
    .unwrap();

    assert_eq!(target, "demo-backend-dev");
    assert_eq!(provider.creates.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn concurrent_start_is_idempotent() {
    let provider = FakeProvider::new(Some(InstanceState::Stopped)).with_start_race();

    let outcome = ensure_running(
        &provider,
        Some("demo-dev"),
        &VmConfig::default(),
        &GlobalConfig::default(),
        true,
    )
    .await
    .unwrap();

    assert_eq!(outcome, StartOutcome::Started);
    assert_eq!(provider.starts.load(Ordering::SeqCst), 1);
    assert_eq!(provider.creates.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn shell_starts_a_stopped_environment_before_connecting() {
    let provider = FakeProvider::new(Some(InstanceState::Stopped));
    let config = project_config();
    let target = resolve_or_create_target(&provider, &config, &GlobalConfig::default(), None)
        .await
        .unwrap();

    handle_ssh(
        Box::new(provider.clone()),
        Some(&target),
        Some(PathBuf::from(".")),
        config,
        GlobalConfig::default(),
    )
    .await
    .unwrap();

    assert_eq!(provider.starts.load(Ordering::SeqCst), 1);
    assert_eq!(provider.shells.load(Ordering::SeqCst), 1);
    assert_eq!(provider.creates.load(Ordering::SeqCst), 0);
    assert_eq!(provider.command_ready_checks.load(Ordering::SeqCst), 0);
    assert_eq!(provider.shell_ready_checks.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn exec_starts_a_stopped_environment_before_running() {
    let provider = FakeProvider::new(Some(InstanceState::Stopped));

    handle_exec(
        Box::new(provider.clone()),
        Some("demo-dev"),
        vec!["true".to_string()],
        VmConfig::default(),
        GlobalConfig::default(),
    )
    .await
    .unwrap();

    assert_eq!(provider.starts.load(Ordering::SeqCst), 1);
    assert_eq!(provider.execs.load(Ordering::SeqCst), 1);
    assert_eq!(provider.creates.load(Ordering::SeqCst), 0);
    assert_eq!(provider.command_ready_checks.load(Ordering::SeqCst), 1);
    assert_eq!(provider.shell_ready_checks.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn shell_and_exec_reconcile_managed_package_access() {
    let provider = FakeProvider::new(Some(InstanceState::Running));
    let mut config = project_config();
    config.package_edge = Some(vm_config::config::PackageEdgeConfig {
        image: "package-edge:local".into(),
        internal_gateway: "http://127.0.0.1:3080".into(),
        client_gateway: "http://package-edge:3080".into(),
        read_token: "read-token".into(),
        revision: "revision-1".into(),
    });

    handle_ssh(
        Box::new(provider.clone()),
        Some("demo-dev"),
        Some(PathBuf::from(".")),
        config.clone(),
        GlobalConfig::default(),
    )
    .await
    .unwrap();
    handle_exec(
        Box::new(provider.clone()),
        Some("demo-dev"),
        vec!["true".to_string()],
        config,
        GlobalConfig::default(),
    )
    .await
    .unwrap();

    assert_eq!(provider.runtime_reconciliations.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn readiness_wait_obeys_its_real_deadline() {
    let provider = FakeProvider::new(Some(InstanceState::Running)).never_ready();
    let started = tokio::time::Instant::now();

    let error = wait_until_ready_for(
        &provider,
        Some("demo-dev"),
        "demo-dev",
        ReadyFor::Shell,
        std::time::Duration::from_millis(20),
        std::time::Duration::from_millis(5),
    )
    .await
    .unwrap_err();

    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    assert!(error.to_string().contains("wait until ready"));
    assert!(provider.shell_ready_checks.load(Ordering::SeqCst) >= 2);
    assert_eq!(provider.command_ready_checks.load(Ordering::SeqCst), 0);
}
