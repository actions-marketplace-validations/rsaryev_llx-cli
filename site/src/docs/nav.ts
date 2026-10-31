/** The docs' one table of contents: the groups in order, each page's slug and its markdown in the
 *  repository. A page's title (its first heading) and description (its first paragraph) come from
 *  the markdown; a page is reached at /docs/<slug>. */
export type NavPage = { slug: string; source: string };
export type NavGroup = { title: string; blurb: string; pages: NavPage[] };

export const NAV: NavGroup[] = [
  {
    title: 'Start',
    blurb: 'The command line, a session and its keys, signing in and choosing a model.',
    pages: [{ slug: 'usage', source: 'docs/usage.md' }],
  },
  {
    title: 'CI and scripts',
    blurb: 'llx exec and llx decide in a pipeline: the GitHub Action, exit codes, shell recipes and ready-made workflows.',
    pages: [
      { slug: 'ci', source: 'docs/ci.md' },
      { slug: 'recipes', source: 'docs/recipes.md' },
      { slug: 'examples', source: 'examples/README.md' },
    ],
  },
  {
    title: 'Reference',
    blurb: 'The five tools and result, modes and approvals, llx in other agents, the exit codes and the JSON events.',
    pages: [
      { slug: 'tools', source: 'docs/tools.md' },
      { slug: 'modes', source: 'docs/modes.md' },
      { slug: 'agents', source: 'docs/agents.md' },
      { slug: 'reference', source: 'docs/reference.md' },
    ],
  },
];

/** Markdown under docs/ that is not a page here, and why. A link to it goes to GitHub; any other
 *  file under docs/ must be in NAV or here, so a new doc is never left off the site unnoticed. */
export const UNLISTED: Record<string, string> = {
  'docs/daemon.md': "the daemon's design, for contributors",
};

/** The index at /docs. */
export const INDEX = {
  title: 'llx docs',
  description:
    'How llx works at your terminal, in scripts and CI, and for other agents: the command line, the GitHub Action and its recipes, the five tools, the modes, how a task ends, and the exit codes and events a pipeline relies on.',
};
