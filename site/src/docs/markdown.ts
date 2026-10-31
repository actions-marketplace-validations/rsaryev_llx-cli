import { Marked, Parser, Renderer, TextRenderer, type Token, type Tokens } from 'marked';
import { escapeHtml, highlight } from './highlight';
import { slugger } from './hrefs';

/** A heading of a page, for its table of contents. */
export type Heading = { depth: number; id: string; text: string };

/** A page's markdown, read: the title (its first heading, taken out of the body, which the page
 *  draws itself), the first paragraph as plain text, every heading with GitHub's id, and the
 *  tokens left to render once their links are rewritten. */
export type Parsed = { title: string; titleId: string; summary: string; headings: Heading[]; tokens: Token[] };

const ids = new WeakMap<Token, string>();
const text = new TextRenderer();
const inline = new Parser();

/** Inline tokens as plain text, whitespace collapsed. */
export function plain(tokens: Token[]): string {
  return inline
    .parseInline(tokens, text)
    .replace(/<[^>]+>/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Each code block framed with its language and a Copy the page's script reveals; each table in
 *  a frame that scrolls sideways on a phone, so the page itself never does. */
const marked = new Marked({
  gfm: true,
  renderer: {
    heading(token: Tokens.Heading) {
      const id = ids.get(token);
      const body = this.parser.parseInline(token.tokens);
      if (id === undefined) return `<h${token.depth}>${body}</h${token.depth}>\n`;
      const hash = `<a class="hash" href="#${id}" aria-hidden="true" tabindex="-1">#</a>`;
      return `<h${token.depth} id="${id}">${body}${hash}</h${token.depth}>\n`;
    },
    code({ text: code, lang }: Tokens.Code) {
      const name = (lang ?? '').trim().split(/\s/)[0] ?? '';
      const bar = `<div class="code-bar"><span>${escapeHtml(name)}</span><button type="button" class="code-copy" hidden>Copy</button></div>`;
      return `<div class="code">${bar}<pre><code>${highlight(code, name)}</code></pre></div>\n`;
    },
    table(token: Tokens.Table) {
      return `<div class="table">${Renderer.prototype.table.call(this, token)}</div>\n`;
    },
    codespan({ text: code }: Tokens.Codespan) {
      const words = code.split(/(\s+)/).map(unbroken).join('');
      return `<code>${words}</code>`;
    },
    // A list of this page's own sections is what the rail on the right shows: marked, so it
    // hides while the rail is there.
    list(token: Tokens.List) {
      const html = Renderer.prototype.list.call(this, token);
      return token.items.every(sectionLink) ? html.replace('<ul>', '<ul class="contents">') : html;
    },
  },
});

/** Inline code breaks at its spaces, never after a flag's `--`: a short flag stays whole, and a
 *  longer one breaks only at an inner hyphen, so a table's column still fits a phone. */
function unbroken(word: string): string {
  if (!word.includes('-')) return escapeHtml(word);
  const whole = word.length <= 16 ? word : (/^-*[^-]+/.exec(word)?.[0] ?? word);
  return `<span class="nobr">${escapeHtml(whole)}</span>${escapeHtml(word.slice(whole.length))}`;
}

function sectionLink(item: Tokens.ListItem): boolean {
  const [only, ...rest] = item.tokens;
  const inner = only && 'tokens' in only ? (only.tokens ?? []) : [];
  const link = inner[0];
  return rest.length === 0 && inner.length === 1 && link?.type === 'link' && String(link.href).startsWith('#');
}

export function parse(markdown: string): Parsed {
  const tokens = marked.lexer(markdown);
  const slug = slugger();
  const headings: Heading[] = [];
  for (const token of tokens) {
    if (token.type !== 'heading') continue;
    const words = plain(token.tokens ?? []);
    const id = slug(words);
    ids.set(token, id);
    headings.push({ depth: token.depth as number, id, text: words });
  }
  const first = tokens[tokens.findIndex((token) => token.type !== 'space')];
  if (first?.type !== 'heading' || first.depth !== 1) throw new Error('a doc starts with its title, a # heading');
  const paragraph = tokens.find((token): token is Tokens.Paragraph => token.type === 'paragraph');
  return {
    title: plain(first.tokens ?? []),
    titleId: ids.get(first) ?? '',
    summary: plain(paragraph?.tokens ?? []),
    headings: headings.slice(1),
    tokens: tokens.filter((token) => token !== first),
  };
}

export function render(tokens: Token[]): string {
  return marked.parser(tokens);
}

/** Calls `visit` with every link and image of the tokens, nested ones too. */
export function eachLink(tokens: Token[], visit: (token: Tokens.Link | Tokens.Image) => void): void {
  marked.walkTokens(tokens, (token) => {
    if (token.type === 'link' || token.type === 'image') visit(token as Tokens.Link | Tokens.Image);
  });
}
