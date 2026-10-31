import { GitHubIcon } from './GitHubIcon';
import { DOCS, GITHUB } from '../links';
import './Nav.css';

/** The bar over every page. On the landing the wordmark scrolls to the top; on a docs page it goes
 *  home, and Docs is marked as where the reader is. */
export function Nav({ at = 'home' }: { at?: 'home' | 'docs' }) {
  return (
    <>
      <a className="skip" href="#main">
        Skip to the content
      </a>
      <header className="nav wrap">
      <a className="wordmark" href={at === 'home' ? '#top' : '/'} aria-label="llx home">
        llx
      </a>
      <nav className="nav-links" aria-label="Site">
        <a className="nav-link" href={DOCS} aria-current={at === 'docs' ? 'true' : undefined}>
          Docs
        </a>
        <a className="gh" href={GITHUB} aria-label="llx on GitHub" title="GitHub">
          <GitHubIcon size={20} />
        </a>
      </nav>
      </header>
    </>
  );
}
