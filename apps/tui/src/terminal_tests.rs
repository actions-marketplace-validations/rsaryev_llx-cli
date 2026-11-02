use std::io::Read;
use std::sync::mpsc;
use std::thread;

use super::*;

/// The reader reads until the editor asks for the keys; it gives way between two reads (woken out
/// of the one it is in), reads nothing while the editor has them, and reads again once given back.
#[test]
fn the_reader_gives_the_keys_to_the_editor_and_takes_them_back() {
    let keys = Keys::new();
    let (key, keyboard) = mpsc::sync_channel::<()>(4);
    let (passed, passes) = mpsc::sync_channel::<()>(4);
    let keys = &keys;
    thread::scope(|scope| {
        let reader = thread::Builder::new().name("reader".into());
        let reading = reader.spawn_scoped(scope, move || {
            // Each key read, then the turn looked at, as `read_keys` does.
            while keyboard.recv().is_ok() {
                passed.send(()).unwrap();
                keys.between();
            }
        });
        key.send(()).unwrap();
        passes.recv().unwrap();
        keys.take(|| key.send(()).unwrap());
        // The wake was read if the reader was in its read, and left for later if it gave way
        // before it read again: either way its pass, if any, came before the turn was given.
        let _ = passes.try_recv();
        key.send(()).unwrap();
        let read = passes.recv_timeout(Duration::from_millis(200));
        assert!(read.is_err(), "a key read while the editor has them");
        keys.give_back();
        passes.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(key);
        reading.unwrap().join().unwrap();
    });
}

/// A frame is one buffer, and it reaches the terminal in one write: nothing of it (its last row,
/// the cursor's place, the update's end) waits for a flush, as std's line-buffered stdout keeps
/// what follows a frame's last line break for a second write.
#[test]
fn a_frame_reaches_the_terminal_whole_before_any_flush() {
    let (mut terminal, end) = io::pipe().unwrap();
    let mut out = unbuffered(&end).unwrap();
    let frame = "\x1b[?2026h\r\x1b[Jdone\r\n❯ typing\x1b[?25h\x1b[?2026l".as_bytes();
    out.write_all(frame).unwrap();
    let mut read = vec![0; 256];
    let got = terminal.read(&mut read).unwrap();
    assert_eq!(&read[..got], frame);
}

/// The editor's draft is a file made new, that only the person reads: one planted at its name
/// (in a shared `/tmp`, a link to the person's own file) is never written through.
#[test]
fn the_editors_draft_never_writes_through_a_planted_link() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("llx-draft-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let victim = dir.join("victim");
    std::fs::write(&victim, "keep").unwrap();
    std::os::unix::fs::symlink(&victim, dir.join(draft_name(1))).unwrap();
    let path = draft(&dir, "the prompt").unwrap();
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "the prompt");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "a draft others can read");
    std::fs::remove_dir_all(&dir).unwrap();
}
