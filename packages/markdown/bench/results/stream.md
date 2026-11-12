| Stream | Size | Reads | `markdown` | `pulldown-cmark` | `pulldown-cmark, kept` | `comrak` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| reply, 2 KB | 2.3 KB | 73 | 253 µs | 348 µs | 376 µs | 1.28 ms |
| reply, 8 KB | 8.1 KB | 259 | 1.74 ms | 2.39 ms | 2.72 ms | 9.97 ms |
| reply, 32 KB | 34 KB | 1088 | 31 ms | 49 ms | 55 ms | 204 ms |
| reply, 128 KB | 128 KB | 4098 | 469 ms | 765 ms | 841 ms | 3.21 s |

The slowest single read of each stream:

| Stream | `markdown` | `pulldown-cmark` | `pulldown-cmark, kept` | `comrak` |
| --- | ---: | ---: | ---: | ---: |
| reply, 2 KB | 7.12 µs | 10 µs | 8.17 µs | 35 µs |
| reply, 8 KB | 14 µs | 20 µs | 23 µs | 84 µs |
| reply, 32 KB | 67 µs | 96 µs | 110 µs | 442 µs |
| reply, 128 KB | 346 µs | 525 µs | 512 µs | 2.25 ms |
