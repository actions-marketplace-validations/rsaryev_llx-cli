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
          <code>llx exec</code> runs one task and ends with a result and an exit code your pipeline can branch on. One
          binary under 4 MB, on 1,000+ models from 200+ providers: a ChatGPT sign-in, OpenRouter or your own key. It is
          a minimal agent at your terminal, too.
        </p>
        <InstallCommand tabs={INSTALL} />
        <p className="hero-under">
          macOS and Linux · MIT · <a href={DOCS}>Docs</a>
        </p>
      </div>
      <figure className="hero-demo">
        <Film src="/demo-llx.cast" label="An llx session fixes a failing test in its sandbox, asks how a tie should be decided with each option's code beside it, and builds the choice; then llx exec checks the change as CI would" />
      </figure>
    </section>
  );
}
