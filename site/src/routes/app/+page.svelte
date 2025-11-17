<script lang="ts">
  import WorkspaceList from '$lib/components/WorkspaceList.svelte';
  import CreateWorkspaceDrawer from '$lib/components/CreateWorkspaceDrawer.svelte';

  let showCreateDrawer = false;
  let refreshWorkspaces: () => void;

  function handleCreated() {
    if (refreshWorkspaces) {
      refreshWorkspaces();
    }
  }
</script>

<svelte:head>
  <title>Workspaces - VM Manager</title>
</svelte:head>

<div class="min-h-screen bg-gray-50">
  <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
    <div class="mb-8">
      <div class="flex justify-between items-center">
        <div>
          <h1 class="text-3xl font-bold text-gray-900">Workspaces</h1>
          <p class="mt-2 text-sm text-gray-600">
            Manage your development environments
          </p>
        </div>
        <button
          on:click={() => showCreateDrawer = true}
          class="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 font-medium"
        >
          + New Workspace
        </button>
      </div>
    </div>

    <div class="bg-white shadow rounded-lg p-6">
      <WorkspaceList bind:onRefresh={refreshWorkspaces} />
    </div>
  </div>
</div>

<CreateWorkspaceDrawer bind:open={showCreateDrawer} onCreated={handleCreated} />

<style>
  :global(body) {
    margin: 0;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
  }
</style>
