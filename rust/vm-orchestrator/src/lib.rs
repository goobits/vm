//! Workspace orchestration business logic
//!
//! This crate contains the core business logic for managing hosted workspaces.
//! It is consumed by the vm-api HTTP service but can also be used by CLI commands,
//! background workers, or other entry points.

pub mod db;
pub mod error;
pub mod operation;
pub mod workspace;

pub use error::{OrchestratorError, Result};
pub use operation::{Operation, OperationStatus, OperationType};
pub use workspace::{CreateWorkspaceRequest, Workspace, WorkspaceFilters, WorkspaceOrchestrator};
