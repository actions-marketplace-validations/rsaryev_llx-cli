// The docs pages' styles and their only script. The pages are whole HTML from the build, with no
// React in the browser: this adds the code blocks' Copy and marks the section being read in the
// table of contents. Without it a page reads the same.
import '../styles/global.css';
import '../styles/syntax.css';
import '../components/Nav.css';
import '../components/Footer.css';
import './docs.css';
import './prose.css';

for (const button of document.querySelectorAll<HTMLButtonElement>('.code-copy')) {
  const code = button.closest('.code')?.querySelector('pre');
  if (!code) continue;
  let timer = 0;
  button.hidden = false;
  button.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(code.textContent ?? '');
    } catch {
      return;
    }
    button.textContent = 'Copied';
    window.clearTimeout(timer);
    timer = window.setTimeout(() => (button.textContent = 'Copy'), 1600);
  });
}

const links = [...document.querySelectorAll<HTMLAnchorElement>('.docs-toc a')];
const sections = links.map((link) => document.getElementById(decodeURIComponent(link.hash.slice(1))));
let frame = 0;

/** The section being read is the last whose heading has passed the top fifth of the window, or
 *  the last one once the page is scrolled to its end. */
function mark() {
  frame = 0;
  const line = window.innerHeight / 5;
  const end = window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 2;
  let at = 0;
  sections.forEach((section, index) => {
    if (section && section.getBoundingClientRect().top < line) at = index;
  });
  if (end) at = sections.length - 1;
  links.forEach((link, index) => {
    if (index === at) link.setAttribute('aria-current', 'location');
    else link.removeAttribute('aria-current');
  });
}

if (links.length > 0) {
  window.addEventListener('scroll', () => (frame ||= window.requestAnimationFrame(mark)), { passive: true });
  mark();
}
