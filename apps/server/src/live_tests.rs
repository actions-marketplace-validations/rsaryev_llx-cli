use contracts::chat::TaskId;

use super::*;
use crate::testing::{ScriptedChat, answers};

fn ask(chat: &dyn Chat) -> Result<Completion, ChatError> {
    chat.complete(Request {
        conversation: &[],
        tools: &[],
        calling: contracts::chat::Calling::Nothing,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    })
}

#[test]
fn a_session_signed_out_says_how_to_sign_in_until_a_model_is_put_in() {
    let live = LiveChat::new(Box::new(SignedOut(Provider::openai())));
    let refused = ask(&live).unwrap_err().to_string();
    assert!(
        refused.contains("not signed in") && refused.contains("/login"),
        "{refused}"
    );
    live.set(Box::new(ScriptedChat::new(answers(1))));
    assert!(ask(&live).is_ok());
}
