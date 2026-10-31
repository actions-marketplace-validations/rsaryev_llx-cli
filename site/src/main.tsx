import { StrictMode } from 'react';
import { createRoot, hydrateRoot } from 'react-dom/client';
import { App } from './App';
import './styles/global.css';

const container = document.getElementById('root');
if (!container) throw new Error('#root is missing from index.html');

const app = (
  <StrictMode>
    <App />
  </StrictMode>
);

// The build prerenders the page into #root (prerender.mjs), so the text is there before any script
// runs; the dev server serves an empty #root.
if (container.hasChildNodes()) hydrateRoot(container, app);
else createRoot(container).render(app);
