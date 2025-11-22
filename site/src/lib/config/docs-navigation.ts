/**
 * Docs Navigation Configuration
 * Required by @goobits/docs-engine DocsSidebar component
 */

export interface DocsLink {
	href: string;
	title: string;
	description?: string;
	order?: number;
	audience?: string;
	section?: string;
}

export interface DocsSection {
	title: string;
	description?: string;
	links: DocsLink[];
}

// This function is called by the DocsSidebar for search functionality
// Returns all links flattened from the navigation structure
export function getAllDocsLinks(navigation?: DocsSection[]): Array<DocsLink & { section: string }> {
	if (!navigation || !Array.isArray(navigation)) {
		return [];
	}

	const links: Array<DocsLink & { section: string }> = [];

	for (const section of navigation) {
		for (const link of section.links || []) {
			links.push({
				...link,
				section: section.title
			});
		}
	}

	return links;
}
