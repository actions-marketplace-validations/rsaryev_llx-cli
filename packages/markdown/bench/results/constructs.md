| Construct | What is repeated | `markdown` | `pulldown-cmark` | `pulldown-cmark, kept` | `comrak` |
| --- | --- | ---: | ---: | ---: | ---: |
| words | plain prose | 0.56 | 1.30 | 1.52 | 8.63 |
| paragraphs | a line, a blank line | 1.18 | 2.34 | 2.59 | 10.03 |
| strong | `**word**` | 2.95 | 5.20 | 5.88 | 16.47 |
| emphasis | `*word*` | 3.49 | 5.26 | 6.10 | 18.99 |
| strike | `~~word~~` | 3.04 | 5.14 | 5.90 | 16.76 |
| code | `` `word` `` | 1.11 | 3.57 | 3.96 | 10.01 |
| links | `[word](address)` | 1.57 | 3.06 | 3.41 | 7.87 |
| references | `&amp;` | 1.06 | 4.84 | 5.28 | 24.99 |
| snake_case | an underscore inside a word | 1.23 | 1.58 | 1.75 | 17.95 |
| bare links | `https://…` (only `markdown` and `comrak` link them) | 0.70 | 0.71 | 0.85 | 12.71 |
| bullets | `- item` | 2.02 | 2.59 | 2.90 | 8.75 |
| ordered | `1. item` | 1.61 | 2.48 | 2.72 | 8.51 |
| tasks | `- [x] item` | 1.90 | 2.35 | 2.67 | 13.97 |
| quote | `> text` | 0.62 | 0.88 | 1.06 | 4.48 |
| headings | `## text` | 2.28 | 3.44 | 3.95 | 15.04 |
| fences | a fenced block of 4 lines | 0.58 | 1.03 | 1.14 | 6.38 |
| table | 400 rows of 3 cells | 2.60 | 3.30 | 3.99 | 22.07 |
| callouts | `> [!NOTE]` and its text | 1.86 | 2.22 | 2.54 | 9.19 |
| steps | `> [!STEPS]` and three steps | 4.83 | 8.24 | 9.21 | 26.75 |

Nanoseconds a byte of text; lower is faster.
