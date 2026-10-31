import { Check, Copy } from "./icons";
import "./InstallCommand.css";
import { Tokens } from "./Code";
import { shellLine } from "./highlight";
import { useEffect, useRef, useState } from "react";

/** The install commands as one block with one Copy, in the ghost-button measure of the hero; a
 *  tab row only when there are several ways in. */
export function InstallCommand({
  tabs,
}: {
  tabs: readonly (readonly [string, string])[];
}) {
  const [shown, setShown] = useState(0);
  const commands = [tabs[shown]?.[1] ?? ""];
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(commands.join("\n"));
      setCopied(true);
    } catch {
      return;
    }
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setCopied(false), 1600);
  };

  return (
    <div className="install-box">
      {tabs.length > 1 && (
      <div className="install-tabs" role="tablist">
        {tabs.map(([name], at) => (
          <button
            key={name}
            type="button"
            role="tab"
            aria-selected={at === shown}
            className={at === shown ? "on" : undefined}
            onClick={() => {
              setShown(at);
              setCopied(false);
            }}
          >
            {name}
          </button>
        ))}
      </div>
      )}
      <div className="install">
        <code>
          {commands.map((command) => (
            <span key={command} className="install-line">
              <span className="tk-prompt" aria-hidden="true">$ </span>
              <Tokens line={shellLine(command, false)} />
            </span>
          ))}
        </code>
        <button
          type="button"
          className="install-copy"
          onClick={copy}
          aria-label="Copy the install commands"
        >
          {copied ? <Check /> : <Copy />}
          <span>{copied ? "Copied" : "Copy"}</span>
        </button>
      </div>
    </div>
  );
}
