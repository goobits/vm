<script lang="ts">
	import { DocsLayout } from '@goobits/docs-engine/components';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	// Get navigation from layout data
	const navigation = $derived((data as any).navigation || []);
</script>

<svelte:head>
	<title>{data.title} | VM Documentation</title>
	{#if data.frontmatter?.description}
		<meta name="description" content={data.frontmatter.description} />
	{/if}
</svelte:head>

<DocsLayout
	content={data.content}
	title={data.title}
	{navigation}
	currentPath={data.currentPath}
	theme="dracula"
	footer={{
		text: 'VM Documentation'
	}}
/>
