//! When a schedule runs: a 5-field cron expression (`minute hour day-of-month month day-of-week`,
//! local time) with `*`, values, ranges, steps and lists; or a delay
//! (`45m`, `2h`). When both day fields are restricted, a day matches either, as vixie-cron does.

/// A parsed cron expression: the values each field allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cron {
    minutes: u64,
    hours: u64,
    days: u64,
    months: u64,
    weekdays: u64,
    /// Whether day-of-month and day-of-week were both restricted.
    either_day: bool,
    text: String,
}

/// A moment in local time, as cron reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Local {
    pub minute: u32,
    pub hour: u32,
    pub day: u32,
    pub month: u32,
    /// 0 is Sunday.
    pub weekday: u32,
}

impl Cron {
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let fields: Vec<&str> = text.split_whitespace().collect();
        let [minute, hour, day, month, weekday] = fields[..] else {
            return Err(format!(
                "`{text}` is not 5 fields: minute hour day-of-month month day-of-week"
            ));
        };
        let mut weekdays = field(weekday, 0, 7)?;
        // 7 is Sunday too.
        if weekdays & (1 << 7) != 0 {
            weekdays |= 1;
        }
        Ok(Self {
            minutes: field(minute, 0, 59)?,
            hours: field(hour, 0, 23)?,
            days: field(day, 1, 31)?,
            months: field(month, 1, 12)?,
            weekdays,
            either_day: day != "*" && weekday != "*",
            text: fields.join(" "),
        })
    }

    fn matches(&self, at: Local) -> bool {
        let has = |set: u64, value: u32| set & (1 << value) != 0;
        let day = has(self.days, at.day);
        let weekday = has(self.weekdays, at.weekday);
        let day_matches = match self.either_day {
            true => day || weekday,
            false => day && weekday,
        };
        has(self.minutes, at.minute)
            && has(self.hours, at.hour)
            && has(self.months, at.month)
            && day_matches
    }

    /// The first minute after `after` (Unix seconds) the expression matches, within a year.
    pub(crate) fn next(&self, after: u64) -> Option<u64> {
        let start = after / 60 * 60 + 60;
        (0..366 * 24 * 60)
            .map(|minute| start + minute * 60)
            .find(|&at| self.matches(local(at)))
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }
}

/// The values one field allows, as bits.
fn field(text: &str, low: u32, high: u32) -> Result<u64, String> {
    let mut set = 0u64;
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => {
                let step: u32 = step
                    .parse()
                    .ok()
                    .filter(|&step| step > 0)
                    .ok_or_else(|| format!("`{part}`: the step is not a number above 0"))?;
                (range, step)
            }
            None => (part, 1),
        };
        let number = |text: &str| -> Result<u32, String> {
            text.parse()
                .ok()
                .filter(|value| (low..=high).contains(value))
                .ok_or_else(|| format!("`{text}` is not a value from {low} to {high}"))
        };
        let (from, to) = match range {
            "*" => (low, high),
            range => match range.split_once('-') {
                Some((from, to)) => (number(from)?, number(to)?),
                None if step > 1 => (number(range)?, high),
                None => (number(range)?, number(range)?),
            },
        };
        if from > to {
            return Err(format!("`{part}`: the range runs backwards"));
        }
        for value in (from..=to).step_by(step as usize) {
            set |= 1 << value;
        }
    }
    Ok(set)
}

/// `45m`, `2h`, `1d`, `30s` as seconds (seconds rounded up to a minute, as cron's granularity).
pub(crate) fn delay(text: &str) -> Result<u64, String> {
    let text = text.trim();
    let (number, unit) = text.split_at(text.trim_end_matches(char::is_alphabetic).len());
    let count: u64 = number
        .trim()
        .parse()
        .map_err(|error| format!("`{text}` is not a delay such as 45m or 2h: {error}"))?;
    let seconds = match unit.trim() {
        "s" | "sec" => count.div_ceil(60) * 60,
        "m" | "min" | "" => count * 60,
        "h" => count * 3600,
        "d" => count * 86_400,
        _ => return Err(format!("`{text}`: the unit is s, m, h or d")),
    };
    Ok(seconds.max(60))
}

/// `at` (Unix seconds) in local time.
fn local(at: u64) -> Local {
    let Ok(time) = libc::time_t::try_from(at) else {
        return utc(at);
    };
    // SAFETY: a tm is integers and a pointer localtime_r sets: all zero is one.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: localtime_r fills `tm` from `time`; both live through the call.
    let filled = unsafe { libc::localtime_r(&time, &mut tm) };
    if !filled.is_null() {
        // localtime_r gives each field in its range: none is negative.
        return Local {
            minute: tm.tm_min.cast_unsigned(),
            hour: tm.tm_hour.cast_unsigned(),
            day: tm.tm_mday.cast_unsigned(),
            month: tm.tm_mon.cast_unsigned() + 1,
            weekday: tm.tm_wday.cast_unsigned(),
        };
    }
    utc(at)
}

/// `at` in UTC: where no local time can be read.
fn utc(at: u64) -> Local {
    let days = at / 86_400;
    let seconds = at % 86_400;
    let (_, month, day) = crate::context::civil(days);
    Local {
        minute: (seconds / 60 % 60) as u32,
        hour: (seconds / 3600) as u32,
        day,
        month,
        weekday: ((days + 4) % 7) as u32,
    }
}

/// `10:21`, the clock time of `at` in local time.
pub(crate) fn clock(at: u64) -> String {
    let local = local(at);
    format!("{:02}:{:02}", local.hour, local.minute)
}

#[cfg(test)]
#[path = "cron_tests.rs"]
mod tests;
