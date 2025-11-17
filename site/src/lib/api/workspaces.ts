import type { Workspace, CreateWorkspaceRequest } from '$lib/types/workspace';

const API_BASE = '/api/v1';

export async function listWorkspaces(): Promise<Workspace[]> {
  const response = await fetch(`${API_BASE}/workspaces`, {
    headers: {
      'x-user': 'testuser', // Phase 1: mock auth
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to list workspaces: ${response.statusText}`);
  }

  return response.json();
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

  if (!response.ok) {
    throw new Error(`Failed to create workspace: ${response.statusText}`);
  }

  return response.json();
}

export async function deleteWorkspace(id: string): Promise<void> {
  const response = await fetch(`${API_BASE}/workspaces/${id}`, {
    method: 'DELETE',
    headers: {
      'x-user': 'testuser', // Phase 1: mock auth
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to delete workspace: ${response.statusText}`);
  }
}
