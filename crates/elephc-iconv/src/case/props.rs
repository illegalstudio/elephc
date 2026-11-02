//! Purpose:
//! Answers the two Unicode property questions mbstring's final-sigma rule asks: is a
//! code point Cased, and is it Case_Ignorable.
//!
//! Called from:
//! - `crate::case::mapping` while deciding whether a capital sigma ends a word.
//!
//! Key details:
//! - Both answers come from mbstring's own generated ranges (`tables::CASED` and
//!   `tables::CASE_IGNORABLE`), not from Rust's Unicode data, so the context test agrees
//!   with php-src even where the two property sets could drift apart.
//! - Error markers and other values outside Unicode simply fall outside every range,
//!   which is also how php-src's `prop_lookup()` treats them.

use super::tables::{CASED, CASE_IGNORABLE};

/// Reports whether `code` has mbstring's Cased property.
pub(super) fn is_cased(code: u32) -> bool {
    in_ranges(CASED, code)
}

/// Reports whether `code` has mbstring's Case_Ignorable property.
pub(super) fn is_case_ignorable(code: u32) -> bool {
    in_ranges(CASE_IGNORABLE, code)
}

/// Binary-searches one sorted table of inclusive, non-overlapping ranges.
fn in_ranges(ranges: &[(u32, u32)], code: u32) -> bool {
    ranges
        .binary_search_by(|&(low, high)| {
            if high < code {
                std::cmp::Ordering::Less
            } else if low > code {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}
