-- Initial schema for workspace management

CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    owner TEXT NOT NULL,
    repo_url TEXT,
    template TEXT,
    provider TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    ttl_seconds INTEGER,
    expires_at INTEGER,
    metadata TEXT
);

CREATE INDEX IF NOT EXISTS idx_workspaces_owner ON workspaces(owner);
CREATE INDEX IF NOT EXISTS idx_workspaces_status ON workspaces(status);
CREATE INDEX IF NOT EXISTS idx_workspaces_expires_at ON workspaces(expires_at) WHERE expires_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS operations (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    operation_type TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    error TEXT,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_operations_workspace ON operations(workspace_id);
CREATE INDEX IF NOT EXISTS idx_operations_status ON operations(status);
