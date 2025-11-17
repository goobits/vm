export interface Snapshot {
  id: string;
  workspace_id: string;
  name: string;
  created_at: string;
  size_bytes?: number;
  metadata?: Record<string, unknown>;
}

export interface CreateSnapshotRequest {
  name: string;
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

export async function listSnapshots(workspaceId: string): Promise<Snapshot[]> {
  const response = await fetch(`${API_BASE}/workspaces/${workspaceId}/snapshots`, {
    headers: {
      'x-user': 'testuser', // Phase 1: mock auth
    },
  });

  return handleResponse(response);
}

export async function createSnapshot(
  workspaceId: string,
  name: string
): Promise<Snapshot> {
  const response = await fetch(`${API_BASE}/workspaces/${workspaceId}/snapshots`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'x-user': 'testuser', // Phase 1: mock auth
    },
    body: JSON.stringify({ name }),
  });

  return handleResponse(response);
}

export async function restoreSnapshot(
  workspaceId: string,
  snapshotId: string
): Promise<void> {
  const response = await fetch(
    `${API_BASE}/workspaces/${workspaceId}/snapshots/${snapshotId}/restore`,
    {
      method: 'POST',
      headers: {
        'x-user': 'testuser', // Phase 1: mock auth
      },
    }
  );

  if (response.status === 401) {
    window.location.href = '/login';
    throw new Error('Unauthorized');
  }

  if (!response.ok) {
    throw new Error(`Failed to restore snapshot: ${response.statusText}`);
  }
}
