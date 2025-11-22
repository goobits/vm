/**
 * Theme Configuration for VM Tool Website
 * Uses @goobits/themes for light/dark theme support
 */
import type { ThemeConfig } from '@goobits/themes/core';

export const themeConfig: ThemeConfig = {
	defaultTheme: 'system',
	defaultScheme: 'default',
	schemes: {
		default: {
			name: 'Default',
			description: 'Clean, modern theme'
		}
	},
	// Route-specific theme overrides (optional)
	routeOverrides: {
		// Example: force dark theme on docs pages
		// '/docs': { theme: 'dark' }
	}
};
