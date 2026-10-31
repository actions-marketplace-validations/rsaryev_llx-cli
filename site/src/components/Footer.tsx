import { DOCS, GITHUB } from '../links';
import './Footer.css';

export function Footer() {
  return (
    <footer className="foot wrap">
      <span>llx is MIT-licensed and open source.</span>
      <nav aria-label="Footer">
        <a href={DOCS}>Docs</a>
        <a href={GITHUB}>GitHub</a>
        <a href={`${GITHUB}/blob/main/MODELS.md`}>Models</a>
        <a href={`${GITHUB}/blob/main/CHANGELOG.md`}>Changelog</a>
      </nav>
    </footer>
  );
}
