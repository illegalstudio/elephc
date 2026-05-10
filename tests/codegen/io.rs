//! Purpose:
//! Groups the I/O integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for printing, files, streams, filesystem, misc, and related suites.

use crate::support::*;

#[path = "io/printing.rs"]
mod printing;
#[path = "io/files.rs"]
mod files;
#[path = "io/streams.rs"]
mod streams;
#[path = "io/filesystem.rs"]
mod filesystem;
#[path = "io/misc.rs"]
mod misc;
#[path = "io/stat_ext.rs"]
mod stat_ext;
#[path = "io/paths/mod.rs"]
mod paths;
#[path = "io/modify.rs"]
mod modify;
