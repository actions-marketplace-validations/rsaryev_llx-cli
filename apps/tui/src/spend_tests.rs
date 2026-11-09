use super::*;

fn total(input_tokens: u64, cached_tokens: u64, output_tokens: u64, cost: f64) -> Total {
    Total {
        input_tokens,
        cached_tokens,
        output_tokens,
        cost,
    }
}

#[test]
fn the_spend_reads_as_tokens_in_the_share_from_the_cache_tokens_out_and_money() {
    assert_eq!(
        spent(&total(24_100, 21_000, 1_200, 0.042)),
        "24.1k in · 87% cached · 1.2k out · $0.042"
    );
    assert_eq!(
        spent(&total(2_500_000, 0, 41_000, 12.5)),
        "2.5M in · 41.0k out · $12.50"
    );
    // Under a cent, what was spent still shows, as the task's end says it.
    assert_eq!(
        spent(&total(6_800, 6_500, 70, 0.000_14)),
        "6.8k in · 95% cached · 70 out · $0.0001"
    );
    // Just short of a million, the tenth rounds up: a million, said as one.
    assert_eq!(spent(&total(999_960, 0, 999, 0.0)), "1.0M in · 999 out");
    // A subscription bills nothing per token: no money to show.
    assert_eq!(spent(&total(900, 0, 40, 0.0)), "900 in · 40 out");
    assert_eq!(spent(&Total::default()), "");
}

#[test]
fn the_spend_sits_at_the_end_of_the_row_and_gives_way_when_it_does_not_fit() {
    assert_eq!(
        across("default mode", "1k in", 24),
        "default mode       1k in"
    );
    assert_eq!(
        across(
            "default mode · shift+tab to cycle",
            "24.1k in · 1.2k out",
            40
        ),
        "default mode · shift+tab to cycle"
    );
    assert_eq!(across("default mode", "", 40), "default mode");
}
