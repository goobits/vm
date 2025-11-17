import type { Workspace, CreateWorkspaceRequest } from '$lib/types/workspace';

const API_BASE = '/api/v1';

async function handleResponse<T>(response: Response): Promise<T> {
  if (response.status === 401) {
    // Redirect to login (in Phase 2, this will be OAuth flow)
    window.location.href = '/login';
    throw new Error('Unauthorized');
  }

  if (!response.ok) {
    const error = await response.json().catch(() => ({ error: response.statusText }));
    throw new Error(error.error || response.statusText);
  }

  return response.json();
}

export async function listWorkspaces(): Promise<Workspace[]> {
  const response = await fetch(`${API_BASE}/workspaces`, {
    headers: {
      'x-user': 'testuser', // Phase 1: mock auth
    },
  });

  return handleResponse(response);
}

export async function createWorkspace(req: CreateWorkspaceRequest): Promise<Workspace> {
  const response = await fetch(`${API_BASE}/workspaces`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'x-user': 'testuser', // Phase 1: mock auth
    },
    body: JSON.stringify(req),
  });

  return handleResponse(response);
}

export async function deleteWorkspace(id: string): Promise<void> {
  const response = await fetch(`${API_BASE}/workspaces/${id}`, {
    method: 'DELETE',
    headers: {
      'x-user': 'testuser', // Phase 1: mock auth
    },
  });

  if (response.status === 401) {
    window.location.href = '/login';
    throw new Error('Unauthorized');
  }

  if (!response.ok) {
    throw new Error(`Failed to delete workspace: ${response.statusText}`);
  }
}
