//! Purpose:
//! PHP's incremental-hashing object surface — the `HashContext` class plus the
//! `hash_init` / `hash_update` / `hash_final` / `hash_copy` functions — declared in Rust
//! through `crate::synthetic_class` on top of the internal `__elephc_hash_ctx_*` builtins.
//! PHP 8 migrated these from a resource to an object (the same migration GD made with
//! `GdImage`), and this prelude is what makes `hash_init()` return a real object
//! that `is_object`, `get_class`, `instanceof`, and `var_dump` all agree about.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`,
//!   after include resolution and before name resolution.
//!
//! Key details:
//! - WHY A PRELUDE AND NOT A NATIVE CLASS. Every hash-context `RuntimeFnId` declares
//!   `BuiltinRequirement::Bridge("elephc_crypto")` (`crate::ir::runtime_fn`), so the
//!   surface is bridge-gated: a program that never hashes must neither declare
//!   `HashContext` nor link `-lelephc_crypto`. Injecting only on demand preserves that,
//!   and the whole feature then compiles through the ordinary class/function pipeline
//!   with NO new assembly — so both architectures get it at the same time.
//! - WHY IT IS BUILT, NOT PARSED. The surface was PHP source text in a `&'static str`,
//!   tokenized and parsed on every compile that touched it. `synthetic_class` builds the
//!   SAME declaration nodes directly, so the pipeline still sees ordinary class and
//!   function declarations — collected, checked, lowered and emitted like user code —
//!   while the compiler stops carrying a PHP program inside a string literal. The MEMBER
//!   SET and the BODIES are transposed unchanged; this is a delivery-mechanism change, not
//!   a semantic one. `synthetic_class::parser_agreement` pins the builders to what the
//!   parser produces, which is what makes that claim checkable rather than asserted.
//! - WHY THE BUILTINS WERE RENAMED. A prelude function cannot shadow a builtin of the
//!   same name (`Cannot redeclare built-in function`), so the four raw builtins became
//!   `internal: true` `__elephc_hash_ctx_*` (see
//!   `crate::builtins::string::__elephc_hash_ctx_init`) and the PHP names are these
//!   wrappers.
//! - OWNERSHIP: the object holds the Mixed context cell in `$__elephc_ctx` and adds NO
//!   retain/release of its own. That cell already owns the native `elephc_crypto`
//!   context: the standard object-free path (`__rt_heap_free` → `object_free_deep`)
//!   releases property storage, which reaches `__rt_mixed_free_deep` and thence
//!   `__rt_hash_ctx_free`. `hash_copy()` boxes a *fresh* native context into a *fresh*
//!   cell in a *fresh* object, so each object frees exactly one context. There is
//!   deliberately no `__destruct`: adding one would introduce a second free path.
//! - `$__elephc_ctx` is public for the same reason `GdImage::$handle` is — the free
//!   functions read it — but it is hidden from `var_dump` because `__debugInfo()` below
//!   projects only `algo`, exactly as php-src's `HashContext` does.
//! - EVERY wrapper BINDS `$context->__elephc_ctx` TO A LOCAL (`$raw`) BEFORE CALLING.
//!   This is not style. Passing a `mixed` object property *inline* as a call argument
//!   trips a pre-existing codegen leak: the boxed temporary produced by the property
//!   read is never released, costing one heap block per call. It is not hash-specific
//!   and reproduces with no hashing at all —
//!   `class R { public mixed $m = null; } function f(R $r): string { return gettype($r->m); }`
//!   leaks one block per `f()` call under `--heap-debug`, while the identical body with
//!   `$v = $r->m;` first is `allocs == frees`. Binding to a local sidesteps it, so these
//!   wrappers are leak-free today; if that codegen bug is ever fixed the locals become
//!   redundant but stay harmless. `tests/codegen/runtime_gc/resource_scope_cleanup.rs`
//!   pins the clean-heap property so a careless "simplification" back to the inline form
//!   fails a test instead of silently leaking.
//! - The `flags`/`key` HMAC-streaming parameters of PHP's `hash_init()` remain
//!   unsupported (they were already blocked by the builtin's `max_args`). The wrapper
//!   keeps the one-parameter signature so passing them stays a COMPILE-TIME error
//!   (`Function 'hash_init' expects 1 arguments, got 3`) rather than being silently
//!   accepted. Use `hash_hmac()` for HMAC.
//! - `__serialize()` THROWS rather than returning a reduced array. PHP's `HashContext`
//!   really is serializable — it emits its full internal digest state
//!   (`O:11:"HashContext":5:{...}`) and round-trips — and elephc cannot reproduce that
//!   from the opaque bridge handle. Emitting a plausible-looking but state-free
//!   `O:11:"HashContext":1:{s:4:"algo";...}` would be the worst outcome: it looks like
//!   a serialized context and silently is not one. Failing loudly is the honest
//!   divergence, and `tests/resource_id_and_hash_context_tests.rs` pins it as a
//!   divergence rather than as parity.

