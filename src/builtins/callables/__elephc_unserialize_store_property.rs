//! Purpose:
//! Home of the internal `__elephc_unserialize_store_property` hydration helper: its
//! declaration, checker contract, and semantic target. Compiler-synthesized; not
//! PHP-visible.
//!
//! Called from:
//! - The synthetic `DateTime`/`DateTimeImmutable::__unserialize()` body, which restores
//!   subclass properties the hook itself does not own.
//!
//! Key details:
//! - The target is the decoder's `__rt_obj_store_prop` helper, so visibility, mangled
//!   private/protected keys and declared-type checks match ordinary property injection.

builtin! {
    contract: "__elephc_unserialize_store_property",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcUnserializeStoreProperty,
    ),
}
