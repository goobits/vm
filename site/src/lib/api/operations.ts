export type OperationType =
  | 'Create'
  | 'Delete'
  | 'Start'
  | 'Stop'
  | 'Restart'
  | 'Rebuild'
  | 'Snapshot'
  | 'SnapshotRestore';

export type OperationStatus = 'Pending' | 'Running' | 'Success' | 'Failed';

export interface Operation {
  id: string;
  workspace_id: string;
  operation_type: OperationType;
  status: OperationStatus;
  started_at: string;
  completed_at?: string;
  error?: string;
}

const API_BASE = '/api/v1';

async function handleResponse<T>(response: Response): Promise<T> {
  if (response.status === 401) {
    window.location.href = '/login';
    throw new Error('Unauthorized');
  }

  if (!response.ok) {
    const error = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(error.error || response.statusText);
  }

  return response.json();
}

export async function listOperations(workspaceId?: string): Promise<Operation[]> {
  const params = new URLSearchParams();
  if (workspaceId) {
    params.append('workspace_id', workspaceId);
  }

  const url = `${API_BASE}/operations${params.toString() ? '?' + params.toString() : ''}`;
  const response = await fetch(url, {
    headers: {
      'x-user': 'testuser', // Phase 1: mock auth
    },
  });

  return handleResponse(response);
}

export async function getOperation(id: string): Promise<Operation> {
  const response = await fetch(`${API_BASE}/operations/${id}`, {
    headers: {
      'x-user': 'testuser', // Phase 1: mock auth
    },
  });

  return handleResponse(response);
}
