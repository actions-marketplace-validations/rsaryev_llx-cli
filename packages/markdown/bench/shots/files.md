## Where the parser lives

```tree
packages/markdown/
├── src/
│   ├── blocks.rs    # the first pass, line by line
│   ├── inline.rs    # the second, a paragraph's text
│   └── forms.rs     # a reply's forms
└── README.md        # what it reads and how fast
```

### What changed

```rust src/lib.rs
pub fn parse(text: &str) -> Document<'_> {
    parse_as(text, Dialect::Reply)
}
```

```diff
@@ -12,3 +12,3 @@
-    parse_as(text, Dialect::Github)
+    parse_as(text, Dialect::Reply)
 }
```

Reading a text costs $O(n)$ in its length[^1]. A reply of $k$ pieces of $c$ bytes, read
again as each arrives, costs

$$
c + 2c + \dots + kc = \frac{ck(k+1)}{2}
$$

> Simple is better than complex.
> — Tim Peters, *The Zen of Python*

[^1]: The times are in `bench/results/`.
