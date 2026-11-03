//! Purpose:
//! Shared trailing-timezone parsing for date builtins: a compile-time splitter used by the
//! optimizer for literal `strtotime()` arguments and the abbreviation table the synthetic
//! `DateTime` helpers embed.
//!
//! Called from:
//! - `crate::optimize::fold::expr` rewrites literal `strtotime()` calls.
//! - `crate::types::checker::builtin_types::datetime::zone_parse` builds its runtime table.
//!
//! Key details:
//! - Only fixed zones are resolved here: `Z`, `±HH[:MM]` / `±HHMM`, `GMT±...`, `UTC[±...]`,
//!   and tzdb abbreviations. IANA identifiers stay untouched (the runtime parser handles them).
//! - Abbreviations use PHP's fixed tzdb offsets (the first `listAbbreviations()` row), because
//!   PHP parses `CET` as +01:00 even in August.

/// One fixed timezone token found at the end of a datetime string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TrailingZone {
    /// The datetime text with the zone token removed.
    pub stripped: String,
    /// PHP-visible zone name (`WEST`, `+03:40`, `UTC`, ...).
    pub name: String,
    /// Fixed offset from UTC in seconds, positive east of UTC.
    pub offset: i64,
}

/// Returns the fixed trailing timezone of `input`, or `None` when the string carries no fixed
/// zone (including IANA identifiers, which the runtime parser owns).
pub(crate) fn split_fixed_zone(input: &str) -> Option<TrailingZone> {
    let end = input
        .trim_end_matches(|c: char| c.is_ascii_whitespace())
        .len();
    if end == 0 {
        return None;
    }
    let trimmed = &input[..end];
    let (before, token) = match trimmed.rfind(|c: char| c.is_ascii_whitespace()) {
        Some(index) => (&trimmed[..=index], &trimmed[index + 1..]),
        None => ("", trimmed),
    };
    if let Some((name, offset)) = parse_zone_token(token) {
        return Some(TrailingZone {
            stripped: before.trim_end().to_string(),
            name,
            offset,
        });
    }
    if let Some(zone) = split_attached(before, token) {
        return Some(zone);
    }
    None
}

/// Returns the abbreviation table as `(UPPERCASE, offset_seconds)` in PHP order.
pub(crate) fn abbreviation_offsets() -> Vec<(String, i64)> {
    ABBREVIATIONS_DATA
        .lines()
        .filter_map(|line| {
            let (abbreviation, rows) = line.split_once('\t')?;
            let offset = rows.split(';').next()?.split(':').nth(1)?.parse().ok()?;
            Some((abbreviation.to_ascii_uppercase(), offset))
        })
        .collect()
}

/// Splits an offset or `Z` attached to the datetime inside a whitespace-free token.
fn split_attached(before: &str, token: &str) -> Option<TrailingZone> {
    let last = token.chars().last()?;
    if last == 'Z' || last == 'z' {
        let head = &token[..token.len() - 1];
        if head.contains(':') {
            return Some(TrailingZone {
                stripped: format!("{before}{head}"),
                name: "Z".to_string(),
                offset: 0,
            });
        }
    }
    let sign = token
        .char_indices()
        .filter(|(index, ch)| *index > 0 && (*ch == '+' || *ch == '-'))
        .map(|(index, _)| index)
        .last()?;
    let head = &token[..sign];
    if !head.contains(':') && !head.contains('T') {
        return None;
    }
    let (name, offset) = parse_zone_token(&token[sign..])?;
    Some(TrailingZone {
        stripped: format!("{before}{head}"),
        name,
        offset,
    })
}

/// Resolves one whitespace-free timezone token to `(name, offset)`.
fn parse_zone_token(token: &str) -> Option<(String, i64)> {
    if token == "Z" || token == "z" {
        return Some(("Z".to_string(), 0));
    }
    if token.len() >= 3 {
        let head = token[..3].to_ascii_uppercase();
        if head == "GMT" || head == "UTC" {
            if token.len() == 3 {
                return Some((head, 0));
            }
            let rest = &token[3..];
            if rest.starts_with('+') || rest.starts_with('-') {
                let (name, offset) = parse_offset(rest)?;
                return Some(if head == "UTC" {
                    ("UTC".to_string(), 0)
                } else {
                    (name, offset)
                });
            }
            return None;
        }
    }
    if token.starts_with('+') || token.starts_with('-') {
        return parse_offset(token);
    }
    if token.chars().all(|c| c.is_ascii_alphabetic()) {
        let key = token.to_ascii_uppercase();
        let offset = abbreviation_offsets()
            .into_iter()
            .find(|(abbreviation, _)| *abbreviation == key)
            .map(|(_, offset)| offset)?;
        return Some((key, offset));
    }
    None
}

/// Parses `±H`, `±HH`, `±HMM`, `±HHMM`, or `±H[:MM]` into `(±HH:MM, seconds)`.
fn parse_offset(text: &str) -> Option<(String, i64)> {
    let sign = match text.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let body = &text[1..];
    let (hours, minutes) = match body.split_once(':') {
        Some((hours, minutes)) => {
            if hours.is_empty()
                || minutes.len() != 2
                || !hours.chars().all(|c| c.is_ascii_digit())
                || !minutes.chars().all(|c| c.is_ascii_digit())
            {
                return None;
            }
            (hours.parse::<i64>().ok()?, minutes.parse::<i64>().ok()?)
        }
        None => {
            if !body.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            match body.len() {
                1 | 2 => (body.parse().ok()?, 0),
                3 => (
                    body[..1].parse().ok()?,
                    body[1..].parse().ok()?,
                ),
                4 => (
                    body[..2].parse().ok()?,
                    body[2..].parse().ok()?,
                ),
                _ => return None,
            }
        }
    };
    if minutes > 59 || hours > 24 || (hours == 24 && minutes != 0) {
        return None;
    }
    Some((
        format!("{}{:02}:{:02}", if sign < 0 { "-" } else { "+" }, hours, minutes),
        sign * (hours * 3600 + minutes * 60),
    ))
}

/// The bridge crate's tzdb abbreviation snapshot, in PHP's `listAbbreviations()` order.
const ABBREVIATIONS_DATA: &str =
    include_str!("../crates/elephc-tz/data/abbreviations.data");

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies fixed abbreviation and numeric offsets resolve to PHP's name and seconds.
    #[test]
    fn splits_fixed_trailing_zones() {
        assert_eq!(
            split_fixed_zone("2012-01-01 00:00:00 WEST"),
            Some(TrailingZone {
                stripped: "2012-01-01 00:00:00".to_string(),
                name: "WEST".to_string(),
                offset: 3600,
            })
        );
        assert_eq!(
            split_fixed_zone("2012-01-01 00:00:00 +0340"),
            Some(TrailingZone {
                stripped: "2012-01-01 00:00:00".to_string(),
                name: "+03:40".to_string(),
                offset: 13200,
            })
        );
        assert_eq!(
            split_fixed_zone("2012-01-01T00:00:00-03:00"),
            Some(TrailingZone {
                stripped: "2012-01-01T00:00:00".to_string(),
                name: "-03:00".to_string(),
                offset: -10800,
            })
        );
    }

    /// Verifies non-zone tails and IANA names stay untouched.
    #[test]
    fn leaves_non_fixed_tails_alone() {
        assert_eq!(split_fixed_zone("2012-01-01"), None);
        assert_eq!(split_fixed_zone("1 July 2012"), None);
        assert_eq!(split_fixed_zone("2012-01-01 00:00:00 Europe/Paris"), None);
        assert_eq!(split_fixed_zone("next Tuesday"), None);
    }
}
