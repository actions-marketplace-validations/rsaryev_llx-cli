use super::super::Kind;
use super::super::class::Class;
use super::{Assert, Node, parse, syntax};

fn one(c: char) -> Node {
    Node::Class(Class::one(c))
}

fn class(of: &str) -> Class {
    match parse(of).unwrap() {
        Node::Class(class) => class,
        other @ (Node::Concat(_) | Node::Alt(_) | Node::Repeat(..) | Node::Assert(_)) => {
            panic!("{of} is {other:?}")
        }
    }
}

fn invalid(source: &str) -> bool {
    parse(source).is_err_and(|e| e.kind == Kind::Invalid)
}

fn unchecked(source: &str) -> bool {
    parse(source).is_err_and(|e| e.kind == Kind::Unchecked)
}

#[test]
fn a_sequence_of_literals_and_anchors() {
    assert_eq!(
        parse("^ab$").unwrap(),
        Node::Concat(vec![
            Node::Assert(Assert::Start),
            one('a'),
            one('b'),
            Node::Assert(Assert::End)
        ])
    );
    assert_eq!(parse("").unwrap(), Node::Concat(vec![]));
}

#[test]
fn groups_are_dropped_and_alternation_kept() {
    assert_eq!(
        parse("(?:a|b)(?<n>c)(d)").unwrap(),
        Node::Concat(vec![
            Node::Alt(vec![one('a'), one('b')]),
            one('c'),
            one('d')
        ])
    );
    assert_eq!(
        parse("a|").unwrap(),
        Node::Alt(vec![one('a'), Node::Concat(vec![])])
    );
}

#[test]
fn quantifiers_and_their_lazy_forms() {
    let repeat = |min, max| Node::Repeat(Box::new(one('a')), min, max);
    assert_eq!(parse("a*").unwrap(), repeat(0, None));
    assert_eq!(parse("a+?").unwrap(), repeat(1, None));
    assert_eq!(parse("a??").unwrap(), repeat(0, Some(1)));
    assert_eq!(parse("a{3}").unwrap(), repeat(3, Some(3)));
    assert_eq!(parse("a{3,}?").unwrap(), repeat(3, None));
    assert_eq!(parse("a{2,5}").unwrap(), repeat(2, Some(5)));
    assert_eq!(
        parse("a{99999999999}").unwrap(),
        repeat(u32::MAX, Some(u32::MAX))
    );
}

#[test]
fn a_brace_that_starts_no_count_is_a_literal() {
    assert_eq!(parse("a{").unwrap(), Node::Concat(vec![one('a'), one('{')]));
    assert_eq!(
        parse("{x}").unwrap(),
        Node::Concat(vec![one('{'), one('x'), one('}')])
    );
    assert_eq!(
        parse("a{,3}").unwrap(),
        Node::Concat(vec![one('a'), one('{'), one(','), one('3'), one('}')])
    );
    assert_eq!(parse("]").unwrap(), one(']'));
}

#[test]
fn character_escapes() {
    let cases = [
        ("\\t", '\t'),
        ("\\n", '\n'),
        ("\\r", '\r'),
        ("\\f", '\u{C}'),
        ("\\v", '\u{B}'),
        ("\\0", '\0'),
        ("\\cC", '\u{3}'),
        ("\\cc", '\u{3}'),
        ("\\x41", 'A'),
        ("\\u00e9", 'é'),
        ("\\u{1F600}", '😀'),
        ("\\uD83D\\uDE00", '😀'),
        ("\\/", '/'),
        ("\\-", '-'),
        ("\\.", '.'),
        ("\\é", 'é'),
    ];
    for (source, c) in cases {
        assert_eq!(parse(source).unwrap(), one(c), "{source}");
    }
}

#[test]
fn class_escapes_and_assertions() {
    assert_eq!(class("\\d"), Class::digit());
    assert_eq!(class("\\W"), Class::word().negated());
    assert_eq!(class("\\S"), Class::space().negated());
    assert_eq!(class("."), Class::dot());
    assert_eq!(parse("\\b").unwrap(), Node::Assert(Assert::Word));
    assert_eq!(parse("\\B").unwrap(), Node::Assert(Assert::NotWord));
}

