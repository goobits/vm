use utoipa::OpenApi;
use vm_orchestrator::{
    CreateSnapshotRequest, CreateWorkspaceRequest, Operation, OperationStatus, OperationType,
    Snapshot, Workspace, WorkspaceFilters, WorkspaceStatus,
};

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::routes::health::health_check,
        crate::routes::health::readiness_check,
        crate::routes::workspaces::list_workspaces,
        crate::routes::workspaces::create_workspace,
        crate::routes::workspaces::get_workspace,
        crate::routes::workspaces::delete_workspace,
        crate::routes::workspaces::start_workspace,
        crate::routes::workspaces::stop_workspace,
        crate::routes::workspaces::restart_workspace,
        crate::routes::snapshots::list_snapshots,
        crate::routes::snapshots::create_snapshot,
        crate::routes::snapshots::restore_snapshot,
        crate::routes::operations::list_operations,
        crate::routes::operations::get_operation,
    ),
    components(
        schemas(
            Workspace,
            WorkspaceStatus,
            CreateWorkspaceRequest,
            Snapshot,
            CreateSnapshotRequest,
            WorkspaceFilters,
            Operation,
            OperationStatus,
            OperationType
        )
    ),
    tags(
        (name = "vm-api", description = "VM Orchestration API")
    )
)]
pub struct ApiDoc;
