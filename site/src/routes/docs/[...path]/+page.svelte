<script lang="ts">
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();
</script>

<svelte:head>
	<title>{data.title} | VM Tool Documentation</title>
</svelte:head>

<article class="prose">
	{@html data.content}
</article>

<style>
	.prose {
		max-width: 65ch;
		margin: 2rem auto;
		padding: 0 2rem;
		font-family: system-ui, -apple-system, sans-serif;
		line-height: 1.6;
		color: #333;
	}

	.prose :global(h1) {
		font-size: 2.5rem;
		margin-top: 0;
		margin-bottom: 1rem;
	}

	.prose :global(h2) {
		font-size: 1.8rem;
		margin-top: 2rem;
		margin-bottom: 1rem;
		border-bottom: 1px solid #eee;
		padding-bottom: 0.3rem;
	}

	.prose :global(h3) {
		font-size: 1.4rem;
		margin-top: 1.5rem;
		margin-bottom: 0.8rem;
	}

	.prose :global(code) {
		background: #f5f5f5;
		padding: 0.2em 0.4em;
		border-radius: 3px;
		font-size: 0.9em;
	}

	.prose :global(pre) {
		background: #f5f5f5;
		padding: 1rem;
		border-radius: 5px;
		overflow-x: auto;
	}

	.prose :global(pre code) {
		background: none;
		padding: 0;
	}

	.prose :global(a) {
		color: #0066cc;
		text-decoration: none;
	}

	.prose :global(a:hover) {
		text-decoration: underline;
	}
</style>
