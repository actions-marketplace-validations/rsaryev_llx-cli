//! `ask`: one to three questions to the person, answered by whoever composes the tools
//! (`contracts::person::Person`). It changes nothing outside the conversation, so it asks no gate.
//! Its arguments are read leniently: a model that writes one question without the `questions`
//! list, leaves out an id or gives options as plain strings still gets its question asked, instead
//! of losing a turn to a schema error.

use contracts::person::{Answer, Choice, Person, Question, Reply};
use contracts::tool::{Input, Tool, ToolError, ToolOutput, ToolSpec};
use serde_json::{Map, Value, json};

const NAME: &str = "ask";
const MAX_QUESTIONS: usize = 3;
const MAX_OPTIONS: usize = 4;
/// Characters of a header: a chip's.
const HEADER: usize = 12;

const DESCRIPTION: &str = "Ask the person you work for one to three short questions, and wait \
for the answers. Use it only when the answer changes what you do and nothing you can read or run \
tells you: a preference, a tradeoff between real options, a requirement that is unclear, a value \
only they know. Look first, then ask; do not ask what you can find out, and do not ask the same \
thing twice. Offer 2 to 4 options when the choices are clear, the recommended one first with \
\" (Recommended)\" after its label; set `multiple` when several can be picked together (which \
features, which files). When options are best compared by how they look (a layout, an API's \
shape, a configuration), give each a `preview`. Never use it to ask for permission to run a \
command or change a file: those are approved as they happen.";

const SKIPPED: &str = "The person skipped the question. Go on with your best assumption and \
state it in your answer; do not ask the same thing again unless it truly blocks the work.";
const STOPPED: &str = "the person stopped the task";

pub struct AskTool<'a> {
    pub person: &'a dyn Person,
}

impl Tool for AskTool<'_> {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: NAME.into(),
            description: DESCRIPTION.into(),
            input: Input::Json(parameters()),
        }
    }

    fn call(&self, arguments: &Value) -> Result<ToolOutput, ToolError> {
        let questions = questions(arguments).map_err(ToolError::Failed)?;
        match self.person.ask(&questions) {
            Reply::Answers(answers) => Ok(ToolOutput {
                shown: shown(&questions, &answers),
                ..report(&questions, &answers).into()
            }),
            Reply::Nobody(why) => Err(ToolError::Failed(format!(
                "{why}: go on with your best assumption and state it in your answer"
            ))),
            Reply::Stopped => Err(ToolError::Failed(STOPPED.into())),
        }
    }
}

fn parameters() -> Value {
    let option = json!({
        "type": "object",
        "required": ["label"],
        "additionalProperties": false,
        "properties": {
            "label": {"type": "string", "description": "1 to 5 words."},
            "description": {"type": "string",
                "description": "One short sentence: what choosing it means."},
            "preview": {"type": "string",
                "description": "Optional: a mockup or snippet of a few lines, under 60 columns, \
                    shown beside the options as written."},
        },
    });
    let question = json!({
        "type": "object",
        "required": ["id", "question"],
        "additionalProperties": false,
        "properties": {
            "id": {"type": "string",
                "description": "A short snake_case key the answer comes back under."},
            "header": {"type": "string",
                "description": "A word or two, at most 12 characters, shown as the question's \
                    tab (`Scope`, `Approach`)."},
            "question": {"type": "string",
                "description": "One sentence the person can answer without reading the code."},
            "options": {
                "type": "array",
                "maxItems": MAX_OPTIONS,
                "description": "2 to 4 choices, the recommended one first: mutually exclusive \
                    unless `multiple`. Leave out for an open question; the person can always type \
                    their own answer.",
                "items": option,
            },
            "multiple": {"type": "boolean",
                "description": "true when the person may pick several options together."},
        },
    });
    json!({
        "type": "object",
        "required": ["questions"],
        "additionalProperties": false,
        "properties": {
            "questions": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_QUESTIONS,
                "description": "One question is best; at most three, asked in order.",
                "items": question,
            },
        },
    })
}

/// The questions `arguments` ask, or why they cannot be asked.
fn questions(arguments: &Value) -> Result<Vec<Question>, String> {
    let listed: &[Value] = match arguments.get("questions") {
        Some(Value::Array(list)) => list,
        Some(_) => return Err("`questions` must be a list of questions".into()),
        // One question written at the top level.
        None if arguments.get("question").is_some() => std::slice::from_ref(arguments),
        None => return Err("give `questions`: a list of one to three questions".into()),
    };
    if listed.is_empty() {
        return Err("give at least one question".into());
    }
    if listed.len() > MAX_QUESTIONS {
        return Err(format!(
            "{} questions: ask at most {MAX_QUESTIONS} at once, the ones that matter most",
            listed.len()
        ));
    }
    listed
        .iter()
        .enumerate()
        .map(|(at, item)| question(item, at + 1))
        .collect()
}

