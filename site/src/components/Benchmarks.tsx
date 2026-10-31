import { type KeyboardEvent, useEffect, useRef, useState } from 'react';
import { BOARDS } from '../bench/data';
import { useReplay } from '../bench/useReplay';
import type { Board } from '../bench/types';
import { BenchTable } from './BenchTable';
import './Benchmarks.css';

/** The benchmark: one task, each agent on the same model, a tab per model. A board is a table
 *  (the accessible source of every number) whose cells carry their own bars: the score with its
 *  range on a fixed scale from 0, the rest against the board's largest. Every run of a batch
 *  started at once, so the board replays it: each run ends when it did, with what it scored, and
 *  the rows take their places as the medians change. */
export function Benchmarks() {
  const [at, setAt] = useState(0);
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);

  // A board named in the address opens: `#bench-claude-haiku-5.5`, from a link someone shared.
  useEffect(() => {
    const named = BOARDS.findIndex((board) => `#bench-${board.id}` === window.location.hash);
    if (named > 0) setAt(named);
  }, []);

  const choose = (index: number, focus = false) => {
    const chosen = BOARDS[index];
    if (!chosen) return;
    setAt(index);
    history.replaceState(null, '', `#bench-${chosen.id}`);
    if (focus) tabs.current[index]?.focus();
  };
  const keys = (event: KeyboardEvent) => {
    const last = BOARDS.length - 1;
    const next = { ArrowRight: at + 1, ArrowLeft: at - 1, Home: 0, End: last }[event.key];
    if (next === undefined) return;
    event.preventDefault();
    choose((next + BOARDS.length) % BOARDS.length, true);
  };
  const board = BOARDS[at];
  const section = useRef<HTMLElement>(null);
  const replay = useReplay(board ?? EMPTY, section);
  if (!board) return null;

  return (
    <section ref={section} className="bench wrap" aria-labelledby="bench-title">
      <header className="bench-head">
        <h2 id="bench-title">Benchmarks</h2>
      </header>
      {BOARDS.length > 1 && (
        <div className="bench-tabs" role="tablist" aria-label="Model" onKeyDown={keys}>
          {BOARDS.map((each, index) => (
            <button
              key={each.id}
              ref={(button) => {
                tabs.current[index] = button;
              }}
              id={`bench-tab-${each.id}`}
              type="button"
              role="tab"
              aria-selected={index === at}
              aria-controls="bench-panel"
              tabIndex={index === at ? 0 : -1}
              onClick={() => choose(index)}
            >
              {each.model}
              <span className="bench-count">{each.rows.length}</span>
            </button>
          ))}
        </div>
      )}
      <div
        id="bench-panel"
        role={BOARDS.length > 1 ? 'tabpanel' : undefined}
        aria-labelledby={BOARDS.length > 1 ? `bench-tab-${board.id}` : 'bench-title'}
      >
        <BenchTable board={board} at={replay.at} playing={replay.playing} />
      </div>
    </section>
  );
}

const EMPTY: Board = { id: '', model: '', stamp: '', effort: '', rows: [] };
