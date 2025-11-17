<script lang="ts">
  import type { Operation } from '$lib/api/operations';
  import { listOperations } from '$lib/api/operations';
  import { onMount, onDestroy } from 'svelte';

  export let workspaceId: string;

  let operations: Operation[] = [];
  let loading = true;
  let error: string | null = null;
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  async function loadOperations() {
    try {
      loading = true;
      error = null;
      operations = await listOperations(workspaceId);
      // Sort by started_at descending (newest first)
      operations.sort((a, b) => new Date(b.started_at).getTime() - new Date(a.started_at).getTime());
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load operations';
    } finally {
      loading = false;
    }
  }

  function formatDate(dateStr: string): string {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffSecs = Math.floor(diffMs / 1000);
    const diffMins = Math.floor(diffSecs / 60);
    const diffHours = Math.floor(diffMins / 60);
    const diffDays = Math.floor(diffHours / 24);

    if (diffSecs < 60) return 'Just now';
    if (diffMins < 60) return `${diffMins}m ago`;
    if (diffHours < 24) return `${diffHours}h ago`;
    if (diffDays < 7) return `${diffDays}d ago`;
    return date.toLocaleDateString();
  }

  function getDuration(startedAt: string, completedAt?: string): string {
    const start = new Date(startedAt).getTime();
    const end = completedAt ? new Date(completedAt).getTime() : Date.now();
    const diffMs = end - start;
    const diffSecs = Math.floor(diffMs / 1000);

    if (diffSecs < 60) return `${diffSecs}s`;
    const diffMins = Math.floor(diffSecs / 60);
    if (diffMins < 60) return `${diffMins}m ${diffSecs % 60}s`;
    const diffHours = Math.floor(diffMins / 60);
    return `${diffHours}h ${diffMins % 60}m`;
  }

  function getOperationIcon(type: string): string {
    switch (type) {
      case 'Create': return '🔨';
      case 'Delete': return '🗑️';
      case 'Start': return '▶️';
      case 'Stop': return '⏹️';
      case 'Restart': return '🔄';
      case 'Rebuild': return '🔧';
      case 'Snapshot': return '📸';
      case 'SnapshotRestore': return '⏮️';
      default: return '⚙️';
    }
  }

  onMount(() => {
    loadOperations();
    // Poll every 3 seconds for operations that are Pending or Running
    pollInterval = setInterval(() => {
      const hasActiveOperations = operations.some(
        op => op.status === 'Pending' || op.status === 'Running'
      );
      if (hasActiveOperations || operations.length === 0) {
        loadOperations();
      }
    }, 3000);
  });

  onDestroy(() => {
    if (pollInterval) {
      clearInterval(pollInterval);
    }
  });
</script>

<div class="operations-history">
  <div class="header">
    <h4>Activity</h4>
    <button class="btn-refresh" on:click={loadOperations} disabled={loading}>
      {loading ? '🔄' : '↻'} Refresh
    </button>
  </div>

  {#if loading && operations.length === 0}
    <div class="loading">Loading activity...</div>
  {:else if error}
    <div class="error">Error: {error}</div>
  {:else if operations.length === 0}
    <div class="empty">No operations yet</div>
  {:else}
    <div class="operations-list">
      {#each operations as operation}
        <div class="operation-item operation-{operation.status.toLowerCase()}">
          <div class="operation-header">
            <span class="operation-icon">{getOperationIcon(operation.operation_type)}</span>
            <span class="operation-type">{operation.operation_type}</span>
            <span class="operation-status status-{operation.status.toLowerCase()}">
              {operation.status}
            </span>
            <span class="operation-time">{formatDate(operation.started_at)}</span>
          </div>

          <div class="operation-details">
            <div class="detail-row">
              <span class="detail-label">Duration:</span>
              <span class="detail-value">
                {getDuration(operation.started_at, operation.completed_at)}
                {#if operation.status === 'Running'}
                  <span class="pulse">●</span>
                {/if}
              </span>
            </div>
            {#if operation.error}
              <div class="operation-error">
                <span class="error-icon">⚠️</span>
                <span class="error-message">{operation.error}</span>
              </div>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .operations-history {
    padding: 1rem;
    background: white;
    border-radius: 0.5rem;
    box-shadow: 0 1px 3px 0 rgba(0, 0, 0, 0.1);
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid #e5e7eb;
  }

  h4 {
    font-size: 1rem;
    font-weight: 600;
    margin: 0;
    color: #111827;
  }

  .btn-refresh {
    background: none;
    border: 1px solid #d1d5db;
    border-radius: 0.25rem;
    padding: 0.25rem 0.5rem;
    font-size: 0.75rem;
    cursor: pointer;
    color: #6b7280;
    transition: all 0.2s;
  }

  .btn-refresh:hover:not(:disabled) {
    background-color: #f3f4f6;
    border-color: #9ca3af;
  }

  .btn-refresh:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .loading,
  .error,
  .empty {
    padding: 2rem;
    text-align: center;
    color: #6b7280;
    font-size: 0.875rem;
  }

  .error {
    color: #dc2626;
    background-color: #fee2e2;
    border-radius: 0.375rem;
  }

  .operations-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .operation-item {
    border: 1px solid #e5e7eb;
    border-radius: 0.375rem;
    padding: 0.75rem;
    transition: all 0.2s;
  }

  .operation-item:hover {
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.05);
  }

  .operation-pending {
    border-left: 3px solid #f59e0b;
  }

  .operation-running {
    border-left: 3px solid #3b82f6;
    animation: pulse-border 2s infinite;
  }

  .operation-success {
    border-left: 3px solid #10b981;
  }

  .operation-failed {
    border-left: 3px solid #dc2626;
  }

  @keyframes pulse-border {
    0%, 100% { border-left-color: #3b82f6; }
    50% { border-left-color: #60a5fa; }
  }

  .operation-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
  }

  .operation-icon {
    font-size: 1rem;
  }

  .operation-type {
    font-weight: 500;
    color: #111827;
    font-size: 0.875rem;
  }

  .operation-status {
    display: inline-block;
    padding: 0.125rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.7rem;
    font-weight: 600;
    text-transform: uppercase;
  }

  .status-pending {
    background-color: #fef3c7;
    color: #92400e;
  }

  .status-running {
    background-color: #dbeafe;
    color: #1e40af;
  }

  .status-success {
    background-color: #d1fae5;
    color: #065f46;
  }

  .status-failed {
    background-color: #fee2e2;
    color: #991b1b;
  }

  .operation-time {
    margin-left: auto;
    font-size: 0.75rem;
    color: #6b7280;
  }

  .operation-details {
    padding-left: 1.5rem;
  }

  .detail-row {
    display: flex;
    gap: 0.5rem;
    font-size: 0.75rem;
    color: #6b7280;
  }

  .detail-label {
    font-weight: 500;
  }

  .detail-value {
    color: #374151;
  }

  .pulse {
    color: #3b82f6;
    animation: pulse 2s infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }

  .operation-error {
    margin-top: 0.5rem;
    padding: 0.5rem;
    background-color: #fee2e2;
    border-radius: 0.25rem;
    display: flex;
    gap: 0.5rem;
    align-items: flex-start;
  }

  .error-icon {
    font-size: 0.875rem;
  }

  .error-message {
    font-size: 0.75rem;
    color: #991b1b;
    flex: 1;
  }
</style>
