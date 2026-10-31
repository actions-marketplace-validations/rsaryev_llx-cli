import { type CSSProperties, type ReactNode, useEffect, useRef } from 'react';
import { Tokens } from './Code';
import { type Block, CARDS, type Cell } from './cards';
import { highlight } from './highlight';
import './Loop.css';

/** The steps one after another, each card held at the top while the next slides over it; a
 *  card's lines come in once it is seen. Without script everything simply stands. */
export function Loop() {
  const root = useRef<HTMLElement>(null);

  useEffect(() => {
    const cards = [...(root.current?.querySelectorAll<HTMLElement>('.loop-card') ?? [])];
    const seen = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          entry.target.setAttribute('data-reveal', 'seen');
          seen.unobserve(entry.target);
        }
      },
      { threshold: 0.35 },
    );
    for (const card of cards) {
      if (card.getBoundingClientRect().top > window.innerHeight) card.setAttribute('data-reveal', 'wait');
      seen.observe(card);
    }
    return () => seen.disconnect();
  }, []);

  return (
    <section ref={root} className="loop wrap" aria-labelledby="loop-title">
      <h2 id="loop-title">One binary, the whole loop</h2>
      <div className="loop-stack">
        {CARDS.map((card, index) => (
          <article key={card.title} className="loop-card" style={{ '--n': index } as CSSProperties}>
            <div className="loop-text">
              <h3>{card.title}</h3>
              <p>{card.text}</p>
              {card.link && (
                <a className="loop-link" href={card.link.href}>
                  {card.link.label}
                </a>
              )}
            </div>
            <pre className="loop-panel" aria-label={`${card.title}: what llx printed`}>
              {panelRows(card.panel)}
            </pre>
          </article>
        ))}
      </div>
    </section>
  );
}

const tone = (name: string) => (name ? `t-${name}` : undefined);

function cell([name, text]: [string, string], col: number, align: string | undefined) {
  return (
    <span key={col} className={[tone(name), align?.[col] === 'r' && 'r'].filter(Boolean).join(' ') || undefined}>
      {text}
    </span>
  );
}

/** The panel's blocks as rows, numbered across all of them for the lines that come in one by one.
 *  A table's rows share its columns (a subgrid), so the columns line up whatever the text. */
function panelRows(panel: Block[]): ReactNode[] {
  const out: ReactNode[] = [];
  let n = 0;
  const row = (children: ReactNode, grid = false) => {
    const row = (
      <span key={n} className={grid ? 'loop-row loop-cells' : 'loop-row'} style={{ '--i': n } as CSSProperties}>
        {children}
      </span>
    );
    n += 1;
    return row;
  };
  for (const block of panel) {
    if ('lang' in block) {
      for (const line of highlight(block.text, block.lang, true)) out.push(row(<Tokens line={line} />));
    } else if ('out' in block) {
      out.push(row(<span className={tone(block.tone ?? '')}>{block.out}</span>));
    } else if ('ui' in block) {
      for (const line of block.ui) {
        out.push(
          row(
            line.map(([name, text], part) => (
              <span key={part} className={tone(name)}>
                {text}
              </span>
            )),
          ),
        );
      }
    } else {
      const cols = Math.max(...block.grid.map((cells) => cells.length));
      const style = { gridTemplateColumns: `repeat(${cols}, max-content)` };
      out.push(
        <span key={`grid-${n}`} className="loop-grid" style={style}>
          {block.grid.map((cells) =>
            row(
              cells.map((c: Cell, col) => cell(typeof c === 'string' ? ['', c] : c, col, block.align)),
              true,
            ),
          )}
        </span>,
      );
    }
  }
  return out;
}
