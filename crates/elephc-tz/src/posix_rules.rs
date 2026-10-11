//! Purpose:
//! Resolves timezone offsets from PHP timelib's complete TZif rows and POSIX footers.
//!
//! Called from:
//! - `crate::zone_offset_at()` for default timezone ID resolution.
//!
//! Key details:
//! - Ported from PHP 8.5.6 timelib `parse_posix.c` (MIT), commit
//!   `fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633`, tzdb 2026.1.
//! - Wrapping timestamp arithmetic intentionally matches timelib's signed 64-bit operations.
//!
//! MIT License
//!
//! Copyright (c) 2021 MongoDB, Inc.
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

use std::collections::HashMap;
use std::sync::OnceLock;

const RULES: &str = include_str!("../data/posix_rules.data");

#[derive(Clone, Copy)]
/// Calendar-day selector used by a POSIX transition rule.
enum DayRule { JulianNoLeap(i64), Julian(i64), MonthWeekDay(i64, i64, i64) }

#[derive(Clone, Copy)]
/// One daylight transition's day selector and local wall-clock second.
struct TransitionRule { day: DayRule, seconds: i64 }

/// Parsed standard/daylight names, offsets, and optional transition pair.
struct ParsedRule {
    standard_offset: i64,
    standard_name: String,
    daylight_offset: Option<i64>,
    daylight_name: Option<String>,
    begin: Option<TransitionRule>,
    end: Option<TransitionRule>,
}

/// Result for a known zone; `Overflow` preserves timelib's null-offset endpoint behavior.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Resolution {
    Offset(i32, bool, String),
    Overflow,
}

/// Builds the embedded zone-to-runtime-metadata lookup once.
fn index() -> &'static HashMap<&'static str, &'static str> {
    static INDEX: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    INDEX.get_or_init(|| RULES.lines().filter(|line| !line.starts_with('#')).filter_map(|line| line.split_once('\t')).collect())
}

/// Selects the last explicit TZif transition at or before `timestamp`.
fn explicit_offset(rows: &str, timestamp: i64) -> Option<(i32, bool, String, i64)> {
    let mut selected = None;
    for row in rows.split(';').filter(|row| !row.is_empty()) {
        let mut fields = row.splitn(4, ',');
        let transition: i64 = fields.next()?.parse().ok()?;
        let offset: i32 = fields.next()?.parse().ok()?;
        let daylight = fields.next()? == "1";
        let abbreviation = fields.next()?.to_string();
        if transition > timestamp { break; }
        selected = Some((offset, daylight, abbreviation, transition));
    }
    selected
}

/// Parses a POSIX alphabetic or angle-bracketed timezone abbreviation.
fn name(input: &str) -> Option<(String, &str)> {
    if let Some(rest) = input.strip_prefix('<') {
        let (value, rest) = rest.split_once('>')?;
        return (!value.is_empty()).then(|| (value.to_string(), rest));
    }
    let end = input.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(input.len());
    (end > 0).then(|| (input[..end].to_string(), &input[end..]))
}

/// Parses one unsigned decimal integer prefix.
fn number(input: &str) -> Option<(i64, &str)> {
    let end = input.find(|c: char| !c.is_ascii_digit()).unwrap_or(input.len());
    if end == 0 { return None; }
    Some((input[..end].parse().ok()?, &input[end..]))
}

/// Parses a POSIX offset, reversing its sign into seconds east of UTC.
fn offset(input: &str) -> Option<(i64, &str)> {
    let (sign, input) = match input.as_bytes().first() { Some(b'+') => (1, &input[1..]), Some(b'-') => (-1, &input[1..]), _ => (1, input) };
    let (hour, mut rest) = number(input)?;
    let mut minute = 0;
    let mut second = 0;
    if let Some(next) = rest.strip_prefix(':') { (minute, rest) = number(next)?; }
    if let Some(next) = rest.strip_prefix(':') { (second, rest) = number(next)?; }
    Some((-sign * (hour * 3600 + minute * 60 + second), rest))
}

/// Parses one `Jn`, `n`, or `Mm.w.d` transition specification.
fn transition(input: &str) -> Option<(TransitionRule, &str)> {
    let (day, mut rest) = if let Some(next) = input.strip_prefix('M') {
        let (month, next) = number(next)?; let next = next.strip_prefix('.')?;
        let (week, next) = number(next)?; let next = next.strip_prefix('.')?;
        let (weekday, next) = number(next)?;
        (DayRule::MonthWeekDay(month, week, weekday), next)
    } else if let Some(next) = input.strip_prefix('J') {
        let (day, next) = number(next)?; (DayRule::JulianNoLeap(day), next)
    } else { let (day, next) = number(input)?; (DayRule::Julian(day), next) };
    let mut seconds = 7200;
    if let Some(next) = rest.strip_prefix('/') { let (parsed, next) = offset(next)?; seconds = -parsed; rest = next; }
    Some((TransitionRule { day, seconds }, rest))
}

/// Parses one complete TZif POSIX footer.
fn parse(input: &str) -> Option<ParsedRule> {
    let (standard_name, rest) = name(input)?;
    let (standard_offset, rest) = offset(rest)?;
    if rest.is_empty() { return Some(ParsedRule { standard_offset, standard_name, daylight_offset: None, daylight_name: None, begin: None, end: None }); }
    let (daylight_name, rest) = name(rest)?;
    let (daylight_offset, rest) = if rest.starts_with(',') { (standard_offset + 3600, rest) } else { offset(rest)? };
    let rest = rest.strip_prefix(',')?;
    let (begin, rest) = transition(rest)?;
    let rest = rest.strip_prefix(',')?;
    let (end, rest) = transition(rest)?;
    rest.is_empty().then(|| ParsedRule { standard_offset, standard_name, daylight_offset: Some(daylight_offset), daylight_name: Some(daylight_name), begin: Some(begin), end: Some(end) })
}