mod detect;

use crate::parser::ast::{Program, TypeExpr};
use crate::synthetic_class::{
    class, e_array_assoc, e_bool, e_call, e_new_fq, e_new_self, e_null, e_prop, e_static_call,
    e_str, e_this_prop, e_var, function, internal_declarations, method, s_assign, s_prop_assign,
    s_return, s_throw, t_array, t_class, t_mixed,
};

/// The message `HashContext::__serialize()` throws with. Spelled out rather than
/// summarized: it is what a user sees when a `serialize()` of a hash context fails, and
/// `tests/resource_id_and_hash_context_tests.rs` matches on it.
const SERIALIZE_REFUSAL: &str =
    "Serialization of 'HashContext' is not supported: the native hashing state cannot be captured";

/// Builds the hash surface: the `HashContext` class and the four PHP functions that
/// produce and consume it.
pub(crate) fn hash_declarations() -> Program {
    internal_declarations(|| {
        vec![
            class("HashContext")
                .final_()
                .prop("algo", TypeExpr::Str, Some(e_str("")))
                .prop("__elephc_ctx", t_mixed(), Some(e_null()))
                // Private: a context is minted by `hash_init()`, never by `new`.
                .method(method("__construct").private())
                .method(
                    method("__elephc_wrap")
                        .static_()
                        .param("algo", TypeExpr::Str)
                        .param("raw", t_mixed())
                        .returns(t_class("HashContext"))
                        .body(vec![
                            s_assign("ctx", e_new_self(vec![])),
                            s_prop_assign(e_var("ctx"), "algo", e_var("algo")),
                            s_prop_assign(e_var("ctx"), "__elephc_ctx", e_var("raw")),
                            s_return(e_var("ctx")),
                        ]),
                )
                // Projects only `algo`, hiding the opaque handle from `var_dump`.
                .method(
                    method("__debugInfo")
                        .returns(t_array())
                        .returning(e_array_assoc(vec![(e_str("algo"), e_this_prop("algo"))])),
                )
                // Throws rather than emitting a state-free context — see the module header.
                .method(
                    method("__serialize")
                        .returns(t_array())
                        .body(vec![s_throw(e_new_fq(
                            "Exception",
                            vec![e_str(SERIALIZE_REFUSAL)],
                        ))]),
                )
                .build(),
            function("hash_init")
                .param("algo", TypeExpr::Str)
                .returns(t_class("HashContext"))
                .returning(e_static_call(
                    "HashContext",
                    "__elephc_wrap",
                    vec![
                        e_var("algo"),
                        e_call("__elephc_hash_ctx_init", vec![e_var("algo")]),
                    ],
                ))
                .build(),
            function("hash_update")
                .param("context", t_class("HashContext"))
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_prop(e_var("context"), "__elephc_ctx")),
                    s_return(e_call(
                        "__elephc_hash_ctx_update",
                        vec![e_var("raw"), e_var("data")],
                    )),
                ])
                .build(),
            function("hash_final")
                .param("context", t_class("HashContext"))
                .param_default("binary", TypeExpr::Bool, e_bool(false))
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("raw", e_prop(e_var("context"), "__elephc_ctx")),
                    s_return(e_call(
                        "__elephc_hash_ctx_final",
                        vec![e_var("raw"), e_var("binary")],
                    )),
                ])
                .build(),
            function("hash_copy")
                .param("context", t_class("HashContext"))
                .returns(t_class("HashContext"))
                .body(vec![
                    s_assign("raw", e_prop(e_var("context"), "__elephc_ctx")),
                    s_return(e_static_call(
                        "HashContext",
                        "__elephc_wrap",
                        vec![
                            e_prop(e_var("context"), "algo"),
                            e_call("__elephc_hash_ctx_copy", vec![e_var("raw")]),
                        ],
                    )),
                ])
                .build(),
        ]
    })
}

