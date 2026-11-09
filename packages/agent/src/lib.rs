//! The agent: a chat model reaches a goal through tools and answers. It works with any chat model
//! (`contracts::chat::Chat`) and any tools (`contracts::tool::Tool`), and no particular one: what
//! a tool's world is like comes in its spec, and where the run happens in the app's context.
//! - [`agent`]: the loop and its stop conditions.
//! - `calls`: a turn's calls carried out, those that may run together at once.
//! - [`history`]: how the conversation is fitted to the model's window before each request.
//! - [`observer`]: what a run shows as it goes (calls, results, turns).
//! - [`conversation`]: the messages, which only grow; a result too long for one keeps its head
//!   and tail.
//! - `schema`: the `result` call of `--schema`, offered with every request and forced after an
//!   answer in words, with its data.

pub mod agent;
mod calls;
pub mod conversation;
pub mod history;
pub mod observer;
mod schema;

#[cfg(test)]
mod fakes;
