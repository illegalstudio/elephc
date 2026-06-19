//! Purpose:
//! Pure-Rust IANA timezone-introspection bridge staticlib for elephc's
//! `DateTimeZone::getLocation`/`getTransitions`/`listAbbreviations` family. The
//! three tables are baked from the PHP interpreter (see `data/generate.php`) and
//! embedded with `include_str!`, so results are byte-for-byte identical to PHP
//! with no runtime dependency but `std`. The only computed value is a
//! transition's `time` string, regenerated from its timestamp by a
//! proleptic-Gregorian formatter that stays exact at extreme timestamps (where
//! elephc's `gmdate` would not).
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
//!   data in PHP; lookups return `None` so the marshalling yields `false`.

use std::collections::HashMap;
use std::sync::OnceLock;

mod abi;

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

/// Splits a Unix timestamp into whole days since the epoch and the seconds within
/// that day, using Euclidean division so negative timestamps floor correctly
/// (and without overflowing at `i64::MIN`, unlike truncating `/`).
fn days_and_secs(ts: i64) -> (i64, i64) {
    (ts.div_euclid(86_400), ts.rem_euclid(86_400))
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

/// Formats a Unix timestamp as the UTC civil string PHP places in a transition's
/// `time` field: `Y-m-d\TH:i:sP` where `P` is the literal `+00:00`. The year is
/// the full signed integer (no fixed width, so extreme negative years render in
/// full, e.g. `-292277022657`), while month/day/hour/minute/second are
/// zero-padded to two digits.
pub fn format_utc_iso(ts: i64) -> String {
    let (days, secs) = days_and_secs(ts);
    let (year, month, day) = civil_from_days(days);
    let hour = secs / 3_600;
    let minute = (secs % 3_600) / 60;
    let second = secs % 60;
    format!(
        "{}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00",
        year, month, day, hour, minute, second
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
}
