import { StrictMode } from 'react';
import { renderToString } from 'react-dom/server';
import { App } from './App';

/** The page as HTML, for prerender.mjs to put into dist/index.html at build time. */
export function render(): string {
  return renderToString(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

/** The docs, for prerender.mjs (and the dev server): each page as HTML, the files written beside
 *  them, and what the build checks and copies from the repository. */
export { DOCS_ROUTES, renderDocsPage } from './docs/render';
export { llmsDocs, llmsFull, sitemap } from './docs/files';
export { IMAGES, REPO_FILES } from './docs/content';
