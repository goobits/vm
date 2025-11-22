import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, loadEnv } from 'vite';

// SSOT: WEB_PORT defined in .env (must match vm.yaml ports._range[0])
const WEB_PORT = 3120;

export default defineConfig(({ mode }) => {
	const env = loadEnv(mode, process.cwd(), '');
	const port = parseInt(env.WEB_PORT) || WEB_PORT;

	return {
		plugins: [sveltekit()],
		optimizeDeps: {
			// Exclude docs-engine components from pre-bundling since they use $lib alias
			exclude: ['@goobits/docs-engine']
		},
		server: {
			port,
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
			port,
			host: '0.0.0.0'
		}
	};
});
