<script lang="ts">
	import { ThemeToggle } from '@goobits/themes/svelte';
</script>

<div class="container">
	<header class="header">
		<h1>VM Tool Documentation</h1>
		<ThemeToggle />
	</header>
	<p>Create development environments that automatically configure themselves.</p>

	<nav class="docs-nav">
		<h2>Documentation</h2>
		<ul>
			<li><a href="/docs/getting-started/quick-start">Quick Start</a></li>
			<li><a href="/docs/getting-started/installation">Installation</a></li>
			<li><a href="/docs/getting-started/examples">Examples</a></li>
			<li><a href="/docs/user-guide/configuration">Configuration Guide</a></li>
			<li><a href="/docs/user-guide/cli-reference">CLI Reference</a></li>
			<li><a href="/docs/development/guide">Development Guide</a></li>
		</ul>
	</nav>

	<nav class="docs-nav" style="margin-top: 2rem;">
		<h2>API</h2>
		<ul>
			<li><a href="/reference">API Reference</a></li>
			<li><a href="/app">Dashboard</a></li>
		</ul>
	</nav>
</div>

<style>
	.container {
		max-width: 800px;
		margin: 4rem auto;
		padding: 0 2rem;
		font-family: system-ui, -apple-system, sans-serif;
	}

	.header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 0.5rem;
	}

	h1 {
		font-size: 3rem;
		margin: 0;
		color: var(--color-text);
	}

	p {
		font-size: 1.2rem;
		color: var(--color-text-secondary);
		margin-bottom: 3rem;
	}

	.docs-nav {
		background: var(--color-surface);
		padding: 2rem;
		border-radius: 8px;
	}

	.docs-nav h2 {
		margin-top: 0;
		font-size: 1.5rem;
		color: var(--color-text);
	}

	.docs-nav ul {
		list-style: none;
		padding: 0;
		margin: 0;
	}

	.docs-nav li {
		margin: 0.8rem 0;
	}

	.docs-nav a {
		color: var(--color-primary);
		text-decoration: none;
		font-size: 1.1rem;
	}

	.docs-nav a:hover {
		text-decoration: underline;
	}
</style>
