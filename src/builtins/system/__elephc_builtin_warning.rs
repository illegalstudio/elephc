//! Purpose:
//! Raises an engine-level PHP warning from a compiler-provided prelude.
//!
//! Called from:
//! - Internal prelude failure paths, including fileinfo.
//!
//! Key details:
//! - E_WARNING reaches the active PHP handler and reporting mask through typed Core EIR.
//! - This internal alias never appears in the PHP-visible function catalog.

use super::core_support::core_builtin_home;
core_builtin_home!("__elephc_builtin_warning", BuiltinWarning);
