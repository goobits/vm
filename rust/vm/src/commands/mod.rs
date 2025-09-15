use anyhow::Result;
use vm_config::config::VmConfig;
use vm_provider::Provider;

pub struct CommandCtx {
    pub config: VmConfig,
    pub provider: Box<dyn Provider>,
}

pub mod create;
pub mod start;
pub mod stop;
pub mod destroy;
pub mod ssh;
pub mod status;
pub mod exec;
pub mod logs;
pub mod validate;
pub mod restart;
pub mod init;
pub mod preset {
    pub mod list;
    pub mod show;
}

/// Helper to chain provider operations when needed
pub fn stop_then_start(p: &dyn Provider) -> Result<()> {
    p.stop()?;
    p.start()
}

