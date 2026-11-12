| Text | Size | `markdown` | at four times | grows | `pulldown-cmark` | at four times | grows |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `[` again and again | 4.9 KB | 56 µs | 223 µs | 4.0x | 74 µs | 296 µs | 4.0x |
| `*a ` again and again | 15 KB | 120 µs | 536 µs | 4.5x | 156 µs | 644 µs | 4.1x |
| `**a` again and again | 15 KB | 116 µs | 477 µs | 4.1x | 178 µs | 1.44 ms | 8.1x |
| `_a_ *b* ` again and again | 39 KB | 493 µs | 2.85 ms | 5.8x | 686 µs | 3.58 ms | 5.2x |
| stars, a word, stars | 9.8 KB | 33 µs | 129 µs | 3.9x | 95 µs | 405 µs | 4.3x |
| brackets, a word, brackets | 9.8 KB | 74 µs | 294 µs | 4.0x | 172 µs | 697 µs | 4.0x |
| `[a](` again and again | 20 KB | 406 µs | 1.60 ms | 3.9x | 685 µs | 3.47 ms | 5.1x |
| `![[]()` again and again | 29 KB | 145 µs | 585 µs | 4.0x | 307 µs | 1.91 ms | 6.2x |
| `` `a `` again and again | 9.8 KB | 22 µs | 87 µs | 3.9x | 101 µs | 427 µs | 4.2x |
| backtick runs, each one longer | 4.9 KB | 4.18 µs | 15 µs | 3.5x | 9.32 µs | 22 µs | 2.4x |
| `<` again and again | 4.9 KB | 53 µs | 210 µs | 3.9x | 77 µs | 298 µs | 3.9x |
| `<!--` again and again | 20 KB | 0.80 µs | 3.13 µs | 3.9x | 0.88 µs | 3.26 µs | 3.7x |
| `&` again and again | 4.9 KB | 10 µs | 40 µs | 3.9x | 11 µs | 42 µs | 3.8x |
| backslashes | 4.9 KB | 8.50 µs | 33 µs | 3.9x | 24 µs | 93 µs | 3.8x |
| quotes inside quotes | 1.0 KB | 7.97 µs | 26 µs | 3.2x | 14 µs | 59 µs | 4.3x |
| lists inside lists | 1.0 KB | 0.49 µs | 1.58 µs | 3.2x | 1.07 µs | 4.38 µs | 4.1x |
| a table with cells missing | 1.7 KB | 20 µs | 80 µs | 4.1x | 1.05 ms | 7.29 ms | 7.0x |
| a definition, then its links | 1.2 KB | 16 µs | 66 µs | 4.0x | 29 µs | 83 µs | 2.9x |
| bare links | 54 KB | 3.55 µs | 15 µs | 4.1x | 18 µs | 68 µs | 3.7x |
