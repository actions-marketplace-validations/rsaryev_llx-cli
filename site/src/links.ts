export const SITE = 'https://llx.sh';
export const GITHUB = 'https://github.com/rsaryev/llx';
/** The docs, on this site: each page prerendered from the repository's markdown (src/docs/). */
export const DOCS = '/docs';
/** The CI guide and the shell recipes. */
export const CI_DOCS = `${DOCS}/ci`;
export const RECIPES_DOCS = `${DOCS}/recipes`;
/** How another agent calls llx from its shell. */
export const AGENTS_DOCS = `${DOCS}/agents`;
/** The install: one tab per way in (a tab row shows only with several). */
export const INSTALL = [['llx', 'npm install -g @localixai/llx']] as const;
