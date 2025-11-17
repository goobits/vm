<script lang="ts">
  import { onMount } from 'svelte';
  import SnapshotManager from '$lib/components/SnapshotManager.svelte';
  import {
    listWorkspaces,
    deleteWorkspace,
    startWorkspace,
    stopWorkspace,
    restartWorkspace,
  } from '$lib/api/workspaces';
  import type { Workspace } from '$lib/types/workspace';

  let workspaces: Workspace[] = [];
  let loading = true;
  let error: string | null = null;
  let actionInProgress: Record<string, boolean> = {};
  let expandedWorkspaces: Record<string, boolean> = {};

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

  async function runAction(id: string, action: () => Promise<unknown>) {
    try {
      actionInProgress = { ...actionInProgress, [id]: true };
      await action();
      await loadWorkspaces();
    } catch (e) {
      alert(e instanceof Error ? e.message : 'Workspace action failed');
    } finally {
      actionInProgress = { ...actionInProgress, [id]: false };
    }
  }

  function toggleSnapshots(id: string) {
    expandedWorkspaces = { ...expandedWorkspaces, [id]: !expandedWorkspaces[id] };
  }

  function formatDate(dateStr: string): string {
    return new Date(dateStr).toLocaleString();
  }

  function formatTTL(seconds: number | undefined): string {
    if (!seconds) return 'None';
    const hours = Math.floor(seconds / 3600);
    if (hours >= 24) {
      const days = Math.floor(hours / 24);
      return `${days}d`;
    }
    return `${hours}h`;
  }

  onMount(() => {
    loadWorkspaces();
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
    <div class="empty">No workspaces yet. Create one to get started!</div>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Name</th>
          <th>Template</th>
          <th>Status</th>
          <th>Connection</th>
          <th>Created</th>
          <th>TTL</th>
          <th>Actions</th>
        </tr>
      </thead>
      <tbody>
        {#each workspaces as workspace}
          <tr>
            <td>
              <div class="name">{workspace.name}</div>
              <div class="meta">{workspace.id.slice(0, 8)}…</div>
              {#if workspace.error_message}
                <div class="error-message" title={workspace.error_message}>
                  ⚠ {workspace.error_message}
                </div>
              {/if}
            </td>
            <td>{workspace.template || 'default'}</td>
            <td>
              <span class="status status-{workspace.status}">{workspace.status}</span>
            </td>
            <td>
              {#if workspace.provider_id}
                <div class="connection">
                  <div>Provider: {workspace.provider_id.slice(0, 12)}…</div>
                  {#if workspace.connection_info?.container_id}
                    <div>Container: {workspace.connection_info.container_id.slice(0, 12)}…</div>
                  {/if}
                  {#if workspace.status === 'running' && workspace.connection_info?.ssh_command}
                    <a
                      class="btn-connect"
                      href="vscode://open?url={encodeURIComponent('vm://' + workspace.name)}"
                      title={workspace.connection_info.ssh_command}
                    >
                      Open in Claude Code
                    </a>
                  {/if}
                </div>
              {:else}
                <span class="meta">-</span>
              {/if}
            </td>
            <td>{formatDate(workspace.created_at)}</td>
            <td>{formatTTL(workspace.ttl_seconds)}</td>
            <td>
              <div class="actions">
                {#if workspace.status === 'stopped'}
                  <button
                    class="btn-start"
                    on:click={() => runAction(workspace.id, () => startWorkspace(workspace.id))}
                    disabled={actionInProgress[workspace.id]}
                  >
                    {actionInProgress[workspace.id] ? 'Starting…' : 'Start'}
                  </button>
                {/if}
                {#if workspace.status === 'running'}
                  <button
                    class="btn-stop"
                    on:click={() => runAction(workspace.id, () => stopWorkspace(workspace.id))}
                    disabled={actionInProgress[workspace.id]}
                  >
                    {actionInProgress[workspace.id] ? 'Stopping…' : 'Stop'}
                  </button>
                  <button
                    class="btn-restart"
                    on:click={() => runAction(workspace.id, () => restartWorkspace(workspace.id))}
                    disabled={actionInProgress[workspace.id]}
                  >
                    {actionInProgress[workspace.id] ? 'Restarting…' : 'Restart'}
                  </button>
                {/if}
                <button class="btn-snapshots" on:click={() => toggleSnapshots(workspace.id)}>
                  {expandedWorkspaces[workspace.id] ? 'Hide Snapshots' : 'Snapshots'}
                </button>
                <button
                  class="btn-delete"
                  on:click={() =>
                    runAction(workspace.id, () => deleteWorkspace(workspace.id))}
                  disabled={actionInProgress[workspace.id]}
                >
                  {actionInProgress[workspace.id] ? 'Deleting…' : 'Delete'}
                </button>
              </div>
            </td>
          </tr>
          {#if expandedWorkspaces[workspace.id]}
            <tr class="snapshot-row">
              <td colspan="7">
                <SnapshotManager workspaceId={workspace.id} />
              </td>
            </tr>
          {/if}
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .workspace-list {
    margin-top: 2rem;
  }

  h2 {
    font-size: 1.5rem;
    font-weight: 600;
    margin-bottom: 1rem;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    background: white;
    border-radius: 0.75rem;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
    overflow: hidden;
  }

  thead {
    background: #f3f4f6;
  }

  th,
  td {
    padding: 0.9rem 1rem;
    text-align: left;
    font-size: 0.95rem;
  }

  tbody tr:nth-child(every) {
    border-bottom: 1px solid #e5e7eb;
  }

  .name {
    font-weight: 600;
  }

  .meta {
    font-size: 0.85rem;
    color: #6b7280;
  }

  .error-message {
    color: #dc2626;
    font-size: 0.8rem;
    margin-top: 0.25rem;
  }

  .status {
    padding: 0.2rem 0.75rem;
    border-radius: 999px;
    font-size: 0.85rem;
    font-weight: 600;
    text-transform: capitalize;
  }

  .status-running {
    background: #dcfce7;
    color: #15803d;
  }

  .status-stopped {
    background: #fee2e2;
    color: #b91c1c;
  }

  .status-creating {
    background: #dbeafe;
    color: #1d4ed8;
  }

  .status-failed {
    background: #fef3c7;
    color: #b45309;
  }

  .connection {
    font-size: 0.85rem;
    color: #374151;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .btn-connect {
    margin-top: 0.25rem;
    color: #2563eb;
    font-weight: 600;
    text-decoration: none;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }

  .actions button {
    border: none;
    border-radius: 999px;
    padding: 0.35rem 0.9rem;
    font-size: 0.85rem;
    cursor: pointer;
    font-weight: 600;
  }

  .btn-start {
    background: #22c55e;
    color: white;
  }

  .btn-stop {
    background: #f97316;
    color: white;
  }

  .btn-restart {
    background: #3b82f6;
    color: white;
  }

  .btn-snapshots {
    background: #c084fc;
    color: white;
  }

  .btn-delete {
    background: #ef4444;
    color: white;
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .snapshot-row td {
    background: #f9fafb;
  }

  .loading,
  .error,
  .empty {
    padding: 1.5rem;
    text-align: center;
    color: #6b7280;
  }

  .error {
    color: #dc2626;
  }
</style>
