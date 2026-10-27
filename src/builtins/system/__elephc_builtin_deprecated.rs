//! Purpose:
//! Raises an engine-level PHP deprecation from compiler-provided preludes.
//!
//! Called from:
//! - Version-gated fileinfo compatibility wrappers.
//!
//! Key details:
//! - Uses E_DEPRECATED and the ordinary error-handler and reporting machinery.

use super::core_support::core_builtin_home;
core_builtin_home!("__elephc_builtin_deprecated", BuiltinDeprecated);
