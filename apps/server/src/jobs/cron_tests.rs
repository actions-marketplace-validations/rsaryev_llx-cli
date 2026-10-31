use super::*;

fn at(minute: u32, hour: u32, day: u32, month: u32, weekday: u32) -> Local {
    Local {
        minute,
        hour,
        day,
        month,
        weekday,
    }
}

#[test]
fn fields_take_stars_values_ranges_steps_and_lists() {
    let every_five = Cron::parse("*/5 * * * *").unwrap();
    assert!(every_five.matches(at(10, 3, 1, 1, 1)));
    assert!(!every_five.matches(at(11, 3, 1, 1, 1)));
    let weekdays = Cron::parse("0 9 * * 1-5").unwrap();
    assert!(weekdays.matches(at(0, 9, 7, 6, 3)));
    assert!(!weekdays.matches(at(0, 9, 7, 6, 0)), "Sunday");
    let listed = Cron::parse("7,37 * * * *").unwrap();
    assert!(listed.matches(at(37, 0, 1, 1, 0)) && !listed.matches(at(8, 0, 1, 1, 0)));
    let sunday = Cron::parse("0 0 * * 7").unwrap();
    assert!(sunday.matches(at(0, 0, 1, 1, 0)), "7 is Sunday too");
}

#[test]
fn both_day_fields_restricted_match_either() {
    let cron = Cron::parse("0 0 15 * 1").unwrap();
    assert!(cron.matches(at(0, 0, 15, 3, 4)), "the 15th");
    assert!(cron.matches(at(0, 0, 3, 3, 1)), "a Monday");
    assert!(!cron.matches(at(0, 0, 3, 3, 2)));
}

#[test]
fn bad_expressions_say_what_is_wrong() {
    assert!(Cron::parse("* * *").unwrap_err().contains("not 5 fields"));
    assert!(Cron::parse("61 * * * *").unwrap_err().contains("0 to 59"));
    assert!(Cron::parse("*/0 * * * *").unwrap_err().contains("step"));
    assert!(
        Cron::parse("5-1 * * * *")
            .unwrap_err()
            .contains("backwards")
    );
    assert!(Cron::parse("MON * * * *").is_err());
}

#[test]
fn the_next_run_is_the_first_matching_minute_after() {
    let hourly = Cron::parse("0 * * * *").unwrap();
    let after = 1_800_000_000;
    let next = hourly.next(after).unwrap();
    assert!(next > after && next - after <= 3600 && next.is_multiple_of(60));
    assert_eq!(local(next).minute, 0);
}

#[test]
fn delays_read_as_seconds_at_least_a_minute() {
    assert_eq!(delay("45m"), Ok(2700));
    assert_eq!(delay("2h"), Ok(7200));
    assert_eq!(delay("1d"), Ok(86_400));
    assert_eq!(delay("30s"), Ok(60));
    assert_eq!(delay("10"), Ok(600));
    assert!(delay("soon").is_err());
}

#[test]
fn utc_dates_are_civil() {
    // 2026-10-23 12:34 UTC was a Friday.
    let friday = utc(1_792_758_840);
    assert_eq!(
        (
            friday.month,
            friday.day,
            friday.hour,
            friday.minute,
            friday.weekday
        ),
        (10, 23, 12, 34, 5)
    );
}
