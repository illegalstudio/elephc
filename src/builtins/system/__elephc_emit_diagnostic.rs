//! Purpose:
//! Register internal prelude diagnostics through the typed Core EIR handler dispatcher.
//!
//! Called from:
//! - `crate::builtins::system` during builtin inventory collection.
//!
//! Key details:
//! - Engine warning/notice/deprecation levels retain their original handler-mask bits.
//! - This compiler-internal helper is unavailable to Magician's PHP surface.

use super::core_support::core_builtin_home;

core_builtin_home!("__elephc_emit_diagnostic", EmitDiagnostic);
