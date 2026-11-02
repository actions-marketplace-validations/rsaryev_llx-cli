//! The layering rules (ARCHITECTURE.md L1-L8), checked on the manifests and sources of the
//! workspace, with no dependency renamed past them; and two rules of its files: unit tests in
//! their own files (S3, every source), English only (S7, every file of the repository).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{crates, root, shown, walk};

/// Every workspace package: (directory group, package name) → the workspace packages it depends on.
fn packages() -> BTreeMap<(String, String), Vec<String>> {
    let mut manifests = BTreeMap::new();
    for dir in crates() {
        let text = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        let name = text
            .lines()
            .find_map(|line| line.strip_prefix("name = "))
            .unwrap()
            .trim_matches('"')
            .to_string();
        let group = dir.parent().map(shown).unwrap_or_default();
        manifests.insert((group, name), text);
    }
    let names: Vec<String> = manifests.keys().map(|(_, name)| name.clone()).collect();
    manifests
        .into_iter()
        .map(|(key, text)| {
            let mut section = String::new();
            let mut uses = vec![];
            for line in text.lines() {
                if line.starts_with('[') {
                    section = line.to_string();
                } else if section.ends_with("dependencies]") {
                    let dependency = line.split(['.', ' ', '=']).next().unwrap_or("");
                    if names.iter().any(|name| name == dependency) {
                        uses.push(dependency.to_string());
                    }
                }
            }
            (key, uses)
        })
        .collect()
}

/// The dependency lines of `manifest` that rename a package (`kit = { package = "provider-kit" }`,
/// or `package =` under `[dependencies.kit]`): [`packages`] reads a dependency by the name its
/// line starts with, so a renamed one would escape the layer rules.
fn renamed(manifest: &str) -> Vec<String> {
    let mut section = "";
    let mut found = vec![];
    for line in manifest.lines() {
        if line.starts_with('[') {
            section = line;
        } else if section.contains("dependencies")
            && line.split(['{', ',']).any(|part| {
                let part = part.trim_start();
                part.strip_prefix("package")
                    .is_some_and(|rest| rest.trim_start().starts_with('='))
            })
        {
            found.push(line.trim().to_string());
        }
    }
    found
}

/// L1-L8 read each dependency by its package's name: none is renamed.
#[test]
fn no_dependency_is_renamed() {
    for dir in crates() {
        let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        let renamed = renamed(&manifest);
        assert!(
            renamed.is_empty(),
            "{} renames {renamed:?}: name the dependency as its package is named",
            shown(&dir)
        );
    }
}

#[test]
fn a_renamed_dependency_is_told_apart() {
    let manifest = "[package]\nname = \"x\"\n\n[dependencies]\ncontracts.workspace = true\n\
                    kit = { package = \"provider-kit\", path = \"../kit\" }\n\n\
                    [dependencies.wire]\npackage = \"protocol\"\n\n[dev-dependencies]\nserde = \"1\"\n";
    assert_eq!(
        renamed(manifest),
        [
            "kit = { package = \"provider-kit\", path = \"../kit\" }",
            "package = \"protocol\""
        ]
    );
}

/// The packages that speak only in contracts: the agent's loop.
const CORES: [&str; 1] = ["agent"];

#[test]
fn l1_the_contracts_depend_on_nothing_in_the_workspace() {
    let packages = packages();
    let contracts = &packages[&("packages".into(), "contracts".into())];
    assert!(contracts.is_empty(), "contracts depends on {contracts:?}");
}

#[test]
fn l2_the_agent_knows_only_the_contracts() {
    let packages = packages();
    for core in CORES {
        let uses = &packages[&("packages".into(), core.into())];
        assert_eq!(uses, &["contracts"], "{core} depends on {uses:?}");
    }
}

/// What the model providers share: plumbing with no port, which knows no workspace package.
const KIT: &str = "provider-kit";
/// Primitives the app composes (the lanes' queue): no port, no workspace package.
const PRIMITIVES: [&str; 1] = ["queue"];

#[test]
fn l3_tools_providers_and_stores_know_only_the_contracts() {
    let packages = packages();
    for ((group, name), uses) in &packages {
        if group == "packages" && PRIMITIVES.contains(&name.as_str()) {
            assert!(uses.is_empty(), "{name} depends on {uses:?}");
        } else if group == "packages"
            && !matches!(name.as_str(), "contracts" | WIRE)
            && !CORES.contains(&name.as_str())
        {
            assert_eq!(uses, &["contracts"], "{name} depends on {uses:?}");
        }
    }
    let providers: Vec<_> = packages
        .iter()
        .filter(|((group, _), _)| group == "packages/providers")
        .collect();
    assert!(
        providers.len() >= 3,
        "the kit and two providers: {providers:?}"
    );
    for ((_, name), uses) in providers {
        match name.as_str() {
            KIT => assert!(uses.is_empty(), "the kit depends on {uses:?}"),
            _ => assert_eq!(uses, &["contracts", KIT], "{name} depends on {uses:?}"),
        }
    }
}

