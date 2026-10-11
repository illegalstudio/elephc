//! Purpose:
//! Pure-Rust IANA timezone-introspection bridge staticlib for elephc's
//! `DateTimeZone::getLocation`/`getTransitions`/`listAbbreviations` family. The
//! three tables are baked from the PHP interpreter (see `data/generate.php`) and
//! embedded with `include_str!`, so results are byte-for-byte identical to PHP
//! with no runtime dependency but `std`. Wide Gregorian conversion formats
//! transition timestamps and supplies calendar components to eval. Default
//! timezone offsets use the matching complete TZif data and POSIX rules.
//!
//! Called from:
//! - Compiled PHP programs via the `elephc_tz_*` C ABI (see the `abi` module).
//! - `cargo test -p elephc-tz` (the rlib) for in-isolation validation against
//!   reference values captured from PHP 8.5.6 / timelib tz 2026.1.
//!
//! Key details:
//! - Baking is required for parity: tz Rust crates ship slim TZif (no fat
//!   recurring-transition expansion) and Ramadan zones are not POSIX-expressible,
//!   so a computed approach diverges from PHP. getLocation/listAbbreviations are
//!   PHP-internal timelib tables with no algorithm at all.
//! - The 11 legacy abbreviation-zones (`CET`, `EET`, `EST`, `GMT`, `GMT+0`,
//!   `GMT-0`, `HST`, `MET`, `MST`, `UCT`, `WET`) carry no transition/location
//!   object data in PHP; introspection returns `None` for `false`. Default
//!   timezone ID resolution remains distinct and uses their complete TZif data.

use std::collections::HashMap;
use std::sync::OnceLock;

mod abi;
mod posix_rules;

/// Embedded transition table: one line per zone, `<zone>\t<field>` where field is
/// `F` (false), `=<canonical>` (alias), or `ts,off,dst,abbr;...`.
const TRANSITIONS: &str = include_str!("../data/transitions.data");
/// Embedded location table: `<zone>\tF` or `<zone>\t<cc>\t<lat>\t<lon>\t<comments>`.
const LOCATIONS: &str = include_str!("../data/location.data");
/// Embedded abbreviation table: `<abbr>\t<dst>:<off>:<id>;...` in PHP order.
const ABBREVIATIONS: &str = include_str!("../data/abbreviations.data");
/// Embedded timelib/IANA release string the tables above were baked from, captured
/// by `data/generate.php` as `timezone_version_get()`. Trimmed of the trailing newline
/// so callers see exactly e.g. `2026.1`.
const VERSION: &str = include_str!("../data/version.data");

/// Reports the timelib/IANA release the embedded introspection tables were baked
/// from, matching what PHP's `timezone_version_get()` returned during generation.
/// Re-running `data/generate.php` re-pins this in lockstep with the data files.
pub fn tz_version() -> &'static str {
    VERSION.trim()
}

/// One reconstructed `getTransitions()` row. Field set and order mirror PHP's
/// array exactly: timestamp, UTC civil time string, offset seconds, DST flag,
/// abbreviation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TzTransition {
    /// Unix timestamp of the transition (`i64::MIN` for the synthetic first row).
    pub ts: i64,
    /// UTC civil time, formatted `Y-m-d\TH:i:sP` with a `+00:00` suffix.
    pub time: String,
    /// Offset from UTC in seconds in effect from this transition onward.
    pub offset: i64,
    /// Whether the offset is a daylight-saving offset.
    pub isdst: bool,
    /// Timezone abbreviation in effect from this transition onward (e.g. `CET`).
    pub abbr: String,
}

/// UTC proleptic-Gregorian components for one Unix timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UtcCivilTime {
    /// Full signed proleptic-Gregorian year.
    pub year: i64,
    /// Calendar month in the range 1 through 12.
    pub month: u32,
    /// Calendar day in the range 1 through 31.
    pub day: u32,
    /// Hour in the range 0 through 23.
    pub hour: u32,
    /// Minute in the range 0 through 59.
    pub minute: u32,
    /// Second in the range 0 through 59.
    pub second: u32,
    /// Sunday is zero, matching C/PHP's `tm_wday` convention.
    pub weekday: u32,
    /// Zero-based day of year, matching C/PHP's `tm_yday` convention.
    pub yearday: u32,
}

