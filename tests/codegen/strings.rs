//! Purpose:
//! Groups the strings integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for search, transform, encoding, formatting, interpolation and hashes, and related suites.

use crate::support::*;

#[path = "strings/search.rs"]
mod search;
#[path = "strings/transform.rs"]
mod transform;
#[path = "strings/encoding.rs"]
mod encoding;
#[path = "strings/formatting.rs"]
mod formatting;
#[path = "strings/interpolation_and_hashes.rs"]
mod interpolation_and_hashes;
#[path = "strings/misc.rs"]
mod misc;
