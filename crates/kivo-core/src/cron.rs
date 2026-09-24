//! Schedules for routines (ROUT-11): the five-field cron format (`minute hour day month weekday`)
//! with `*`, lists, ranges and steps, month and weekday names, and the shorthands `@hourly`,
//! `@daily`, `@weekly`, `@monthly`, `@weekdays`. Times are wall-clock times in a time zone (the
//! PC's own unless one is named), so "7:30 every weekday" stays 7:30 across daylight saving.
//! As in cron, when both the day of the month and the weekday are restricted, either matches.

use jiff::civil::{Date, DateTime, Time};
use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan};

/// A parsed schedule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule {
    minutes: Vec<bool>,
    hours: Vec<bool>,
    days: Vec<bool>,
    months: Vec<bool>,
    /// Sunday is 0.
    weekdays: Vec<bool>,
    any_day: bool,
    any_weekday: bool,
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const WEEKDAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

impl Schedule {
    /// Parses a schedule; the error says what's wrong in plain words.
    pub fn parse(expr: &str) -> Result<Self, String> {
        let expanded = match expr.trim().to_lowercase().as_str() {
            "@hourly" => "0 * * * *".to_owned(),
            "@daily" | "@midnight" => "0 0 * * *".to_owned(),
            "@weekly" => "0 0 * * 0".to_owned(),
            "@monthly" => "0 0 1 * *".to_owned(),
            "@weekdays" => "0 9 * * 1-5".to_owned(),
            other => other.to_owned(),
        };
        let fields: Vec<&str> = expanded.split_whitespace().collect();
        let [minute, hour, day, month, weekday] = fields[..] else {
            return Err(format!(
                "a schedule has five parts (minute hour day month weekday), not {}",
                fields.len()
            ));
        };
        let mut weekdays = field(weekday, 0, 7, &WEEKDAYS, 0)?;
        // 7 is Sunday too.
        if weekdays[7] {
            weekdays[0] = true;
        }
        weekdays.truncate(7);
        Ok(Self {
            minutes: field(minute, 0, 59, &[], 0)?,
            hours: field(hour, 0, 23, &[], 0)?,
            days: field(day, 1, 31, &[], 1)?,
            months: field(month, 1, 12, &MONTHS, 1)?,
            weekdays,
            any_day: day == "*",
            any_weekday: weekday == "*",
        })
    }

    fn day_matches(&self, date: Date) -> bool {
        let dom = self.days[usize::try_from(date.day()).unwrap_or(0)];
        let dow =
            self.weekdays[usize::try_from(date.weekday().to_sunday_zero_offset()).unwrap_or(0)];
        match (self.any_day, self.any_weekday) {
            (true, true) => true,
            (false, true) => dom,
            (true, false) => dow,
            (false, false) => dom || dow,
        }
    }

    /// The first time after `after` the schedule fires, in `tz`. `None` if it never does (the
    /// 31st of February) within five years.
    pub fn next_after(&self, after: Timestamp, tz: &TimeZone) -> Option<Timestamp> {
        let start = after.to_zoned(tz.clone()).datetime();
        // The next whole minute.
        let mut t = start
            .with()
            .second(0)
            .subsec_nanosecond(0)
            .build()
            .ok()?
            .checked_add(1.minute())
            .ok()?;
        let limit = start.checked_add(5.years()).ok()?;
        while t < limit {
            let date = t.date();
            if !self.months[usize::try_from(date.month()).unwrap_or(0)] {
                // The first day of the next month.
                t = DateTime::from_parts(
                    date.first_of_month().checked_add(1.month()).ok()?,
                    Time::midnight(),
                );
                continue;
            }
            if !self.day_matches(date) {
                t = DateTime::from_parts(date.tomorrow().ok()?, Time::midnight());
                continue;
            }
            if !self.hours[usize::try_from(t.hour()).unwrap_or(0)] {
                t = t
                    .with()
                    .minute(0)
                    .build()
                    .ok()?
                    .checked_add(1.hour())
                    .ok()?;
                continue;
            }
            if !self.minutes[usize::try_from(t.minute()).unwrap_or(0)] {
                t = t.checked_add(1.minute()).ok()?;
                continue;
            }
            // A wall-clock time skipped by daylight saving fires just after the jump.
            let zoned = tz.to_ambiguous_zoned(t).compatible().ok()?;
            if zoned.timestamp() > after {
                return Some(zoned.timestamp());
            }
            t = t.checked_add(1.minute()).ok()?;
        }
        None
    }
}