/// Injects the hash prelude when the program references the incremental-hashing
/// surface, leaving every other program untouched.
///
/// `force` comes from an explicit opt-in (the codegen harness); otherwise the
/// decision is `detect::program_uses_hash_context`. The prelude carries only
/// declarations, so prepending it is order-independent — PHP hoists them.
pub fn inject_if_used(program: crate::parser::ast::Program, force: bool) -> crate::parser::ast::Program {
    if !force && !detect::program_uses_hash_context(&program) {
        return program;
    }
    let mut combined = hash_declarations();
    combined.extend(program);
    combined
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// The surface is fixed: the class first, then the four streaming functions. A member
    /// disappearing silently is exactly the failure the PHP-text form could not catch, since
    /// a typo there produced an error only when the text happened to be malformed.
    #[test]
    fn declares_the_class_and_the_four_functions() {
        let declared: Vec<String> = hash_declarations()
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::ClassDecl { name, .. } => Some(name.clone()),
                StmtKind::FunctionDecl { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            declared,
            vec![
                "HashContext",
                "hash_init",
                "hash_update",
                "hash_final",
                "hash_copy",
            ]
        );
    }

    /// `hash_init` keeps its ONE-parameter signature so PHP's `flags`/`key` HMAC-streaming
    /// arguments stay a compile-time error instead of being silently accepted.
    #[test]
    fn hash_init_takes_exactly_one_parameter() {
        let init = hash_declarations()
            .into_iter()
            .find(|stmt| matches!(&stmt.kind, StmtKind::FunctionDecl { name, .. } if name == "hash_init"))
            .expect("hash_init must be declared");
        let StmtKind::FunctionDecl { params, .. } = &init.kind else {
            unreachable!("filtered above");
        };
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].0, "algo");
    }

    /// Every wrapper must bind the context property to a LOCAL before passing it to a
    /// builtin. Passing the `mixed` property inline leaks one heap block per call — see the
    /// module header. A "simplification" that inlines the read would pass a naive shape test
    /// but reintroduce the leak, so the binding is asserted structurally.
    #[test]
    fn wrappers_bind_the_context_property_to_a_local() {
        for stmt in hash_declarations() {
            let StmtKind::FunctionDecl { name, body, .. } = &stmt.kind else {
                continue;
            };
            if name == "hash_init" {
                continue; // Mints the context; it has no context parameter to read.
            }
            assert!(
                matches!(&body[0].kind, StmtKind::Assign { name, .. } if name == "raw"),
                "{} must bind $context->__elephc_ctx to a local first",
                name
            );
        }
    }

    /// The constructor is private: a context is minted by `hash_init()`, never by `new`.
    #[test]
    fn the_constructor_is_private() {
        let class_decl = hash_declarations()
            .into_iter()
            .find(|stmt| matches!(&stmt.kind, StmtKind::ClassDecl { name, .. } if name == "HashContext"))
            .expect("HashContext must be declared");
        let StmtKind::ClassDecl { methods, is_final, .. } = &class_decl.kind else {
            unreachable!("filtered above");
        };
        assert!(*is_final, "HashContext is final");
        let ctor = methods
            .iter()
            .find(|m| m.name == "__construct")
            .expect("HashContext must declare a constructor");
        assert_eq!(ctor.visibility, crate::parser::ast::Visibility::Private);
    }
}
