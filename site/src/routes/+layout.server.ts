/**
 * Root Layout Server Load
 * Passes theme preferences to client for hydration
 */
import type { LayoutServerLoad } from './$types';
import { loadThemePreferences } from '@goobits/themes/server';
import { themeConfig } from '$lib/config/theme';

export const load: LayoutServerLoad = async ({ cookies }) => {
	const preferences = loadThemePreferences(cookies, themeConfig);

	return {
		preferences
	};
};
