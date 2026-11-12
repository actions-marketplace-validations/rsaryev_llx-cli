# The whole text, read once

Apple M5, Darwin 25.6.0, rustc 1.96.1 (31fca3adb 2026-06-26)

## Over the whole corpus

| Parser | Gives | Throughput | markdown is faster by | Allocations | Allocated |
| --- | --- | ---: | ---: | ---: | ---: |
| `markdown` | a tree | 429 MB/s | 1.0x | 89 | 4.4 MB |
| `markdown, walked` | a tree, every node visited | 408 MB/s | 1.0x | 89 | 4.4 MB |
| `pulldown-cmark` | events, dropped | 349 MB/s | 1.2x | 262 | 5.2 MB |
| `pulldown-cmark, kept` | events in a `Vec` | 303 MB/s | 1.4x | 339 | 13 MB |
| `comrak` | a tree | 87 MB/s | 5.0x | 217,541 | 54 MB |

## Text by text: the time of one read

| Text | Size | `markdown` | `markdown, walked` | `pulldown-cmark` | `pulldown-cmark, kept` | `comrak` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| reply, 1 KB | 2.0 KB | 2.26 µs | 2.52 µs | 3.21 µs | 3.82 µs | 13 µs |
| reply, 8 KB | 8.5 KB | 17 µs | 20 µs | 27 µs | 30 µs | 102 µs |
| reply, 64 KB | 64 KB | 122 µs | 133 µs | 193 µs | 203 µs | 828 µs |
| reply, 512 KB | 512 KB | 1.61 ms | 1.68 ms | 1.82 ms | 2.15 ms | 6.71 ms |
| github, 64 KB | 64 KB | 113 µs | 127 µs | 178 µs | 188 µs | 752 µs |
| crdt.md | 76 KB | 31 µs | 36 µs | 59 µs | 64 µs | 344 µs |
| spec.txt | 200 KB | 214 µs | 214 µs | 314 µs | 341 µs | 1.71 ms |

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
