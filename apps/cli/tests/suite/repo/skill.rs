//! The skill's reference is the help this llx prints: `skills/llx/SKILL.md` ends with
//! `llx exec --help` and `llx decide --help` as `scripts/skill-help` writes them, so an agent
//! reading the skill reads what the binary does.

use std::fmt::Write;

use super::root;
use crate::common::home::Home;

/// The line after which `scripts/skill-help` writes the reference.
const MARKER: &str =
    "<!-- Written by scripts/skill-help from the help llx prints; a test checks it matches. -->";

#[test]
fn the_skills_reference_is_the_help_llx_prints() {
    let skill = std::fs::read_to_string(root().join("skills/llx/SKILL.md")).unwrap();
    let (_, written) = skill
        .split_once(&format!("{MARKER}\n"))
        .expect("the skill has the marker line scripts/skill-help writes after");
    let home = Home::new();
    let mut printed = String::new();
    for command in ["exec", "decide"] {
        let out = home.run(&[command, "--help"]);
        assert!(out.status.success(), "llx {command} --help");
        let help = String::from_utf8(out.stdout).unwrap();
        write!(
            printed,
            "\n### `llx {command} --help`\n\n```text\n{help}```\n"
        )
        .unwrap();
    }
    assert!(
        written == printed,
        "skills/llx/SKILL.md's reference is not the help llx prints: run scripts/skill-help"
    );
}
