import type { Token } from 'marked';
import { DOCS, GITHUB, SITE } from '../links';
import { classify } from './hrefs';
import { eachLink, parse, render, type Heading } from './markdown';
import { NAV, UNLISTED } from './nav';

/** The repository's markdown, read when the site is built (and on each request in dev). */
const FILES = import.meta.glob<string>(['../../../docs/*.md', '../../../examples/README.md'], {
  query: '?raw',
  import: 'default',
  eager: true,
});
const SOURCES = new Map(Object.entries(FILES).map(([key, text]) => [key.replace(/^(\.\.\/)+/, ''), text]));

export type DocPage = {
  slug: string;
  route: string;
  source: string;
  group: string;
  title: string;
  titleId: string;
  /** The first paragraph, cut to what a search result or a link preview shows. */
  description: string;
  headings: Heading[];
  html: string;
  /** The markdown with every link made absolute, for llms-full.txt. */
  markdown: string;
};

type Draft = Omit<DocPage, 'html' | 'markdown'> & { tokens: Token[]; text: string };

function cut(text: string, most = 200): string {
  if (text.length <= most) return text;
  const head = text.slice(0, most - 1);
  return `${head.slice(0, head.lastIndexOf(' ')).replace(/[,:;.]$/, '')}…`;
}

const problems: string[] = [];
const drafts: Draft[] = NAV.flatMap((group) =>
  group.pages.flatMap(({ slug, source }) => {
    const text = SOURCES.get(source);
    if (text === undefined) {
      problems.push(`${source}: in nav.ts, but there is no such file`);
      return [];
    }
    try {
      const { summary, ...parsed } = parse(text);
      return [{ ...parsed, slug, source, text, route: `${DOCS}/${slug}`, group: group.title, description: cut(summary) }];
    } catch (error) {
      problems.push(`${source}: ${error instanceof Error ? error.message : String(error)}`);
      return [];
    }
  }),
);
for (const source of SOURCES.keys()) {
  if (source.startsWith('docs/') && !drafts.some((page) => page.source === source) && !(source in UNLISTED)) {
    problems.push(`${source}: neither a page in nav.ts nor UNLISTED there`);
  }
}

/** Repository files the docs link to on GitHub, and pictures copied into the build: prerender.mjs
 *  checks the first exist and copies the second to the same path under dist/. */
export const REPO_FILES = new Set<string>();
export const IMAGES = new Set<string>();

/** Where a link of `page` goes on the site: a page here (its anchor checked), a picture copied
 *  beside the pages, or the file on GitHub. A link that goes nowhere is a problem. */
function site(page: Draft, href: string, image: boolean): string {
  const to = classify(page.source, href);
  const wrong = (why: string) => {
    problems.push(`${page.source}: ${href} (${why})`);
    return href;
  };
  if (to.kind === 'url') return href;
  if (to.kind === 'outside') return wrong('outside the repository');
  const target = to.kind === 'anchor' ? page : drafts.find((draft) => draft.source === to.path);
  if (target) {
    const known = to.hash === '' || to.hash === target.titleId || target.headings.some((h) => h.id === to.hash);
    if (!known) return wrong(`${target.source} has no heading #${to.hash}`);
    const hash = to.hash ? `#${to.hash}` : '';
    return target === page && to.kind === 'anchor' ? hash : `${target.route}${hash}`;
  }
  if (to.kind === 'anchor') return wrong('no such page');
  if (image) {
    IMAGES.add(to.path);
    return `/${to.path}`;
  }
  if (to.path.startsWith('docs/') && to.path.endsWith('.md') && !(to.path in UNLISTED)) {
    return wrong('no such doc');
  }
  REPO_FILES.add(to.path);
  return `${GITHUB}/blob/main/${to.path}${to.hash ? `#${to.hash}` : ''}`;
}

/** Markdown links outside fenced code, as llms-full.txt keeps them. */
function absolute(page: Draft, links: Map<string, string>): string {
  return page.text
    .split(/(^```[\s\S]*?^```)/m)
    .map((part) =>
      part.startsWith('```')
        ? part
        : part.replace(/(\]\()([^)\s]+)(\))/g, (all, open: string, href: string, close: string) => {
            const to = links.get(href);
            if (to === undefined) return all;
            const url = to.startsWith('#') ? `${SITE}${page.route}${to}` : to.startsWith('/') ? `${SITE}${to}` : to;
            return `${open}${url}${close}`;
          }),
    )
    .join('');
}

/** Every page, in the order of nav.ts. */
export const PAGES: DocPage[] = drafts.map((draft) => {
  const links = new Map<string, string>();
  eachLink(draft.tokens, (token) => {
    const to = site(draft, token.href, token.type === 'image');
    links.set(token.href, to);
    token.href = to;
  });
  const { tokens: _tokens, text: _text, ...page } = draft;
  return { ...page, html: render(draft.tokens), markdown: absolute(draft, links) };
});

if (problems.length > 0) {
  throw new Error(`The docs' markdown has problems:\n  ${problems.join('\n  ')}`);
}

/** The groups of nav.ts with their pages. */
export const GROUPS = NAV.map((group) => ({
  title: group.title,
  blurb: group.blurb,
  pages: PAGES.filter((page) => page.group === group.title),
}));
