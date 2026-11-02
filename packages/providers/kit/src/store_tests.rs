use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("llx-store-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn a_saved_secret_is_readable_by_the_person_alone() {
    let dir = scratch("save");
    let path = dir.join("nested").join("secret");
    save(&path, "  token \n").unwrap();
    assert_eq!(read(&path).as_deref(), Some("token"));
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert!(remove(&path).unwrap());
    assert!(!remove(&path).unwrap(), "already gone");
    assert_eq!(read(&path), None);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_file_saved_with_wider_permissions_is_narrowed() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("narrow");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("secret");
    std::fs::write(&path, "old").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    save(&path, "new").unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_save_replaces_the_file_whole_and_leaves_nothing_behind() {
    // Written in place, a save cut short would leave a truncated file that looks fresh.
    let dir = scratch("whole");
    let path = dir.join("models.json");
    save(&path, "old catalog").unwrap();
    let mut before = std::fs::File::open(&path).unwrap();
    save(&path, "new catalog").unwrap();
    let mut seen = String::new();
    std::io::Read::read_to_string(&mut before, &mut seen).unwrap();
    assert_eq!(seen, "old catalog", "the old file was never cut");
    assert_eq!(read(&path).as_deref(), Some("new catalog"));
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_pasted_key_has_no_spaces_inside() {
    assert_eq!(as_key("  sk-or-v1-abc \n"), Some("sk-or-v1-abc"));
    assert_eq!(as_key("sk or"), None);
    assert_eq!(as_key("\n"), None);
}

#[test]
fn a_key_shows_only_its_ends() {
    assert_eq!(masked("sk-or-v1-0123456789abcdef"), "sk-or-...cdef");
    assert_eq!(masked("sk-or-abc"), "...");
}

#[test]
fn two_files_saved_at_once_from_two_threads_keep_their_own_text() {
    // A process's threads (a catalog's refresh, a sign-in) may save at once: a catalog and its
    // ETag (one stem) must not share a temporary file.
    let dir = scratch("threads");
    let catalog = dir.join("models.json");
    let etag = dir.join("models.etag");
    let barrier = std::sync::Barrier::new(2);
    // Counted, not asserted in the threads: a panicked saver would leave the other at the barrier.
    let wrong = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for (path, text) in [(&catalog, "catalog ".repeat(4096)), (&etag, "etag".into())] {
            let (barrier, wrong) = (&barrier, &wrong);
            let saver = move || {
                for _ in 0..200 {
                    barrier.wait();
                    let saved = save(path, &text).is_ok();
                    if !saved || read(path).as_deref() != Some(text.trim()) {
                        wrong.fetch_add(1, Ordering::Relaxed);
                    }
                }
            };
            std::thread::Builder::new()
                .name("saver".into())
                .spawn_scoped(scope, saver)
                .unwrap();
        }
    });
    assert_eq!(
        wrong.into_inner(),
        0,
        "saves that failed or read another's text"
    );
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2, "nothing left");
    std::fs::remove_dir_all(&dir).unwrap();
}
