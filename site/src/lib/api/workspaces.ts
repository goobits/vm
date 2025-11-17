import type { Workspace, CreateWorkspaceRequest } from '$lib/types/workspace';

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

function authHeaders(extra: Record<string, string> = {}) {
  return {
    'x-user': 'testuser', // Phase 1 mock auth
    ...extra,
  };
}

export async function listWorkspaces(): Promise<Workspace[]> {
  const response = await fetch(`${API_BASE}/workspaces`, {
    headers: authHeaders(),
  });

  return handleResponse(response);
}

export async function createWorkspace(req: CreateWorkspaceRequest): Promise<Workspace> {
  const response = await fetch(`${API_BASE}/workspaces`, {
    method: 'POST',
    headers: authHeaders({ 'Content-Type': 'application/json' }),
    body: JSON.stringify(req),
  });

  return handleResponse(response);
}

export async function deleteWorkspace(id: string): Promise<void> {
  const response = await fetch(`${API_BASE}/workspaces/${id}`, {
    method: 'DELETE',
    headers: authHeaders(),
  });

  if (response.status === 401) {
    window.location.href = '/login';
    throw new Error('Unauthorized');
  }

  if (!response.ok) {
    throw new Error(`Failed to delete workspace: ${response.statusText}`);
  }
}

export async function startWorkspace(id: string): Promise<Workspace> {
  const response = await fetch(`${API_BASE}/workspaces/${id}/start`, {
    method: 'POST',
    headers: authHeaders(),
  });
  return handleResponse(response);
}

export async function stopWorkspace(id: string): Promise<Workspace> {
  const response = await fetch(`${API_BASE}/workspaces/${id}/stop`, {
    method: 'POST',
    headers: authHeaders(),
  });
  return handleResponse(response);
}

export async function restartWorkspace(id: string): Promise<Workspace> {
  const response = await fetch(`${API_BASE}/workspaces/${id}/restart`, {
    method: 'POST',
    headers: authHeaders(),
  });
  return handleResponse(response);
}
