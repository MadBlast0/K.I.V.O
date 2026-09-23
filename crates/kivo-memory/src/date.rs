//! Calendar dates for front-matter and log file names (`2026-09-23`), in the user's local time.
//! Callers pass the time and the UTC offset; nothing here reads a clock.

const DAY_MS: i64 = 86_400_000;

/// Days since 1970-01-01 → (year, month, day). (Howard Hinnant's civil-from-days.)
fn civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `2026-09-23` for epoch milliseconds, in local time.
pub fn format(ms: i64, utc_offset_minutes: i32) -> String {
    let local = ms + i64::from(utc_offset_minutes) * 60_000;
    let (y, m, d) = civil(local.div_euclid(DAY_MS));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Local midnight of `2026-09-23` (also accepts `2026-09-23T10:00…`, keeping the date), as epoch
/// milliseconds. `None` for anything else.
pub fn parse(text: &str, utc_offset_minutes: i32) -> Option<i64> {
    let date = text.trim().trim_matches(['"', '\'']);
    let date = date.get(..10)?;
    let mut parts = date.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || date.as_bytes().get(4) != Some(&b'-') {
        return None;
    }
    Some(days_from_civil(y, m, d) * DAY_MS - i64::from(utc_offset_minutes) * 60_000)
}

/// Whole days from `earlier` to `later` (by local date).
pub fn days_between(earlier: i64, later: i64, utc_offset_minutes: i32) -> i64 {
    let off = i64::from(utc_offset_minutes) * 60_000;
    (later + off).div_euclid(DAY_MS) - (earlier + off).div_euclid(DAY_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip_in_local_time() {
        // 2026-09-23 20:30 UTC is already the 24th in India (UTC+5:30).
        let ms = 1_790_195_400_000;
        assert_eq!(format(ms, 0), "2026-09-23");
        assert_eq!(format(ms, 330), "2026-09-24");
        let midnight = parse("2026-09-24", 330).unwrap();
        assert_eq!(format(midnight, 330), "2026-09-24");
        assert_eq!(format(midnight - 1, 330), "2026-09-23");
        assert_eq!(parse("2026-09-24T08:00:00Z", 330), Some(midnight));
        assert_eq!(parse("tomorrow", 0), None);
        assert_eq!(parse("2026-13-01", 0), None);
        assert_eq!(days_between(parse("2026-09-01", 0).unwrap(), ms, 0), 22);
    }
}
