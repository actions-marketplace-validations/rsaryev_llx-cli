import { SITE } from '../links';
import { escapeHtml } from './highlight';

/** The card every link preview shows, as on the landing (index.html). */
const IMAGE = `${SITE}/og.png`;
const IMAGE_ALT =
  'llx at a shell: an exec task answered with a path, and a decide that picked minor';

export type Meta = { title: string; headline: string; description: string; route: string };

/** A docs page's own head: its title, description, canonical URL, link previews and what it is. */
export function head({ title, headline, description, route }: Meta): string {
  const url = `${SITE}${route}`;
  const [t, d] = [escapeHtml(title), escapeHtml(description)];
  const data = {
    '@context': 'https://schema.org',
    '@type': 'TechArticle',
    headline,
    description,
    url,
    isPartOf: { '@type': 'WebSite', name: 'llx', url: `${SITE}/` },
  };
  return [
    `<title>${t}</title>`,
    `<meta name="description" content="${d}">`,
    `<link rel="canonical" href="${url}">`,
    '<meta property="og:type" content="article">',
    '<meta property="og:site_name" content="llx">',
    '<meta property="og:locale" content="en_US">',
    `<meta property="og:url" content="${url}">`,
    `<meta property="og:title" content="${t}">`,
    `<meta property="og:description" content="${d}">`,
    `<meta property="og:image" content="${IMAGE}">`,
    '<meta property="og:image:type" content="image/png">',
    '<meta property="og:image:width" content="1200">',
    '<meta property="og:image:height" content="630">',
    `<meta property="og:image:alt" content="${IMAGE_ALT}">`,
    '<meta name="twitter:card" content="summary_large_image">',
    `<meta name="twitter:title" content="${t}">`,
    `<meta name="twitter:description" content="${d}">`,
    `<meta name="twitter:image" content="${IMAGE}">`,
    `<meta name="twitter:image:alt" content="${IMAGE_ALT}">`,
    `<script type="application/ld+json">${JSON.stringify(data).replace(/</g, '\\u003c')}</script>`,
  ].join('\n  ');
}
