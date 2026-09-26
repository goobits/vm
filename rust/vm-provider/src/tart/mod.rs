mod command;
mod storage;

pub(crate) mod base;

pub use base::{build_vibe_base, ensure_configured_vibe_base, PreparedTartBase, TartBaseSource};
#[cfg(feature = "tart")]
pub(crate) use command::validate_environment;
pub(crate) use command::TartCommand;
pub use storage::project_home as tart_project_home;
#[cfg(feature = "tart")]
pub use storage::{
    refresh_runtime_identity as refresh_tart_runtime_identity,
    remove_storage as remove_tart_storage, storage_inventory as tart_storage_inventory,
    validate_restore_target as validate_tart_restore_target, TartStorageEntry,
};

#[cfg(feature = "tart")]
mod creation;
#[cfg(feature = "tart")]
mod host_sync;
#[cfg(feature = "tart")]
mod image;
#[cfg(feature = "tart")]
pub mod instance;
#[cfg(feature = "tart")]
mod metrics;
#[cfg(feature = "tart")]
mod mounts;
#[cfg(feature = "tart")]
mod preview;
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
pub use preview::render_tart_preview;
#[cfg(feature = "tart")]
pub use provider::TartProvider;
