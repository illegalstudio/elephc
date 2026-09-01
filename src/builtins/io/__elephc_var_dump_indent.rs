//! Purpose:
//! Home of the internal `__elephc_var_dump_indent` builtin used by synthetic
//! ext/date debug renderers to share the recursive runtime indentation state.
//!
//! Called from:
//! - Synthetic date/time `__elephc_debug_dump()` method bodies.
//!
//! Key details:
//! - `$delta` adjusts the runtime indent before the current value is returned.
//! - The builtin stays internal because `_vd_indent` is compiler runtime state,
//!   not part of PHP's public function surface.

builtin! {
    contract: "__elephc_var_dump_indent",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcVarDumpIndent,
    ),
}
