# The whole text, read once

Apple M5, Darwin 25.6.0, rustc 1.96.1 (31fca3adb 2026-06-26)

## Over the whole corpus

| Parser | Gives | Throughput | markdown is faster by | Allocations | Allocated |
| --- | --- | ---: | ---: | ---: | ---: |
| `markdown` | a tree | 205 MB/s | 1.0x | 89 | 4.4 MB |
| `markdown, walked` | a tree, every node visited | 179 MB/s | 1.1x | 89 | 4.4 MB |
| `pulldown-cmark` | events, dropped | 182 MB/s | 1.1x | 262 | 5.2 MB |
| `pulldown-cmark, kept` | events in a `Vec` | 169 MB/s | 1.2x | 339 | 13 MB |
| `comrak` | a tree | 54 MB/s | 3.8x | 217,541 | 54 MB |

## Text by text: the time of one read

| Text | Size | `markdown` | `markdown, walked` | `pulldown-cmark` | `pulldown-cmark, kept` | `comrak` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| reply, 1 KB | 2.0 KB | 7.48 µs | 8.46 µs | 8.43 µs | 8.71 µs | 26 µs |
| reply, 8 KB | 8.5 KB | 54 µs | 65 µs | 66 µs | 70 µs | 191 µs |
| reply, 64 KB | 64 KB | 358 µs | 421 µs | 434 µs | 428 µs | 1.45 ms |
| reply, 512 KB | 512 KB | 3.00 ms | 3.41 ms | 3.32 ms | 3.65 ms | 11 ms |
| github, 64 KB | 64 KB | 314 µs | 381 µs | 385 µs | 397 µs | 1.23 ms |
| crdt.md | 76 KB | 115 µs | 131 µs | 125 µs | 129 µs | 499 µs |
| spec.txt | 200 KB | 560 µs | 630 µs | 642 µs | 658 µs | 2.50 ms |

## Text by text: allocations of one read (exact)

| Text | `markdown` | `markdown, walked` | `pulldown-cmark` | `pulldown-cmark, kept` | `comrak` |
| --- | ---: | ---: | ---: | ---: | ---: |
| reply, 1 KB | 9 / 16 KB | 9 / 16 KB | 9 / 23 KB | 15 / 33 KB | 311 / 48 KB |
| reply, 8 KB | 11 / 60 KB | 11 / 60 KB | 17 / 116 KB | 26 / 196 KB | 2,382 / 641 KB |
| reply, 64 KB | 18 / 382 KB | 18 / 382 KB | 31 / 429 KB | 43 / 1.0 MB | 16,793 / 4.8 MB |
| reply, 512 KB | 25 / 3.0 MB | 25 / 3.0 MB | 87 / 3.3 MB | 102 / 8.3 MB | 132,283 / 38 MB |
| github, 64 KB | 13 / 374 KB | 13 / 374 KB | 33 / 430 KB | 45 / 1.0 MB | 15,977 / 3.1 MB |
| crdt.md | 5 / 114 KB | 5 / 114 KB | 15 / 249 KB | 25 / 409 KB | 7,283 / 1.6 MB |
| spec.txt | 8 / 488 KB | 8 / 488 KB | 70 / 659 KB | 83 / 1.9 MB | 42,512 / 5.9 MB |

The texts:

- **reply, 1 KB**: a short answer.
- **reply, 8 KB**: an answer with its forms.
- **reply, 64 KB**: a long report.
- **reply, 512 KB**: far past what a model writes.
- **github, 64 KB**: the same, GitHub's Markdown only.
- **crdt.md**: pulldown-cmark's own benchmark text.
- **spec.txt**: the CommonMark specification.
