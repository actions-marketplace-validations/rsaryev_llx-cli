use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::time::{Duration, SystemTime};

use super::*;
use crate::testing::Scratch;

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn a_spill_is_a_private_file_of_every_line() {
    let root = Scratch::new();
    let mut spill = Spill::open(root.path()).unwrap();
    spill.write("first", Ends::Line);
    spill.write("second, é", Ends::Line);
    let path = spill.finish();
    assert_eq!(fs::read_to_string(&path).unwrap(), "first\nsecond, é\n");
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(path.parent().unwrap()), 0o700);
    assert!(path.starts_with(root.path()));
}

#[test]
fn two_spills_never_share_a_file() {
    let root = Scratch::new();
    let one = Spill::open(root.path()).unwrap().finish();
    let two = Spill::open(root.path()).unwrap().finish();
    assert_ne!(one, two);
}

#[test]
fn a_directory_another_could_write_is_not_used() {
    // On a shared /tmp another user could make the directory first, or a link in its place.
    let root = Scratch::new();
    let dir = root.path().join(name());
    DirBuilder::new().create(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(Spill::open(root.path()).is_none());
    fs::remove_dir(&dir).unwrap();
    symlink(root.path(), &dir).unwrap();
    assert!(Spill::open(root.path()).is_none());
}

#[test]
fn old_spills_are_removed_when_a_new_one_is_made() {
    let root = Scratch::new();
    let old = Spill::open(root.path()).unwrap().finish();
    let day_ago = SystemTime::now() - KEEP_FOR - Duration::from_secs(60);
    File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(day_ago)
        .unwrap();
    let recent = Spill::open(root.path()).unwrap().finish();
    let _ = Spill::open(root.path()).unwrap().finish();
    assert!(!old.exists());
    assert!(recent.exists());
}

#[test]
fn a_spill_past_its_size_says_how_much_it_did_not_keep() {
    let root = Scratch::new();
    let mut spill = Spill::open(root.path()).unwrap();
    spill.most = 11;
    for line in ["12345", "6789", "kept no more", "nor this"] {
        spill.write(line, Ends::Line);
    }
    let text = fs::read_to_string(spill.finish()).unwrap();
    assert_eq!(text, "12345\n6789\n... 2 more lines were not kept ...\n");
}
