//! Purpose:
//! Groups the class syntax integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for declarations, traits, access, static members, modifiers.

use super::*;

#[path = "classes/declarations.rs"]
mod declarations;
#[path = "classes/traits.rs"]
mod traits;
#[path = "classes/access.rs"]
mod access;
#[path = "classes/static_members.rs"]
mod static_members;
#[path = "classes/modifiers.rs"]
mod modifiers;
