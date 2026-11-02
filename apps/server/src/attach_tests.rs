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

#[expect(
    clippy::disallowed_methods,
    reason = "a test's fixture: the command ends by itself"
)]
#[test]
fn a_picture_from_a_pipe_is_read_no_further_than_the_limit() {
    use std::io::Write;
    let dir = Scratch::new();
    let pipe = dir.path().join("shot");
    let made = std::process::Command::new("mkfifo")
        .arg(&pipe)
        .status()
        .unwrap();
    assert!(made.success());
    let writing = pipe.clone();
    let writer = std::thread::Builder::new()
        .name("llx-test-pipe".into())
        .spawn(move || {
            let mut sink = std::fs::OpenOptions::new()
                .write(true)
                .open(writing)
                .unwrap();
            // Far past the limit, as `--image <(command)` may give.
            let chunk = vec![0_u8; 1 << 20];
            (0..64).try_for_each(|_| sink.write_all(&chunk))
        })
        .unwrap();
    let refused = read(&pipe).unwrap_err();
    assert!(
        refused.ends_with("shot is more than 8 MB; a picture may be at most 8 MB"),
        "{refused}"
    );
    assert!(writer.join().unwrap().is_err(), "the rest was never read");
}
