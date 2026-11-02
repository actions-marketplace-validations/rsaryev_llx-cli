//! Checkpoints: where the conversation stood before each task, so `/rewind` (or Esc Esc on an
//! empty prompt) can take it back to before one of them. Files are not kept: nothing records a
//! command's or a patch's files (ARCHITECTURE.md B4).

/// One task: what it asked, and where the conversation stood before it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Task {
    pub(crate) goal: String,
    pub(crate) at: usize,
}
