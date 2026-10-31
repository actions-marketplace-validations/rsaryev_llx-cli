use super::*;
use crate::test_support::Scratch;

/// A 3x2 PNG's first bytes: the signature and the header chunk.
fn png() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    bytes.extend(3u32.to_be_bytes());
    bytes.extend(2u32.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0]);
    bytes
}

#[test]
fn a_dropped_path_is_known_however_the_terminal_writes_it() {
    let dir = Scratch::new();
    let picture = dir.path().join("shot\u{202f}one.png");
    std::fs::write(&picture, png()).unwrap();
    let notes = dir.path().join("my notes.txt");
    std::fs::write(&notes, "hi").unwrap();
    let shown = picture.display().to_string();
    assert_eq!(
        resolve(&format!("'{shown}'")),
        Some(Dropped::Image(picture.clone()))
    );
    assert_eq!(resolve(&shown), Some(Dropped::Image(picture)));
    let escaped = notes.display().to_string().replace(' ', "\\ ");
    assert_eq!(resolve(&escaped), Some(Dropped::File(notes.clone())));
    let address = format!("file://{}", notes.display()).replace(' ', "%20");
    assert_eq!(resolve(&address), Some(Dropped::File(notes)));
    assert_eq!(
        resolve(&dir.path().display().to_string()),
        Some(Dropped::File(dir.path().to_path_buf()))
    );
    assert_eq!(resolve("relative/path.png"), None);
    assert_eq!(
        resolve(&dir.path().join("missing.png").display().to_string()),
        None
    );
}

#[test]
fn a_kept_picture_is_a_copy_that_outlives_the_original() {
    let dir = Scratch::new();
    let original = dir.path().join("shot.png");
    std::fs::write(&original, png()).unwrap();
    let kept = keep(&original, &dir.path().join("pasted"));
    std::fs::remove_file(&original).unwrap();
    assert!(kept.is_file() && kept.to_string_lossy().ends_with("-shot.png"));
}
