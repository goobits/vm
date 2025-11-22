import type { LayoutServerLoad } from './$types';
import { buildNavigation, createDocFile } from '@goobits/docs-engine/utils';
import { readFile, readdir } from 'fs/promises';
import { join } from 'path';

const DOCS_ROOT = join(process.cwd(), '..', 'docs');

async function scanDocsDir(dir: string, basePath: string = ''): Promise<Array<{ path: string; content: string }>> {
	const files: Array<{ path: string; content: string }> = [];

	try {
		const entries = await readdir(dir, { withFileTypes: true });

		for (const entry of entries) {
			const fullPath = join(dir, entry.name);

			if (entry.isDirectory()) {
				const subFiles = await scanDocsDir(fullPath, join(basePath, entry.name));
				files.push(...subFiles);
			} else if (entry.name.endsWith('.md') && entry.name !== 'README.md') {
				const relativePath = join(basePath, entry.name);
				const content = await readFile(fullPath, 'utf-8');
				files.push({ path: relativePath, content });
			}
		}
	} catch (err) {
		console.error('Error scanning directory:', dir, err);
	}

	return files;
}

// Strip non-serializable properties (like icon functions) from navigation
function makeSerializable(nav: any[]): any[] {
	return nav.map((section) => ({
		title: section.title,
		description: section.description,
		links: section.links?.map((link: any) => ({
			href: link.href,
			title: link.title,
			description: link.description,
			order: link.order,
			audience: link.audience
		})) || []
	}));
}

export const load: LayoutServerLoad = async () => {
	try {
		// Scan docs directory for all markdown files
		const docFiles = await scanDocsDir(DOCS_ROOT);

		if (docFiles.length === 0) {
			console.warn('No documentation files found in:', DOCS_ROOT);
			return { navigation: [] };
		}

		// Convert to DocFile format using createDocFile
		const files = docFiles.map((f) =>
			createDocFile({
				path: f.path,
				content: f.content,
				basePath: '/docs'
			})
		);

		// Build navigation
		const navigation = buildNavigation(files, {});

		// Make serializable (strip icon functions)
		const serializableNav = makeSerializable(navigation);

		return {
			navigation: serializableNav
		};
	} catch (err) {
		console.error('Error loading docs navigation:', err);
		return {
			navigation: []
		};
	}
};
