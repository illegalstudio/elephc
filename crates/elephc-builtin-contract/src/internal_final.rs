//! Purpose:
//! Lists PHP-visible classes php-src declares `internal` and `final`, which is the pair
//! `ReflectionClass::newInstanceWithoutConstructor()` refuses to instantiate.
//!
//! Called from:
//! - `elephc::ir_lower::expr::reflection_class_calls` and the generated compatibility docs.
//!
//! Key details:
//! - elephc declares these classes from a prelude with source-like spans so their PHP-visible
//!   methods (including `__debugInfo()`) run through the ordinary user-class pipeline. That
//!   makes `ClassInfo::is_internal` false, so the reflection refusal has to key off this list
//!   instead of the class-table flag.
//! - Names are canonical PHP spellings without a leading separator.

/// Classes php-src declares both `internal` and `final` while elephc declares them in a prelude.
pub const INTERNAL_FINAL_CLASSES: &[&str] = &["Uri\\Rfc3986\\Uri", "Uri\\WhatWg\\Url"];
