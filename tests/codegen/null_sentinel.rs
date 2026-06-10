//! Purpose:
//! Groups the null-sentinel collision integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Covers the in-band null sentinel (`PHP_INT_MAX - 1` = `0x7fff_ffff_ffff_fffe`) colliding
//!   with the real integer of the same bit pattern in unboxed scalar slots.

use crate::support::*;

#[path = "null_sentinel/benches.rs"]
mod benches;
#[path = "null_sentinel/repros.rs"]
mod repros;
#[path = "null_sentinel/tagged.rs"]
mod tagged;
