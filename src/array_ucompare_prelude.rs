//! Purpose:
//! Declares php's user-comparator set functions the compiler has no native lowering for —
//! `array_udiff_assoc`, `array_udiff_uassoc`, `array_diff_ukey`, `array_diff_uassoc`,
//! `array_uintersect_assoc`, `array_uintersect_uassoc`, `array_intersect_ukey`,
//! `array_intersect_uassoc` — as AST-built functions over one shared helper.
//!
//! Called from:
//! - `crate::pipeline::compile()` in the compat-prelude phase (after autoload expansion, so a
//!   call only an autoloaded class makes is seen) and the codegen test harness.
//!
//! Key details:
//! - Every function asks the same question: keep an entry of the first array when some other
//!   array holds a match (diff drops it) or when every other array does (intersect keeps it).
//!   A match compares the value and/or the key, each by php's string comparison or by a user
//!   comparator answering `0` for equal. Keys and values of the first array are preserved.
//! - The comparators are the trailing arguments, the value comparator before the key
//!   comparator, as in php; the wrappers pop them off the variadic tail.
//! - The interpreter has its own implementation
//!   (`elephc_magician::interpreter::builtins::array::array_ucompare`): Symfony's
//!   `UrlGenerator::doGenerate()` calls `array_udiff_assoc()` for every generated URL, and in an
//!   eval-heavy build it runs interpreted.
//! - Pay-for-use: injected only when the program names one of the functions, and never over a
//!   declaration of the same name the program makes itself.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, StmtKind, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_cast, e_index, e_int, e_not, e_null,
    e_ternary, e_var, function, php_semantics_declarations, s_array_assign, s_assign, s_break,
    s_continue, s_expr, s_foreach, s_if, s_return, t_array, t_mixed,
};

/// Prelude inventory group holding the declarations.
pub(crate) const GROUP: &str = "array_ucompare";

/// The shared helper every wrapper calls.
const HELPER: &str = "__elephc_array_ucompare";

/// Comparison mode: this side of an entry is not compared.
const IGNORED: i64 = 0;
/// Comparison mode: php's string comparison.
const BUILTIN: i64 = 1;
/// Comparison mode: a user comparator.
const USER: i64 = 2;

/// Each PHP function this prelude declares, with (intersect, value mode, key mode).
const FAMILY: &[(&str, bool, i64, i64)] = &[
    ("array_diff_uassoc", false, BUILTIN, USER),
    ("array_diff_ukey", false, IGNORED, USER),
    ("array_intersect_uassoc", true, BUILTIN, USER),
    ("array_intersect_ukey", true, IGNORED, USER),
    ("array_udiff_assoc", false, USER, BUILTIN),
    ("array_udiff_uassoc", false, USER, USER),
    ("array_uintersect_assoc", true, USER, BUILTIN),
    ("array_uintersect_uassoc", true, USER, USER),
];

/// The PHP functions this prelude declares.
pub(crate) fn functions() -> impl Iterator<Item = &'static str> {
    FAMILY.iter().map(|(name, ..)| *name)
}

/// `$matched = <value comparison of $value and $candidate>;`
fn s_match_value() -> Stmt {
    s_if(
        e_binop(e_var("valueMode"), BinOp::StrictEq, e_int(IGNORED)),
        vec![s_assign("matched", e_bool(true))],
        vec![(
            e_binop(e_var("valueMode"), BinOp::StrictEq, e_int(BUILTIN)),
            vec![s_assign(
                "matched",
                e_binop(
                    e_cast(CastType::String, e_var("value")),
                    BinOp::StrictEq,
                    e_cast(CastType::String, e_var("candidate")),
                ),
            )],
        )],
        Some(vec![s_assign(
            "matched",
            e_binop(
                e_call("call_user_func", vec![e_var("valueCb"), e_var("value"), e_var("candidate")]),
                BinOp::Eq,
                e_int(0),
            ),
        )]),
    )
}

