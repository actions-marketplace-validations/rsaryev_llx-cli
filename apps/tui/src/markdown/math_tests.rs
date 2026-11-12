use std::time::Instant;

use super::{rows, text};
use crate::wrap::width;

#[test]
fn the_latex_a_reply_holds() {
    // Written composed, as a terminal's font has the letter: `ŷ` is one character, and so is the
    // `∉` that `\not\in` is (U+2208 with U+0338 composed).
    let written: &[(&str, &str)] = &[
        ("r = n / t", "r = n / t"),
        (
            r"\bar{r} = \frac{1}{k} \sum_{i=1}^{k} r_i",
            "r\u{304} = 1/k ∑ᵢ₌₁ᵏ rᵢ",
        ),
        ("E = mc^2", "E = mc²"),
        (r"e^{i\pi} + 1 = 0", "e^(iπ) + 1 = 0"),
        (
            r"\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}",
            "∫₀^∞ e^(-x²) dx = √π/2",
        ),
        (
            r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
            "x = (-b ± √(b² - 4ac))/2a",
        ),
        (
            r"\lim_{x \to 0} \frac{\sin x}{x} = 1",
            "lim_(x → 0) (sin x)/x = 1",
        ),
        (r"a_{ij} \in \mathbb{R}^{n \times m}", "aᵢⱼ ∈ ℝ^(n × m)"),
        (
            r"\hat{y} = \vec{w} \cdot \vec{x} + b",
            "ŷ = w\u{20d7} · x\u{20d7} + b",
        ),
        (
            r"\sqrt[3]{x+1} \le \left( \alpha + \beta \right)",
            "³√(x+1) ≤ (α + β)",
        ),
        (r"A^T A \ne I, \quad 90^\circ", "Aᵀ A ≠ I, 90°"),
        (
            r"\frac{\partial f}{\partial x} \approx \nabla f",
            "∂f/∂x ≈ ∇ f",
        ),
        (
            r"P(A \mid B) = \frac{P(B \mid A)\,P(A)}{P(B)}",
            "P(A ∣ B) = (P(B ∣ A) P(A))/P(B)",
        ),
        (
            r"\sigma = \sqrt{\frac{1}{N} \sum_{i=1}^{N} (x_i - \mu)^2}",
            "σ = √(1/N ∑ᵢ₌₁ᴺ (xᵢ - μ)²)",
        ),
        (
            r"O(n \log n), \; \binom{n}{k}, \; 10^{-3}",
            "O(n log n), C(n, k), 10⁻³",
        ),
        (
            r"x \not\in S \implies \forall y: y \geq 0",
            "x ∉ S ⇒ ∀ y: y ≥ 0",
        ),
        (
            r"\left\{ x \mid x > 0 \right\} \cup [0, 1)",
            "{ x ∣ x > 0 } ∪ [0, 1)",
        ),
        (
            r"\text{rate} = \mathrm{d}x / \mathrm{d}t \tag{1}",
            "rate = dx / dt (1)",
        ),
        (r"50\% \text{ of } \$10", "50% of $10"),
        (r"T_{eff} + x_{max}", "T_(eff) + xₘₐₓ"),
    ];
    for &(source, expected) in written {
        assert_eq!((source, text(source).as_str()), (source, expected));
    }
}

#[test]
fn a_formula_that_breaks_its_lines_is_one_row_for_each() {
    assert_eq!(
        rows(r"f(x) = \begin{cases} 1 & x > 0 \\ 0 & \text{otherwise} \end{cases}"),
        ["f(x) = { 1, x > 0", "0, otherwise"]
    );
    assert_eq!(
        rows(r"\begin{aligned} a &= b + c \\ &= d \end{aligned}"),
        ["a = b + c", "= d"]
    );
    assert_eq!(
        rows(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}"),
        ["a b", "c d"]
    );
    // A line ending of the source is not one of the formula.
    assert_eq!(rows("a +\nb\n\n= c"), ["a + b = c"]);
}

#[test]
fn a_command_it_does_not_know_stays_as_written() {
    assert_eq!(
        text(r"\foo{x}{y} + \unknown_1 \@"),
        r"\foo{x}{y} + \unknown₁ \@"
    );
}

#[test]
fn nothing_to_write_is_nothing() {
    for source in ["", "   ", r"\!", "{}", r"\left. \right."] {
        assert_eq!((source, rows(source)), (source, Vec::<String>::new()));
    }
}

#[test]
fn what_it_writes_is_as_wide_as_it_looks() {
    assert_eq!(
        width(&text(r"\bar{r} = \frac{1}{k} \sum_{i=1}^{k} r_i")),
        16
    );
    assert_eq!(
        width(&text(r"\hat{y} \vec{w} \alpha \to \infty \in \mathbb{R}")),
        13
    );
}

#[test]
fn seeded_noise_is_read_without_a_panic() {
    let mut parts: Vec<String> = r"\frac \sqrt \sqrt[ \sum \bar \hat \left \right \begin{cases} \end{cases} \begin{ \text \mathbb \not \\ \, \{ \} \ \foo \binom \tag \pmod { } [ ] ( ) ^ _ & ~ $ x y 1 2 + - = , . α ∑"
        .split(' ')
        .map(str::to_owned)
        .collect();
    for extra in [
        " ",
        "\u{a0}",
        "\n",
        "\t",
        "\u{1b}",
        "\u{1b}[31m",
        "\u{202e}",
        "\u{7}",
        "\u{6f22}",
        "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}",
        "e\u{301}",
        "\\",
    ] {
        parts.push(extra.to_owned());
    }
    for seed in 1..=600_u64 {
        let mut state = seed;
        let mut next = |below: usize| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            usize::try_from(state >> 33).unwrap() % below
        };
        let size = 1 + next(40);
        let source: String = (0..size)
            .map(|_| parts[next(parts.len())].as_str())
            .collect();
        for row in rows(&source) {
            assert!(!row.contains('\n'), "seed {seed}: {row:?}");
            assert_eq!(row.trim(), row, "seed {seed}");
        }
        let _ = text(&source);
    }
}

#[test]
fn nesting_without_end_is_read_in_the_time_its_length_takes() {
    let deep = [
        "{".repeat(200_000),
        "^".repeat(200_000),
        "_{".repeat(100_000),
        r"\frac".repeat(60_000),
        r"\sqrt[".repeat(60_000),
        r"\bar{".repeat(60_000),
        r"\not".repeat(60_000),
        r"\begin{cases}".repeat(30_000),
        format!("{}x", "(".repeat(100_000)),
        format!(r"\frac{{{}(}}{{b}}", "a".repeat(100_000)),
    ];
    let started = Instant::now();
    for source in &deep {
        let _ = rows(source);
    }
    // Linear: about a second in a debug build; a quadratic walk would take minutes.
    assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());
}
