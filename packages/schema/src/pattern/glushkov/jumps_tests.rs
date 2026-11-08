use super::super::super::bits::Bits;
use super::Jumps;

fn set(positions: &[usize]) -> u64 {
    positions.iter().fold(u64::none(), |set, &p| set.with(p))
}

#[test]
fn positions_with_the_same_followers_share_one_set() {
    let jumps = Jumps::new(&[set(&[9]), set(&[]), set(&[9]), set(&[7]), set(&[9])]);
    assert_eq!(jumps.sets.len(), 2);
    assert!(jumps.any());
    assert_eq!(jumps.of(set(&[0])), set(&[9]));
    assert_eq!(jumps.of(set(&[0, 2, 4])), set(&[9]));
    assert_eq!(jumps.of(set(&[2, 3])), set(&[7, 9]));
    assert_eq!(jumps.of(set(&[1])), set(&[]));
    assert!(!Jumps::new(&[set(&[]), set(&[])]).any());
}
