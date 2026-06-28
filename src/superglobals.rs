//! Purpose:
//! The canonical set of HTTP-request superglobals exposed under `--web`, and the
//! shared PhpType for them. Single source of truth consumed by the type checker,
//! the IR lowering global-storage path, and `__rt_web_reset`.
//!
//! Called from:
//! - `crate::types::checker` (seeding), `crate::ir_lower::context` (global
//!   storage), `crate::codegen_ir::web` (per-request reset).
//!
//! Key details:
//! - These names use `_eir_global_*` symbol storage in EVERY scope (true
//!   superglobals), unlike `$argc`/`$argv` which are top-level only.

use crate::types::PhpType;

/// PHP request superglobals visible in every scope under `--web`.
pub const SUPERGLOBALS: &[&str] =
    &["_SERVER", "_GET", "_POST", "_COOKIE", "_REQUEST", "_ENV", "_FILES"];

/// Returns true when `name` (without leading `$`) is a request superglobal.
pub fn is_superglobal(name: &str) -> bool {
    SUPERGLOBALS.contains(&name)
}

/// The shared type of every request superglobal: a string-keyed associative
/// array of heterogeneous (Mixed) values.
pub fn superglobal_type() -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    }
}
