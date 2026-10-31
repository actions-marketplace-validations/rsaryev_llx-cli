use super::*;

#[test]
fn the_same_parts_give_the_same_key_and_a_part_moved_across_a_boundary_another() {
    assert_eq!(key(["model", "rules"]), key(["model", "rules"]));
    assert_ne!(key(["ab", "c"]), key(["a", "bc"]));
    assert!(key(["x"]).starts_with("llx-"));
}
