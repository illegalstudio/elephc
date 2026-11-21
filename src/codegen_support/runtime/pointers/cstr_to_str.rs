//! Purpose:
//! Emits the `__rt_cstr_to_str`, `__rt_cstr_to_str_null` runtime helper assembly for C-string to PHP string conversion.
//! Keeps compiler pointer extension conversions and fatal checks aligned with generated pointer operations.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::pointers`.
//!
//! Key details:
//! - String helpers scan or transform byte ranges and return target ABI pointer/length pairs for generated call sites.

pub use elephc_memoir::interop::c_strings::emit_cstr_to_str;
