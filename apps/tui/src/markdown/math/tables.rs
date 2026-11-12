//! What a formula's commands and scripts are written as: the symbols, the names written upright,
//! the marks, the small letters of scripts, and the letters a mark composes with.

use std::collections::HashMap;
use std::sync::LazyLock;

/// Pairs of a command's name and what it is written as.
const NAMED: &str = "alpha α beta β gamma γ delta δ epsilon ϵ varepsilon ε zeta ζ eta η theta θ
vartheta ϑ iota ι kappa κ lambda λ mu μ nu ν xi ξ pi π varpi ϖ rho ρ varrho ϱ sigma σ varsigma ς
tau τ upsilon υ phi ϕ varphi φ chi χ psi ψ omega ω Gamma Γ Delta Δ Theta Θ Lambda Λ Xi Ξ Pi Π
Sigma Σ Upsilon Υ Phi Φ Psi Ψ Omega Ω sum ∑ prod ∏ coprod ∐ int ∫ iint ∬ iiint ∭ oint ∮ infty ∞
partial ∂ nabla ∇ times × cdot · div ÷ pm ± mp ∓ le ≤ leq ≤ ge ≥ geq ≥ ne ≠ neq ≠ ll ≪ gg ≫
approx ≈ equiv ≡ sim ∼ simeq ≃ cong ≅ propto ∝ to → rightarrow → gets ← leftarrow ←
leftrightarrow ↔ Rightarrow ⇒ Leftarrow ⇐ Leftrightarrow ⇔ implies ⇒ iff ⇔ mapsto ↦ uparrow ↑
downarrow ↓ in ∈ notin ∉ ni ∋ subset ⊂ subseteq ⊆ supset ⊃ supseteq ⊇ cup ∪ cap ∩ setminus ∖
emptyset ∅ varnothing ∅ forall ∀ exists ∃ nexists ∄ neg ¬ lnot ¬ land ∧ wedge ∧ lor ∨ vee ∨
oplus ⊕ otimes ⊗ odot ⊙ cdots ⋯ ldots … dots … vdots ⋮ ddots ⋱ angle ∠ perp ⊥ parallel ∥ mid ∣
circ ∘ bullet • star ⋆ ast ∗ dagger † langle ⟨ rangle ⟩ lfloor ⌊ rfloor ⌋ lceil ⌈ rceil ⌉
lvert | rvert | vert | lVert ‖ rVert ‖ Vert ‖ ell ℓ hbar ℏ Re ℜ Im ℑ aleph ℵ prime ′ degree °
top ⊤ bot ⊥ vdash ⊢ models ⊨ therefore ∴ because ∵ triangle △ square □ checkmark ✓ lbrace {
rbrace } colon :";

/// What a command's name is, where this knows it.
#[derive(Clone, Copy)]
pub(super) enum Named {
    /// A symbol, written as it.
    Symbol(&'static str),
    /// A name written upright, as itself.
    Function,
    /// A mark over or under its argument's letters.
    Accent(char),
    /// How its argument looks, not what it says: the argument alone.
    Transparent,
    /// Size or placement: nothing.
    Dropped,
}

/// Every command known by name, built once: a formula looks one up for each of its commands,
/// and a reply may hold tens of thousands.
static NAMES: LazyLock<HashMap<&'static str, Named>> = LazyLock::new(|| {
    let mut table = HashMap::new();
    // From the last to win to the first, as a name in two lists is what the first says.
    for name in DROPPED.split_whitespace() {
        table.insert(name, Named::Dropped);
    }
    for name in TRANSPARENT.split_whitespace() {
        table.insert(name, Named::Transparent);
    }
    for &(name, mark) in &ACCENTS {
        table.insert(name, Named::Accent(mark));
    }
    for name in FUNCTIONS.split_whitespace() {
        table.insert(name, Named::Function);
    }
    let mut words = NAMED.split_whitespace();
    while let (Some(name), Some(symbol)) = (words.next(), words.next()) {
        table.insert(name, Named::Symbol(symbol));
    }
    table
});

/// Names that are written upright, as themselves.
const FUNCTIONS: &str = "sin cos tan cot sec csc arcsin arccos arctan sinh cosh tanh log ln lg exp
lim limsup liminf max min sup inf det dim ker gcd deg arg Pr hom mod bmod";

/// Commands that change how their argument looks and not what it says.
const TRANSPARENT: &str = "text textrm textit textbf textsf texttt mathrm mathit mathbf mathsf
mathtt mathcal mathfrak mathscr boldsymbol bm mbox operatorname boxed underbrace overbrace
substack";

/// Commands about size and placement, which a terminal has one of.
const DROPPED: &str = "left right big Big bigg Bigg bigl bigr Bigl Bigr biggl biggr middle
displaystyle textstyle scriptstyle limits nolimits nonumber notag";

