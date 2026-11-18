//! Purpose:
//! Package P6 of the DOM migration: HTML4 tables and parser (Part::Html).
//!
//! Called from:
//! - `super::extend::PACKAGES`, through `contribute`.
//!
//! Key details:
//! - This file is the package's own: it registers classes, functions and members only through
//!   the `Contributions` API documented at the top of `extend.rs`.

use super::extend::Contributions;

/// Registers this package's declarations. Empty until the package lands.
pub(super) fn contribute(_contributions: &mut Contributions) {}
