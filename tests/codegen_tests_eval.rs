//! Purpose:
//! End-to-end codegen test binary for `eval()` and the Magician interpreter bridge.
//!
//! Called from:
//! - `cargo test --test codegen_tests_eval` through Rust's test harness.
//!
//! Key details:
//! - The codegen suite is split into six `codegen_tests_*` binaries by domain so a library edit
//!   recompiles them in parallel; test names keep the `codegen::<module>::` path, and CI selects
//!   the family with `binary(/^codegen_tests_/)`.
//! - Each binary mounts the shared runner helpers; the ones a domain never calls are expected.

#[allow(dead_code)]
#[path = "support/managed_pcre2.rs"]
mod managed_pcre2_support;

#[allow(dead_code, unused_imports, unused_macros)]
#[path = "codegen/support/mod.rs"]
mod support;

#[allow(dead_code)]
#[path = "support/locals_retype_fixtures.rs"]
mod locals_retype_fixtures;

mod codegen {
    mod eval;
    mod eval_generics;
    mod eval_builtin_parity;
    mod eval_callable_ref_errors;
    mod eval_callables;
    mod eval_callback_receiver_ownership;
    mod eval_closures;
    mod eval_constructors;
    mod eval_reflection_invocation;
    mod eval_global_scope;
    mod eval_escape_guard;
    mod eval_scope_access_mask;
    mod eval_native_catalog;
    mod eval_undefined_functions;
    mod eval_deferred_catalog;
}
