/**
 * Server Hooks
 * Handles theme preferences via cookies for zero-flash SSR
 */
import { createThemeHooks } from '@goobits/themes/server';
import { themeConfig } from '$lib/config/theme';

const themeHooks = createThemeHooks(themeConfig);

export const handle = themeHooks.transform;
