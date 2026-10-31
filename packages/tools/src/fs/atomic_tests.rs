use super::*;
use crate::testing::Scratch;

#[test]
fn a_write_creates_the_parents_and_leaves_no_temporary_file() {
    let dir = Scratch::new();
    let path = dir.path().join("a/b/c.txt");
    write(&path, b"hello").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "hello");
    let names: Vec<_> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(names, ["c.txt"]);
}

#[test]
fn an_executable_stays_executable() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new();
    let path = dir.file("run.sh", "#!/bin/sh\n");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    write(&path, b"#!/bin/sh\necho hi\n").unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o755
    );
}

/// Makes a file read-only, and writable again when dropped, even after a failed assertion, so the
/// scratch directory can be removed.
struct ReadOnly<'a>(&'a Path);

impl<'a> ReadOnly<'a> {
    fn new(path: &'a Path) -> Self {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(path, permissions).unwrap();
        Self(path)
    }
}

impl Drop for ReadOnly<'_> {
    fn drop(&mut self) {
        if let Ok(meta) = fs::metadata(self.0) {
            let mut permissions = meta.permissions();
            #[expect(
                clippy::permissions_set_readonly_false,
                reason = "the test makes the file writable again to clean up"
            )]
            permissions.set_readonly(false);
            let _ = fs::set_permissions(self.0, permissions);
        }
    }
}

#[test]
fn a_read_only_file_is_refused_and_kept() {
    let dir = Scratch::new();
    let path = dir.file("locked.txt", "vendored");
    let _locked = ReadOnly::new(&path);
    let error = write(&path, b"changed").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(error.to_string(), "it is read-only");
    assert_eq!(fs::read_to_string(&path).unwrap(), "vendored");
}

#[test]
fn every_hard_link_sees_the_new_content() {
    let dir = Scratch::new();
    let path = dir.file("a.txt", "old");
    let other = dir.path().join("b.txt");
    fs::hard_link(&path, &other).unwrap();
    write(&path, b"new").unwrap();
    assert_eq!(fs::read_to_string(&other).unwrap(), "new");
}

#[test]
fn a_long_name_still_gets_a_temporary_beside_it() {
    let dir = Scratch::new();
    let path = dir.path().join(format!("{}.txt", "n".repeat(240)));
    write(&path, b"fits").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "fits");
    let name = temporary(&path);
    assert!(name.file_name().unwrap().len() < 140, "{}", name.display());
}

#[test]
fn a_symlink_planted_at_a_temporary_name_is_never_written_through() {
    let dir = Scratch::new();
    let outside = Scratch::new();
    let victim = outside.file("victim.txt", "untouched");
    let path = dir.file("x.txt", "old");
    // Names given in order, not by the process-wide counter, which parallel tests advance: the
    // first ones are symlinks out of the directory, so each of them is surely met.
    let names: Vec<PathBuf> = (0..TRIES)
        .map(|n| dir.path().join(format!(".x.txt.{n}.tmp")))
        .collect();
    let given = |count: usize| {
        let mut names = names[..count].iter().cloned();
        move || names.next().expect("no more names were given")
    };
    for planted in &names[..5] {
        std::os::unix::fs::symlink(&victim, planted).unwrap();
    }
    let (temporary, mut file) = create(given(6)).unwrap();
    assert_eq!(temporary, names[5], "the first free name");
    file.write_all(b"new").unwrap();
    drop(file);
    assert_eq!(fs::read_to_string(&temporary).unwrap(), "new");
    assert_eq!(fs::read_to_string(&victim).unwrap(), "untouched");
    // Every name taken: the write gives up rather than open one.
    fs::remove_file(&temporary).unwrap();
    for planted in &names[5..] {
        std::os::unix::fs::symlink(&victim, planted).unwrap();
    }
    let error = create(given(TRIES)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(&victim).unwrap(), "untouched");
    // And a whole write beside them still replaces the file, not the victim.
    write(&path, b"new").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(fs::read_to_string(&victim).unwrap(), "untouched");
}
