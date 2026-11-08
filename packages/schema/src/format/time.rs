//! RFC 3339's dates and times: `full-date`, `full-time`, `date-time`, and Appendix A's
//! `duration`.

use super::two_digits;

/// `YYYY-MM-DD`, a day the calendar has.
pub(super) fn date(text: &[u8]) -> bool {
    let &[y0, y1, y2, y3, b'-', m0, m1, b'-', d0, d1] = text else {
        return false;
    };
    let (Some(century), Some(years), Some(month), Some(day)) = (
        two_digits(y0, y1),
        two_digits(y2, y3),
        two_digits(m0, m1),
        two_digits(d0, d1),
    ) else {
        return false;
    };
    let year = u16::from(century) * 100 + u16::from(years);
    day >= 1 && day <= days_in(month, year)
}

/// The days of `month` in `year`, none for a month that is not one.
fn days_in(month: u8, year: u16) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        _ => 0,
    }
}

/// `HH:MM:SS[.frac]` and its offset, `Z` or `±HH:MM`; a second 60 only in the last minute of a
/// UTC day, the leap second.
pub(super) fn time(text: &[u8]) -> bool {
    let &[h0, h1, b':', m0, m1, b':', s0, s1, ref rest @ ..] = text else {
        return false;
    };
    let (Some(hour), Some(minute), Some(second)) =
        (two_digits(h0, h1), two_digits(m0, m1), two_digits(s0, s1))
    else {
        return false;
    };
    if hour > 23 || minute > 59 || second > 60 {
        return false;
    }
    let rest = match rest.split_first() {
        Some((b'.', fraction)) => {
            let digits = fraction.iter().take_while(|c| c.is_ascii_digit()).count();
            if digits == 0 {
                return false;
            }
            fraction.get(digits..).unwrap_or_default()
        }
        _ => rest,
    };
    let Some(east) = offset(rest) else {
        return false;
    };
    let utc = (i16::from(hour) * 60 + i16::from(minute) - east).rem_euclid(24 * 60);
    second < 60 || utc == 23 * 60 + 59
}

/// The minutes east of UTC an offset says, `None` when it is not one.
fn offset(text: &[u8]) -> Option<i16> {
    match *text {
        [b'Z' | b'z'] => Some(0),
        [sign @ (b'+' | b'-'), h0, h1, b':', m0, m1] => {
            let (hours, minutes) = (two_digits(h0, h1)?, two_digits(m0, m1)?);
            if hours > 23 || minutes > 59 {
                return None;
            }
            let east = i16::from(hours) * 60 + i16::from(minutes);
            Some(if sign == b'-' { -east } else { east })
        }
        _ => None,
    }
}

/// A date and a time, joined by `T` (in either case).
pub(super) fn date_time(text: &[u8]) -> bool {
    let Some((day, rest)) = text.split_at_checked(10) else {
        return false;
    };
    match rest.split_first() {
        Some((b'T' | b't', clock)) => date(day) && time(clock),
        _ => false,
    }
}

/// `P`, then weeks alone, or a date's years, months and days and a time's hours, minutes and
/// seconds after `T`, each unit following the one before it with none skipped.
pub(super) fn duration(text: &[u8]) -> bool {
    let Some(rest) = text.strip_prefix(b"P") else {
        return false;
    };
    let (date, time) = match rest.iter().position(|&c| c == b'T') {
        Some(at) => (
            rest.get(..at).unwrap_or_default(),
            Some(rest.get(at + 1..).unwrap_or_default()),
        ),
        None => (rest, None),
    };
    if let Some(weeks) = date.strip_suffix(b"W") {
        return time.is_none() && !weeks.is_empty() && weeks.iter().all(u8::is_ascii_digit);
    }
    match time {
        Some(time) => !time.is_empty() && units(date, *b"YMD") && units(time, *b"HMS"),
        None => !date.is_empty() && units(date, *b"YMD"),
    }
}

/// Whether `part` is numbers with their units, the units a run of `order` with none skipped.
fn units(part: &[u8], order: [u8; 3]) -> bool {
    let mut last = None;
    let mut rest = part;
    while !rest.is_empty() {
        let digits = rest.iter().take_while(|c| c.is_ascii_digit()).count();
        let unit = rest
            .get(digits)
            .and_then(|u| order.iter().position(|o| o == u));
        let (true, Some(unit)) = (digits > 0, unit) else {
            return false;
        };
        if last.is_some_and(|last: usize| unit != last + 1) {
            return false;
        }
        last = Some(unit);
        rest = rest.get(digits + 1..).unwrap_or_default();
    }
    true
}
