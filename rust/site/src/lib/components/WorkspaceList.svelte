<script lang="ts">
  import type { Workspace } from '$lib/types/workspace';
  import { listWorkspaces, deleteWorkspace, startWorkspace, stopWorkspace, restartWorkspace } from '$lib/api/workspaces';
  import SnapshotManager from './SnapshotManager.svelte';
  import { onMount } from 'svelte';

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

  async function handleDelete(id: string) {
    if (!confirm('Are you sure you want to delete this workspace?')) {
      return;
    }

    try {
      actionInProgress = { ...actionInProgress, [id]: true };
      await deleteWorkspace(id);
      await loadWorkspaces(); // Refresh list
    } catch (e) {
      alert(e instanceof Error ? e.message : 'Failed to delete workspace');
    } finally {
      actionInProgress = { ...actionInProgress, [id]: false };
    }
  }

  async function handleStart(id: string) {
    try {
      actionInProgress = { ...actionInProgress, [id]: true };
      await startWorkspace(id);
      await loadWorkspaces(); // Refresh list
    } catch (e) {
      alert(e instanceof Error ? e.message : 'Failed to start workspace');
    } finally {
      actionInProgress = { ...actionInProgress, [id]: false };
    }
  }

  async function handleStop(id: string) {
    try {
      actionInProgress = { ...actionInProgress, [id]: true };
      await stopWorkspace(id);
      await loadWorkspaces(); // Refresh list
    } catch (e) {
      alert(e instanceof Error ? e.message : 'Failed to stop workspace');
    } finally {
      actionInProgress = { ...actionInProgress, [id]: false };
    }
  }

  async function handleRestart(id: string) {
    try {
      actionInProgress = { ...actionInProgress, [id]: true };
      await restartWorkspace(id);
      await loadWorkspaces(); // Refresh list
    } catch (e) {
      alert(e instanceof Error ? e.message : 'Failed to restart workspace');
    } finally {
      actionInProgress = { ...actionInProgress, [id]: false };
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

  function toggleSnapshots(id: string) {
    expandedWorkspaces = { ...expandedWorkspaces, [id]: !expandedWorkspaces[id] };
  }

  async function copyToClipboard(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      alert(`${label} copied to clipboard!`);
    } catch (e) {
      alert(`Failed to copy ${label}: ${e instanceof Error ? e.message : 'Unknown error'}`);
    }
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
            <td>
              {#if workspace.provider_id}
                <div class="connection-info">
                  {#if workspace.connection_info}
                    <!-- Container ID with copy button -->
                    {#if workspace.connection_info.container_id}
                      <div class="info-row">
                        <span class="info-label">Container:</span>
                        <code class="info-value" title={workspace.connection_info.container_id}>
                          {workspace.connection_info.container_id.slice(0, 12)}
                        </code>
                        <button
                          class="btn-copy"
                          on:click={() => copyToClipboard(workspace.connection_info.container_id, 'Container ID')}
                          title="Copy container ID"
                        >
                          📋
                        </button>
                      </div>
                    {/if}

                    <!-- SSH Command with copy button -->
                    {#if workspace.connection_info.ssh_command}
                      <div class="info-row">
                        <span class="info-label">SSH:</span>
                        <code class="info-value" title={workspace.connection_info.ssh_command}>
                          {workspace.connection_info.ssh_command}
                        </code>
                        <button
                          class="btn-copy"
                          on:click={() => copyToClipboard(workspace.connection_info.ssh_command, 'SSH command')}
                          title="Copy SSH command"
                        >
                          📋
                        </button>
                      </div>
                    {/if}

                    <!-- Connection Status Badge -->
                    {#if workspace.status === 'running'}
                      <div class="info-row">
                        <span class="connection-badge connection-active">● Connected</span>
                      </div>
                    {:else if workspace.status === 'stopped'}
                      <div class="info-row">
                        <span class="connection-badge connection-inactive">○ Disconnected</span>
                      </div>
                    {:else if workspace.status === 'creating'}
                      <div class="info-row">
                        <span class="connection-badge connection-pending">◌ Provisioning...</span>
                      </div>
                    {/if}

                    <!-- Quick Actions -->
                    {#if workspace.status === 'running' && workspace.connection_info.ssh_command}
                      <div class="quick-actions">
                        <a
                          href="vscode://open?url={encodeURIComponent('vm://' + workspace.name)}"
                          class="btn-connect"
                          title="Open workspace in Claude Code"
                        >
                          🔗 Open in Claude Code
                        </a>
                      </div>
                    {/if}
                  {:else}
                    <div class="info-row">
                      <span class="text-xs text-gray-400">No connection info</span>
                    </div>
                  {/if}
                </div>
              {:else}
                <span class="text-xs text-gray-400">-</span>
              {/if}
            </td>
            <td class="text-sm">{formatDate(workspace.created_at)}</td>
            <td>{formatTTL(workspace.ttl_seconds)}</td>
            <td>
              <div class="actions">
                {#if workspace.status === 'stopped'}
                  <button
                    class="btn-start"
                    on:click={() => handleStart(workspace.id)}
                    disabled={actionInProgress[workspace.id]}
                  >
                    {actionInProgress[workspace.id] ? 'Starting...' : 'Start'}
                  </button>
                {/if}
                {#if workspace.status === 'running'}
                  <button
                    class="btn-stop"
                    on:click={() => handleStop(workspace.id)}
                    disabled={actionInProgress[workspace.id]}
                  >
                    {actionInProgress[workspace.id] ? 'Stopping...' : 'Stop'}
                  </button>
                  <button
                    class="btn-restart"
                    on:click={() => handleRestart(workspace.id)}
                    disabled={actionInProgress[workspace.id]}
                  >
                    {actionInProgress[workspace.id] ? 'Restarting...' : 'Restart'}
                  </button>
                {/if}
                <button
                  class="btn-snapshots"
                  on:click={() => toggleSnapshots(workspace.id)}
                >
                  {expandedWorkspaces[workspace.id] ? 'Hide Snapshots' : 'Snapshots'}
                </button>
                <button
                  class="btn-delete"
                  on:click={() => handleDelete(workspace.id)}
                  disabled={actionInProgress[workspace.id]}
                >
                  {actionInProgress[workspace.id] ? 'Deleting...' : 'Delete'}
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

  .btn-connect {
    display: inline-block;
    background-color: #2563eb;
    color: white;
    padding: 0.375rem 0.75rem;
    border-radius: 0.25rem;
    font-size: 0.75rem;
    font-weight: 500;
    text-decoration: none;
    transition: background-color 0.2s;
  }

  .btn-connect:hover {
    background-color: #1d4ed8;
  }

  .connection-info {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    font-size: 0.75rem;
  }

  .info-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .info-label {
    color: #6b7280;
    font-weight: 500;
    min-width: 70px;
  }

  .info-value {
    background-color: #f3f4f6;
    color: #374151;
    padding: 0.125rem 0.375rem;
    border-radius: 0.25rem;
    font-family: 'Monaco', 'Courier New', monospace;
    font-size: 0.7rem;
  }

  .btn-copy {
    background: none;
    border: none;
    cursor: pointer;
    padding: 0.125rem 0.25rem;
    font-size: 1rem;
    opacity: 0.6;
    transition: opacity 0.2s, transform 0.1s;
  }

  .btn-copy:hover {
    opacity: 1;
    transform: scale(1.1);
  }

  .btn-copy:active {
    transform: scale(0.95);
  }

  .connection-badge {
    display: inline-block;
    padding: 0.25rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.7rem;
    font-weight: 600;
  }

  .connection-active {
    background-color: #d1fae5;
    color: #065f46;
  }

  .connection-inactive {
    background-color: #f3f4f6;
    color: #6b7280;
  }

  .connection-pending {
    background-color: #fef3c7;
    color: #92400e;
  }

  .quick-actions {
    margin-top: 0.25rem;
  }

  .text-gray-600 {
    color: #4b5563;
  }

  .text-gray-400 {
    color: #9ca3af;
  }

  .actions {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .btn-start,
  .btn-stop,
  .btn-restart {
    padding: 0.375rem 0.75rem;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    font-weight: 500;
    border: none;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .btn-start {
    background-color: #10b981;
    color: white;
  }

  .btn-start:hover:not(:disabled) {
    background-color: #059669;
  }

  .btn-stop {
    background-color: #f59e0b;
    color: white;
  }

  .btn-stop:hover:not(:disabled) {
    background-color: #d97706;
  }

  .btn-restart {
    background-color: #3b82f6;
    color: white;
  }

  .btn-restart:hover:not(:disabled) {
    background-color: #2563eb;
  }

  .btn-snapshots {
    background-color: #8b5cf6;
    color: white;
    padding: 0.375rem 0.75rem;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    font-weight: 500;
    border: none;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .btn-snapshots:hover {
    background-color: #7c3aed;
  }

  .btn-start:disabled,
  .btn-stop:disabled,
  .btn-restart:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .snapshot-row td {
    background-color: #f9fafb;
    padding: 0 !important;
  }

  .snapshot-row :global(.snapshot-manager) {
    margin: 1rem;
  }
</style>
