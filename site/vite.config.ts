import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';

/** The docs in dev: /docs and /docs/<slug> rendered on each request, as prerender.mjs writes them
 *  at build time, and the page reloaded when a doc's markdown changes. */
function docs(): Plugin {
  return {
    name: 'llx-docs',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        // originalUrl: Connect sets it, and the site's types know it without Node's.
        const url: string = req.originalUrl ?? '';
        const route = url.split(/[?#]/)[0]?.replace(/(.)\/$/, '$1') ?? '';
        if (route !== '/docs' && !route.startsWith('/docs/')) return next();
        void (async () => {
          const entry = await server.ssrLoadModule('/src/entry-server.tsx');
          const template = (await server.ssrLoadModule('/docs.html?raw')).default as string;
          const html: string | undefined = entry.renderDocsPage(route, template);
          if (html === undefined) return next();
          res.setHeader('Content-Type', 'text/html; charset=utf-8');
          res.end(await server.transformIndexHtml(route, html));
        })().catch(next);
      });
      server.watcher.on('change', (file) => {
        if (file.endsWith('.md')) server.ws.send({ type: 'full-reload' });
      });
    },
  };
}

// The site is served from the domain root (https://llx.sh/); public/ keeps every static URL as it was.
// Two pages are built: the landing, and docs.html, the shell every docs page is prerendered into.
export default defineConfig({
  plugins: [react(), docs()],
  base: '/',
  build: {
    outDir: 'dist',
    target: 'es2022',
    // Every browser the es2022 target reaches preloads modules itself: no polyfill, and so no
    // chunk the two pages would share for it alone.
    modulePreload: { polyfill: false },
    rolldownOptions: {
      input: { main: 'index.html', docs: 'docs.html' },
    },
  },
});
