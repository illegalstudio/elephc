//! Purpose:
//! Home of the internal `__elephc_clone_apply_one` builtin: applies ONE `clone($object,
//! $withProperties)` entry to the clone, through the generated per-class applicator selected by
//! the clone's runtime class and the invocation scope.
//!
//! Called from:
//! - The generated clone override driver (`src/ir_lower/clone_overrides/body.rs`), once per loop
//!   iteration, after the key is stringified, NUL-checked and reference-checked.
//!
//! Key details:
//! - `internal: true`: never PHP-visible.
//! - The driver owns the iteration, so the loop machinery exists ONCE per module instead of once
//!   per `(class, scope)` applicator; the per-class applicator keeps only the name dispatch.

builtin! {
    contract: "__elephc_clone_apply_one",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcCloneApplyOne,
    ),
}
