<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';

  onMount(() => {
    goto('/app');
  });
</script>

<div class="loading">
  Redirecting to workspace manager...
</div>

<style>
  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 100vh;
    font-size: 1.125rem;
    color: #6b7280;
  }
</style>
