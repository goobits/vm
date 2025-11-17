<script lang="ts">
  import WorkspaceList from '$lib/components/WorkspaceList.svelte';
</script>

<div class="container">
  <header>
    <h1>VM Orchestrator</h1>
    <p class="subtitle">Manage your development workspaces</p>
  </header>

  <WorkspaceList />
</div>

<style>
  .container {
    max-width: 1400px;
    margin: 0 auto;
    padding: 2rem;
  }

  header {
    margin-bottom: 2rem;
  }

  h1 {
    font-size: 2rem;
    font-weight: 700;
    color: #111827;
    margin-bottom: 0.5rem;
  }

  .subtitle {
    font-size: 1rem;
    color: #6b7280;
  }
</style>
