import { DOCS, GITHUB, MAKER } from '../links';
import './Footer.css';

/** Under every page, one strip: who makes llx, its one line, then where else to go. */
export function Footer() {
  return (
    <footer className="foot wrap">
      <span>
        <a className="foot-maker" href={MAKER.href}>
          {MAKER.label}
        </a>
        llx is MIT-licensed and open source.
      </span>
      <nav aria-label="Footer">
        <a href={DOCS}>Docs</a>
        <a href={GITHUB}>GitHub</a>
        <a href={`${GITHUB}/blob/main/MODELS.md`}>Models</a>
        <a href={`${GITHUB}/blob/main/CHANGELOG.md`}>Changelog</a>
      </nav>
    </footer>
  );
}
