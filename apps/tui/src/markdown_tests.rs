use super::*;
use crate::style;

fn shown(text: &str, width: usize) -> String {
    render(text, width)
        .iter()
        .map(style::plain)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn blocks_lists_quotes_and_code() {
    let text = "# Done\n\nThe landing is in `site/index.html` and works on **phones** too.\n\n\
- one change in the header, which is long enough to wrap onto a second row\n\
- two\n  1. nested\n  2. again\n\n\
> a quote that also wraps when the width is small\n\n\
```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\n\
See [the docs](https://llx.sh/docs) or https://llx.sh.\n\n---\n\nEnd.";
    insta::assert_snapshot!(shown(text, 40), @r#"
    Done

    The landing is in site/index.html and
    works on phones too.

    • one change in the header, which is
      long enough to wrap onto a second row
    • two
      1. nested
      2. again

    │ a quote that also wraps when the width
    │ is small

    fn main() {
        println!("hi");
    }

    See the docs (https://llx.sh/docs) or
    https://llx.sh.

    ────────────────────────────────────────

    End.
    "#);
}

#[test]
fn tables_keep_columns_rows_and_inline_styles() {
    let text = "| | Theirs | Our edit |\n|---|---|---|\n| Format | **Patch** | `exact replacement` |\n| Operations | creation | replacement |\n\nAfter the table.";
    let rows = render(text, 60);
    let screen = shown(text, 60);
    assert!(screen.contains("│"), "{screen}");
    assert!(!screen.contains("|---|"), "{screen}");
    let header = rows
        .iter()
        .position(|row| style::plain(row).contains("Theirs"))
        .unwrap();
    let format = rows
        .iter()
        .position(|row| style::plain(row).contains("Format"))
        .unwrap();
    let operations = rows
        .iter()
        .position(|row| style::plain(row).contains("Operations"))
        .unwrap();
    assert!(header < format && format < operations, "{screen}");
    assert!(
        rows[format]
            .spans
            .iter()
            .any(|span| span.content == "Patch" && span.style == style::bold())
    );
    assert!(
        rows[format]
            .spans
            .iter()
            .any(|span| span.content.contains("exact") && span.style == style::code())
    );
    assert!(screen.ends_with("\n\nAfter the table."), "{screen}");
}

#[test]
fn tables_wrap_without_losing_cells_in_narrow_terminals_or_quotes() {
    let text = "> | Name | Description |\n> |:---|---:|\n> | 中文 | long words with **style** and [link](https://example.com) |\n> | empty | |";
    assert!(!shown(text, 80).contains("|:---|"));
    for width in [8, 20, 40, 80] {
        let rows = render(text, width);
        let screen = shown(text, width);
        assert!(screen.contains("中文"), "{width}: {screen}");
        assert!(screen.contains("empty"), "{width}: {screen}");
        assert!(
            rows.iter().all(|row| line_width(row) <= width),
            "{width}: {screen}"
        );
        assert!(
            rows.iter()
                .filter(|row| !row.spans.is_empty())
                .all(|row| style::plain(row).starts_with("│ ")),
            "{screen}"
        );
    }
}

#[test]
fn table_alignment_and_empty_cells_do_not_shift_columns() {
    let text = "| Left | Right | Center |\n|:---|---:|:---:|\n| x | y | z |\n| | tail | |";
    let screen = shown(text, 60);
    assert!(screen.contains("│ x    │     y │   z    │"), "{screen}");
    assert!(screen.contains("│      │  tail │        │"), "{screen}");
}

#[test]
fn styles_carry_meaning() {
    let rows = render("**bold** `code` *it*", 40);
    let spans = &rows[0].spans;
    assert_eq!(spans[0].style, style::bold());
    assert_eq!(spans[2].style, style::code());
    assert!(spans[4].style.add_modifier.contains(Modifier::ITALIC));
}

#[test]
fn nothing_is_wider_than_the_width() {
    let text =
        "a ".repeat(200) + "\n\n```\n" + &"x".repeat(100) + "\n```\n- " + &"word ".repeat(50);
    for width in [8, 20, 33] {
        for row in render(&text, width) {
            assert!(
                line_width(&row) <= width,
                "{width}: {:?}",
                style::plain(&row)
            );
        }
    }
}

#[test]
fn escapes_cannot_reach_the_terminal() {
    assert!(!shown("hi \u{1b}[2J there\n\n```\n\u{1b}]0;x\u{7}\n```", 40).contains('\u{1b}'));
}

#[test]
fn a_link_broken_across_rows_keeps_its_address() {
    insta::assert_snapshot!(shown("a **b** c `d` e [x  \ny](http://u.v)", 40), @r"
    a b c d e x
    y (http://u.v)
    ");
}

#[test]
fn an_html_block_reads_as_its_rows() {
    let text = "Before.\n\n<details>\n<summary>x</summary>\n</details>\n\nAfter.";
    insta::assert_snapshot!(shown(text, 40), @r"
    Before.

    <details>
    <summary>x</summary>
    </details>

    After.
    ");
}

#[test]
fn an_item_that_opens_with_code_keeps_its_marker() {
    let text = "1. ```\n   code\n   more\n   ```\n2. two";
    insta::assert_snapshot!(shown(text, 40), @r"
    1. code
       more
    2. two
    ");
}

#[test]
fn a_callout_is_its_name_and_title_behind_a_bar() {
    let text = "> [!WARNING] Downtime\n> The migration locks the table for a minute.\n\nAfter.";
    insta::assert_snapshot!(shown(text, 30), @"
    ▌ Warning · Downtime
    ▌ The migration locks the
    ▌ table for a minute.

    After.
    ");
    let rows = render(text, 30);
    assert_eq!(rows[0].spans[0].style, style::waiting());
}

#[test]
fn steps_are_numbered_down_a_rail() {
    let text = "> [!STEPS] Recovery\n1. **Rest**\n   Sleep eight hours, no screens.\n2. **Walk**\n\n   ```sh\n   walk --minutes 30\n   ```\n3. **Done**\n\nAfter.";
    insta::assert_snapshot!(shown(text, 30), @"
    Recovery
    1  Rest
    │  Sleep eight hours, no
    │  screens.
    │
    2  Walk
    │
    │  walk --minutes 30
    │
    3  Done

    After.
    ");
}

#[test]
fn tabs_are_drawn_one_under_another_each_under_its_label() {
    let text = "> [!TABS] Install\n#### npm\n```sh\nnpm install llx\n```\n#### bun\nRun `bun add llx`.\n\n## Next\n";
    insta::assert_snapshot!(shown(text, 30), @"
    Install
    ─ npm ────────────────────────
    npm install llx

    ─ bun ────────────────────────
    Run bun add llx.

    Next
    ");
}

#[test]
fn figures_no_row_holds_are_a_list() {
    // Sentences where figures were asked for: each still starts at its marker.
    let text = "> [!STATS]\n- **Python: 10 s.** Every value becomes an object.\n- **Rust: 2 s.**\n";
    insta::assert_snapshot!(shown(text, 30), @"
    • Python: 10 s. Every value
      becomes an object.
    • Rust: 2 s.
    ");
}

#[test]
fn the_case_for_and_against_details_and_sources() {
    let text = "> [!PROS]\n- small\n- fast\n\n> [!CONS] Against\n- new\n\n> [!DETAILS] How it was measured\n> The median of nine rounds.\n\n> [!SOURCES]\n- [The spec](https://spec.commonmark.org)\n- [GFM](https://github.github.com/gfm)\n";
    insta::assert_snapshot!(shown(text, 44), @"
    + small
    + fast

    Against
    - new

    ▾ How it was measured
      The median of nine rounds.

    [1] The spec
        https://spec.commonmark.org
    [2] GFM
        https://github.github.com/gfm
    ");
    let rows = render(text, 44);
    assert_eq!(rows[0].spans[0].style, style::done());
}

#[test]
fn a_task_list_an_image_and_a_loose_list() {
    let text = "- [x] done\n- [ ] todo\n\n![a chart](https://a.b/c.png)\n\n1. one\n\n2. two\n";
    insta::assert_snapshot!(shown(text, 40), @"
    ✓ done
    ☐ todo

    a chart (https://a.b/c.png)

    1. one

    2. two
    ");
}

#[test]
fn a_task_hangs_under_its_text_and_a_numbered_one_keeps_its_number() {
    let text =
        "- [ ] check who wrote the commits on the branch\n- [x] back it up\n\n1. [ ] first\n";
    insta::assert_snapshot!(shown(text, 28), @"
    ☐ check who wrote the
      commits on the branch
    ✓ back it up

    1. ☐ first
    ");
    let rows = render(text, 28);
    assert_eq!(rows[2].spans[0].style, style::done());
}

#[test]
fn code_keeps_its_tabs_as_columns() {
    let text =
        "```go\nfunc main() {\n\tif ok {\n\t\treturn\n\t}\n}\n```\n\n```\na\tb\nabcd\te\n```";
    insta::assert_snapshot!(shown(text, 40), @"
    func main() {
        if ok {
            return
        }
    }

    a   b
    abcd    e
    ");
}

#[test]
fn a_word_and_what_runs_into_it_go_to_the_next_row_together() {
    let text = "Use the flag `--force-with-lease`: it refuses.";
    insta::assert_snapshot!(shown(text, 31), @"
    Use the flag
    --force-with-lease: it refuses.
    ");
    // What no row holds whole is still broken where it must be.
    insta::assert_snapshot!(shown("`--force-with-lease`**:**", 10), @"
    --force-wi
    th-lease:
    ");
}

#[test]
fn a_table_of_long_text_rules_its_rows_apart() {
    let text = "| Name | What it does |\n|---|---|\n| merge | joins two histories with a commit of its own |\n| rebase | rewrites commits onto another base |\n";
    insta::assert_snapshot!(shown(text, 36), @"
    │ Name   │ What it does            │
    ├────────┼─────────────────────────┤
    │ merge  │ joins two histories     │
    │        │ with a commit of its    │
    │        │ own                     │
    ├────────┼─────────────────────────┤
    │ rebase │ rewrites commits onto   │
    │        │ another base            │
    ");
    // Short cells that wrap once read well as they are: rules would only make the table taller.
    let short = "| | ripgrep | grep |\n|---|---|---|\n| Recursive by default | Yes | No |\n| Respects .gitignore | Yes | No |\n| Parallel | Yes | No |\n";
    insta::assert_snapshot!(shown(short, 34), @"
    │               │ ripgrep │ grep │
    ├───────────────┼─────────┼──────┤
    │ Recursive by  │ Yes     │ No   │
    │ default       │         │      │
    │ Respects      │ Yes     │ No   │
    │ .gitignore    │         │      │
    │ Parallel      │ Yes     │ No   │
    ");
    // Rows of one line each need no rule between them.
    insta::assert_snapshot!(shown(text, 80), @"
    │ Name   │ What it does                                 │
    ├────────┼──────────────────────────────────────────────┤
    │ merge  │ joins two histories with a commit of its own │
    │ rebase │ rewrites commits onto another base           │
    ");
}

#[test]
fn a_table_gives_its_columns_room_by_what_they_hold() {
    let text = "| Risk | What happens |\n|---|---|\n| A migration that cannot be undone | The old version breaks on the column that is gone while both still run, and rolling the code back does not bring it back. |\n";
    insta::assert_snapshot!(shown(text, 50), @"
    │ Risk           │ What happens                  │
    ├────────────────┼───────────────────────────────┤
    │ A migration    │ The old version breaks on the │
    │ that cannot be │ column that is gone while     │
    │ undone         │ both still run, and rolling   │
    │                │ the code back does not bring  │
    │                │ it back.                      │
    ");
    // A word no column's share holds is broken, and the table still fits.
    let text = "| a | b |\n|---|---|\n| https://example.com/a/very/long/address/that/goes/on | one two three four five six seven |\n";
    assert!(render(text, 30).iter().all(|row| line_width(row) <= 30));
}

#[test]
fn a_code_span_is_not_broken_where_a_row_holds_it() {
    let text = "The line was `ordered[mid - 1] + ordered[mid] / 2`, which halves one value.";
    let screen = shown(text, 40);
    insta::assert_snapshot!(screen, @"
    The line was
    ordered[mid - 1] + ordered[mid] / 2,
    which halves one value.
    ");
    // The spaces are spaces: a command copied from the screen is the one written.
    assert!(!screen.contains('\u{a0}'), "{screen:?}");
    // One no row holds is broken at its spaces, as words are.
    insta::assert_snapshot!(shown("Run `python3 -B -m unittest -v test_stats` now.", 20), @"
    Run python3 -B -m
    unittest -v
    test_stats now.
    ");
}

#[test]
fn the_space_after_a_link_is_not_underlined() {
    let rows = render("See [the spec](https://spec.example/) for it.", 60);
    let spans = &rows[0].spans;
    let at = spans
        .iter()
        .position(|span| span.content == "spec")
        .expect("the link's last word");
    assert!(spans[at].style.add_modifier.contains(Modifier::UNDERLINED));
    // The space is the address's, which is not the link's text.
    assert_eq!(spans[at + 1].content, " ");
    assert!(
        !spans[at + 1]
            .style
            .add_modifier
            .contains(Modifier::UNDERLINED),
        "{spans:?}"
    );
    // Between the link's own words it is underlined with them.
    assert!(
        spans[at - 1]
            .style
            .add_modifier
            .contains(Modifier::UNDERLINED)
    );
}

#[test]
fn code_is_drawn_with_the_spaces_it_was_written_with_wherever_it_stands() {
    let text = "# Run `a b` now\n\nSee `a b`.\n\n- item `a b`\n\n> quote `a b`\n\n\
| `a b` | two `c d` |\n|---|---|\n| `e f` | `g h` |\n\n\
> [!NOTE] Title `a b`\n> Body `a b`.\n\n\
> [!STEPS] Do `a b`\n> 1. **Step `a b`**\n>    Text `a b`.\n\n\
> [!TABS] Pick `a b`\n> #### Tab `a b`\n> Text `a b`.\n\n\
> [!STATS]\n> - **1** `a b`\n\n\
> [!PROS] For `a b`\n> - `a b`\n\n\
> [!DETAILS] More `a b`\n> Text `a b`.\n\n\
> [!SOURCES]\n> - [`a b`](https://x.example/)\n\n\
[`a b`](https://x.example/) ![`a b`](x.png)\n";
    for width in [8, 20, 80] {
        let screen = shown(text, width);
        assert!(!screen.contains('\u{a0}'), "at {width}: {screen:?}");
    }
    assert_eq!(shown(text, 80).matches("a b").count(), 21);
}

fn rows(rows: &[&str]) -> String {
    rows.join("\n")
}

#[test]
fn a_lists_bullet_says_how_deep_it_is_and_a_heading_its_depth_by_its_weight() {
    assert_eq!(
        shown("- a\n  - b\n    - c\n", 40),
        rows(&["• a", "  ◦ b", "    • c"])
    );
    // The depth is the list's own: after a list, and after a nested one, the next starts over.
    assert_eq!(
        shown("- a\n  - b\n\nText.\n\n- c\n  - d\n", 40),
        rows(&["• a", "  ◦ b", "", "Text.", "", "• c", "  ◦ d"])
    );
    let drawn = render("# One\n\n## Two\n\n### Three\n\n#### Four\n", 40);
    let styles: Vec<Style> = drawn
        .iter()
        .filter_map(|row| row.spans.first())
        .map(|span| span.style)
        .collect();
    let italic = Style::new().add_modifier(Modifier::ITALIC);
    assert_eq!(
        styles,
        [
            style::bold().add_modifier(Modifier::UNDERLINED),
            style::bold(),
            style::bold().add_modifier(Modifier::ITALIC),
            italic,
        ]
    );
}

#[test]
fn a_blank_row_stands_only_between_two_blocks_that_are_drawn() {
    assert_eq!(
        shown("> [!NOTE]\n> - a\n>\n> Text.\n>\n> - c\n", 40),
        rows(&["▌ Note", "▌ • a", "▌", "▌ Text.", "▌", "▌ • c"])
    );
    assert_eq!(shown("", 40), "");
    assert_eq!(shown("- \n- b\n", 40), rows(&["• ", "• b"]));
}

#[test]
fn a_step_is_its_name_then_what_is_said_of_it() {
    let text = "> [!STEPS]\n1. **Run the checks.** Everything CI runs.\n2. **Tag it** *a minute* `git tag v1`\n";
    assert_eq!(
        shown(text, 40),
        rows(&[
            "1  Run the checks.",
            "│  Everything CI runs.",
            "│",
            "2  Tag it a minute",
            "   git tag v1",
        ])
    );
    let drawn = render(text, 40);
    assert_eq!(
        drawn[3].spans.last().map(|span| span.style),
        Some(style::dim())
    );
}

#[test]
fn the_numbers_of_a_long_chain_end_in_one_column_and_the_rail_runs_under_it() {
    let steps = |count: usize| {
        let items: Vec<String> = (1..=count)
            .map(|n| format!("{n}. **S{n}**\n   body"))
            .collect();
        format!("> [!STEPS]\n{}\n", items.join("\n"))
    };
    let last = |text: String| {
        let screen = shown(&text, 30);
        let all: Vec<&str> = screen.lines().collect();
        all[all.len() - 5..].join("\n")
    };
    // Nine steps take one column: the tenth is what widens the chain.
    assert_eq!(
        last(steps(9)),
        rows(&["8  S8", "│  body", "│", "9  S9", "   body"])
    );
    assert_eq!(
        last(steps(10)),
        rows(&[" 9  S9", " │  body", " │", "10  S10", "    body"])
    );
}

#[test]
fn figures_stand_side_by_side_each_number_over_what_it_counts() {
    let text = "> [!STATS]\n- **3x** faster\n- **700** allocations\n";
    let grid = rows(&["3x       700", "faster   allocations"]);
    assert_eq!(shown(text, 40), grid);
    // To the column: twenty hold them, nineteen do not.
    assert_eq!(shown(text, 20), grid);
    assert_eq!(shown(text, 19), rows(&["• 3x faster", "• 700 allocations"]));
    let drawn = render(text, 40);
    assert_eq!(drawn[0].spans[0].style, style::bold());
    assert_eq!(drawn[1].spans[0].style, style::dim());
    // A figure with nothing said of it leaves its place under it empty, and no row ends in spaces.
    assert_eq!(
        shown("> [!STATS]\n- **3x** faster\n- **700**\n- **9** ms\n", 40),
        rows(&["3x       700   9", "faster         ms"])
    );
    assert_eq!(shown("> [!STATS]\n- **3x**\n- **700**\n", 40), "3x   700");
}

#[test]
fn a_source_is_its_name_and_its_address_on_a_row_of_its_own() {
    let text = "> [!SOURCES]\n- [The spec](https://spec.commonmark.org)\n- [GFM](https://github.github.com/gfm) — the tables\n- <https://a.example/x>\n- no link here\n";
    assert_eq!(
        shown(text, 44),
        rows(&[
            "[1] The spec",
            "    https://spec.commonmark.org",
            "[2] GFM — the tables",
            "    https://github.github.com/gfm",
            "[3] https://a.example/x",
            "[4] no link here",
        ])
    );
}

#[test]
fn a_quote_names_its_source_a_fence_its_file_a_figure_its_caption() {
    assert_eq!(
        shown("> Simple is better.\n> — Tim Peters, *The Zen*\n", 40),
        rows(&["│ Simple is better.", "│ — Tim Peters, The Zen"])
    );
    assert_eq!(
        shown("> Simple is better.\n>\n> -- Tim Peters\n\nAfter.", 40),
        rows(&["│ Simple is better.", "│ — Tim Peters", "", "After."])
    );
    // A quote that is nothing but the line has no quotation to name the source of.
    assert_eq!(shown("> — alone\n", 40), "│ — alone");
    assert_eq!(
        shown("```ts title=\"a.ts\"\nconst a = 1\n```\n", 40),
        rows(&["a.ts", "const a = 1"])
    );
    assert_eq!(
        shown("```ts src/app.ts\nconst a = 1\n```\n", 40),
        rows(&["src/app.ts", "const a = 1"])
    );
    assert_eq!(shown("```ts {1,3}\nconst a = 1\n```\n", 40), "const a = 1");
    let figure = "![a chart](https://a.b/c.png)\n*Figure 1: latency*\n";
    assert_eq!(
        shown(figure, 40),
        rows(&["a chart (https://a.b/c.png)", "Figure 1: latency"])
    );
    assert_eq!(render(figure, 40)[1].spans[0].style, style::dim());
}

#[test]
fn a_file_tree_is_drawn_with_its_branches_and_what_is_said_of_the_names_in_one_column() {
    assert_eq!(
        shown(
            "```tree\nsrc/\n├── app/\n│   └── App.tsx  # the page\n└── index.ts\n```\n",
            40
        ),
        rows(&["src/", "├─ app/", "│  └─ App.tsx  the page", "└─ index.ts"])
    );
    let tree = "```tree\nsrc/\n├── app/\n│   ├── App.tsx  # the page\n│   └── main.ts\n├── lib/\n│   └── u.ts  # helpers\n└── index.ts  # the entry\nREADME.md\n```\n";
    assert_eq!(
        shown(tree, 40),
        rows(&[
            "src/",
            "├─ app/",
            // A folder's line runs down to the last name in it, and no further.
            "│  ├─ App.tsx  the page",
            "│  └─ main.ts",
            "├─ lib/",
            "│  └─ u.ts     helpers",
            "└─ index.ts    the entry",
            "README.md",
        ])
    );
    let drawn = render(tree, 40);
    assert_eq!(drawn[0].spans[0].style, style::bold());
    assert_eq!(drawn[2].spans[0].style, style::dim());
    assert_eq!(
        drawn[2].spans.last().map(|span| span.style),
        Some(style::dim())
    );
    // Under the last name of a folder nothing runs down: its own names hang in the clear.
    assert_eq!(
        shown(
            "```tree\na/\n└── b/\n    ├── c.ts\n    └── d/\n        └── e.ts\n```\n",
            40
        ),
        rows(&["a/", "└─ b/", "   ├─ c.ts", "   └─ d/", "      └─ e.ts"])
    );
    // Where the column does not fit the row, each is said right after its name.
    assert_eq!(shown(tree, 23).lines().nth(5), Some("│  └─ u.ts  helpers"));
}

#[test]
fn a_diff_says_what_it_adds_and_takes_away_a_diagram_what_it_is() {
    let drawn = render("```diff\n@@ -1 +1 @@\n-old\n+new\n same\n```\n", 40);
    let styles: Vec<Style> = drawn.iter().map(|row| row.spans[0].style).collect();
    assert_eq!(
        styles,
        [style::dim(), style::failed(), style::done(), style::code()]
    );
    // Only a diff is read by its lines: a list in a plain fence is code.
    for fence in ["```\n+ a\n- b\n```\n", "```sh\n+ a\n- b\n```\n"] {
        for row in render(fence, 40) {
            assert_eq!(row.spans[0].style, style::code());
        }
    }
    assert_eq!(
        render("```PATCH\n+a\n```\n", 40)[0].spans[0].style,
        style::done()
    );
    assert_eq!(
        shown("```mermaid\ngraph TD\n  A --> B\n```\n", 40),
        rows(&["Diagram", "graph TD", "  A --> B"])
    );
    // A fence with a name of its own keeps it, and an empty one says nothing.
    assert_eq!(
        shown("```mermaid title=\"flow.mmd\"\ngraph TD\n```\n", 40),
        rows(&["flow.mmd", "graph TD"])
    );
    assert_eq!(shown("```mermaid\n```\n", 40), "");
}

#[test]
fn the_last_tab_ends_at_a_rule_where_the_reply_goes_on() {
    let text = "> [!TABS]\n#### a\nx\n\n---\n\nAfter.";
    assert_eq!(
        shown(text, 20),
        rows(&["─ a ────────────────", "x", "", "After.",])
    );
}

#[test]
fn nesting_deeper_than_the_width_holds_is_drawn_flat_and_still_read() {
    let text = format!("{}deep down\n", "> ".repeat(12));
    for width in [8, 12, 30] {
        let drawn = render(&text, width);
        for row in &drawn {
            assert!(line_width(row) <= width, "{row:?}");
        }
        let read: String = shown(&text, width)
            .chars()
            .filter(|c| *c != '│' && !c.is_whitespace())
            .collect();
        assert_eq!(read, "deepdown");
    }
}

#[test]
fn a_formula_is_written_as_a_terminal_can_inside_a_sentence_and_on_rows_of_its_own() {
    let drawn = render("Throughput follows $r = n / t$, over $k$ samples.", 60);
    assert_eq!(
        shown("Throughput follows $r = n / t$, over $k$ samples.", 60),
        "Throughput follows r = n / t, over k samples."
    );
    let slanted: Vec<&str> = drawn[0]
        .spans
        .iter()
        .filter(|span| span.style.add_modifier.contains(Modifier::ITALIC))
        .map(|span| span.content.trim())
        .filter(|text| !text.is_empty())
        .collect();
    assert_eq!(slanted.join(" "), "r = n / t k");
    let formula = r"\bar{r} = \frac{1}{k} \sum_{i=1}^{k} r_i";
    let set_in = "The mean is\n\n  r\u{304} = 1/k ∑ᵢ₌₁ᵏ rᵢ\n\nDone.";
    for written in [
        format!("The mean is\n\n$$\n{formula}\n$$\n\nDone."),
        format!("The mean is\n\n```math\n{formula}\n```\n\nDone."),
        format!("The mean is\n\n$$ {formula} $$\n\nDone."),
        format!("The mean is\n\n$${formula}$$\n\nDone."),
    ] {
        assert_eq!(shown(&written, 40), set_in, "{written}");
    }
    assert_eq!(
        shown(
            "$$\nf(x) = \\begin{cases} 1 & x > 0 \\\\ 0 & \\text{otherwise} \\end{cases}\n$$",
            40
        ),
        "  f(x) = { 1, x > 0\n  0, otherwise"
    );
    assert_eq!(
        shown(
            "> [!NOTE]\n> Inside: $a_i^2$ and\n>\n> $$\n> E = mc^2\n> $$\n",
            40
        ),
        "▌ Note\n▌ Inside: aᵢ² and\n▌\n▌   E = mc²"
    );
    assert_eq!(
        shown("- cost $O(n^2)$\n- space $O(n)$\n", 40),
        "• cost O(n²)\n• space O(n)"
    );
    assert_eq!(
        shown("| f | g |\n| - | - |\n| $x^2$ | $\\alpha$ |\n", 40),
        "│ f  │ g │\n├────┼───┤\n│ x² │ α │"
    );
}

#[test]
fn money_a_shell_line_and_code_are_not_formulas() {
    for plain in [
        "It costs $5 and $10.",
        "Run `echo $HOME` or `$x^2$`.",
        "for i in $(seq 1 12); do echo $i; done",
        "A $ alone, and $ x $ spaced.",
    ] {
        assert_eq!(shown(plain, 80), plain.replace('`', ""));
    }
    assert_eq!(shown("```sh\necho $a^2$\n```", 40), "echo $a^2$");
    // Beside a real formula too: a price does not open one that the formula's dollar closes.
    assert_eq!(
        shown(
            "It costs $5 or $10 for $n$ rows, in $HOME/bin and $PATH.",
            80
        ),
        "It costs $5 or $10 for n rows, in $HOME/bin and $PATH."
    );
}

#[test]
fn a_footnote_is_its_number_where_it_is_named_and_where_it_is_told() {
    assert_eq!(
        shown("A claim.[^a]\n\n[^a]: Its source.\n", 40),
        rows(&["A claim.[1]", "", "[1] Its source."])
    );
    // By the order they are named in, whatever the order they are told in.
    assert_eq!(
        shown(
            "One[^b] and two[^a], one again[^b].\n\n[^a]: A.\n[^b]: B.\n[^c]: Never named.\n",
            40
        ),
        rows(&["One[1] and two[2], one again[1].", "", "[1] B.", "[2] A."])
    );
    assert_eq!(shown("No such[^x] note.", 40), "No such[^x] note.");
}
