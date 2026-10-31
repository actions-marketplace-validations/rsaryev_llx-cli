import { useEffect, useRef, useState } from 'react';
import '@xterm/xterm/css/xterm.css';
import { type Chapter, parseCast } from './cast';
import { Player, type Status, THEME } from './player';
import './Film.css';

/** Mono's advance, in em: the font size that fits the recording's columns in the frame. */
const CELL = 0.6;
const PADDING = 32;

/**
 * A real llx run, recorded with asciinema and played into a real terminal that takes no input:
 * it scrolls back, selects and copies like one. It starts when it comes into view; its chapters
 * jump to a command. xterm.js loads in the browser only.
 */
export function Film({ src, label }: { src: string; label: string }) {
  const host = useRef<HTMLDivElement>(null);
  const player = useRef<Player | null>(null);
  const [status, setStatus] = useState<Status | 'loading'>('loading');
  const [frame, setFrame] = useState(0);
  const [chapters, setChapters] = useState<Chapter[]>([]);

  useEffect(() => {
    let alive = true;
    let cleanup = () => {};
    const fonts = document.fonts.load(`14px Mono`).catch(() => []);
    void Promise.all([import('@xterm/xterm'), fetch(src).then((r) => r.text()), fonts]).then(([{ Terminal }, text]) => {
      const node = host.current;
      if (!alive || !node) return;
      const cast = parseCast(text);
      const term = new Terminal({
        cols: cast.cols,
        rows: cast.rows,
        disableStdin: true,
        cursorBlink: true,
        scrollback: 2000,
        fontFamily: 'Mono, ui-monospace, monospace',
        lineHeight: 1.15,
        theme: THEME,
      });
      term.open(node);
      const fit = () => {
        const size = (node.clientWidth - PADDING) / (cast.cols * CELL);
        term.options.fontSize = Math.max(6, Math.min(24, Math.floor(size * 2) / 2));
      };
      fit();
      const resized = new ResizeObserver(fit);
      resized.observe(node);
      const made = new Player(term, cast, (now, at) => {
        setStatus(now);
        setFrame(at);
      });
      player.current = made;
      setChapters(cast.chapters);
      const seen = new IntersectionObserver(([entry]) => {
        if (!entry?.isIntersecting) return;
        seen.disconnect();
        if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) made.finish();
        else made.play();
      });
      seen.observe(node);
      cleanup = () => {
        seen.disconnect();
        resized.disconnect();
        made.dispose();
      };
    });
    return () => {
      alive = false;
      player.current = null;
      cleanup();
    };
  }, [src]);

  const current = chapters.reduce((last, chapter, index) => (chapter.frame <= frame ? index : last), -1);
  const toggle = () => {
    if (status === 'playing') player.current?.pause();
    else player.current?.play();
  };

  return (
    <>
      <div ref={host} className="film" role="region" aria-label={label} />
      <div className="demo-caption">
        <span className="film-chapters">
          {chapters.map((chapter, index) => (
            <button
              key={chapter.label}
              type="button"
              aria-current={index === current ? 'step' : undefined}
              onClick={() => player.current?.seek(chapter.frame)}
            >
              {chapter.label}
            </button>
          ))}
        </span>
        <button type="button" className="film-toggle" disabled={status === 'loading'} onClick={toggle}>
          {status === 'playing' ? 'Pause' : status === 'ended' ? 'Replay' : 'Play'}
        </button>
      </div>
    </>
  );
}
