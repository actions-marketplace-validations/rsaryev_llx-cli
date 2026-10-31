//! Steering: what the person sends while a task works goes to that task at its next step; a
//! slash command, a picture, or Tab waits for the task to end instead. The server decides which
//! (`server/inputs.rs`); the interface only says when an entry is for after the task.

use super::app::App;

impl App {
    /// The composer's text for after the task: Tab while it works.
    pub(super) fn queue_for_later(&mut self) {
        // As Enter does: a path typed or dropped in without a paste goes as its file.
        self.adopt_paths();
        let entry = self.composer.text();
        if entry.trim().is_empty() {
            return;
        }
        self.composer.clear();
        let (text, images) = self.composer.take_attached(&entry);
        self.tell(text, images, true);
    }
}
