export type WorkspaceStatus = 'creating' | 'running' | 'stopped' | 'failed';

export interface Workspace {
  id: string;
  name: string;
  owner: string;
  repo_url?: string;
  template?: string;
  provider: string;
  status: WorkspaceStatus;
  created_at: string;
  updated_at: string;
  ttl_seconds?: number;
  expires_at?: string;
  provider_id?: string;
  connection_info?: {
    container_id?: string;
    status?: string;
    ssh_command?: string;
  };
  error_message?: string;
  metadata?: Record<string, unknown>;
}

export interface CreateWorkspaceRequest {
  name: string;
  repo_url?: string;
  template?: string;
  provider?: string;
  ttl_seconds?: number;
}
