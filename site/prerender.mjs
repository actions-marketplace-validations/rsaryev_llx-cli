// Puts the rendered page into dist/index.html, so the text is in the HTML before any script runs
// (for crawlers, link previews and visitors without JavaScript); the client then hydrates it.
// Then writes each docs page as a whole HTML file (/docs/ci is dist/docs/ci.html, which
// Cloudflare Pages serves at that URL with no redirect), and the files that list them.
// Runs after `vite build` and `vite build --ssr`, then removes the server bundle.
import { existsSync } from 'node:fs';
import { cp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const dist = fileURLToPath(new URL('./dist/', import.meta.url));
const server = fileURLToPath(new URL('./dist-ssr/', import.meta.url));
const repo = fileURLToPath(new URL('../', import.meta.url));

// Importing the bundle reads the docs' markdown, and fails on a link to a page or a heading that
// does not exist.
const ssr = await import(new URL('entry-server.js', `file://${server}`).href);
const page = `${dist}index.html`;
const html = await readFile(page, 'utf8');
const mount = '<div id="root"></div>';
if (!html.includes(mount)) throw new Error(`${mount} is missing from dist/index.html`);
await writeFile(page, html.replace(mount, `<div id="root">${ssr.render()}</div>`));

const missing = [...ssr.REPO_FILES].filter((path) => !existsSync(repo + path));
if (missing.length > 0) {
  throw new Error(`The docs link to files the repository does not have:\n  ${missing.join('\n  ')}`);
}
const template = await readFile(`${dist}docs.html`, 'utf8');
for (const route of ssr.DOCS_ROUTES) {
  const file = `${dist}${route.slice(1)}.html`;
  await mkdir(dirname(file), { recursive: true });
  await writeFile(file, ssr.renderDocsPage(route, template));
}
for (const image of ssr.IMAGES) {
  await mkdir(dirname(dist + image), { recursive: true });
  await cp(repo + image, dist + image);
}
await writeFile(`${dist}sitemap.xml`, ssr.sitemap());
await writeFile(`${dist}llms-full.txt`, ssr.llmsFull());
const llms = await readFile(`${dist}llms.txt`, 'utf8');
await writeFile(`${dist}llms.txt`, `${llms.trimEnd()}\n\n${ssr.llmsDocs()}`);

await rm(server, { recursive: true, force: true });
console.log(`prerendered dist/index.html and ${ssr.DOCS_ROUTES.length} docs pages`);
