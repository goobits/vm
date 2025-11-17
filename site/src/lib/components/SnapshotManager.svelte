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
    if (
      !confirm(
        `Restore workspace to "${snapshotName}"? Current state will be replaced by the snapshot.`
      )
    ) {
      return;
    }

    try {
      restoring = true;
      await restoreSnapshot(workspaceId, snapshotId);
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
      on:click={() => (showCreateDialog = true)}
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
  <div class="dialog-overlay" on:click={() => (showCreateDialog = false)}>
    <div class="dialog" on:click|stopPropagation>
      <h4>Create Snapshot</h4>
      <input
        type="text"
        bind:value={newSnapshotName}
        placeholder="Snapshot name (e.g., Before refactor)"
        class="snapshot-input"
        on:keydown={(e) => e.key === 'Enter' && handleCreateSnapshot()}
      />
      <div class="dialog-buttons">
        <button
          class="btn-cancel"
          on:click={() => (showCreateDialog = false)}
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
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
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
    background-color: #7c3aed;
    color: white;
    padding: 0.4rem 1rem;
    border-radius: 0.375rem;
    border: none;
    cursor: pointer;
    font-weight: 500;
  }

  .btn-create:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .loading,
  .error,
  .empty {
    padding: 1rem;
    text-align: center;
    color: #6b7280;
  }

  .error {
    color: #dc2626;
  }

  .snapshot-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .snapshot-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem;
    border: 1px solid #e5e7eb;
    border-radius: 0.5rem;
  }

  .snapshot-info {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .snapshot-name {
    font-weight: 600;
  }

  .snapshot-meta {
    font-size: 0.875rem;
    color: #6b7280;
    display: flex;
    gap: 1rem;
    flex-wrap: wrap;
  }

  .btn-restore {
    border: none;
    border-radius: 999px;
    padding: 0.4rem 1.25rem;
    background-color: #2563eb;
    color: white;
    cursor: pointer;
    font-weight: 500;
  }

  .btn-restore:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .dialog-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .dialog {
    background: white;
    padding: 1.5rem;
    border-radius: 0.5rem;
    width: 320px;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .snapshot-input {
    width: 100%;
    padding: 0.5rem;
    border: 1px solid #e5e7eb;
    border-radius: 0.375rem;
  }

  .dialog-buttons {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  .btn-cancel,
  .btn-submit {
    border: none;
    border-radius: 0.375rem;
    padding: 0.4rem 0.8rem;
    cursor: pointer;
  }

  .btn-cancel {
    background: #e5e7eb;
    color: #111827;
  }

  .btn-submit {
    background: #2563eb;
    color: white;
  }

  .btn-submit:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
