//! Purpose:
//! Raises an engine-level PHP deprecation from compiler-provided preludes.
//!
//! Called from:
//! - Version-gated fileinfo compatibility wrappers.
//! - Deprecated constant reads (`MT_RAND_PHP`, `DATE_RFC7231`, `DateTime::RFC7231`), which
//!   `crate::name_resolver` rewrites into an inline call at the read's span.
//!
//! Key details:
//! - Uses E_DEPRECATED and the ordinary error-handler and reporting machinery.

use super::core_support::core_builtin_home;
core_builtin_home!("__elephc_builtin_deprecated", BuiltinDeprecated);