/// A zone's `getLocation()` result. `latitude`/`longitude` keep PHP's verbatim
/// shortest-round-trip decimal text, which a PHP `(float)` cast turns back into
/// the exact IEEE-754 value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// ISO country code, or `??` for zones without a country (e.g. `UTC`).
    pub country_code: &'static str,
    /// Latitude as PHP's exact decimal text (e.g. `48.866659999999996`).
    pub latitude: &'static str,
    /// Longitude as PHP's exact decimal text (e.g. `2.3333299999999895`).
    pub longitude: &'static str,
    /// Free-text comment (often empty).
    pub comments: &'static str,
}

/// One `listAbbreviations()` row: the DST flag, UTC offset in seconds, and the
/// timezone identifier (absent for the 25 PHP rows with a null `timezone_id`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbbrevRow {
    /// Whether this mapping describes a daylight-saving offset.
    pub dst: bool,
    /// UTC offset in seconds.
    pub offset: i64,
    /// Timezone identifier, or `None` for PHP's null-id rows.
    pub timezone_id: Option<&'static str>,
}

/// Converts a day count relative to the Unix epoch into a proleptic-Gregorian
/// `(year, month, day)` using Howard Hinnant's `civil_from_days` algorithm, which
/// is exact across the full `i64` day range including the deeply negative day of
/// `i64::MIN` seconds. `month` is 1-12 and `day` is 1-31.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Shift the epoch to 0000-03-01 so leap days fall at the end of the 400-year era.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // day of era, [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11] (March = 0)
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Converts a Unix timestamp to UTC calendar components without libc range limits.
pub fn utc_civil_time(ts: i64) -> UtcCivilTime {
    civil_time_at_offset(ts, 0)
}

/// Converts a Unix timestamp to civil components after applying a UTC offset in seconds.
pub fn civil_time_at_offset(ts: i64, offset: i32) -> UtcCivilTime {
    let shifted = i128::from(ts) + i128::from(offset);
    let days = shifted.div_euclid(86_400);
    let secs = shifted.rem_euclid(86_400);
    let days = i64::try_from(days).expect("an i64 timestamp plus an i32 offset fits i64 days");
    let (year, month, day) = civil_from_days(days);
    let leap = year.rem_euclid(4) == 0
        && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0);
    let month_starts = if leap {
        [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335]
    } else {
        [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334]
    };
    UtcCivilTime {
        year,
        month,
        day,
        hour: (secs / 3_600) as u32,
        minute: ((secs % 3_600) / 60) as u32,
        second: (secs % 60) as u32,
        weekday: (days + 4).rem_euclid(7) as u32,
        yearday: month_starts[(month - 1) as usize] + day - 1,
    }
}

/// Converts validated UTC calendar components to a Unix timestamp without libc limits.
pub fn utc_timestamp_from_civil(
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<i64> {
    let leap = year.rem_euclid(4) == 0
        && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0);
    let month_lengths = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if !(1..=12).contains(&month)
        || day == 0
        || day > month_lengths[(month - 1) as usize]
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let adjusted_year = year.checked_sub(i64::from(month <= 2))?;
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year.rem_euclid(400);
    let shifted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era
        .checked_mul(146_097)?
        .checked_add(day_of_era)?
        .checked_sub(719_468)?;
    let timestamp = i128::from(days) * 86_400
        + i128::from(hour) * 3_600
        + i128::from(minute) * 60
        + i128::from(second);
    i64::try_from(timestamp).ok()
}

