//! Purpose:
//! Injects `preg_grep()` as a declared PHP function that compiles its pattern once and
//! matches each entry through the internal compiled-pattern builtins.
//!
//! Called from:
//! - `crate::pipeline::compile()`, next to the version prelude and before name resolution,
//!   so a namespaced caller still resolves to the injected declaration.
//! - `crate::autoload::prelude_probe`, through `program_uses_surface`, for a call that only
//!   an autoloaded class file makes.
//!
//! Key details:
//! - Pay-for-use with a redeclaration guard, like `crate::version_prelude`: a program that
//!   never mentions `preg_grep` carries nothing, and a program declaring its own global
//!   `preg_grep` keeps it. A namespaced `App\preg_grep` does not suppress the injection.
//! - The body follows php-src's `preg_grep()`. `__elephc_preg_compile()` resets the
//!   `preg_last_error()` state once and compiles the pattern once; an invalid pattern
//!   returns `false` with `PREG_INTERNAL_ERROR` before any entry is converted, even for an
//!   empty array. Each entry is matched as its string value by
//!   `__elephc_preg_match_compiled()` and keeps its key; the handle is freed in a `finally`
//!   block, so a throwing `__toString()` does not leak it.
//! - The per-entry match does not reset the error state. A nested `preg_*` error an entry's
//!   `__toString()` records therefore stays in `preg_last_error()` after the call, and a
//!   later nested call that succeeds clears it, as in php-src. A match error stops the scan
//!   and returns the entries collected so far, like php-src before 8.6. From PHP 8.6
//!   (php-src GH-11936) an error left in `preg_last_error()` makes the call return `false`.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_cast, e_const, e_int, e_var, function,
    internal_declarations, s_array_assign, s_assign, s_break, s_expr, s_foreach, s_if,
    s_return, s_try, t_array, t_union,
};

/// `preg_grep(string $pattern, array $array, int $flags = 0): array|false`.
fn preg_grep_decl() -> Stmt {
    let matched = || e_var("matched");
    function("preg_grep")
        .param("pattern", TypeExpr::Str)
        .param("array", t_array())
        .param_default("flags", TypeExpr::Int, e_int(0))
        .returns(t_union(vec![t_array(), TypeExpr::False]))
        .body(vec![
            // Resets the error state once per call and compiles the pattern once. An invalid
            // pattern fails before any entry is inspected.
            s_assign("regex", e_call("__elephc_preg_compile", vec![e_var("pattern")])),
            s_if(
                e_binop(e_var("regex"), BinOp::StrictEq, e_int(0)),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_assign(
                "invert",
                e_binop(
                    e_binop(e_var("flags"), BinOp::BitAnd, e_const("PREG_GREP_INVERT")),
                    BinOp::StrictNotEq,
                    e_int(0),
                ),
            ),
            s_assign("result", e_array(vec![])),
            s_try(
                vec![s_foreach(
                    e_var("array"),
                    Some("key"),
                    "value",
                    vec![
                        s_assign(
                            "matched",
                            e_call(
                                "__elephc_preg_match_compiled",
                                vec![e_var("regex"), e_cast(CastType::String, e_var("value"))],
                            ),
                        ),
                        // A match error is recorded in `preg_last_error()` and stops the scan.
                        s_if(
                            e_binop(matched(), BinOp::Lt, e_int(0)),
                            vec![s_break(1)],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(
                                e_binop(matched(), BinOp::StrictEq, e_int(1)),
                                BinOp::StrictNotEq,
                                e_var("invert"),
                            ),
                            vec![s_array_assign("result", e_var("key"), e_var("value"))],
                            vec![],
                            None,
                        ),
                    ],
                )],
                vec![],
                Some(vec![s_expr(e_call("__elephc_preg_free", vec![e_var("regex")]))]),
            ),
            // PHP 8.6 (php-src GH-11936) returns false instead of the partial result when the
            // call's own match or an entry's conversion left an error.
            s_if(
                e_binop(
                    e_binop(e_const("PHP_VERSION_ID"), BinOp::GtEq, e_int(80600)),
                    BinOp::And,
                    e_binop(
                        e_call("preg_last_error", vec![]),
                        BinOp::StrictNotEq,
                        e_const("PREG_NO_ERROR"),
                    ),
                ),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_return(e_var("result")),
        ])
        .build()
}

/// Whether `program` calls `preg_grep()`: the usage half of the test `inject_if_used` applies.
///
/// Also run by `crate::autoload::prelude_probe` over the autoload closure, whose files are only
/// loaded after this prelude has been decided.
pub(crate) fn program_uses_surface(program: &[Stmt]) -> bool {
    crate::opcache_prelude::detect::program_references(program, "preg_grep")
}

/// Prepends `preg_grep()` when the program (or, with `autoloaded`, a file of its autoload
/// closure) references it and the program does not declare it.
///
/// Runs under `crate::compiler_stack::with_compiler_stack` like the version prelude, whose
/// usage scan (`opcache_prelude::detect`) it shares.
pub fn inject_if_used(
    program: Program,
    autoloaded: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        if !(autoloaded || program_uses_surface(&program))
            || crate::opcache_prelude::detect::program_declares_global(&program, "preg_grep")
        {
            return program;
        }
        let mut combined = pcre_declarations();
        inventory.record_program("pcre", &combined);
        combined.extend(program);
        combined
    })
}

/// Builds the injected PCRE declarations.
pub(crate) fn pcre_declarations() -> Program {
    internal_declarations(|| vec![preg_grep_decl()])
}
