//! Purpose:
//! Shares checked dimension-read flags and diagnostics across native and eval consumers.
//!
//! Called from:
//! - EIR element read lowering and runtime lookup emitters.
//!
//! Key details:
//! - Checked absence is independent of key normalization and nullability policy.

/// Existing warning mode for ordinary PHP reads.
pub const WARN_ON_MISSING: i64 = 1;
/// Existing retaining fetch mode used by read-for-write paths.
pub const FOR_WRITE: i64 = 2;
/// Throws only after a lookup proves an ordinary element absent.
pub const CHECKED: i64 = 4;
/// Reports absence to a Rust caller without unwinding across its stack.
pub const REPORT_MISSING: i64 = 8;
/// The caller already supplied its canonical key tuple; skip receiver-side normalization.
pub const NORMALIZED_KEY_TUPLE: i64 = 16;
/// Stable diagnostic for an absent array or hash entry.
pub const MISSING_ARRAY_KEY: &str = "--strict-indexing: missing array key";
/// Stable diagnostic for an invalid string byte offset.
pub const MISSING_STRING_OFFSET: &str = "--strict-indexing: string offset out of bounds";
