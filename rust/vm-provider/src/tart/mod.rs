mod command;
mod storage;

pub(crate) mod base;

pub use base::{build_vibe_base, ensure_configured_vibe_base, PreparedTartBase, TartBaseSource};
#[cfg(feature = "tart")]
pub(crate) use command::validate_environment;
pub(crate) use command::TartCommand;

#[cfg(feature = "tart")]
mod creation;
#[cfg(feature = "tart")]
mod host_sync;
#[cfg(feature = "tart")]
pub mod instance;
#[cfg(feature = "tart")]
mod metrics;
#[cfg(feature = "tart")]
mod mounts;
#[cfg(feature = "tart")]
mod provider;
#[cfg(feature = "tart")]
mod provisioner;
#[cfg(feature = "tart")]
mod readiness;
#[cfg(feature = "tart")]
mod resources;
#[cfg(feature = "tart")]
mod shell;
#[cfg(feature = "tart")]
mod ssh_identity;
#[cfg(feature = "tart")]
mod temp;
#[cfg(feature = "tart")]
mod workspace;

#[cfg(feature = "tart")]
pub use provider::TartProvider;
