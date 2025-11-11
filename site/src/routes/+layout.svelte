<script lang="ts">
	import favicon from '$lib/assets/favicon.svg';
	import '@goobits/docs-engine/styles';

	let { children } = $props();
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

{@render children()}