#[test]
fn the_properties_known_exactly() {
    assert_eq!(class("\\p{Zs}"), Class::of(Class::ranges("Zs").unwrap()));
    assert_eq!(
        class("\\P{ASCII}"),
        Class::of(Class::ranges("ASCII").unwrap()).negated()
    );
    let mut both = Class::of(Class::ranges("Zs").unwrap());
    both.union(&Class::one('a'));
    assert_eq!(class("[\\p{Zs}a]"), both);
}

#[test]
fn bracketed_classes() {
    let mut expected = Class::range('a', 'z').unwrap();
    expected.union(&Class::digit());
    expected.union(&Class::one('_'));
    expected.union(&Class::one('-'));
    assert_eq!(class("[a-z\\d_-]"), expected);
    assert_eq!(class("[-a]"), {
        let mut c = Class::one('-');
        c.union(&Class::one('a'));
        c
    });
    assert_eq!(class("[^0-9]"), Class::digit().negated());
    assert_eq!(class("[]"), Class::empty());
    assert_eq!(class("[^]"), Class::empty().negated());
    assert_eq!(class("[\\b]"), Class::one('\u{8}'));
    assert_eq!(class("[.]"), Class::one('.'));
    assert_eq!(class("[\\]]"), Class::one(']'));
    // Annex B: a class escape at an end of a range makes the dash a literal.
    let mut dashed = Class::digit();
    dashed.union(&Class::one('-'));
    dashed.union(&Class::one('z'));
    assert_eq!(class("[\\d-z]"), dashed);
}

#[test]
fn what_is_not_a_regular_expression_is_invalid() {
    for source in [
        "(",
        ")",
        "a)",
        "[a",
        "[z-a]",
        "*a",
        "a**",
        "a{2}{3}",
        "+",
        "a{3,2}",
        "\\",
        "\\x4",
        "\\u12",
        "\\u{110000}",
        "\\c1",
        "\\a",
        "\\q",
        "(?i)a",
        "(?<1a>x)",
        "^*",
        "$+",
        "\\b*",
        "\\01",
        "[\\B]",
        "\\p",
        "\\p{Zs",
    ] {
        assert!(
            invalid(source),
            "{source} should be invalid: {:?}",
            parse(source)
        );
    }
}

#[test]
fn what_is_valid_but_not_checked_is_said() {
    for source in [
        "(a)\\1",
        "(?<n>a)\\k<n>",
        "a(?=b)",
        "a(?!b)",
        "(?<=a)b",
        "(?<!a)b",
        "\\p{L}",
        "\\P{Lu}",
        "\\p{Letter}",
        "\\uD800",
    ] {
        assert!(
            unchecked(source),
            "{source} should be unchecked: {:?}",
            parse(source)
        );
    }
    assert!(unchecked(&format!(
        "{}a{}",
        "(".repeat(100),
        ")".repeat(100)
    )));
}

#[test]
fn the_syntax_alone_is_judged_as_the_whole_parse_judges_it() {
    let sources = [
        r#"^\s*(it|describe)\(\s*['"]([^'"]+)['"]"#,
        r"^[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}$",
        r"\d{3}-\D\w\W\s\S.[^\]-a]\b\B(?:x|y)*?(?<n>z){2,5}\u{1F600}\x41\cJ",
        "[",
        "a**",
        "{2}",
        "[z-a]",
        "(?=x)",
        "(?<!x)",
        r"\1",
        r"\p{L}",
        r"\q",
        "(a",
        "a)",
        "x{3,2}",
        "",
        &"a".repeat(400),
        &format!("({}", "(".repeat(70)),
    ];
    for source in sources {
        let whole = parse(source).map(|_| ()).map_err(|why| why.kind);
        assert_eq!(syntax(source).map_err(|why| why.kind), whole, "{source:?}");
    }
}
