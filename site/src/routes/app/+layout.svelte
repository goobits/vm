<script lang="ts">
  import { onMount } from 'svelte';

  // Phase 1: Mock auth check
  // Phase 2: Real auth via vm-auth-proxy
  let authenticated = true;

  onMount(() => {
    // TODO: Check auth status, redirect to login if needed
  });
</script>

{#if authenticated}
  <slot />
{:else}
  <div class="min-h-screen flex items-center justify-center">
    <div class="text-center">
      <h1 class="text-2xl font-bold mb-4">Authentication Required</h1>
      <p class="text-gray-600 mb-4">Please log in to continue</p>
      <a href="/login" class="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700">
        Log In
      </a>
    </div>
  </div>
{/if}
