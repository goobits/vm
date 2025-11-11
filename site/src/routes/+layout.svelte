<script lang="ts">
	import favicon from '$lib/assets/favicon.svg';
	import {
		CodeTabsHydrator,
		FileTreeHydrator,
		MermaidHydrator,
		ScreenshotHydrator
	} from '@goobits/docs-engine/components';
	import '@goobits/docs-engine/styles';

	let { children } = $props();
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

<CodeTabsHydrator theme="dracula" />
<FileTreeHydrator allowCopy={true} />
<MermaidHydrator />
<ScreenshotHydrator />

{@render children()}