/// What the interface and the server say to each other.
const WIRE: &str = "protocol";

/// L6: the seam is a leaf: no crate at all, its frames never encoded.
#[test]
fn l6_the_protocol_is_a_leaf() {
    let packages = packages();
    let uses = &packages[&("packages".into(), WIRE.into())];
    assert!(uses.is_empty(), "the protocol depends on {uses:?}");
    let manifest = std::fs::read_to_string(root().join("packages/protocol/Cargo.toml")).unwrap();
    let mut section = "";
    let mut external = vec![];
    for line in manifest.lines() {
        if line.starts_with('[') {
            section = line;
        } else if section == "[dependencies]"
            && let Some((name, _)) = line.split_once('=')
        {
            external.push(name.trim().to_string());
        }
    }
    assert!(external.is_empty(), "the protocol uses {external:?}");
}

/// What the interface must not name (L7): the server (whose library is `llx`), and every
/// workspace package but the protocol. Inside the interface's crate, `crate::` is the interface
/// itself, its client to the server too.
const BEYOND_THE_INTERFACE: &[&str] = &[
    "llx::",
    "contracts::",
    "tools::",
    "agent::",
    "session::",
    "queue::",
    "provider_kit::",
    "models::",
    "openai::",
    "openrouter::",
];

/// The interfaces (L7): the terminal's, a crate of its own under `apps/`.
const INTERFACES: [&str; 1] = ["tui"];

/// L7, on the manifest: each interface depends, among the workspace's crates, on the protocol
/// alone, in every section (its tests' too).
#[test]
fn l7_the_interface_depends_only_on_the_protocol() {
    let packages = packages();
    for interface in INTERFACES {
        let uses = &packages[&("apps".into(), interface.into())];
        assert_eq!(uses, &[WIRE], "{interface} depends on {uses:?}");
    }
}

/// L7, on the sources: each interface (`apps/<interface>/src`, its tests too) speaks only
/// through the protocol, with its own client: what it shows and what it may do, the server says.
#[test]
fn l7_the_interface_knows_only_the_protocol() {
    for interface in INTERFACES {
        let files = sources_in(&root().join("apps").join(interface).join("src"));
        assert!(!files.is_empty(), "no sources of the interface {interface}");
        for path in files
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        {
            let text = std::fs::read_to_string(path).unwrap();
            let code = text.lines().filter(|l| !l.trim_start().starts_with("//"));
            for line in code {
                let named = named_beyond(line);
                assert!(
                    named.is_none(),
                    "{} names `{}` in `{}`: the interface knows only itself and `protocol` \
                     (L7); have the server say it through the protocol (an `Event`, a \
                     `Done`), or move the presentation into `{interface}`",
                    shown(path),
                    named.unwrap_or_default(),
                    line.trim()
                );
            }
        }
    }
}

/// The first path in `line` that leaves the interface: one of [`BEYOND_THE_INTERFACE`] at the
/// start of a path (not a module of the interface's own, `super::queue::`).
fn named_beyond(line: &str) -> Option<&'static str> {
    BEYOND_THE_INTERFACE.iter().copied().find(|word| {
        line.match_indices(word).any(|(at, _)| {
            let before = &line[..at];
            let segment = |s: &str| s.ends_with(|c: char| c.is_alphanumeric() || c == '_');
            !(segment(before) || before.strip_suffix("::").is_some_and(segment))
        })
    })
}

/// The binary (`apps/cli`), the one crate that may depend on an app (L4).
const BINARY: &str = "llx";
/// The apps the binary composes (D7): the server and the interface.
const COMPOSED: &[&str] = &["server", "tui"];

/// L4: only apps compose packages, and nothing depends on an app but the binary on those it
/// composes, in any section: no app dev-depends on another either (a test that needs both a
/// screen and a server is the binary's).
#[test]
fn l4_only_apps_compose_packages() {
    let packages = packages();
    let apps: Vec<&String> = packages
        .keys()
        .filter(|(group, _)| group == "apps")
        .map(|(_, name)| name)
        .collect();
    for ((group, name), uses) in &packages {
        for used in uses.iter().filter(|used| apps.contains(used)) {
            let binary = group == "apps" && name == BINARY;
            assert!(
                binary && COMPOSED.contains(&used.as_str()),
                "{group}/{name} depends on the app {used}: only the binary depends on an app, \
                 one it composes (L4, D7)"
            );
        }
    }
    let binary = &packages[&("apps".into(), BINARY.into())];
    for composed in COMPOSED {
        assert!(
            binary.iter().any(|used| used == composed),
            "the binary composes {composed}: {binary:?}"
        );
    }
}

/// The workspace's crates the binary knows (the layers' table): the server, the interface, and
/// the protocol whose frames and offer it hands over (P7: neither re-exports one).
const THE_BINARY_KNOWS: &[&str] = &["server", "tui", "protocol"];

