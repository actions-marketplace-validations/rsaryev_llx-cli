use super::*;
use crate::testing::Scratch;

#[test]
fn a_workspace_that_is_not_a_directory_is_refused() {
    let scratch = Scratch::new();
    scratch.file("a.txt", "");
    let why = at(&scratch.path().join("a.txt")).err().unwrap();
    assert_eq!(why, "not a directory");
}
