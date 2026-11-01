//! Purpose:
//! Collects the `ext/filter` (`filter_var()`) runtime emitters: dedicated
//! decimal-only int/float parsers, the boolean token matcher, and the
//! IPv4/IPv6 literal validators, plus the shared PHP-filter whitespace
//! trimmer they all use. The module owns re-export wiring only — each leaf
//! file implements exactly one helper.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` during the filter runtime section.
//!
//! Key details:
//! - These are DEDICATED parsers (never `strtol`/`intval`/`is_numeric` reuse):
//!   PHP's `FILTER_VALIDATE_INT`/`FILTER_VALIDATE_FLOAT` reject octal/hex forms
//!   that generic numeric-string parsing accepts, so validation must not be
//!   delegated to a parser with a different accepted grammar.

mod int_range;
mod trim_ws;
mod validate_bool;
mod validate_float;
mod validate_int;
mod validate_ip4;
mod validate_ip6;

/// Refines a `FILTER_VALIDATE_INT` result against a compile-time `min_range`/
/// `max_range` constraint.
pub use int_range::emit_filter_int_range;
/// Trims PHP-filter whitespace (`{0x09, 0x0A, 0x0B, 0x0D, 0x20}`, NOT form feed)
/// from both ends of a PHP string.
pub use trim_ws::emit_filter_trim_ws;
/// Dedicated decimal-only integer parser backing `FILTER_VALIDATE_INT`.
pub use validate_int::emit_filter_validate_int;
/// Dedicated decimal-only float grammar scanner (value computed via `strtod`
/// only after acceptance) backing `FILTER_VALIDATE_FLOAT`.
pub use validate_float::emit_filter_validate_float;
/// Boolean token matcher backing `FILTER_VALIDATE_BOOL(EAN)`.
pub use validate_bool::emit_filter_validate_bool_str;
/// Strict IPv4 dotted-quad check (`inet_pton(AF_INET, ...)`) backing
/// `FILTER_VALIDATE_IP` with `FILTER_FLAG_IPV4` (and the either-family path).
pub use validate_ip4::emit_filter_validate_ip4;
/// IPv6 literal check (`inet_pton(AF_INET6, ...)`) backing
/// `FILTER_VALIDATE_IP` with `FILTER_FLAG_IPV6` (and the either-family path).
pub use validate_ip6::emit_filter_validate_ip6;
