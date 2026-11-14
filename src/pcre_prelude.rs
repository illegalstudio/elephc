//! Purpose:
//! Injects `preg_grep()` as a declared PHP function built on `preg_match()`.
//!
//! Called from:
//! - `crate::pipeline::compile()`, next to the version prelude and before name resolution,
//!   so a namespaced caller still resolves to the injected declaration.
//! - `crate::autoload::prelude_probe`, through `program_uses_surface`, for a call that only
//!   an autoloaded class file makes.
//!
//! Key details:
//! - Pay-for-use with a redeclaration guard, like `crate::version_prelude`: a program that
//!   never mentions `preg_grep` carries nothing, and a program declaring its own keeps it.
//! - The body is PHP's algorithm over the shared regex builtins, so every target and the
//!   regex runtime detection see ordinary `preg_match()` calls. The pattern is validated
//!   once up front (an invalid pattern is `false` even for an empty array); each entry is
//!   matched as its string value and keeps its key; a match error stops the scan and
//!   returns the entries collected so far with `preg_last_error()` still set, like php-src.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_cast, e_const, e_int, e_str, e_var, function,
    internal_declarations, s_array_assign, s_assign, s_break, s_foreach, s_if, s_return,
    t_array, t_union,
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
            // An invalid pattern fails before any entry is inspected.
            s_if(
                e_binop(
                    e_call("preg_match", vec![e_var("pattern"), e_str("")]),
                    BinOp::StrictEq,
                    e_bool(false),
                ),
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
            s_foreach(
                e_var("array"),
                Some("key"),
                "value",
                vec![
                    s_assign(
                        "matched",
                        e_call(
                            "preg_match",
                            vec![e_var("pattern"), e_cast(CastType::String, e_var("value"))],
                        ),
                    ),
                    s_if(
                        e_binop(matched(), BinOp::StrictEq, e_bool(false)),
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
            || crate::opcache_prelude::detect::program_declares(&program, "preg_grep")
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