/// function __elephc_array_ucompare(array $arrays, int $valueMode, mixed $valueCb,
///                                  int $keyMode, mixed $keyCb, bool $intersect): array
fn helper() -> Stmt {
    // With a built-in key comparison the candidate is a direct lookup.
    let by_key_lookup = vec![s_if(
        e_call("array_key_exists", vec![e_var("key"), e_var("other")]),
        vec![
            s_assign("candidate", e_index(e_var("other"), e_var("key"))),
            s_match_value(),
        ],
        Vec::new(),
        None,
    )];
    // Otherwise every entry of the other array is a candidate.
    let by_scan = vec![s_foreach(
        e_var("other"),
        Some("otherKey"),
        "candidate",
        vec![
            s_if(
                e_binop(
                    e_binop(e_var("keyMode"), BinOp::StrictEq, e_int(USER)),
                    BinOp::And,
                    e_binop(
                        e_call("call_user_func", vec![e_var("keyCb"), e_var("key"), e_var("otherKey")]),
                        BinOp::NotEq,
                        e_int(0),
                    ),
                ),
                vec![s_continue(1)],
                Vec::new(),
                None,
            ),
            s_match_value(),
            s_if(e_var("matched"), vec![s_break(1)], Vec::new(), None),
        ],
    )];
    let per_other = vec![
        s_assign("matched", e_bool(false)),
        s_if(
            e_binop(e_var("keyMode"), BinOp::StrictEq, e_int(BUILTIN)),
            by_key_lookup,
            Vec::new(),
            Some(by_scan),
        ),
        s_if(
            e_var("matched"),
            vec![s_assign("somewhere", e_bool(true))],
            Vec::new(),
            Some(vec![s_assign("everywhere", e_bool(false))]),
        ),
    ];
    let per_entry = vec![
        s_assign("everywhere", e_bool(true)),
        s_assign("somewhere", e_bool(false)),
        s_foreach(e_var("arrays"), None, "other", per_other),
        s_if(
            e_ternary(
                e_var("intersect"),
                e_var("everywhere"),
                e_not(e_var("somewhere")),
            ),
            vec![s_array_assign("result", e_var("key"), e_var("value"))],
            Vec::new(),
            None,
        ),
    ];
    function(HELPER)
        .param("arrays", t_array())
        .param("valueMode", TypeExpr::Int)
        .param("valueCb", t_mixed())
        .param("keyMode", TypeExpr::Int)
        .param("keyCb", t_mixed())
        .param("intersect", TypeExpr::Bool)
        .returns(t_array())
        .body(vec![
            s_assign("first", e_call("array_shift", vec![e_var("arrays")])),
            s_assign("result", e_array(Vec::new())),
            s_foreach(e_var("first"), Some("key"), "value", per_entry),
            s_return(e_var("result")),
        ])
        .build()
}

/// One public wrapper: pops its comparator(s) off the variadic tail and calls the helper.
fn wrapper(name: &str, intersect: bool, value_mode: i64, key_mode: i64) -> Stmt {
    let mut body = Vec::new();
    // The key comparator is the last argument, the value comparator the one before it.
    if key_mode == USER {
        body.push(s_assign("keyCb", e_call("array_pop", vec![e_var("rest")])));
    }
    if value_mode == USER {
        body.push(s_assign("valueCb", e_call("array_pop", vec![e_var("rest")])));
    }
    body.push(s_expr(e_call(
        "array_unshift",
        vec![e_var("rest"), e_var("array")],
    )));
    let callback = |mode: i64, var: &str| if mode == USER { e_var(var) } else { e_null() };
    body.push(s_return(e_call(
        HELPER,
        vec![
            e_var("rest"),
            e_int(value_mode),
            callback(value_mode, "valueCb"),
            e_int(key_mode),
            callback(key_mode, "keyCb"),
            e_bool(intersect),
        ],
    )));
    function(name)
        .param("array", t_array())
        // Typed `mixed` on purpose: an untyped variadic takes its element type from the call
        // site, and `[$array2, $callback]` would type the popped comparator as an array.
        .variadic("rest", Some(t_mixed()))
        .returns(t_array())
        .body(body)
        .build()
}

/// Builds the helper and the eight wrappers.
fn function_declarations() -> Vec<Stmt> {
    let mut declarations = vec![helper()];
    for (name, intersect, value_mode, key_mode) in FAMILY {
        declarations.push(wrapper(name, *intersect, *value_mode, *key_mode));
    }
    declarations
}

/// Returns the declarations to inject.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(function_declarations)
}

/// Returns whether a top-level, namespace-less, guarded or synthetic statement declares the
/// function `name`.
fn program_declares_function(program: &[Stmt], name: &str) -> bool {
    program.iter().any(|stmt| match &stmt.kind {
        StmtKind::FunctionDecl { name: declared, .. } => declared.eq_ignore_ascii_case(name),
        StmtKind::NamespaceBlock { name: None, body }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => program_declares_function(body, name),
        _ => false,
    })
}

/// Prepends the declarations when the program names one of the functions and declares none of
/// them itself.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    let used = crate::prelude_prune::usage::collect(&program);
    let names_function = functions().any(|name| {
        crate::opcache_prelude::detect::program_references(&program, name)
            || used.literals.contains(&crate::names::php_symbol_key(name))
    });
    if !names_function
        || functions().any(|name| program_declares_function(&program, name))
        || program_declares_function(&program, HELPER)
    {
        return program;
    }
    let mut combined = declarations();
    inventory.record_program(GROUP, &combined);
    combined.extend(program);
    combined
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The surface is the helper plus the eight wrappers.
    #[test]
    fn declares_the_helper_and_eight_wrappers() {
        let names: Vec<String> = declarations()
            .into_iter()
            .filter_map(|stmt| match stmt.kind {
                StmtKind::FunctionDecl { name, .. } => Some(name),
                _ => None,
            })
            .collect();
        let mut expected = vec![HELPER.to_string()];
        expected.extend(functions().map(str::to_string));
        assert_eq!(names, expected);
    }
}
