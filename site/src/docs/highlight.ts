import { highlight as tokens, langOf } from '../components/highlight';

export function escapeHtml(text: string): string {
  return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

/** A code block as HTML, coloured as the landing's panels (syntax.css): a shell's commands, flags
 *  and strings, JSON's and YAML's keys and values. Any other language as it is. */
export function highlight(code: string, name: string): string {
  const lang = langOf(name);
  if (!lang) return escapeHtml(code.replace(/\n$/, ''));
  return tokens(code, lang)
    .map((line) =>
      line
        .map(([kind, text]) =>
          kind === 'plain' ? escapeHtml(text) : `<span class="tk-${kind}">${escapeHtml(text)}</span>`,
        )
        .join(''),
    )
    .join('\n');
}
