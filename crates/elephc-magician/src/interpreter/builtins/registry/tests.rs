//! Purpose:
//! Test module wiring for eval builtin registry discovery and metadata checks.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Focused child modules keep large registry assertions near their area while
//!   still sharing access to private registry helpers.

mod direct_hooks;
mod exposure;
mod metadata_core;
mod metadata_filesystem;
mod metadata_misc;
mod metadata_regex;
mod metadata_streams;
mod metadata_time_and_env;
mod strict_mode;

use super::*;
