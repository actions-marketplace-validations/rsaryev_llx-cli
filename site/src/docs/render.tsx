import { renderToStaticMarkup } from 'react-dom/server';
import { Footer } from '../components/Footer';
import { Nav } from '../components/Nav';
import { DOCS } from '../links';
import { Article } from './Article';
import { PAGES, type DocPage } from './content';
import { DocsIndex } from './DocsIndex';
import { head } from './head';
import { INDEX } from './nav';
import { Sidebar } from './Sidebar';
import { Toc } from './Toc';

const HEAD = '<!--docs:head-->';
const BODY = '<!--docs:body-->';

/** Every docs route the build writes: the index, then each page. */
export const DOCS_ROUTES = [DOCS, ...PAGES.map((page) => page.route)];

/** The landing's nav and footer around the docs: the menu (a phone) or the rail (wider), the
 *  page, and its sections on the right of a wide screen. No script draws any of it. */
function Layout({ page, missing }: { page?: DocPage | undefined; missing?: true }) {
  return (
    <>
      <Nav at={missing ? 'elsewhere' : 'docs'} />
      <div className="docs wrap">
        <details className="docs-menu">
          <summary>
            <span className="docs-label">Docs</span>
            <span>{page?.title ?? 'Overview'}</span>
          </summary>
          <Sidebar active={page?.slug} />
        </details>
        <aside className="docs-side">
          <Sidebar active={page?.slug} />
        </aside>
        <main id="main" className="docs-main">
          {missing ? <NotFound /> : page ? <Article page={page} /> : <DocsIndex />}
        </main>
        {page && <Toc headings={page.headings} />}
      </div>
      <Footer />
    </>
  );
}

/** What any URL the site does not have shows (404.html, which Pages serves with a 404 status
 *  instead of the landing with a 200): where to go instead, the docs' menu beside it. */
function NotFound() {
  return (
    <div className="docs-index">
      <header className="docs-head">
        <p className="docs-label">404</p>
        <h1>Page not found</h1>
        <p className="docs-lede">
          Nothing lives at this address. Go to the <a href="/">home page</a> or the <a href={DOCS}>docs</a>.
        </p>
      </header>
    </div>
  );
}

/** 404.html: the docs' template around NotFound, kept out of search results. */
export function renderNotFound(template: string): string {
  const meta = ['<title>Page not found · llx</title>', '<meta name="robots" content="noindex">'].join('\n  ');
  const body = renderToStaticMarkup(<Layout missing />);
  return template.replace(HEAD, () => meta).replace(BODY, () => body);
}

/** A docs route as a whole document: `template` (docs.html, as built or as written) with the
 *  page's head and body in its two marks. Undefined for a route that is no page. */
export function renderDocsPage(route: string, template: string): string | undefined {
  const page = PAGES.find((candidate) => candidate.route === route);
  if (!page && route !== DOCS) return undefined;
  for (const mark of [HEAD, BODY]) {
    if (!template.includes(mark)) throw new Error(`${mark} is missing from docs.html`);
  }
  const meta = page
    ? { title: `${page.title} · llx docs`, headline: page.title, description: page.description, route }
    : { ...INDEX, headline: INDEX.title, route };
  const body = renderToStaticMarkup(<Layout page={page} />);
  // Replaced by functions: a `$&` in a page is text, not a pattern.
  return template.replace(HEAD, () => head(meta)).replace(BODY, () => body);
}