/// A mark over or under a letter, as the combining character that draws it.
const ACCENTS: [(&str, char); 11] = [
    ("bar", '\u{304}'),
    ("overline", '\u{305}'),
    ("hat", '\u{302}'),
    ("widehat", '\u{302}'),
    ("tilde", '\u{303}'),
    ("widetilde", '\u{303}'),
    ("vec", '\u{20d7}'),
    ("overrightarrow", '\u{20d7}'),
    ("dot", '\u{307}'),
    ("ddot", '\u{308}'),
    ("underline", '\u{332}'),
];

/// Pairs of a character and its small form.
const SUPER: &str = "0⁰1¹2²3³4⁴5⁵6⁶7⁷8⁸9⁹+⁺-⁻−⁻=⁼(⁽)⁾aᵃbᵇcᶜdᵈeᵉfᶠgᵍhʰiⁱjʲkᵏlˡmᵐnⁿoᵒpᵖrʳsˢtᵗuᵘvᵛ\
wʷxˣyʸzᶻAᴬBᴮDᴰEᴱGᴳHᴴIᴵJᴶKᴷLᴸMᴹNᴺOᴼPᴾRᴿTᵀUᵁVⱽWᵂ∘°′′";
const SUB: &str = "0₀1₁2₂3₃4₄5₅6₆7₇8₈9₉+₊-₋−₋=₌(₍)₎aₐeₑhₕiᵢjⱼkₖlₗmₘnₙoₒpₚrᵣsₛtₜuᵤvᵥxₓβᵦγᵧρᵨφᵩχᵪ";

/// The sets of numbers, the letters `\mathbb` is asked for that every font has.
const DOUBLE: &str = "CℂHℍNℕPℙQℚRℝZℤ";

/// Each mark with the pairs of a character and what it is with the mark, as one character: the
/// compositions of Unicode's NFC for the marks a formula writes. A letter missing here keeps
/// its mark combining, as NFC keeps one it has no character for.
const COMPOSED: [(char, &str); 6] = [
    ('\u{304}', "aāeēiīoōuūyȳgḡAĀEĒIĪOŌUŪYȲGḠæǣÆǢ"),
    (
        '\u{302}',
        "aâcĉeêgĝhĥiîjĵoôsŝuûwŵyŷzẑAÂCĈEÊGĜHĤIÎJĴOÔSŜUÛWŴYŶZẐ",
    ),
    ('\u{303}', "aãeẽiĩnñoõuũvṽyỹAÃEẼIĨNÑOÕUŨVṼYỸ"),
    (
        '\u{307}',
        "aȧbḃcċdḋeėfḟgġhḣmṁnṅoȯpṗrṙsṡtṫwẇxẋyẏzżAȦBḂCĊDḊEĖFḞGĠHḢIİMṀNṄOȮPṖRṘSṠTṪWẆXẊYẎZŻ",
    ),
    ('\u{308}', "aäeëhḧiïoötẗuüwẅxẍyÿAÄEËHḦIÏOÖUÜWẄXẌYŸ"),
    (
        '\u{338}',
        "=≠<≮>≯∈∉∋∌⊂⊄⊃⊅⊆⊈⊇⊉≤≰≥≱≡≢∼≁≃≄≅≇≈≉∃∄∣∤∥∦⊢⊬⊨⊭←↚→↛↔↮⇐⇍⇒⇏⇔⇎",
    ),
];

pub(super) fn named(name: &str) -> Option<Named> {
    NAMES.get(name).copied()
}

/// `ch` in the pairs of `pairs`: its other half.
fn paired(pairs: &str, ch: char) -> Option<char> {
    let mut chars = pairs.chars();
    while let (Some(from), Some(to)) = (chars.next(), chars.next()) {
        if from == ch {
            return Some(to);
        }
    }
    None
}

pub(super) fn superscript(ch: char) -> Option<char> {
    paired(SUPER, ch)
}

pub(super) fn subscript(ch: char) -> Option<char> {
    paired(SUB, ch)
}

pub(super) fn double(ch: char) -> char {
    paired(DOUBLE, ch).unwrap_or(ch)
}

/// `base` with `mark` as one character, where Unicode has one.
pub(super) fn composed(base: char, mark: char) -> Option<char> {
    COMPOSED
        .iter()
        .find(|&&(with, _)| with == mark)
        .and_then(|&(_, pairs)| paired(pairs, base))
}

/// A mark drawn over or under the character before it: the combining ranges a formula's marks
/// are in.
pub(super) fn is_mark(ch: char) -> bool {
    matches!(ch, '\u{300}'..='\u{36f}' | '\u{20d0}'..='\u{20ff}')
}
