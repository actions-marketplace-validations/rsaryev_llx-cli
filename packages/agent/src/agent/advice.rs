//! The advisor's word at a task's edges (C9): as a conversation's first task starts, the level it
//! thinks at; once the model answers, whether the task goes on, twice a task at most.

use contracts::advice::{Review, Task};
use contracts::chat::Message;

use super::{Agent, Run};
use crate::conversation::Conversation;

/// Times a task goes on after an answer at the advisor's word.
const MAX_PUSHES: usize = 2;

/// Whether a task opens its conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opening {
    /// A new conversation: its level may still be chosen.
    Fresh,
    /// One that goes on (a follow-up, a fork): its level stays.
    Continued,
}

/// Where a task starts: its goal, its first message, and whether it opens the conversation.
#[derive(Debug, Clone, Copy)]
pub(super) struct Begun<'g> {
    goal: &'g str,
    from: usize,
    opening: Opening,
}

impl<'g> Begun<'g> {
    /// The task whose goal is the last message of `conversation`. It opens the conversation when
    /// the model has said nothing there yet (what came before is the app's: a brief, a mode).
    pub(super) fn at(goal: &'g str, conversation: &Conversation) -> Self {
        let messages = conversation.messages();
        let from = messages.len().saturating_sub(1);
        let opening = match (messages.iter()).any(|m| matches!(m, Message::Assistant { .. })) {
            true => Opening::Continued,
            false => Opening::Fresh,
        };
        Self {
            goal,
            from,
            opening,
        }
    }

    fn task<'a>(&'a self, run: &'a Run) -> Task<'a> {
        Task {
            goal: self.goal,
            conversation: run.conversation.messages(),
            from: self.from,
        }
    }
}

impl Agent<'_> {
    /// The advisor's level for a conversation the task opens; one that goes on is not asked.
    pub(super) fn advise_start(&self, begun: &Begun<'_>, run: &mut Run) {
        let Some(advisor) = self.advisor.filter(|_| begun.opening == Opening::Fresh) else {
            return;
        };
        if let Some(effort) = advisor.level(begun.goal) {
            run.conversation.set_effort(Some(effort));
        }
    }

    /// Whether the task goes on after the model's `answer` at the advisor's word: its line is
    /// said after the answer.
    pub(super) fn pushed_on(
        &self,
        begun: &Begun<'_>,
        answer: &str,
        pushes: &mut usize,
        run: &mut Run,
    ) -> bool {
        let Some(advisor) = self.advisor.filter(|_| *pushes < MAX_PUSHES) else {
            return false;
        };
        match advisor.review(&begun.task(run), answer) {
            Review::Done => false,
            Review::GoOn(line) => {
                *pushes += 1;
                self.observer.advised(&line);
                run.conversation.push(Message::User(line));
                true
            }
        }
    }
}

#[cfg(test)]
#[path = "advice_tests.rs"]
mod tests;
