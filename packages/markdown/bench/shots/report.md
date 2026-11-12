## The parser, measured

> [!STATS]
> - **1.2x** pulldown-cmark
> - **5x** comrak
> - **89** allocations

| Parser | Gives | MB/s |
| :-- | :-- | --: |
| `markdown` | a tree | 429 |
| `pulldown-cmark` | events | 349 |
| `comrak` | a tree | 87 |

- [x] CommonMark's 652 examples
- [x] GitHub's extensions
- [x] footnotes and formulas
- [ ] definition lists

> [!PROS] For
> - one pass, one arena
> - forms a reply is made of

> [!CONS] Against
> - no definition lists

> [!DETAILS] How it was measured
> The median of 51 rounds, the parsers a round each in turn.

> [!SOURCES]
> - [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/)
> - [GitHub Flavored Markdown](https://github.github.com/gfm/)