/// L4, D7: the binary is thin: no tool, provider, store or core of its own; what it runs, the
/// server composes.
#[test]
fn l4_the_binary_knows_only_what_it_composes() {
    let packages = packages();
    let binary = &packages[&("apps".into(), BINARY.into())];
    let beyond: Vec<_> = (binary.iter())
        .filter(|used| !THE_BINARY_KNOWS.contains(&used.as_str()))
        .collect();
    assert!(beyond.is_empty(), "the binary depends on {beyond:?}");
}

/// Words that name a particular tool or provider: the contracts and the agent must not say
/// them.
const TOOL_WORDS: &[&str] = &[
    "browser",
    "webview",
    "web page",
    "page",
    "openrouter",
    "openai",
    "chatgpt",
    "jev",
    "url",
    "html",
    "bash",
    "sh -c",
    // Model vendors and families, and the catalog. Not "claude": the prompt names CLAUDE.md.
    "anthropic",
    "gpt",
    "gemini",
    "deepseek",
    "models.dev",
];

#[test]
fn l5_the_contracts_and_the_agent_name_no_tool_or_provider() {
    for package in ["contracts", "agent"] {
        let dir = root().join("packages").join(package).join("src");
        let files = sources_in(&dir);
        assert!(!files.is_empty(), "no sources in {}", dir.display());
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap().to_lowercase();
            for word in TOOL_WORDS {
                let named = text.match_indices(word).any(|(at, _)| {
                    !text[at + word.len()..].starts_with(|c: char| c.is_alphanumeric())
                        && !text[..at].ends_with(|c: char| c.is_alphanumeric())
                });
                assert!(!named, "{} names \"{word}\"", shown(&path));
            }
        }
    }
}

/// Every file under `dir` (code, the prompt), at any depth.
fn sources_in(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    walk(dir, &|_| true, &mut out);
    out
}

/// Every `.rs` file under the workspace's `src` directories.
fn sources() -> Vec<PathBuf> {
    let mut out = vec![];
    for dir in crates() {
        let rust = sources_in(&dir.join("src"))
            .into_iter()
            .filter(|path| path.extension().is_some_and(|ext| ext == "rs"));
        out.extend(rust);
    }
    out
}

/// The files of the repository: those git tracks (files a run left in the tree are not the
/// repository's until they are added), else, with no git or no checkout (a source archive),
/// every file but the build's and git's own.
#[expect(
    clippy::disallowed_methods,
    reason = "a test's fixture: `git ls-files` ends by itself"
)]
fn repository_files() -> Vec<PathBuf> {
    let listed = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root())
        .output();
    if let Ok(listed) = listed
        && listed.status.success()
        && !listed.stdout.is_empty()
    {
        return String::from_utf8_lossy(&listed.stdout)
            .split('\0')
            .filter(|name| !name.is_empty())
            .map(|name| root().join(name))
            .collect();
    }
    let mut out = vec![];
    for entry in std::fs::read_dir(root()).unwrap().flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if matches!(name.to_str(), Some("target" | ".git")) {
            continue;
        }
        match path.is_dir() {
            true => {
                let built = |path: &Path| path.components().any(|c| c.as_os_str() == "target");
                walk(&path, &|path| !built(path), &mut out);
            }
            false => out.push(path),
        }
    }
    out
}

/// The repository is English only: no Cyrillic in any text file of it, tests included.
#[test]
fn no_cyrillic_anywhere() {
    let files = repository_files();
    assert!(files.len() > 20, "found only {} files", files.len());
    let mut found = vec![];
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue; // gone, or not text (fonts, images)
        };
        if text.chars().any(|c| ('\u{400}'..='\u{52f}').contains(&c)) {
            found.push(shown(&path));
        }
    }
    assert!(found.is_empty(), "Cyrillic in {found:?}");
}

/// Unit tests live beside the code they test, never inside it: `x.rs` ends with
/// `#[cfg(test)] #[path = "x_tests.rs"] mod tests;` and the tests are in `x_tests.rs`.
#[test]
fn tests_live_in_their_own_files() {
    let sources = sources();
    assert!(sources.len() > 20, "found only {} sources", sources.len());
    for path in &sources {
        let text = std::fs::read_to_string(path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let shown = shown(path);
        if let Some(stem) = name.strip_suffix("_tests.rs") {
            let code = path.with_file_name(format!("{stem}.rs"));
            let wired = std::fs::read_to_string(&code)
                .is_ok_and(|code| code.contains(&format!("#[path = \"{name}\"]")));
            assert!(wired, "{shown} is not wired from {stem}.rs");
            continue;
        }
        assert!(
            !text.contains("#[test]") && !text.contains("mod tests {"),
            "{shown} has tests inside: move them to its _tests.rs"
        );
    }
}
