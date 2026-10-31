import { type RefObject, useCallback, useEffect, useRef, useState } from 'react';
import { length } from './replay';
import type { Board } from './types';

/** How long a replay takes, whatever the batch's length: long enough to watch the order change. */
const WATCH = 6;
const SPEEDS = [5, 10, 20, 30, 40, 50, 60, 80, 100, 150, 200, 250, 300];

export type Replay = {
  /** Seconds into the batch the board shows. */
  at: number;
  speed: number;
  playing: boolean;
};

/** A board's batch played again, faster, once: when the board first comes into view, or is
 *  first chosen. Before it starts, and without JavaScript or with
 *  reduced motion asked for, the board shows how the batch ended. */
export function useReplay(board: Board, seen: RefObject<HTMLElement | null>): Replay {
  const end = length(board.rows);
  const speed = SPEEDS.find((each) => end / each <= WATCH) ?? SPEEDS[SPEEDS.length - 1] ?? 1;
  const [at, setAt] = useState(end);
  const [playing, setPlaying] = useState(false);
  const frame = useRef(0);
  const played = useRef(new Set<string>());

  const stop = () => cancelAnimationFrame(frame.current);
  const play = useCallback(() => {
    stop();
    const started = performance.now();
    setPlaying(true);
    const tick = (now: number) => {
      // Counted once a frame is drawn: a run that React set up and tore down at once never played.
      played.current.add(board.id);
      const next = ((now - started) / 1000) * speed;
      if (next >= end) {
        setAt(end);
        setPlaying(false);
        return;
      }
      setAt(next);
      frame.current = requestAnimationFrame(tick);
    };
    setAt(0);
    frame.current = requestAnimationFrame(tick);
  }, [board.id, end, speed]);
  const skip = useCallback(() => {
    stop();
    setAt(end);
    setPlaying(false);
  }, [end]);

  // Each board plays once, then stays at its end: the first when it comes into view, another the
  // first time it is chosen.
  const shown = useRef(false);
  useEffect(() => {
    const element = seen.current;
    if (!element || played.current.has(board.id) || matchMedia('(prefers-reduced-motion: reduce)').matches)
      return skip();
    if (shown.current) {
      play();
      return stop;
    }
    setAt(0);
    const watcher = new IntersectionObserver(
      ([entry]) => {
        if (!entry?.isIntersecting) return;
        shown.current = true;
        watcher.disconnect();
        play();
      },
      { threshold: 0.35 },
    );
    watcher.observe(element);
    return () => {
      watcher.disconnect();
      stop();
    };
  }, [board, seen, play, skip]);

  return { at, speed, playing };
}
