<script lang="ts">
  import type { Workspace } from '$lib/types/workspace';
  import { listWorkspaces, deleteWorkspace } from '$lib/api/workspaces';
  import { onMount } from 'svelte';

  let workspaces: Workspace[] = [];
  let loading = true;
  let error: string | null = null;

  async function loadWorkspaces() {
    try {
      loading = true;
      error = null;
      workspaces = await listWorkspaces();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load workspaces';
    } finally {
      loading = false;
    }
  }

  async function handleDelete(id: string) {
    if (!confirm('Are you sure you want to delete this workspace?')) {
      return;
    }

    try {
      await deleteWorkspace(id);
      await loadWorkspaces(); // Refresh list
    } catch (e) {
      alert(e instanceof Error ? e.message : 'Failed to delete workspace');
    }
  }

  function formatDate(dateStr: string): string {
    return new Date(dateStr).toLocaleString();
  }

  function formatTTL(seconds: number | undefined): string {
    if (!seconds) return 'None';
    const hours = Math.floor(seconds / 3600);
    return `${hours}h`;
  }

  onMount(() => {
    loadWorkspaces();
    // Auto-refresh every 10 seconds
    const interval = setInterval(loadWorkspaces, 10000);
    return () => clearInterval(interval);
  });
</script>

<div class="workspace-list">
  <h2>Workspaces</h2>

  {#if loading && workspaces.length === 0}
    <div class="loading">Loading workspaces...</div>
  {:else if error}
    <div class="error">Error: {error}</div>
  {:else if workspaces.length === 0}
    <div class="empty">No workspaces found. Create one to get started!</div>
  {:else}
    <table>
      <thead class="bg-gray-50">
        <tr>
          <th>Name</th>
          <th>Template</th>
          <th>Status</th>
          <th>Created</th>
          <th>TTL</th>
          <th>Actions</th>
        </tr>
      </thead>
      <tbody>
        {#each workspaces as workspace}
          <tr>
            <td>
              <div class="text-sm font-medium text-gray-900">{workspace.name}</div>
              <div class="text-sm text-gray-500">{workspace.id.slice(0, 8)}...</div>
              {#if workspace.error_message}
                <div class="text-xs text-red-600 mt-1" title={workspace.error_message}>
                  ⚠ {workspace.error_message.slice(0, 50)}{workspace.error_message.length > 50 ? '...' : ''}
                </div>
              {/if}
            </td>
            <td>{workspace.template || 'default'}</td>
            <td>
              <span class="status status-{workspace.status}">
                {workspace.status}
              </span>
            </td>
            <td class="text-sm">{formatDate(workspace.created_at)}</td>
            <td>{formatTTL(workspace.ttl_seconds)}</td>
            <td>
              <button
                class="btn-delete"
                on:click={() => handleDelete(workspace.id)}
                disabled={loading}
              >
                Delete
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .workspace-list {
    padding: 1rem;
  }

  h2 {
    font-size: 1.5rem;
    font-weight: 600;
    margin-bottom: 1rem;
  }

  .loading,
  .error,
  .empty {
    padding: 2rem;
    text-align: center;
    color: #666;
  }

  .error {
    color: #dc2626;
    background-color: #fee2e2;
    border-radius: 0.375rem;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    background: white;
    box-shadow: 0 1px 3px 0 rgba(0, 0, 0, 0.1);
    border-radius: 0.5rem;
    overflow: hidden;
  }

  thead {
    background-color: #f9fafb;
  }

  th {
    padding: 0.75rem 1rem;
    text-align: left;
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    color: #6b7280;
    letter-spacing: 0.05em;
  }

  td {
    padding: 1rem;
    border-top: 1px solid #e5e7eb;
  }

  .text-sm {
    font-size: 0.875rem;
  }

  .text-xs {
    font-size: 0.75rem;
  }

  .font-medium {
    font-weight: 500;
  }

  .text-gray-900 {
    color: #111827;
  }

  .text-gray-500 {
    color: #6b7280;
  }

  .text-red-600 {
    color: #dc2626;
  }

  .mt-1 {
    margin-top: 0.25rem;
  }

  .status {
    display: inline-block;
    padding: 0.25rem 0.5rem;
    font-size: 0.75rem;
    font-weight: 600;
    border-radius: 0.25rem;
    text-transform: uppercase;
  }

  .status-creating {
    background-color: #dbeafe;
    color: #1e40af;
  }

  .status-running {
    background-color: #d1fae5;
    color: #065f46;
  }

  .status-stopped {
    background-color: #f3f4f6;
    color: #4b5563;
  }

  .status-failed {
    background-color: #fee2e2;
    color: #991b1b;
  }

  .btn-delete {
    background-color: #dc2626;
    color: white;
    padding: 0.375rem 0.75rem;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    font-weight: 500;
    border: none;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .btn-delete:hover:not(:disabled) {
    background-color: #b91c1c;
  }

  .btn-delete:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
