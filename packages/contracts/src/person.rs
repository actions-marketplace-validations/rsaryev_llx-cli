//! The person port: a tool asks the person it works for a question, and whoever composes the tools
//! answers (at a terminal, or nobody). A tool knows nothing of who answers.

/// One option the person may pick.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choice {
    /// A few words.
    pub label: String,
    /// What choosing it means; empty for none.
    pub description: String,
    /// What it looks like, to compare the options by (a mockup, a snippet): lines shown as they
    /// are, beside the options; empty for none.
    pub preview: String,
}

/// One question, with the options offered (none for an open question).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Question {
    /// The key the answer comes back under.
    pub id: String,
    /// A word or two naming the question, shown as its tab among the others; empty for none.
    pub header: String,
    pub text: String,
    pub choices: Vec<Choice>,
    /// Whether several options may be picked together; otherwise one at most.
    pub multiple: bool,
}

/// The person's answer to one question: options, words of their own, both, or neither (skipped).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answer {
    /// The indexes of the options picked, in order: one at most unless the question is `multiple`.
    pub choices: Vec<usize>,
    /// What they wrote: beside the option, or instead of one.
    pub note: Option<String>,
}

/// What came back from asking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// One answer per question, in order.
    Answers(Vec<Answer>),
    /// No answer can come, and why: nobody is there to give one.
    Nobody(String),
    /// The person stopped the task instead of answering.
    Stopped,
}

pub trait Person: Send + Sync {
    /// Ask `questions`, in order, and wait for the answers.
    fn ask(&self, questions: &[Question]) -> Reply;
}
