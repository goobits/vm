<script lang="ts">
  import { createWorkspace } from '$lib/api/workspaces';
  import type { CreateWorkspaceRequest } from '$lib/types/workspace';

  export let open = false;
  export let onCreated: () => void;

  let formData: CreateWorkspaceRequest = {
    name: '',
    template: 'nodejs',
    ttl_seconds: 86400, // 24 hours default
  };

  let creating = false;
  let error = '';

  const templates = [
    { value: 'nodejs', label: 'Node.js' },
    { value: 'python', label: 'Python' },
    { value: 'rust', label: 'Rust' },
    { value: 'go', label: 'Go' },
  ];

  async function handleSubmit() {
    if (!formData.name.trim()) {
      error = 'Workspace name is required';
      return;
    }

    creating = true;
    error = '';

    try {
      await createWorkspace(formData);
      open = false;
      formData = { name: '', template: 'nodejs', ttl_seconds: 86400 };
      onCreated();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to create workspace';
    } finally {
      creating = false;
    }
  }

  function handleClose() {
    open = false;
    error = '';
  }
</script>

{#if open}
  <div class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
    <div class="bg-white rounded-lg shadow-xl max-w-md w-full p-6">
      <div class="flex justify-between items-center mb-4">
        <h2 class="text-xl font-bold">Create Workspace</h2>
        <button on:click={handleClose} class="text-gray-500 hover:text-gray-700">
          ✕
        </button>
      </div>

      <form on:submit|preventDefault={handleSubmit} class="space-y-4">
        <div>
          <label for="name" class="block text-sm font-medium text-gray-700 mb-1">
            Workspace Name
          </label>
          <input
            id="name"
            type="text"
            bind:value={formData.name}
            placeholder="my-workspace"
            class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
            required
          />
        </div>

        <div>
          <label for="template" class="block text-sm font-medium text-gray-700 mb-1">
            Template
          </label>
          <select
            id="template"
            bind:value={formData.template}
            class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            {#each templates as template}
              <option value={template.value}>{template.label}</option>
            {/each}
          </select>
        </div>

        <div>
          <label for="ttl" class="block text-sm font-medium text-gray-700 mb-1">
            TTL (Time to Live)
          </label>
          <select
            id="ttl"
            bind:value={formData.ttl_seconds}
            class="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
          >
            <option value={3600}>1 hour</option>
            <option value={86400}>24 hours</option>
            <option value={604800}>7 days</option>
          </select>
        </div>

        {#if error}
          <div class="text-red-600 text-sm">{error}</div>
        {/if}

        <div class="flex justify-end gap-2">
          <button
            type="button"
            on:click={handleClose}
            class="px-4 py-2 text-gray-700 bg-gray-100 rounded-md hover:bg-gray-200"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={creating}
            class="px-4 py-2 text-white bg-blue-600 rounded-md hover:bg-blue-700 disabled:opacity-50"
          >
            {creating ? 'Creating...' : 'Create'}
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}
