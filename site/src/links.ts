export const SITE = 'https://llx.sh';
/** Who makes llx: the lab's page of everything it makes, signed first in the footer. */
export const MAKER = { label: 'Localix Lab', href: 'https://localix.ai' } as const;
export const GITHUB = 'https://github.com/localixai/localix-cli';
/** The docs, on this site: each page prerendered from the repository's markdown (src/docs/). */
export const DOCS = '/docs';
/** The CI guide and its ready-made workflows. */
export const CI_DOCS = `${DOCS}/ci`;
export const EXAMPLES_DOCS = `${DOCS}/examples`;
/** How another agent calls llx from its shell. */
export const AGENTS_DOCS = `${DOCS}/agents`;
/** The install: one tab per way in (a tab row shows only with several). */
export const INSTALL = [['llx', 'curl -fsSL https://llx.sh/install | sh']] as const;
