import adapter from '@sveltejs/adapter-auto';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { mdsvex } from 'mdsvex';
import remarkMath from 'remark-math';
import {
	filetreePlugin,
	calloutsPlugin,
	mermaidPlugin,
	tabsPlugin,
	codeHighlightPlugin,
	katexPlugin,
	remarkTableOfContents,
	linksPlugin,
	screenshotPlugin
} from '@goobits/docs-engine/plugins';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	extensions: ['.svelte', '.md'],

	preprocess: [
		vitePreprocess(),
		mdsvex({
			extensions: ['.md'],
			remarkPlugins: [
				filetreePlugin(),
				calloutsPlugin(),
				mermaidPlugin(),
				tabsPlugin(),
				remarkTableOfContents(),
				linksPlugin(),
				screenshotPlugin(),
				remarkMath,
				katexPlugin(),
				codeHighlightPlugin({
					theme: 'dracula',
					showLineNumbers: false
				})
			]
		})
	],

	kit: {
		adapter: adapter()
	}
};

export default config;