/// Formats a Unix timestamp as the UTC civil string PHP places in a transition's
/// `time` field: `Y-m-d\TH:i:sP` where `P` is the literal `+00:00`. The year is
/// the full signed integer (no fixed width, so extreme negative years render in
/// full, e.g. `-292277022657`), while month/day/hour/minute/second are
/// zero-padded to two digits.
pub fn format_utc_iso(ts: i64) -> String {
    let civil = utc_civil_time(ts);
    format!(
        "{}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00",
        civil.year, civil.month, civil.day, civil.hour, civil.minute, civil.second
    )
}

/// Builds, once, an index from a baked table's zone name to the text after the
/// first tab on its line. Slicing the embedded `&'static str`, so no allocation
/// of the field data itself.
fn index(table: &'static str) -> HashMap<&'static str, &'static str> {
    table
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .collect()
}

/// Memoized index over the embedded transitions table.
fn transitions_index() -> &'static HashMap<&'static str, &'static str> {
    static IDX: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    IDX.get_or_init(|| index(TRANSITIONS))
}

/// Memoized index over the embedded locations table.
fn locations_index() -> &'static HashMap<&'static str, &'static str> {
    static IDX: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    IDX.get_or_init(|| index(LOCATIONS))
}

/// Parses one transition row `ts,off,dst,abbr`, computing the `time` string from
/// `ts`. Returns `None` on a malformed row (which the committed data never holds).
fn parse_transition(row: &str) -> Option<TzTransition> {
    let mut parts = row.splitn(4, ',');
    let ts: i64 = parts.next()?.parse().ok()?;
    let offset: i64 = parts.next()?.parse().ok()?;
    let isdst = match parts.next()? {
        "1" => true,
        "0" => false,
        _ => return None,
    };
    let abbr = parts.next()?.to_string();
    Some(TzTransition {
        ts,
        time: format_utc_iso(ts),
        offset,
        isdst,
        abbr,
    })
}

/// Returns PHP's no-argument `getTransitions()` rows for a named IANA zone, or
/// `None` when the zone has no transition data (the 11 abbreviation-zones) or the
/// name is unknown — both of which PHP surfaces as `false`. Alias lines (`=`) are
/// resolved to the canonical zone's rows.
pub fn zone_transitions(name: &str) -> Option<Vec<TzTransition>> {
    let mut field = *transitions_index().get(name)?;
    if let Some(canonical) = field.strip_prefix('=') {
        field = *transitions_index().get(canonical)?;
    }
    if field == "F" {
        return None;
    }
    Some(field.split(';').filter_map(parse_transition).collect())
}

/// Returns a zone's `getLocation()` data, or `None` when the zone has no location
/// (the 11 abbreviation-zones) or the name is unknown — both `false` in PHP.
pub fn zone_location(name: &str) -> Option<Location> {
    let field = *locations_index().get(name)?;
    if field == "F" {
        return None;
    }
    let mut parts = field.splitn(4, '\t');
    Some(Location {
        country_code: parts.next()?,
        latitude: parts.next()?,
        longitude: parts.next()?,
        comments: parts.next().unwrap_or(""),
    })
}

/// Returns whether `name` is one of PHP/timelib's known timezone identifiers.
///
/// This deliberately consults the complete baked identifier table rather than
/// [`zone_transitions`]: PHP accepts aliases and its legacy abbreviation zones
/// (such as `CET`) even though those zones deliberately expose no transition
/// rows through `DateTimeZone::getTransitions()`.
pub fn is_known_timezone_identifier(name: &str) -> bool {
    transitions_index().contains_key(name)
}

