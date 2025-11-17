export type WorkspaceStatus = 'creating' | 'running' | 'stopped' | 'failed';

export interface CreateWorkspaceRequest {
  name: string;
  template?: string;
  repo_url?: string;
  ttl_seconds?: number;
}

export interface Workspace {
  id: string;
  name: string;
  owner: string;
  repo_url?: string;
  template?: string;
  provider: string;
  status: WorkspaceStatus;
  created_at: string; // ISO 8601 string from backend
  updated_at: string; // ISO 8601 string from backend
  ttl_seconds?: number;
  expires_at?: string; // ISO 8601 string from backend
  provider_id?: string;
  connection_info?: {
    container_id?: string;
    status?: string;
    ssh_command?: string;
  };
  error_message?: string; // New field for displaying provisioning errors
  metadata?: Record<string, unknown>;
}
