/**
 * Theme Configuration for VM Tool Website
 * Uses @goobits/themes for light/dark theme support
 */
import { createThemeConfig } from '@goobits/themes/core';

export const themeConfig = createThemeConfig({
	schemes: {
		default: {
			name: 'default',
			displayName: 'Default',
			description: 'Clean, modern theme',
			preview: {
				primary: '#2563eb',
				accent: '#0f766e',
				background: '#ffffff'
			}
		}
	},
	// Route-specific theme overrides (optional)
	routeThemes: {
		// Example: force dark theme on docs pages
		// '/docs': { theme: 'dark' }
	}
});
