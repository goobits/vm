import { error } from '@sveltejs/kit';
import { readFile, stat } from 'fs/promises';
import { join } from 'path';
import { marked } from 'marked';
import type { PageServerLoad } from './$types';

const DOCS_DIR = join(process.cwd(), '..', 'docs');

// Configure marked for better rendering
// Note: Full docs-engine plugin support (mermaid, callouts, tabs) requires
// build-time processing with mdsvex. For now, we use runtime markdown parsing.
marked.setOptions({
	gfm: true,
	breaks: false,
	pedantic: false
});

export const load: PageServerLoad = async ({ params }) => {
	try {
		// Handle root path and normalize
		const pathParts = params.path ? params.path.split('/').filter(Boolean) : [];

		// Try with .md extension
		const filePath = join(DOCS_DIR, ...pathParts) + '.md';

		// Check if file exists
		const stats = await stat(filePath);

		if (!stats.isFile()) {
			error(404, 'Not found');
		}

		// Read markdown content
		const markdownContent = await readFile(filePath, 'utf-8');

		// Parse markdown to HTML
		const htmlContent = marked(markdownContent);

		return {
			content: htmlContent,
			path: params.path || 'index',
			title: extractTitle(markdownContent)
		};
	} catch (err) {
		console.error('Error loading docs:', err);
		error(404, 'Documentation page not found');
	}
};

function extractTitle(markdown: string): string {
	const match = markdown.match(/^#\s+(.+)$/m);
	return match ? match[1] : 'Documentation';
}
