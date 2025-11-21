<script lang="ts">
    let openApiUrl = '/api-docs/openapi.json';
</script>

<svelte:head>
    <title>API Reference</title>
    <meta name="description" content="VM Tool API Documentation" />
</svelte:head>

<div class="scalar-container">
    <script
        id="api-reference"
        data-url={openApiUrl}
        src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"
    ></script>
</div>

<style>
    :global(body) {
        margin: 0;
        padding: 0;
    }
    /* Ensure Scalar takes full height */
    .scalar-container {
        display: flex;
        flex-direction: column;
        height: 100vh;
        width: 100vw;
    }
</style>
