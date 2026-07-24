use std::path::Path;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use super::{ensure_running, StartOutcome};
use crate::commands::vm_ops::interaction::{handle_exec, handle_ssh};
use vm_config::{config::VmConfig, GlobalConfig};
use vm_provider::{
    InstanceInfo, InstanceState, Provider, ProviderContext, VmError as ProviderError,
    VmResult as ProviderResult, VmStatusReport,
};

#[derive(Clone)]
struct FakeProvider {
    state: Arc<Mutex<Option<InstanceState>>>,
    starts: Arc<AtomicUsize>,
    creates: Arc<AtomicUsize>,
    shells: Arc<AtomicUsize>,
    execs: Arc<AtomicUsize>,
    fail_start_after_transition: bool,
}

impl FakeProvider {
    fn new(state: Option<InstanceState>) -> Self {
        Self {
            state: Arc::new(Mutex::new(state)),
            starts: Arc::new(AtomicUsize::new(0)),
            creates: Arc::new(AtomicUsize::new(0)),
            shells: Arc::new(AtomicUsize::new(0)),
            execs: Arc::new(AtomicUsize::new(0)),
            fail_start_after_transition: false,
        }
    }

    fn with_start_race(mut self) -> Self {
        self.fail_start_after_transition = true;
        self
    }
}

impl Provider for FakeProvider {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn create(&self, _context: &ProviderContext) -> ProviderResult<()> {
        self.creates.fetch_add(1, Ordering::SeqCst);
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
            name: "demo-dev".to_string(),
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

    fn provision(&self, _container: Option<&str>) -> ProviderResult<()> {
        Ok(())
    }

    fn get_sync_directory(&self) -> String {
        "/workspace".to_string()
    }

    fn list_instances(&self) -> ProviderResult<Vec<InstanceInfo>> {
        Ok(Vec::new())
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
async fn missing_environment_is_never_created() {
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

    handle_ssh(
        Box::new(provider.clone()),
        Some("demo-dev"),
        Some(PathBuf::from(".")),
        VmConfig::default(),
        GlobalConfig::default(),
    )
    .await
    .unwrap();

    assert_eq!(provider.starts.load(Ordering::SeqCst), 1);
    assert_eq!(provider.shells.load(Ordering::SeqCst), 1);
    assert_eq!(provider.creates.load(Ordering::SeqCst), 0);
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
}
