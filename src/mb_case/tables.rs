//! Purpose:
//! Builds compact Unicode case-mapping tables for AOT `__rt_mb_convert_case`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::mb_convert_case` while emitting
//!   gated runtime data.
//!
//! Key details:
//! - Simple tables store 1:1 mappings; full tables store only 1:N expansions.
//! - Tables are computed once per process from Rust `char` methods (Unicode 16).

use super::{
    collect_full, is_cased, simple_or_self, CASE_IGNORABLE_RANGES, MB_CASE_LOWER, MB_CASE_TITLE,
    MB_CASE_UPPER,
};
use std::sync::OnceLock;

/// One 1:N Unicode case expansion stored for the AOT runtime.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FullMap {
    /// Source code point.
    pub from: u32,
    /// Number of mapped code points in `1..=3`.
    pub len: u32,
    /// Mapped code points; unused slots are zero.
    pub out: [u32; 3],
}

/// Compact case-mapping tables used by the AOT runtime helper.
#[derive(Debug)]
pub(crate) struct CaseTables {
    /// Inclusive `Case_Ignorable` ranges.
    pub ignorable: &'static [(u32, u32)],
    /// Inclusive Unicode `Cased` ranges.
    pub cased: Vec<(u32, u32)>,
    /// 1:1 uppercase mappings.
    pub simple_upper: Vec<(u32, u32)>,
    /// 1:1 lowercase mappings.
    pub simple_lower: Vec<(u32, u32)>,
    /// 1:1 titlecase mappings.
    pub simple_title: Vec<(u32, u32)>,
    /// 1:N uppercase expansions.
    pub full_upper: Vec<FullMap>,
    /// 1:N lowercase expansions.
    pub full_lower: Vec<FullMap>,
    /// 1:N titlecase expansions.
    pub full_title: Vec<FullMap>,
    /// 1:N case-fold expansions (`MB_CASE_FOLD`).
    pub full_fold: Vec<FullMap>,
}

/// Returns the process-wide AOT case-mapping tables.
pub(crate) fn case_tables() -> &'static CaseTables {
    static TABLES: OnceLock<CaseTables> = OnceLock::new();
    TABLES.get_or_init(build_case_tables)
}

/// Scans Unicode scalars and builds the compact mapping tables.
fn build_case_tables() -> CaseTables {
    let mut cased_points = Vec::new();
    let mut simple_upper = Vec::new();
    let mut simple_lower = Vec::new();
    let mut simple_title = Vec::new();
    let mut full_upper = Vec::new();
    let mut full_lower = Vec::new();
    let mut full_title = Vec::new();
    let mut full_fold = Vec::new();

    for code in 0u32..=0x10FFFF {
        let Some(ch) = char::from_u32(code) else {
            continue;
        };
        if is_cased(code) {
            cased_points.push(code);
        }

        push_maps(
            code,
            collect_full(ch.to_uppercase(), code),
            simple_or_self(ch.to_uppercase(), code),
            &mut simple_upper,
            &mut full_upper,
        );
        push_maps(
            code,
            collect_full(ch.to_lowercase(), code),
            simple_or_self(ch.to_lowercase(), code),
            &mut simple_lower,
            &mut full_lower,
        );
        push_maps(
            code,
            collect_full(super::titlecase_chars(ch), code),
            simple_or_self(super::titlecase_chars(ch), code),
            &mut simple_title,
            &mut full_title,
        );
        let folded = super::casefold_chars(ch);
        if folded.len() > 1 {
            push_maps(code, collect_full(folded, code), code, &mut Vec::new(), &mut full_fold);
        }
    }

    let _ = (MB_CASE_UPPER, MB_CASE_LOWER, MB_CASE_TITLE);
    CaseTables {
        ignorable: CASE_IGNORABLE_RANGES,
        cased: collapse_ranges(&cased_points),
        simple_upper,
        simple_lower,
        simple_title,
        full_upper,
        full_lower,
        full_title,
        full_fold,
    }
}

/// Records a 1:1 mapping or a 1:N expansion for `code`.
fn push_maps(
    code: u32,
    full: Vec<u32>,
    simple: u32,
    simple_out: &mut Vec<(u32, u32)>,
    full_out: &mut Vec<FullMap>,
) {
    if simple != code {
        simple_out.push((code, simple));
    }
    if full.len() > 1 {
        let mut out = [0u32; 3];
        for (index, mapped) in full.iter().take(3).enumerate() {
            out[index] = *mapped;
        }
        full_out.push(FullMap {
            from: code,
            len: full.len().min(3) as u32,
            out,
        });
    }
}

/// Collapses a sorted list of code points into inclusive ranges.
fn collapse_ranges(points: &[u32]) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    let mut iter = points.iter().copied();
    let Some(mut lo) = iter.next() else {
        return ranges;
    };
    let mut hi = lo;
    for point in iter {
        if point == hi + 1 {
            hi = point;
        } else {
            ranges.push((lo, hi));
            lo = point;
            hi = point;
        }
    }
    ranges.push((lo, hi));
    ranges
}
