use super::*;
use crate::testing::Scratch;

/// A 3x2 PNG's first bytes: the signature and the header chunk.
fn png() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    bytes.extend(3u32.to_be_bytes());
    bytes.extend(2u32.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0]);
    bytes
}

#[test]
fn a_picture_is_read_as_base64_with_its_name() {
    let dir = Scratch::new();
    let path = dir.path().join("shot one.png");
    std::fs::write(&path, png()).unwrap();
    let attached = read(&path).unwrap();
    assert_eq!(attached.name, "shot one.png");
    assert_eq!((attached.image.width, attached.image.height), (3, 2));
    assert!(attached.image.data.starts_with("iVBORw0KGgo"));
    let text = dir.path().join("notes.txt");
    std::fs::write(&text, "hi").unwrap();
    assert!(
        read(&text)
            .unwrap_err()
            .contains("not a PNG, JPEG, GIF or WebP")
    );
}

#[test]
fn a_picture_too_large_says_its_size_rounded_up() {
    let dir = Scratch::new();
    let path = dir.path().join("big.png");
    let mut bytes = png();
    bytes.resize((8 << 20) + (512 << 10), 0);
    std::fs::write(&path, bytes).unwrap();
    let refused = read(&path).unwrap_err();
    assert!(
        refused.ends_with("big.png is 9 MB; a picture may be at most 8 MB"),
        "{refused}"
    );
}
