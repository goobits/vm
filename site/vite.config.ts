import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		port: 3120,
		host: '0.0.0.0',
		proxy: {
			'/api': {
				target: 'http://localhost:3121',
				changeOrigin: true,
			},
			'/health': {
				target: 'http://localhost:3121',
				changeOrigin: true,
			}
		}
	},
	preview: {
		port: 3120,
		host: '0.0.0.0'
	}
});
