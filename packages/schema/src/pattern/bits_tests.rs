use super::Bits;

fn listed<B: Bits>(set: B) -> Vec<usize> {
    let mut out = vec![];
    set.each(|p| out.push(p));
    out
}

fn round_trip<B: Bits>() {
    let last = B::WIDTH - 1;
    let set = B::none().with(0).with(5).with(last);
    assert_eq!(listed(set), vec![0, 5, last]);
    assert!(set.any() && !B::none().any());
    assert_eq!(set.first(), Some(0));
    assert_eq!(B::none().with(last).first(), Some(last));
    assert_eq!(B::none().first(), None);
    assert_eq!(listed(set.and(B::none().with(5).with(7))), vec![5]);
    assert_eq!(listed(B::none().with(7).or(B::none().with(3))), vec![3, 7]);
    assert_eq!(listed(set.minus(B::none().with(5))), vec![0, last]);
    assert_eq!(
        listed(set.shifted()),
        vec![1, 6],
        "the last position is let go"
    );
    assert_eq!(
        listed(B::none().with(63).shifted()),
        vec![64]
            .into_iter()
            .filter(|&p| p < B::WIDTH)
            .collect::<Vec<_>>()
    );
    assert!(
        !B::none().with(B::WIDTH).any(),
        "a position past the set is none"
    );
}

#[test]
fn every_width_holds_its_positions() {
    round_trip::<u64>();
    round_trip::<u128>();
    round_trip::<[u64; 4]>();
    round_trip::<[u64; 8]>();
    round_trip::<[u64; 64]>();
}

#[test]
fn a_shift_carries_across_words() {
    let set = <[u64; 4]>::none().with(63).with(127).with(200);
    assert_eq!(listed(set.shifted()), vec![64, 128, 201]);
}