/// The `number`th question.
fn question(item: &Value, number: usize) -> Result<Question, String> {
    let empty = Map::new();
    let (fields, text) = match item {
        Value::String(text) => (&empty, text.as_str()),
        Value::Object(fields) => (
            fields,
            fields.get("question").and_then(Value::as_str).unwrap_or(""),
        ),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) => {
            return Err(format!("question {number} is not an object"));
        }
    };
    let text = text.trim();
    if text.is_empty() {
        return Err(format!(
            "question {number} is empty: give its `question` text"
        ));
    }
    let id = match fields.get("id").and_then(Value::as_str).map(str::trim) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => format!("q{number}"),
    };
    let options: &[Value] = match fields.get("options") {
        None | Some(Value::Null) => &[],
        Some(Value::Array(options)) => options,
        Some(_) => return Err(format!("the options of question {number} must be a list")),
    };
    if options.len() > MAX_OPTIONS {
        return Err(format!(
            "question {number} has {} options: offer at most {MAX_OPTIONS}",
            options.len()
        ));
    }
    let choices: Vec<Choice> = options
        .iter()
        .map(|option| choice(option, number))
        .collect::<Result<_, _>>()?;
    // Read leniently: the names other agents' ask tools use for it work too.
    let multiple = ["multiple", "multiSelect", "multi_select"]
        .iter()
        .any(|key| fields.get(*key).and_then(Value::as_bool) == Some(true));
    let header = fields
        .get("header")
        .and_then(Value::as_str)
        .map_or_else(String::new, chip);
    Ok(Question {
        id,
        header,
        text: text.to_string(),
        multiple: multiple && !choices.is_empty(),
        choices,
    })
}

/// One option: `{label, description}`, or a plain string as its label.
fn choice(option: &Value, number: usize) -> Result<Choice, String> {
    let text = |key: &str| option.get(key).and_then(Value::as_str).unwrap_or("").trim();
    let (label, description) = match option.as_str() {
        Some(label) => (label.trim(), ""),
        None => (text("label"), text("description")),
    };
    if label.is_empty() {
        return Err(format!("an option of question {number} has an empty label"));
    }
    // A preview's lines keep their leading spaces: a mockup's columns are its point.
    let preview = option.get("preview").and_then(Value::as_str).unwrap_or("");
    let preview = preview.trim_end().trim_start_matches(['\n', '\r']);
    Ok(Choice {
        label: label.to_string(),
        description: description.to_string(),
        preview: preview.to_string(),
    })
}

/// `header` as a chip shows it: trimmed, and cut short with `…` past [`HEADER`] characters.
fn chip(header: &str) -> String {
    let header = header.trim();
    if header.chars().count() <= HEADER {
        return header.to_string();
    }
    let kept: String = header.chars().take(HEADER - 1).collect();
    format!("{}…", kept.trim_end())
}

/// The answers as the model reads them, one line per question in order; the skip text when
/// none was answered.
fn report(questions: &[Question], answers: &[Answer]) -> String {
    let none = Answer::default();
    let mut answered = false;
    let mut lines = vec!["The person answered:".to_string()];
    for (question, answer) in questions.iter().zip(each(answers, &none)) {
        let (labels, note) = told(question, answer);
        let picked = (!labels.is_empty()).then(|| labels.join(", "));
        answered |= picked.is_some() || note.is_some();
        let said = match (picked, note) {
            (Some(picked), None) => picked,
            (Some(picked), Some(note)) => format!("{picked}; they added: {note}"),
            (None, Some(note)) => format!("in their words: {note}"),
            (None, None) => "no answer; use your best assumption and state it".into(),
        };
        lines.push(format!("- {}: {said}", question.id));
    }
    match answered {
        true => lines.join("\n"),
        false => SKIPPED.into(),
    }
}

/// The answers as the person reads them: each question, then what they said (`→ skipped`).
fn shown(questions: &[Question], answers: &[Answer]) -> String {
    let none = Answer::default();
    let lines: Vec<String> = (questions.iter().zip(each(answers, &none)))
        .map(|(question, answer)| {
            let (picked, note) = told(question, answer);
            let mut said = vec![];
            if !picked.is_empty() {
                said.push(picked.join(", "));
            }
            said.extend(note.map(str::to_string));
            let said = match said.is_empty() {
                true => "skipped".to_string(),
                false => said.join(" · "),
            };
            format!("{} → {said}", question.text)
        })
        .collect();
    lines.join("\n")
}

/// `answers` in order, then `none` for every question the person left without one.
fn each<'a>(answers: &'a [Answer], none: &'a Answer) -> impl Iterator<Item = &'a Answer> {
    answers.iter().chain(std::iter::repeat(none))
}

/// What the person said to `question`: the labels of the options they picked, and their own
/// words when there are any.
fn told<'a>(question: &'a Question, answer: &'a Answer) -> (Vec<&'a str>, Option<&'a str>) {
    let picked = (answer.choices.iter())
        .filter_map(|index| question.choices.get(*index))
        .map(|choice| choice.label.as_str())
        .collect();
    let note = answer.note.as_deref().map(str::trim);
    (picked, note.filter(|note| !note.is_empty()))
}

#[cfg(test)]
#[path = "ask_tests.rs"]
mod tests;
