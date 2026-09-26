//! Purpose:
//! Home of the internal `__elephc_object_prop_name` builtin: the bare name of an
//! object's Nth renderable property.
//!
//! Called from:
//! - The injected `var_export` prelude (`src/var_export_prelude.rs`), which quotes
//!   the name as the `'key' =>` part of a `__set_state(...)` / `(object) array(...)`
//!   entry.
//!
//! Key details:
//! - `internal: true`: never PHP-visible.
//! - Returns the BARE property name — `var_export` never annotates visibility,
//!   unlike `print_r`'s `x:protected`. Both spellings live in the same descriptor
//!   row, so they are one edit apart and cannot drift.
//! - Returns the EMPTY string for an out-of-range index, a non-object value, or a
//!   typed property that is still uninitialized. PHP omits uninitialized typed
//!   properties from `var_export` output, and a real property name is never empty,
//!   so the prelude simply skips empty names.
//! - Indices past the declared rows name the instance's dynamic properties in
//!   insertion order (see `runtime::objects::export_props`).

builtin! {
    contract: "__elephc_object_prop_name",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcObjectPropName,
    ),
}
