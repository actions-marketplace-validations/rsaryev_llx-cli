import { type CSSProperties, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { board as standings, type Standing } from '../bench/replay';
import type { Board, Row } from '../bench/types';

/** A board's table at a moment of its replay, in a box that scrolls sideways and says so at the
 *  edge that hides columns. A row moves to its new place when the order changes; the bars keep the
 *  finished board's scale, so they grow as the runs go on. */
export function BenchTable({ board, at, playing }: { board: Board; at: number; playing: boolean }) {
  const box = useRef<HTMLDivElement>(null);
  const edges = useEdges(box, board);
  const rows = standings(board.rows, at);
  const moving = useMoves(rows.map((row) => row.agent).join('|'));

  const most = (pick: (row: Row) => number | null) => Math.max(...board.rows.map((row) => pick(row) ?? 0)) || 1;
  const scale = {
    seconds: most((r) => r.seconds),
    cost: most((r) => r.cost),
    tokens: most((r) => r.tokens),
  };
  return (
    <div className="bench-frame" data-hidden={edges}>
      <div ref={box} className="bench-scroll" role="region" tabIndex={0} aria-label={`Results on ${board.model}`}>
        <table className="bench-table" aria-busy={playing}>
          <thead>
            <tr>
              <th scope="col">Agent</th>
              <th scope="col" className="bench-wide">
                Score <span className="bench-dir">higher is better</span>
              </th>
              <th scope="col">
                Time <span className="bench-dir">lower is better</span>
              </th>
              <th scope="col">
                Cost <span className="bench-dir">lower is better</span>
              </th>
              <th scope="col">
                Tokens in <span className="bench-dir">lower is better</span>
              </th>
              <th scope="col">Version</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr
                key={row.agent}
                ref={moving(row.agent)}
                className={
                  [row.ours && 'bench-ours', row.running && 'bench-running'].filter(Boolean).join(' ') || undefined
                }
              >
                <th scope="row" className="bench-agent">
                  {row.agent}
                </th>
                <td className="bench-wide">
                  <Score row={row} />
                </td>
                <Metric value={row.seconds} most={scale.seconds} text={`${Math.round(row.seconds)} s`} />
                <Metric
                  value={row.cost ?? 0}
                  most={scale.cost}
                  text={row.cost === null || !row.ended ? '—' : dollars(row.cost)}
                />
                <Metric
                  value={row.tokens ?? 0}
                  most={scale.tokens}
                  text={row.tokens === null || !row.ended ? '—' : thousands(row.tokens)}
                />
                <td className="bench-version">{row.version || '—'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/** The median as a bar from 0, the runs' range as a span over the same scale, both as text; a row
 *  with no run ended yet says it is running. */
function Score({ row }: { row: Standing }) {
  if (row.ended === 0)
    return (
      <div className="bench-score">
        <span className="bench-wait">running</span>
        <span className="bench-track bench-pending" aria-hidden="true" />
        <span />
      </div>
    );
  const at = (score: number) => `${(100 * score) / row.total}%`;
  const style = {
    '--median': at(row.score),
    '--low': at(row.low),
    '--high': at(row.high),
  } as CSSProperties;
  const range = row.low === row.high ? '' : `${row.low}–${row.high}`;
  return (
    <div className="bench-score">
      <span className="num bench-value">
        {row.score}
        <span className="bench-of">/{row.total}</span>
      </span>
      <span className="bench-track" style={style} aria-hidden="true">
        <span className="bench-fill" />
        <span className="bench-range" />
      </span>
      <span className="num bench-span">{range}</span>
    </div>
  );
}

function Metric({ value, most, text }: { value: number; most: number; text: string }) {
  return (
    <td>
      <div className="bench-metric">
        <span className="num">{text}</span>
        <span className="bench-track" aria-hidden="true">
          <span className="bench-fill" style={{ width: `${Math.min(100, (100 * value) / most)}%` }} />
        </span>
      </div>
    </td>
  );
}

/** Which edges of the box hide columns, as it scrolls and resizes. */
function useEdges(box: React.RefObject<HTMLDivElement | null>, board: Board) {
  const [edges, setEdges] = useState('');
  useEffect(() => {
    const element = box.current;
    if (!element) return;
    const measure = () => {
      const left = element.scrollLeft > 1;
      const right = element.scrollLeft + element.clientWidth < element.scrollWidth - 1;
      setEdges([left && 'left', right && 'right'].filter(Boolean).join(' '));
    };
    measure();
    const resized = new ResizeObserver(measure);
    resized.observe(element);
    element.addEventListener('scroll', measure, { passive: true });
    return () => {
      resized.disconnect();
      element.removeEventListener('scroll', measure);
    };
  }, [box, board]);
  return edges;
}

/** Rows that change places glide there: each is drawn where it was, then let go to where it is. */
function useMoves(order: string) {
  const rows = useRef(new Map<string, HTMLTableRowElement>());
  const tops = useRef(new Map<string, number>());
  useLayoutEffect(() => {
    const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
    for (const [key, row] of rows.current) {
      const was = tops.current.get(key);
      const now = row.offsetTop;
      tops.current.set(key, now);
      if (still || was === undefined || was === now) continue;
      row.style.transition = 'none';
      row.style.transform = `translateY(${was - now}px)`;
      requestAnimationFrame(() => {
        row.style.transition = 'transform 360ms cubic-bezier(0.2, 0.8, 0.2, 1)';
        row.style.transform = '';
      });
    }
  }, [order]);
  return (key: string) => (row: HTMLTableRowElement | null) => {
    if (row) rows.current.set(key, row);
    else rows.current.delete(key);
  };
}

const dollars = (cost: number) => `$${cost < 0.1 ? cost.toFixed(3) : cost.toFixed(2)}`;
const thousands = (tokens: number) =>
  tokens >= 1e6 ? `${(tokens / 1e6).toFixed(1)}M` : `${Math.round(tokens / 1e3)}k`;
