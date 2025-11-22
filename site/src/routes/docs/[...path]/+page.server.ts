import { error } from '@sveltejs/kit';
import { readFile, stat } from 'fs/promises';
import { join } from 'path';
import type { PageServerLoad } from './$types';
import { extractFrontmatter } from '@goobits/docs-engine/utils';
import { marked } from 'marked';

const DOCS_ROOT = join(process.cwd(), '..', 'docs');

// Configure marked for GFM
marked.setOptions({
	gfm: true,
	breaks: false
});

export const load: PageServerLoad = async ({ params }) => {
	try {
		// Handle root path and normalize
		const pathParts = params.path ? params.path.split('/').filter(Boolean) : ['README'];

		// Try with .md extension
		let filePath = join(DOCS_ROOT, ...pathParts) + '.md';
		let foundFile = false;

		// Check if file exists
		try {
			const stats = await stat(filePath);
			if (stats.isFile()) {
				foundFile = true;
			}
		} catch {
			// File doesn't exist, try index.md
		}

		if (!foundFile) {
			// Try index.md in directory
			const indexPath = join(DOCS_ROOT, ...pathParts, 'index.md');
			try {
				const stats = await stat(indexPath);
				if (stats.isFile()) {
					filePath = indexPath;
					foundFile = true;
				}
			} catch {
				// Not found
			}
		}

		if (!foundFile) {
			error(404, 'Documentation page not found');
		}

		// Read markdown content
		const markdownContent = await readFile(filePath, 'utf-8');

		// Extract frontmatter and body (note: returns 'body' not 'content')
		const { frontmatter, body } = extractFrontmatter(markdownContent);

		// Convert markdown to HTML
		const content = await marked(body || markdownContent);

		// Build current path for navigation highlighting
		const currentPath = '/docs/' + (params.path || '');

		return {
			content,
			title: frontmatter?.title || extractTitle(body || markdownContent),
			path: params.path || 'index',
			currentPath,
			frontmatter
		};
	} catch (err) {
		if ((err as any)?.status === 404) {
			throw err;
		}
		console.error('Error loading docs:', err);
		error(404, 'Documentation page not found');
	}
};

function extractTitle(markdown: string): string {
	const match = markdown.match(/^#\s+(.+)$/m);
	return match ? match[1] : 'Documentation';
}
