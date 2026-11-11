import { DOCS, INSTALL } from '../links';
import { Film } from '../demo/Film';
import { InstallCommand } from './InstallCommand';
import './Hero.css';

/** The landing, one screen: what llx is on the left, a real run of it on the right. */
export function Hero() {
  return (
    <section className="hero wrap">
      <div className="hero-copy">
        <h1>
          <span className="hero-lead">
            <span className="hero-chip">Ultrafast</span>,
          </span>{' '}
          lightweight agent runtime for coding and CI
        </h1>
        <p className="lede">
          <code>llx exec</code> runs one task and ends with an answer and an exit code your pipeline can branch on. One
          binary under 4 MB, on 1,000+ models from 200+ providers: a ChatGPT sign-in, OpenRouter or your own key. It is
          a minimal agent at your terminal, too.
        </p>
        <InstallCommand tabs={INSTALL} />
        <p className="hero-under">
          macOS and Linux · MIT · <a href={DOCS}>Docs</a>
        </p>
      </div>
      <figure className="hero-demo">
        <Film src="/demo-llx.cast" label="An llx session on Claude Haiku 5.5: /model lists the models of every provider signed in and picks one; the agent fixes a failing test in its sandbox; two agents work in parallel and their work is checked; then, from the shell, llx decide answers a question about the diff in under a second and llx exec checks the change as CI would, answering with data" />
      </figure>
    </section>
  );
}
