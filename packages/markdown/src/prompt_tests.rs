use super::prompt;
use crate::forms::{FORMS, form};
use crate::{Kind, parse};

/// The names of the markers in `text`, in the order written.
fn markers(text: &str) -> Vec<&str> {
    text.split("[!")
        .skip(1)
        .filter_map(|rest| rest.split(']').next())
        .collect()
}

/// The examples: each run of lines set in under a rule, as written at the left edge.
fn examples() -> Vec<String> {
    let mut examples = vec![];
    let mut open = String::new();
    for line in prompt().lines().chain([""; 2]) {
        match line.strip_prefix("  ") {
            Some(shown) => {
                open += shown;
                open += "\n";
            }
            None if line.is_empty() && !open.is_empty() && !open.ends_with("\n\n") => open += "\n",
            None if open.is_empty() => {}
            None => examples.push(std::mem::take(&mut open)),
        }
    }
    examples
}

#[test]
fn every_form_read_is_taught_and_every_marker_taught_is_read() {
    let taught = markers(prompt());
    for (name, _) in FORMS {
        assert!(taught.contains(&name), "[!{name}] is read but not taught");
    }
    for name in taught {
        assert!(form(name).is_some(), "[!{name}] is taught but not read");
    }
}

#[test]
fn each_example_reads_as_the_form_it_shows_and_leaves_no_quote() {
    fn quotes(node: crate::Node<'_>) -> usize {
        usize::from(node.kind() == Kind::Quote) + node.children().map(quotes).sum::<usize>()
    }
    let examples = examples();
    assert!(examples.len() >= 3, "{examples:?}");
    for example in examples {
        let shown = form(markers(&example)[0]).unwrap();
        let doc = parse(&example);
        let first = doc.blocks().next().unwrap();
        assert_eq!(first.kind(), shown, "{example}");
        assert!(first.first().is_some(), "the form took nothing: {example}");
        assert_eq!(quotes(doc.root()), 0, "{example}");
    }
}

#[test]
fn tabs_end_at_the_rule_so_what_follows_is_not_in_the_last_one() {
    let tabs = examples()
        .into_iter()
        .find(|example| example.contains("[!TABS]"))
        .unwrap();
    let text = format!("{tabs}\nAfter the tabs.\n");
    let doc = parse(&text);
    let kinds: Vec<Kind> = doc.blocks().map(|block| block.kind()).collect();
    assert_eq!(kinds, [Kind::Tabs, Kind::Paragraph]);
    assert_eq!(doc.blocks().next().unwrap().children().count(), 2);
}
