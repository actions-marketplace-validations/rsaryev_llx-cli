import { DOCS, GITHUB, INSTALL } from '../links';
import { InstallCommand } from './InstallCommand';
import './Closing.css';

/** The page's end: the one thing to do next, with the same command the hero offers. */
export function Closing() {
  return (
    <section className="closing wrap" aria-labelledby="closing-title">
      <h2 id="closing-title">Install it in one line</h2>
      <p className="closing-sub">
        One binary under 4 MB for macOS and Linux. Sign in with <code>/login</code>, or set a provider&apos;s key, and give
        it a task.
      </p>
      <InstallCommand tabs={INSTALL} />
      <p className="closing-links">
        <a href={DOCS}>Read the docs</a>
        <span aria-hidden="true">·</span>
        <a href={GITHUB}>Source on GitHub</a>
      </p>
    </section>
  );
}
