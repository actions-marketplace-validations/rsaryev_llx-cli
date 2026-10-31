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
function Layout({ page }: { page?: DocPage | undefined }) {
  return (
    <>
      <Nav at="docs" />
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
        <main id="main" className="docs-main">{page ? <Article page={page} /> : <DocsIndex />}</main>
        {page && <Toc headings={page.headings} />}
      </div>
      <Footer />
    </>
  );
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
