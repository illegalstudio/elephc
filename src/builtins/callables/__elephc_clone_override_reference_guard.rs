//! Purpose:
//! Home of the internal `__elephc_clone_override_reference_guard` builtin: php 8.5's refusal
//! of a `clone($object, $withProperties)` entry whose value still belongs to a PHP reference
//! set, evaluated for ONE entry at the moment the applicator reaches it.
//!
//! Called from:
//! - The generated clone override applicator body (`src/ir_lower/clone_overrides/body.rs`),
//!   once per loop iteration, after the key is stringified and NUL-checked and before the
//!   property write that entry resolves to.
//!
//! Key details:
//! - `internal: true`: never PHP-visible. The refusal is part of `clone()`'s own semantics,
//!   so there is no PHP-level counterpart to alias.
//! - Per entry, not per array: php applies every earlier override first and only throws when
//!   iteration REACHES the referenced entry, so the whole-array pre-scan this replaces threw
//!   too early and dropped writes php had already performed.
//! - The third argument is the applicator loop's own value local. The by-value `foreach`
//!   element load retains a tag-7 entry's boxed Mixed cell, so the guard discounts that one
//!   borrow instead of reading it as a second owner.

builtin! {
    contract: "__elephc_clone_override_reference_guard",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcCloneOverrideReferenceGuard,
    ),
}
