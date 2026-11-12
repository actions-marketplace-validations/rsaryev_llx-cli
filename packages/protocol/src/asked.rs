//! What the server asks the person through the interface, and the replies. Each question belongs
//! to its session: it carries an id its reply names, and the server may withdraw it (W5).

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// Whether an effect may happen.
    Approve { tool: String, summary: String },
    /// One question of the `ask` tool: the `at`th of `of`, with the headers of all of them (none
    /// named, none sent), for their tabs.
    Question {
        question: Question,
        at: usize,
        of: usize,
        headers: Vec<String>,
    },
    /// A choice of llx's own: a title and its options (a label and a note each), starting on
    /// `marked`.
    Choose {
        title: String,
        options: Vec<(String, String)>,
        marked: Option<usize>,
    },
    /// A choice with a value adjusted inline for each row, its rows in `groups` when it has
    /// any (an interface that does not know them shows one list).
    ChooseAdjustable {
        title: String,
        options: Vec<(String, String)>,
        marked: Option<usize>,
        adjustments: Vec<Adjustment>,
        groups: Vec<Group>,
    },
    /// A line to type, shown as dots when it is a secret.
    Line { prompt: String, hidden: bool },
}

/// The reply to what was asked, of the same kind; `None` when the person cancelled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Approval(Approval),
    Question(Option<Answer>),
    Choice(Option<usize>),
    Adjusted(Option<(usize, usize)>),
    Line(Option<String>),
}

/// The answer to an approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Approval {
    Yes,
    /// Yes, and the same tool is not asked about again in this mode.
    Always,
    /// No, and what the person said beside it.
    No(Option<String>),
}

/// One question, with the options offered (none for an open question).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Question {
    pub id: String,
    /// The question's tab; empty for none.
    pub header: String,
    pub text: String,
    pub choices: Vec<Choice>,
    /// Whether several options may be picked together.
    pub multiple: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    pub description: String,
    /// A mockup or snippet shown beside the options, its lines as written; empty for none.
    pub preview: String,
}

/// The person's answer to one question: options, words of their own, both, or neither.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answer {
    pub choices: Vec<usize>,
    pub note: Option<String>,
}

/// A value a row adjusts inline: its choices and the one set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Adjustment {
    pub values: Vec<String>,
    pub at: usize,
}

/// Consecutive rows of a choice under one name (a provider's models): the first `rows` rows not
/// in an earlier group.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub rows: usize,
    /// What the group is one of, said before the first of them (`Cloud`, `Local`); empty for
    /// none. A group with no rows is its section's note, not a tab.
    pub section: String,
}
