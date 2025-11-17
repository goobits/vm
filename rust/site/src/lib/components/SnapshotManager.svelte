<script lang="ts">
  import type { Snapshot } from '$lib/api/snapshots';
  import { listSnapshots, createSnapshot, restoreSnapshot } from '$lib/api/snapshots';
  import { onMount } from 'svelte';

  export let workspaceId: string;

  let snapshots: Snapshot[] = [];
  let loading = true;
  let error: string | null = null;
  let showCreateDialog = false;
  let newSnapshotName = '';
  let creating = false;
  let restoring = false;

  async function loadSnapshots() {
    try {
      loading = true;
      error = null;
      snapshots = await listSnapshots(workspaceId);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load snapshots';
    } finally {
      loading = false;
    }
  }

  async function handleCreateSnapshot() {
    if (!newSnapshotName.trim()) {
      alert('Please enter a snapshot name');
      return;
    }

    try {
      creating = true;
      await createSnapshot(workspaceId, newSnapshotName.trim());
      newSnapshotName = '';
      showCreateDialog = false;
      await loadSnapshots();
    } catch (e) {
      alert(e instanceof Error ? e.message : 'Failed to create snapshot');
    } finally {
      creating = false;
    }
  }

  async function handleRestoreSnapshot(snapshotId: string, snapshotName: string) {
    if (!confirm(`Are you sure you want to restore from snapshot "${snapshotName}"? This will stop the workspace and restore it to the snapshot state.`)) {
      return;
    }

    try {
      restoring = true;
      await restoreSnapshot(workspaceId, snapshotId);
      alert('Snapshot restored successfully');
      await loadSnapshots();
    } catch (e) {
      alert(e instanceof Error ? e.message : 'Failed to restore snapshot');
    } finally {
      restoring = false;
    }
  }

  function formatDate(dateStr: string): string {
    return new Date(dateStr).toLocaleString();
  }

  function formatSize(bytes: number | undefined): string {
    if (!bytes) return 'Unknown';
    const mb = bytes / (1024 * 1024);
    if (mb < 1024) {
      return `${mb.toFixed(1)} MB`;
    }
    return `${(mb / 1024).toFixed(1)} GB`;
  }

  onMount(() => {
    loadSnapshots();
  });
</script>

<div class="snapshot-manager">
  <div class="header">
    <h3>Snapshots</h3>
    <button
      class="btn-create"
      on:click={() => showCreateDialog = true}
      disabled={loading || creating || restoring}
    >
      Create Snapshot
    </button>
  </div>

  {#if loading && snapshots.length === 0}
    <div class="loading">Loading snapshots...</div>
  {:else if error}
    <div class="error">Error: {error}</div>
  {:else if snapshots.length === 0}
    <div class="empty">No snapshots yet. Create one to save the current workspace state.</div>
  {:else}
    <div class="snapshot-list">
      {#each snapshots as snapshot}
        <div class="snapshot-item">
          <div class="snapshot-info">
            <div class="snapshot-name">{snapshot.name}</div>
            <div class="snapshot-meta">
              <span class="meta-item">
                Created: {formatDate(snapshot.created_at)}
              </span>
              {#if snapshot.size_bytes}
                <span class="meta-item">
                  Size: {formatSize(snapshot.size_bytes)}
                </span>
              {/if}
            </div>
          </div>
          <button
            class="btn-restore"
            on:click={() => handleRestoreSnapshot(snapshot.id, snapshot.name)}
            disabled={restoring || creating}
          >
            Restore
          </button>
        </div>
      {/each}
    </div>
  {/if}
</div>

{#if showCreateDialog}
  <div class="dialog-overlay" on:click={() => showCreateDialog = false}>
    <div class="dialog" on:click|stopPropagation>
      <h4>Create Snapshot</h4>
      <input
        type="text"
        bind:value={newSnapshotName}
        placeholder="Enter snapshot name (e.g., 'Before refactor')"
        class="snapshot-input"
        on:keydown={(e) => e.key === 'Enter' && handleCreateSnapshot()}
      />
      <div class="dialog-buttons">
        <button
          class="btn-cancel"
          on:click={() => showCreateDialog = false}
          disabled={creating}
        >
          Cancel
        </button>
        <button
          class="btn-submit"
          on:click={handleCreateSnapshot}
          disabled={creating || !newSnapshotName.trim()}
        >
          {creating ? 'Creating...' : 'Create'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .snapshot-manager {
    margin-top: 1rem;
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
  }

  h3 {
    font-size: 1.125rem;
    font-weight: 600;
    margin: 0;
  }

  .btn-create {
    background-color: #2563eb;
    color: white;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    font-weight: 500;
    border: none;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .btn-create:hover:not(:disabled) {
    background-color: #1d4ed8;
  }

  .btn-create:disabled {
    opacity: 0.5;
    cursor: not-allowed;
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

  .snapshot-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .snapshot-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem;
    background: #f9fafb;
    border-radius: 0.375rem;
    border: 1px solid #e5e7eb;
  }

  .snapshot-info {
    flex: 1;
  }

  .snapshot-name {
    font-weight: 500;
    color: #111827;
    margin-bottom: 0.25rem;
  }

  .snapshot-meta {
    display: flex;
    gap: 1rem;
    font-size: 0.75rem;
    color: #6b7280;
  }

  .meta-item {
    display: inline-block;
  }

  .btn-restore {
    background-color: #059669;
    color: white;
    padding: 0.375rem 0.75rem;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    font-weight: 500;
    border: none;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .btn-restore:hover:not(:disabled) {
    background-color: #047857;
  }

  .btn-restore:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .dialog-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .dialog {
    background: white;
    padding: 1.5rem;
    border-radius: 0.5rem;
    min-width: 400px;
    box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.1);
  }

  h4 {
    margin: 0 0 1rem 0;
    font-size: 1.125rem;
    font-weight: 600;
  }

  .snapshot-input {
    width: 100%;
    padding: 0.5rem;
    border: 1px solid #d1d5db;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    margin-bottom: 1rem;
  }

  .snapshot-input:focus {
    outline: none;
    border-color: #2563eb;
    box-shadow: 0 0 0 3px rgba(37, 99, 235, 0.1);
  }

  .dialog-buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  .btn-cancel {
    background-color: #f3f4f6;
    color: #374151;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    font-weight: 500;
    border: none;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .btn-cancel:hover:not(:disabled) {
    background-color: #e5e7eb;
  }

  .btn-submit {
    background-color: #2563eb;
    color: white;
    padding: 0.5rem 1rem;
    border-radius: 0.375rem;
    font-size: 0.875rem;
    font-weight: 500;
    border: none;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .btn-submit:hover:not(:disabled) {
    background-color: #1d4ed8;
  }

  .btn-submit:disabled,
  .btn-cancel:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
