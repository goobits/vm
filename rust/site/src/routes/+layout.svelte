<script lang="ts">
  // Root layout
</script>

<svelte:head>
  <title>VM Orchestrator</title>
  <meta name="description" content="Development workspace management" />
</svelte:head>

<main>
  <slot />
</main>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
    background-color: #f9fafb;
  }

  main {
    min-height: 100vh;
  }

  :global(*) {
    box-sizing: border-box;
  }
</style>
