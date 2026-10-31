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
