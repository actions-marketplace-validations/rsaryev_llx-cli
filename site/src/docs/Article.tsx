import { DOCS, GITHUB } from '../links';
import { PAGES, type DocPage } from './content';

/** One page: its group and title, the markdown as HTML, then the pages before and after it and
 *  where to change it. */
export function Article({ page }: { page: DocPage }) {
  const at = PAGES.indexOf(page);
  const previous = at === 0 ? { route: DOCS, title: 'Overview' } : PAGES[at - 1];
  const next = PAGES[at + 1];
  return (
    <article className="docs-article">
      <header className="docs-head">
        <p className="docs-label">{page.group}</p>
        <h1 id={page.titleId}>{page.title}</h1>
      </header>
      <div className="prose" dangerouslySetInnerHTML={{ __html: page.html }} />
      <footer className="docs-foot">
        <nav className="pager" aria-label="Previous and next page">
          {previous && (
            <a className="pager-previous" href={previous.route} rel="prev">
              <span className="docs-label">Previous</span>
              {previous.title}
            </a>
          )}
          {next && (
            <a className="pager-next" href={next.route} rel="next">
              <span className="docs-label">Next</span>
              {next.title}
            </a>
          )}
        </nav>
        <a className="docs-edit" href={`${GITHUB}/edit/main/${page.source}`}>
          Edit this page on GitHub
        </a>
      </footer>
    </article>
  );
}
