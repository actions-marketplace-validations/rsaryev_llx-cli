//! Files as the tools change and read them: [`PatchTool`] (`apply_patch`) carries out a patch:
//! files changed, created, deleted or moved, through a [`Workspace`](crate::workspace::Workspace),
//! the gate asked first (ARCHITECTURE.md B1, B5); [`picture`] is the one reader of the pictures a
//! person attaches. [`named_paths`] says which files a patch names.

mod atomic;
mod diff;
mod patch;
pub mod picture;

pub use patch::PatchTool;
pub use patch::paths as named_paths;
