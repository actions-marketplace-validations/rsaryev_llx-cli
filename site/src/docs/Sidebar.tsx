import { DOCS } from '../links';
import { GROUPS } from './content';

/** Every page, by group, the one shown marked: the rail on a wide screen, the menu on a phone. */
export function Sidebar({ active }: { active?: string | undefined }) {
  return (
    <nav className="docs-nav" aria-label="Docs">
      <a href={DOCS} aria-current={active === undefined ? 'page' : undefined}>
        Overview
      </a>
      {GROUPS.map((group) => (
        <div key={group.title}>
          <p className="docs-label">{group.title}</p>
          <ul>
            {group.pages.map((page) => (
              <li key={page.slug}>
                <a href={page.route} aria-current={page.slug === active ? 'page' : undefined}>
                  {page.title}
                </a>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </nav>
  );
}