/// Resolves a default timezone ID's offset, DST flag, and abbreviation at `ts`.
///
/// Uses PHP 8.5.6 / tzdb 2026.1's complete explicit TZif transitions followed
/// by timelib's POSIX continuation rules, independently from the public
/// [`zone_transitions`] projection. This ID path intentionally differs from
/// `DateTimeZone`'s fixed-abbreviation constructor semantics (for example CET).
/// Unknown IDs return `None`; known-zone endpoint overflow returns PHP's zero
/// offset rather than triggering a host-CRT fallback.
pub fn zone_offset_at(name: &str, ts: i64) -> Option<(i32, bool, String)> {
    match posix_rules::resolve(name, ts)? {
        posix_rules::Resolution::Offset(offset, is_dst, abbreviation) => {
            Some((offset, is_dst, abbreviation))
        }
        // This is distinct from an unknown identifier. At the signed timestamp
        // endpoint timelib's wrapping POSIX transition calculation yields no
        // type; PHP then formats the zero-offset LMT fallback.
        posix_rules::Resolution::Overflow => Some((0, false, "LMT".to_string())),
    }
}

/// Returns the full `listAbbreviations()` table in PHP's exact key and row order,
/// parsed once from the embedded data. An empty id field decodes to `None` (PHP's
/// null `timezone_id`); PHP never emits an empty-string id.
pub fn abbreviations() -> &'static [(&'static str, Vec<AbbrevRow>)] {
    static TABLE: OnceLock<Vec<(&'static str, Vec<AbbrevRow>)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        ABBREVIATIONS
            .lines()
            .filter_map(|line| {
                let (abbr, rows) = line.split_once('\t')?;
                let parsed = rows
                    .split(';')
                    .filter_map(|row| {
                        let mut parts = row.splitn(3, ':');
                        let dst = parts.next()? == "1";
                        let offset: i64 = parts.next()?.parse().ok()?;
                        let id = parts.next()?;
                        Some(AbbrevRow {
                            dst,
                            offset,
                            timezone_id: if id.is_empty() { None } else { Some(id) },
                        })
                    })
                    .collect();
                Some((abbr, parsed))
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Validates the baked tables and the runtime `time` formatter against
    //! reference values captured from PHP 8.5.6 (timelib tz 2026.1).
    //!
    //! Called from:
    //! - `cargo test -p elephc-tz` through Rust's test harness.
    //!
    //! Key details:
    //! - Europe/Paris is the transition canary (185 rows, LMT row 0 at `i64::MIN`
    //!   with a 561-second offset and a far-negative `time` string). Aggregate
    //!   counts (598 zones, 1127 abbreviation rows) guard against truncated data.

    use super::*;

    /// The civil-date formatter reproduces PHP's `time` strings across the range,
    /// including the hardest case (`i64::MIN`'s deeply negative year), the epoch,
    /// a pre-1970 negative timestamp, and a 2037 transition.
    #[test]
    fn formats_times_like_php() {
        assert_eq!(format_utc_iso(i64::MIN), "-292277022657-01-27T08:29:52+00:00");
        assert_eq!(format_utc_iso(0), "1970-01-01T00:00:00+00:00");
        assert_eq!(format_utc_iso(-2_486_592_561), "1891-03-15T23:50:39+00:00");
        assert_eq!(format_utc_iso(2_140_045_200), "2037-10-25T01:00:00+00:00");
        let before_epoch = utc_civil_time(-1);
        assert_eq!(
            utc_timestamp_from_civil(
                before_epoch.year,
                before_epoch.month,
                before_epoch.day,
                before_epoch.hour,
                before_epoch.minute,
                before_epoch.second,
            ),
            Some(-1)
        );
        for timestamp in [i64::MIN, i64::MAX] {
            let civil = utc_civil_time(timestamp);
            assert_eq!(
                utc_timestamp_from_civil(
                    civil.year,
                    civil.month,
                    civil.day,
                    civil.hour,
                    civil.minute,
                    civil.second,
                ),
                Some(timestamp)
            );
        }
        assert_eq!(civil_time_at_offset(i64::MIN, -18_000).second, 52);
        assert_eq!(civil_time_at_offset(i64::MAX, 50_400).second, 7);
        assert_eq!(utc_timestamp_from_civil(2023, 2, 29, 0, 0, 0), None);
        assert_eq!(utc_timestamp_from_civil(2024, 2, 29, 0, 0, 0), Some(1_709_164_800));
    }

    /// Europe/Paris matches PHP's `getTransitions()` exactly at the boundaries:
    /// row count, the synthetic LMT row 0, the first two real transitions, and the
    /// final row — covering offset, DST, abbreviation, and the `time` string.
    #[test]
    fn paris_transitions_match_php() {
        let rows = zone_transitions("Europe/Paris").expect("Europe/Paris is a known zone");
        assert_eq!(rows.len(), 185, "Paris transition count");
        assert_eq!(
            rows[0],
            TzTransition {
                ts: i64::MIN,
                time: "-292277022657-01-27T08:29:52+00:00".to_string(),
                offset: 561,
                isdst: false,
                abbr: "LMT".to_string(),
            }
        );
        assert_eq!(rows[1].ts, -2_486_592_561);
        assert_eq!(rows[1].abbr, "PMT");
        assert_eq!(rows[184].ts, 2_140_045_200);
        assert_eq!(rows[184].abbr, "CET");
        assert_eq!(rows[184].time, "2037-10-25T01:00:00+00:00");
    }

    /// An alias zone resolves to its canonical zone's rows (identical to it).
    #[test]
    fn alias_zone_resolves() {
        let nairobi = zone_transitions("Africa/Nairobi").expect("alias zone");
        let asmera = zone_transitions("Africa/Asmera").expect("canonical zone");
        assert_eq!(nairobi, asmera);
    }

    /// The 11 abbreviation-zones and unknown names have no transition data.
    #[test]
    fn false_and_unknown_transition_zones_are_none() {
        assert!(zone_transitions("CET").is_none());
        assert!(zone_transitions("GMT+0").is_none());
        assert!(zone_transitions("Not/AZone").is_none());
    }

    /// Runtime zone-ID offsets retain hidden TZif rows, footer recurrence, and endpoint behavior.
    #[test]
    fn runtime_offsets_match_php_856_timelib_2026_1() {
        assert_eq!(zone_offset_at("Europe/Paris", 2_161_598_400).unwrap().0, 7200);
        assert_eq!(zone_offset_at("Europe/Paris", 4_118_126_400).unwrap().0, 7200);
        assert_eq!(zone_offset_at("Africa/Casablanca", 2_169_079_200).unwrap().0, 0);
        assert_eq!(zone_offset_at("Europe/Paris", i64::MIN).unwrap().0, 561);
        assert_eq!(zone_offset_at("Europe/Paris", i64::MAX).unwrap().0, 0);
        assert_eq!(zone_offset_at("CET", 2_161_598_400).unwrap().0, 7200);
        assert_eq!(zone_offset_at("WET", 2_161_598_400).unwrap().0, 3600);
        assert!(zone_offset_at("Not/AZone", 0).is_none());
    }

    /// getLocation matches PHP for a normal zone and the special `UTC` values.
    #[test]
    fn locations_match_php() {
        let paris = zone_location("Europe/Paris").expect("Paris location");
        assert_eq!(paris.country_code, "FR");
        assert_eq!(paris.latitude, "48.866659999999996");
        assert_eq!(paris.longitude, "2.3333299999999895");
        assert_eq!(paris.comments, "");

        let utc = zone_location("UTC").expect("UTC location");
        assert_eq!(utc.country_code, "??");
        assert_eq!(utc.latitude, "-90");
        assert_eq!(utc.longitude, "-180");

        assert!(zone_location("CET").is_none());
    }

    /// The abbreviation table reproduces PHP's totals, key order, and null ids.
    #[test]
    fn abbreviations_match_php() {
        let table = abbreviations();
        assert_eq!(table.len(), 144, "abbreviation key count");
        let rows: usize = table.iter().map(|(_, r)| r.len()).sum();
        assert_eq!(rows, 1127, "total abbreviation rows");
        assert_eq!(table.first().unwrap().0, "acdt");
        assert_eq!(table.last().unwrap().0, "z");

        let acdt = &table.iter().find(|(k, _)| *k == "acdt").unwrap().1[0];
        assert_eq!(
            *acdt,
            AbbrevRow {
                dst: true,
                offset: 37_800,
                timezone_id: Some("Australia/Adelaide"),
            }
        );

        let null_ids: usize = table
            .iter()
            .flat_map(|(_, r)| r)
            .filter(|r| r.timezone_id.is_none())
            .count();
        assert_eq!(null_ids, 25, "PHP null timezone_id rows");
    }

    /// Every zone listed in the location table that is not a false-zone also has a
    /// resolvable transition entry, guarding against a desynced pair of tables.
    #[test]
    fn tables_cover_all_zones() {
        assert_eq!(LOCATIONS.lines().count(), 598);
        assert_eq!(TRANSITIONS.lines().count(), 598);
    }

    /// Europe/Paris resolves to standard time (CET, +3600, no DST) at a known
    /// winter instant (2024-01-15 12:00:00 UTC).
    #[test]
    fn zone_offset_at_paris_winter() {
        let (offset, isdst, abbr) = zone_offset_at("Europe/Paris", 1_705_320_000)
            .expect("Europe/Paris resolves in winter");
        assert_eq!(offset, 3600);
        assert!(!isdst);
        assert_eq!(abbr, "CET");
    }

    /// Europe/Paris resolves to daylight time (CEST, +7200, DST) at a known
    /// summer instant (2024-07-15 12:00:00 UTC).
    #[test]
    fn zone_offset_at_paris_summer() {
        let (offset, isdst, abbr) = zone_offset_at("Europe/Paris", 1_721_044_800)
            .expect("Europe/Paris resolves in summer");
        assert_eq!(offset, 7200);
        assert!(isdst);
        assert_eq!(abbr, "CEST");
    }

    /// Link-style aliases use the canonical transition stream for offset
    /// resolution too, not only for `DateTimeZone::getTransitions()`.
    #[test]
    fn zone_offset_at_us_eastern_alias_matches_new_york() {
        let alias = zone_offset_at("US/Eastern", 1_719_835_200).unwrap();
        let canonical = zone_offset_at("America/New_York", 1_719_835_200).unwrap();
        assert_eq!(alias, canonical);
        assert_eq!((alias.0, alias.1, alias.2.as_str()), (-14_400, true, "EDT"));
    }

    /// Before Europe/Paris's first real transition, resolution falls back to the
    /// synthetic LMT row 0 (561-second offset), matching `getTransitions()`'s own
    /// row 0 semantics — exercised with a deeply negative pre-1900 timestamp.
    #[test]
    fn zone_offset_at_pre_first_transition_uses_lmt_row() {
        let (offset, isdst, abbr) =
            zone_offset_at("Europe/Paris", -2_840_140_800).expect("resolves before row 1 (1880)");
        assert_eq!(offset, 561);
        assert!(!isdst);
        assert_eq!(abbr, "LMT");
    }

    /// UTC always resolves to a zero offset, unaffected by the query timestamp.
    #[test]
    fn zone_offset_at_utc_is_always_zero() {
        let (offset, isdst, _) = zone_offset_at("UTC", 0).expect("UTC resolves");
        assert_eq!(offset, 0);
        assert!(!isdst);
    }

    /// An unknown zone name resolves to `None`, mirroring `zone_transitions`.
    #[test]
    fn zone_offset_at_unknown_zone_is_none() {
        assert!(zone_offset_at("Not/AZone", 0).is_none());
    }

    /// Validation follows PHP's identifier namespace, not the narrower
    /// transition-data namespace used by the offset resolver.
    #[test]
    fn known_timezone_identifier_accepts_aliases_and_legacy_zones() {
        for name in ["UTC", "Europe/Paris", "America/New_York", "US/Eastern", "CET"] {
            assert!(is_known_timezone_identifier(name), "{name} must be accepted");
        }
        assert!(!is_known_timezone_identifier(""));
        assert!(!is_known_timezone_identifier("Europe/Nope"));
    }
}
