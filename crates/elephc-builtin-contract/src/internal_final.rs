//! Purpose:
//! Lists PHP-visible classes php-src declares `internal` and `final`, which is the pair
//! `ReflectionClass::newInstanceWithoutConstructor()` refuses to instantiate, plus the
//! prelude classes php-src additionally declares `readonly`.
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

/// Classes php-src declares both `internal` and `readonly` while elephc declares them in a
/// prelude. `readonly` carries the engine's no-dynamic-properties flag, so an undeclared write
/// compiles but raises `Cannot create dynamic property C::$p` at runtime.
pub const INTERNAL_READONLY_CLASSES: &[&str] = &["Uri\\Rfc3986\\Uri", "Uri\\WhatWg\\Url"];

/// Returns whether `class_name` is one of the prelude classes php-src declares internal readonly.
pub fn is_internal_readonly_class(class_name: &str) -> bool {
    INTERNAL_READONLY_CLASSES
        .iter()
        .any(|name| class_name.trim_start_matches('\\').eq_ignore_ascii_case(name))
}
