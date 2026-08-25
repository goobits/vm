//! Durable managed-tool activation orchestration.

mod rollout;
mod user_service;
mod worker;

pub(in crate::commands) use rollout::{activate_deferred, repair};
pub(in crate::commands) use worker::{ensure_worker, remove_worker, run_worker};
