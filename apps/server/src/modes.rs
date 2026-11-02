//! The modes a run can work in: the built-in ones, then the person's from `settings.json`, where
//! one of the same name replaces a built-in one. A mode is only a name, a line on what it does,
//! its rules and what its commands reach (ARCHITECTURE.md K5): no code asks which mode it is.

use serde_json::Value;

use crate::error::Failure;
use crate::mode::{Mode, Reach};
use crate::rules::Rules;

/// What a mode's `reach` may be.
const REACHES: &str = "`reach` is \"anything\", \"workspace\" or \"read-only\"";

/// What `-y` chooses.
const BYPASS: &str = "bypass";
/// What a run starts in when nothing is chosen.
const DEFAULT: &str = "default";

/// The built-in modes: a name, what it does, its allow and deny rules, what its commands reach.
type BuiltIn = (&'static str, &'static str, &'static [&'static str], Reach);
const BUILT_IN: [BuiltIn; 3] = [
    (
        DEFAULT,
        "Works in the workspace without asking: commands in a sandbox that writes only there, \
         file changes there; what must reach further is asked, and with no one to ask, refused.",
        &[],
        Reach::Workspace,
    ),
    (
        "plan",
        "Read-only: nothing can be changed, so a goal to change something is reached by a plan \
         of the change, not by making it: what would change and where, step by step, then the \
         assumptions it rests on. Such a plan achieves the goal, unrun and unchecked as it is.",
        &[],
        Reach::ReadOnly,
    ),
    (
        BYPASS,
        "Everything runs without asking (-y).",
        &["bash", "apply_patch", "jobs"],
        Reach::Anything,
    ),
];

#[derive(Debug, Clone)]
pub(crate) struct Modes(Vec<Mode>);

impl Modes {
    /// The built-in modes alone.
    pub(crate) fn built_in() -> Self {
        let owned = |list: &[&str]| list.iter().map(ToString::to_string).collect::<Vec<_>>();
        let modes = BUILT_IN.iter().map(|(name, hint, allow, reach)| {
            let rules = Rules::new(&owned(allow), &[]).unwrap_or_default();
            Mode::new(name, hint, rules, *reach)
        });
        Self(modes.collect())
    }

    /// The built-in modes and the person's, from the settings file's text (`place` names the file
    /// in an error). Settings that are not JSON, or hold no `modes`, add none.
    fn with_settings(text: Option<&str>, place: &str) -> Result<Self, String> {
        let mut modes = Self::built_in();
        let settings: Option<Value> = text.and_then(|text| serde_json::from_str(text).ok());
        let Some(theirs) = settings
            .as_ref()
            .and_then(|s| s.get("modes"))
            .and_then(Value::as_object)
        else {
            return Ok(modes);
        };
        for (name, mode) in theirs {
            let wrong = |why: &str| format!("{place}: the mode `{name}`: {why}");
            let list = |key: &str| -> Result<Vec<String>, String> {
                match mode.get(key) {
                    None => Ok(vec![]),
                    Some(Value::Array(items)) => (items.iter())
                        .map(|item| item.as_str().map(str::to_string))
                        .collect::<Option<Vec<_>>>()
                        .ok_or_else(|| wrong(&format!("`{key}` holds rules, each a string"))),
                    Some(_) => Err(wrong(&format!("`{key}` is a list of rules"))),
                }
            };
            let rules = Rules::new(&list("allow")?, &list("deny")?).map_err(|why| wrong(&why))?;
            // The older spelling is refused, not ignored: ignored, it would reach anything.
            if mode.get("read_only").is_some() {
                return Err(wrong(&format!("`read_only` is now `reach`: {REACHES}")));
            }
            let reach = match mode.get("reach").map(Value::as_str) {
                None | Some(Some("anything")) => Reach::Anything,
                Some(Some("workspace")) => Reach::Workspace,
                Some(Some("read-only")) => Reach::ReadOnly,
                Some(_) => return Err(wrong(REACHES)),
            };
            let hint = mode.get("description").and_then(Value::as_str);
            let hint = hint.map_or_else(|| "The person's own mode.".to_string(), str::to_string);
            let mode = Mode::new(name, &hint, rules, reach);
            match modes.0.iter().position(|known| known.name() == name) {
                Some(at) => modes.0[at] = mode,
                None => modes.0.push(mode),
            }
        }
        Ok(modes)
    }

    /// The modes this person has: [`Modes::with_settings`] from their settings file.
    pub(crate) fn load() -> Result<Self, Failure> {
        let path = crate::choice::settings_path();
        let text = path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok());
        let place = path.map_or_else(|| "settings.json".into(), |path| path.display().to_string());
        Self::with_settings(text.as_deref(), &place).map_err(Failure::Usage)
    }

    /// Each mode with the command line's `rules` (`--allow`, `--deny`) after its own, joined once
    /// for every mode the run can switch to.
    #[must_use]
    pub(crate) fn with(self, rules: &Rules) -> Self {
        let ruled = |mode: Mode| {
            let joined = mode.rules().with(rules);
            Mode::new(mode.name(), mode.hint(), joined, mode.reach())
        };
        Self(self.0.into_iter().map(ruled).collect())
    }

    pub(crate) fn all(&self) -> &[Mode] {
        &self.0
    }

    pub(crate) fn named(&self, name: &str) -> Option<Mode> {
        self.0
            .iter()
            .find(|mode| mode.name() == name.trim())
            .cloned()
    }

    /// The mode a run starts in: `-y`'s, else the one named, else the default.
    pub(crate) fn choose(&self, named: Option<&str>, yes: bool) -> Result<Mode, Failure> {
        let name = if yes {
            BYPASS
        } else {
            named.unwrap_or(DEFAULT)
        };
        self.named(name).ok_or_else(|| {
            Failure::Usage(format!(
                "--mode {name}: no such mode; the modes are {}",
                self.names()
            ))
        })
    }

    /// The mode after `now`, round again after the last: Shift+Tab's.
    pub(crate) fn after(&self, now: &Mode) -> Mode {
        let at = self.0.iter().position(|mode| mode.name() == now.name());
        let next = at.map_or(0, |at| (at + 1) % self.0.len());
        self.0.get(next).cloned().unwrap_or_else(|| now.clone())
    }

    /// `default, plan, bypass, …`.
    pub(crate) fn names(&self) -> String {
        self.0.iter().map(Mode::name).collect::<Vec<_>>().join(", ")
    }
}

/// A built-in mode by name, for tests.
#[cfg(test)]
pub(crate) fn built(name: &str) -> Mode {
    Modes::built_in().named(name).unwrap()
}

#[cfg(test)]
#[path = "modes_tests.rs"]
mod tests;
