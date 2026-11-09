//! Pictures attached to a message, read for the model: what the interface sent as paths (a file
//! dragged in, the clipboard's image: `tui::attach`) or `--image`. Each goes to the model after
//! the message, as the person's own.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use contracts::tool::Image;
use tools::fs::picture::{self, MAX_BYTES};

/// A picture attached to a message, and the name it is shown with.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Attachment {
    pub name: String,
    pub image: Image,
}

/// The picture at `path`, read and checked.
pub(crate) fn read(path: &Path) -> Result<Attachment, String> {
    let shown = path.display();
    let unread = |error: std::io::Error| format!("{shown} could not be read: {error}");
    let file = File::open(path).map_err(unread)?;
    // No further than the limit: a file of gigabytes, or a pipe that does not end, is not read
    // whole to be refused.
    let mut bytes = Vec::new();
    (&file)
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(unread)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_BYTES {
        let most = MAX_BYTES >> 20;
        return Err(match file.metadata().map(|meta| meta.len()) {
            // Rounded up: a file just past the limit is not said to be the limit.
            Ok(size) if size > MAX_BYTES => format!(
                "{shown} is {} MB; a picture may be at most {most} MB",
                size.div_ceil(1 << 20)
            ),
            // A pipe has no size to say.
            Ok(_) | Err(_) => {
                format!("{shown} is more than {most} MB; a picture may be at most {most} MB")
            }
        });
    }
    let image = picture::image(&bytes)
        .ok_or_else(|| format!("{shown} is not a PNG, JPEG, GIF or WebP picture"))?;
    let name = path.file_name().map_or_else(
        || shown.to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    Ok(Attachment { name, image })
}

#[cfg(test)]
#[path = "attach_tests.rs"]
mod tests;
