//! Purpose:
//! Lists classes php-src declares both `internal` and `final` while elephc declares them in a
//! prelude.
//!
//! Called from:
//! - `crate::class_metadata` and `crate::reflection_class_calls` to report the engine shape.
//!
//! Key details:
//! - A prelude class is ordinary userland to the checker, so `ClassInfo::is_internal` is false;
//!   the reflection refusal and modifier flags key off this list instead.
//! - Names are canonical PHP spellings without a leading separator.

/// Classes php-src declares both `internal` and `final` while elephc declares them in a prelude.
pub const INTERNAL_FINAL_CLASSES: &[&str] = &["Uri\\Rfc3986\\Uri", "Uri\\WhatWg\\Url"];
