<script lang="ts">
	import favicon from '$lib/assets/favicon.svg';
	import '@goobits/docs-engine/styles';
	import '@goobits/themes/themes';
	import { ThemeProvider } from '@goobits/themes/svelte';
	import { themeConfig } from '$lib/config/theme';
	import type { LayoutData } from './$types';

	let { data, children }: { data: LayoutData; children: any } = $props();
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

<ThemeProvider config={themeConfig} serverPreferences={data.preferences}>
	{@render children()}
</ThemeProvider>
