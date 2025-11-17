<script lang="ts">
  import type { WorkspaceStatus } from '$lib/types/workspace';

  export let status: WorkspaceStatus;

  const colors = {
    creating: 'bg-blue-100 text-blue-800',
    running: 'bg-green-100 text-green-800',
    stopped: 'bg-gray-100 text-gray-800',
    failed: 'bg-red-100 text-red-800',
  };

  const icons = {
    creating: '⏳',
    running: '✓',
    stopped: '⏸',
    failed: '✗',
  };
</script>

<span class="inline-flex items-center gap-1 px-2 py-1 rounded-full text-xs font-medium {colors[status]}">
  <span>{icons[status]}</span>
  <span class="capitalize">{status}</span>
</span>