/// One field: `*`, `5`, `1-5`, `*/15`, `1-30/2`, names (`mon`, `jan`) and lists of these.
fn field(
    text: &str,
    min: i32,
    max: i32,
    names: &[&str],
    name_base: i32,
) -> Result<Vec<bool>, String> {
    let mut out = vec![false; usize::try_from(max + 1).unwrap_or(0)];
    let value = |s: &str| -> Result<i32, String> {
        if let Some(i) = names.iter().position(|n| *n == s) {
            return Ok(i32::try_from(i).unwrap_or(0) + name_base);
        }
        s.parse::<i32>()
            .ok()
            .filter(|v| (min..=max).contains(v))
            .ok_or_else(|| format!("“{s}” isn't between {min} and {max}"))
    };
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (
                r,
                s.parse::<i32>()
                    .ok()
                    .filter(|s| *s > 0)
                    .ok_or_else(|| format!("“{s}” isn't a step"))?,
            ),
            None => (part, 1),
        };
        let (from, to) = if range == "*" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            (value(a)?, value(b)?)
        } else {
            let v = value(range)?;
            // `5/10` means from 5 to the end, every 10.
            (v, if step > 1 { max } else { v })
        };
        if from > to {
            return Err(format!("“{range}” runs backwards"));
        }
        let mut v = from;
        while v <= to {
            out[usize::try_from(v).unwrap_or(0)] = true;
            v += step;
        }
    }
    Ok(out)
}

/// The next time `expr` fires after `after_ms` (Unix milliseconds), in the time zone named
/// `tz` (IANA, "Europe/Berlin") or the PC's own.
pub fn next_fire(expr: &str, tz: Option<&str>, after_ms: i64) -> Result<Option<i64>, String> {
    let schedule = Schedule::parse(expr)?;
    let zone = match tz.filter(|t| !t.is_empty()) {
        Some(name) => TimeZone::get(name).map_err(|_| format!("“{name}” isn't a time zone"))?,
        None => TimeZone::system(),
    };
    let after = Timestamp::from_millisecond(after_ms).map_err(|e| e.to_string())?;
    Ok(schedule
        .next_after(after, &zone)
        .map(|t| t.as_millisecond()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(tz: &str, s: &str) -> Timestamp {
        s.parse::<DateTime>()
            .unwrap()
            .to_zoned(TimeZone::get(tz).unwrap())
            .unwrap()
            .timestamp()
    }

    fn next(expr: &str, tz: &str, from: &str) -> String {
        let zone = TimeZone::get(tz).unwrap();
        Schedule::parse(expr)
            .unwrap()
            .next_after(at(tz, from), &zone)
            .unwrap()
            .to_zoned(zone)
            .datetime()
            .to_string()
    }

    #[test]
    fn fields_steps_names_and_shorthands() {
        assert_eq!(
            next("*/15 * * * *", "UTC", "2026-09-24T10:07:30"),
            "2026-09-24T10:15:00"
        );
        assert_eq!(
            next("30 7 * * mon-fri", "UTC", "2026-09-25T08:00"),
            "2026-09-28T07:30:00"
        );
        assert_eq!(
            next("0 9 1 jan *", "UTC", "2026-09-24T00:00"),
            "2027-01-01T09:00:00"
        );
        assert_eq!(
            next("@daily", "UTC", "2026-09-24T10:00"),
            "2026-09-25T00:00:00"
        );
        assert_eq!(
            next("@weekdays", "UTC", "2026-09-26T10:00"),
            "2026-09-28T09:00:00"
        );
        assert_eq!(
            next("0 12 * * 7", "UTC", "2026-09-24T10:00"),
            "2026-09-27T12:00:00"
        );
        // Day of month or weekday: the 1st, or any Monday.
        assert_eq!(
            next("0 8 1 * mon", "UTC", "2026-09-24T10:00"),
            "2026-09-28T08:00:00"
        );
        // Strictly after: the minute it is now doesn't count.
        assert_eq!(
            next("0 10 * * *", "UTC", "2026-09-24T10:00"),
            "2026-09-25T10:00:00"
        );
    }

    #[test]
    fn wall_clock_times_hold_across_daylight_saving() {
        // Berlin moves to summer time on 2027-03-28 at 02:00 → 03:00.
        assert_eq!(
            next("30 7 * * *", "Europe/Berlin", "2027-03-27T08:00"),
            "2027-03-28T07:30:00"
        );
        // 02:30 doesn't exist that day: it fires just after the jump.
        assert_eq!(
            next("30 2 * * *", "Europe/Berlin", "2027-03-27T12:00"),
            "2027-03-28T03:30:00"
        );
    }

    #[test]
    fn bad_schedules_say_why() {
        assert!(Schedule::parse("* * *").unwrap_err().contains("five parts"));
        assert!(
            Schedule::parse("61 * * * *")
                .unwrap_err()
                .contains("between 0 and 59")
        );
        assert!(
            Schedule::parse("5-1 * * * *")
                .unwrap_err()
                .contains("backwards")
        );
        assert!(Schedule::parse("*/0 * * * *").is_err());
        assert_eq!(
            Schedule::parse("0 0 31 2 *")
                .unwrap()
                .next_after(Timestamp::UNIX_EPOCH, &TimeZone::UTC),
            None
        );
        assert!(next_fire("@daily", Some("Mars/Olympus"), 0).is_err());
        assert!(next_fire("@daily", None, 0).unwrap().is_some());
    }
}
