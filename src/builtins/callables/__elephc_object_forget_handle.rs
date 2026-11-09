//! Purpose:
//! Home of the internal `__elephc_object_forget_handle` builtin: returns a live object's
//! `spl_object_id()` handle to the pool, so an engine-side bookkeeping object stays invisible
//! to PHP's object numbering.
//!
//! Called from:
//! - The legacy DOM prelude (`crate::dom_html_prelude::legacy`), whose node identity cache
//!   holds one `WeakReference` per wrapper: php-src's DOM keeps that link in C and allocates
//!   no PHP object for it, so `object(DOMElement)#N` must not count the weak reference.
//!
//! Key details:
//! - `internal: true`: never PHP-visible.
//! - Calls `__rt_object_handle_release`, which clears the side-table slot and pushes the
//!   handle on the LIFO free stack; the later heap free finds handle 0 and skips release, so
//!   a handle is never returned twice. The object reads `#0` afterwards.
//! - A non-object operand is a no-op; the result is always `true`.

builtin! {
    contract: "__elephc_object_forget_handle",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcObjectForgetHandle,
    ),
}