/// Reports whether the signed proleptic-Gregorian year is a leap year.
fn leap(year: i64) -> bool { year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) }

/// Computes a transition's local seconds from the start of `year`.
fn transition_seconds(rule: TransitionRule, year: i64) -> i64 {
    const COMMON: [i64; 12] = [31,28,31,30,31,30,31,31,30,31,30,31];
    let days = match rule.day {
        DayRule::JulianNoLeap(day) => day - 1 + i64::from(leap(year) && day >= 60),
        DayRule::Julian(day) => day,
        DayRule::MonthWeekDay(month, week, weekday) => {
            let mut lengths = COMMON; if leap(year) { lengths[1] = 29; }
            let m1 = (month + 9) % 12 + 1; let yy0 = if month <= 2 { year - 1 } else { year };
            let yy1 = yy0 / 100; let yy2 = yy0 % 100;
            let mut dow = ((26 * m1 - 2) / 10 + 1 + yy2 + yy2 / 4 + yy1 / 4 - 2 * yy1) % 7;
            if dow < 0 { dow += 7; }
            let mut day = weekday - dow; if day < 0 { day += 7; }
            for _ in 1..week { if day + 7 >= lengths[(month - 1) as usize] { break; } day += 7; }
            lengths[..(month - 1) as usize].iter().sum::<i64>() + day
        }
    };
    days.wrapping_mul(86_400).wrapping_add(rule.seconds)
}

/// Computes timelib's wrapping Unix timestamp for January 1 of `year`.
fn year_start(year: i64) -> i64 {
    let leaps = |y: i64| { let y = y - 1; y / 4 - y / 100 + y / 400 };
    86_400i64.wrapping_mul((year - 1970).wrapping_mul(365).wrapping_add(leaps(year)).wrapping_sub(leaps(1970)))
}

/// Resolves a known zone through explicit TZif rows, then its POSIX footer.
pub(crate) fn resolve(name: &str, timestamp: i64) -> Option<Resolution> {
    let encoded = *index().get(name)?;
    let (footer, rows) = encoded.split_once('\t')?;
    if let Some(last_row) = rows.rsplit(';').find(|row| !row.is_empty()) {
        let last_transition = last_row.split(',').next()?.parse::<i64>().ok()?;
        let explicit = explicit_offset(rows, timestamp)?;
        if timestamp < last_transition {
            return Some(Resolution::Offset(explicit.0, explicit.1, explicit.2));
        }
        if footer.is_empty() {
            return Some(Resolution::Offset(explicit.0, explicit.1, explicit.2));
        }
    }
    let rule = parse(footer)?;
    let (Some(dst_offset), Some(dst_name), Some(begin), Some(end)) = (rule.daylight_offset, rule.daylight_name, rule.begin, rule.end) else {
        return Some(Resolution::Offset(rule.standard_offset as i32, false, rule.standard_name));
    };
    let year = crate::utc_civil_time(timestamp).year;
    let mut transitions = Vec::with_capacity(6);
    for year in [year.wrapping_sub(1), year, year.wrapping_add(1)] {
        let start = year_start(year).wrapping_add(transition_seconds(begin, year)).wrapping_sub(rule.standard_offset);
        let stop = year_start(year).wrapping_add(transition_seconds(end, year)).wrapping_sub(dst_offset);
        if start < stop { transitions.extend([(start, true), (stop, false)]); } else { transitions.extend([(stop, false), (start, true)]); }
    }
    for pair in transitions.windows(2) {
        if timestamp < pair[1].0 {
            return if pair[0].1 {
                Some(Resolution::Offset(dst_offset as i32, true, dst_name))
            } else {
                Some(Resolution::Offset(rule.standard_offset as i32, false, rule.standard_name))
            };
        }
    }
    Some(Resolution::Overflow)
}

#[cfg(test)]
mod tests {
    use super::{resolve, Resolution};

    /// Extracts the offset from a successful known-zone resolution.
    fn resolved_offset(zone: &str, timestamp: i64) -> Option<i32> {
        match resolve(zone, timestamp)? {
            Resolution::Offset(offset, _, _) => Some(offset),
            Resolution::Overflow => None,
        }
    }

    /// Pins future, fractional-offset, fixed-zone, and timelib endpoint behavior.
    #[test]
    fn matches_php_856_timelib_2026_1() {
        assert_eq!(resolved_offset("Europe/Paris", 2_161_598_400), Some(7200));
        assert_eq!(resolved_offset("Europe/Paris", 4_118_126_400), Some(7200));
        assert_eq!(resolved_offset("Australia/Lord_Howe", 1_704_200_523), Some(39600));
        assert_eq!(resolved_offset("Pacific/Chatham", 1_704_200_523), Some(49500));
        assert_eq!(resolved_offset("Africa/Casablanca", 2_169_079_200), Some(0));
        assert_eq!(resolve("US/Eastern", 4_118_126_400), resolve("America/New_York", 4_118_126_400));
        assert_eq!(resolved_offset("CET", 2_161_598_400), Some(7200));
        assert_eq!(resolve("Europe/Paris", i64::MAX), Some(Resolution::Overflow));
        assert_eq!(resolved_offset("Europe/Paris", i64::MIN), Some(561));
        assert_eq!(resolved_offset("EET", 2_161_598_400), Some(10800));
        assert_eq!(resolved_offset("WET", 2_161_598_400), Some(3600));
        assert_eq!(resolved_offset("MET", 2_161_598_400), Some(7200));
        assert_eq!(resolved_offset("UCT", 2_161_598_400), Some(0));
        assert!(resolve("Not/AZone", 0).is_none());
    }
}
