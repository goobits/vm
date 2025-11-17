<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listWorkspaces, deleteWorkspace } from '$lib/api/workspaces';
  import StatusBadge from './StatusBadge.svelte';
  import type { Workspace } from '$lib/types/workspace';

  export let onRefresh: () => void;

  let workspaces: Workspace[] = [];
  let loading = true;
  let error = '';
  let deletingId = '';

  let refreshInterval: number;

  async function loadWorkspaces() {
    try {
      workspaces = await listWorkspaces();
      error = '';
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load workspaces';
    } finally {
      loading = false;
    }
  }

  async function handleDelete(id: string, name: string) {
    if (!confirm(`Delete workspace "${name}"?`)) {
      return;
    }

    deletingId = id;

    try {
      await deleteWorkspace(id);
      workspaces = workspaces.filter(w => w.id !== id);
    } catch (e) {
      alert(e instanceof Error ? e.message : 'Failed to delete workspace');
    } finally {
      deletingId = '';
    }
  }

  function formatDate(dateStr: string): string {
    return new Date(dateStr).toLocaleString();
  }

  function formatTTL(workspace: Workspace): string {
    if (!workspace.expires_at) return 'No expiration';

    const expiresAt = new Date(workspace.expires_at);
    const now = new Date();
    const diff = expiresAt.getTime() - now.getTime();

    if (diff < 0) return 'Expired';

    const hours = Math.floor(diff / (1000 * 60 * 60));
    const minutes = Math.floor((diff % (1000 * 60 * 60)) / (1000 * 60));

    if (hours > 24) {
      const days = Math.floor(hours / 24);
      return `${days}d remaining`;
    }

    return `${hours}h ${minutes}m remaining`;
  }

  onMount(() => {
    loadWorkspaces();

    // Auto-refresh every 10 seconds
    refreshInterval = setInterval(loadWorkspaces, 10000);
  });

  onDestroy(() => {
    if (refreshInterval) {
      clearInterval(refreshInterval);
    }
  });

  // Expose refresh function
  $: if (onRefresh) {
    onRefresh = loadWorkspaces;
  }
</script>

<div class="space-y-4">
  {#if loading}
    <div class="text-center py-8 text-gray-500">Loading workspaces...</div>
  {:else if error}
    <div class="text-center py-8 text-red-600">{error}</div>
  {:else if workspaces.length === 0}
    <div class="text-center py-8 text-gray-500">
      No workspaces yet. Create your first workspace to get started!
    </div>
  {:else}
    <div class="overflow-x-auto">
      <table class="min-w-full divide-y divide-gray-200">
        <thead class="bg-gray-50">
          <tr>
            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              Name
            </th>
            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              Template
            </th>
            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              Status
            </th>
            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              Created
            </th>
            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              TTL
            </th>
            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">
              Actions
            </th>
          </tr>
        </thead>
        <tbody class="bg-white divide-y divide-gray-200">
          {#each workspaces as workspace}
            <tr>
              <td class="px-6 py-4 whitespace-nowrap">
                <div class="text-sm font-medium text-gray-900">{workspace.name}</div>
                <div class="text-sm text-gray-500">{workspace.id.slice(0, 8)}...</div>
              </td>
              <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                {workspace.template || '-'}
              </td>
              <td class="px-6 py-4 whitespace-nowrap">
                <StatusBadge status={workspace.status} />
              </td>
              <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                {formatDate(workspace.created_at)}
              </td>
              <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-500">
                {formatTTL(workspace)}
              </td>
              <td class="px-6 py-4 whitespace-nowrap text-sm">
                <button
                  on:click={() => handleDelete(workspace.id, workspace.name)}
                  disabled={deletingId === workspace.id}
                  class="text-red-600 hover:text-red-900 disabled:opacity-50"
                >
                  {deletingId === workspace.id ? 'Deleting...' : 'Delete'}
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>
