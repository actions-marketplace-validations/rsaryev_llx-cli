import { DOCS, SITE } from '../links';
import { GROUPS, PAGES } from './content';
import { INDEX } from './nav';

/** sitemap.xml: the landing, the docs' index and every page. */
export function sitemap(): string {
  const urls = ['/', DOCS, ...PAGES.map((page) => page.route)]
    .map((route) => `  <url>\n    <loc>${SITE}${route === '/' ? '/' : route}</loc>\n    <changefreq>weekly</changefreq>\n  </url>`)
    .join('\n');
  return `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`;
}

/** The docs' part of llms.txt: each page with what it covers, by group, and the whole in one file. */
export function llmsDocs(): string {
  const groups = GROUPS.map(
    (group) =>
      `### ${group.title}\n\n${group.pages.map((page) => `- [${page.title}](${SITE}${page.route}): ${page.description}`).join('\n')}`,
  );
  return `## Docs\n\n${groups.join('\n\n')}\n\nEvery page in one markdown file: ${SITE}/llms-full.txt\n`;
}

/** llms-full.txt: every page's markdown, in the order of the docs, its links made absolute. */
export function llmsFull(): string {
  const pages = PAGES.map((page) => `---\n\nSource: ${SITE}${page.route}\n\n${page.markdown.trim()}\n`);
  return `# ${INDEX.title}\n\n> ${INDEX.description}\n\n${pages.join('\n')}`;
}
