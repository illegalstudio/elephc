//! Purpose:
//! PHP's `random` extension surface, built through the ordinary synthetic AST pipeline:
//! the `Random\Engine`/`CryptoSafeEngine` interfaces, the four engine classes,
//! `Random\Randomizer`, `Random\IntervalBoundary`, the exception hierarchy, and the
//! procedural `mt_*`/`lcg_value`/`random_bytes` functions.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness through `inject_if_used`,
//!   after include resolution and before name resolution.
//!
//! Key details:
//! - Every declaration is built in Rust — no PHP text is parsed outside tests. Engine
//!   algorithms live in the `elephc-random` bridge and are reached through the
//!   `extern "elephc_random"` declarations below, which also make the bridge a required
//!   library exactly when the surface is used.
//! - Injection is pay-for-use: a program that never names the `Random\` namespace or one of
//!   the procedural functions does not carry the prelude and does not link the bridge.
//! - Engine objects own one opaque native handle; `Randomizer` stays in PHP so a user class
//!   implementing `Random\Engine` is dispatched through the ordinary interface vtable.

use crate::parser::ast::{
    BinOp, CastType, CType, Expr, Program, Stmt, StmtKind, TypeExpr,
};
use crate::span::Span;
use crate::synthetic_class::*;

mod engines;
mod randomizer;
mod types;

/// The prelude group id used by declaration reachability and `--with-random`.
pub(crate) const PRELUDE_GROUP: &str = "random";

/// Name of the prelude helper every AOT `array_rand()` call form lowers onto.
pub(crate) const ARRAY_RAND_HELPER: &str = "__elephc_array_rand";

/// Builds the complete random-extension declaration set.
pub(crate) fn random_declarations() -> Program {
    internal_declarations(|| {
        let mut declarations = bridge_externs();
        declarations.extend(procedural_functions());
        declarations.extend(types::declarations());
        declarations.extend(engines::declarations());
        declarations.push(randomizer::declaration());
        declarations
    })
}

/// Declares the bridge symbols the prelude calls.
fn bridge_externs() -> Program {
    let externs: &[(&str, &[(&str, CType)], CType)] = &[
        (
            "elephc_random_engine_new",
            &[
                ("kind", CType::Int),
                ("seed_kind", CType::Int),
                ("seed_int", CType::Int),
                ("seed_hex", CType::Str),
                ("mode", CType::Int),
            ],
            CType::Int,
        ),
        ("elephc_random_engine_free", &[("handle", CType::Int)], CType::Void),
        (
            "elephc_random_engine_clone",
            &[("handle", CType::Int)],
            CType::Int,
        ),
        (
            "elephc_random_engine_generate",
            &[("handle", CType::Int)],
            CType::Int,
        ),
        (
            "elephc_random_engine_size",
            &[("handle", CType::Int)],
            CType::Int,
        ),
        (
            "elephc_random_engine_count",
            &[("handle", CType::Int)],
            CType::Int,
        ),
        (
            "elephc_random_engine_mode",
            &[("handle", CType::Int)],
            CType::Int,
        ),
        (
            "elephc_random_engine_jump",
            &[("handle", CType::Int), ("advance", CType::Int)],
            CType::Void,
        ),
        (
            "elephc_random_engine_jump_short",
            &[("handle", CType::Int)],
            CType::Void,
        ),
        (
            "elephc_random_engine_jump_long",
            &[("handle", CType::Int)],
            CType::Void,
        ),
        (
            "elephc_random_engine_state_hex",
            &[("handle", CType::Int)],
            CType::Str,
        ),
        (
            "elephc_random_engine_restore_hex",
            &[
                ("handle", CType::Int),
                ("state_hex", CType::Str),
                ("count", CType::Int),
                ("mode", CType::Int),
            ],
            CType::Int,
        ),
        (
            "elephc_random_mt_seed",
            &[
                ("seed", CType::Int),
                ("seed_is_null", CType::Int),
                ("mode", CType::Int),
            ],
            CType::Void,
        ),
        ("elephc_random_lcg_value", &[], CType::Float),
        ("elephc_random_bytes_hex", &[("length", CType::Int)], CType::Str),
        ("elephc_random_mt_uniform", &[("bound", CType::Int)], CType::Int),
        (
            "elephc_random_mt_legacy_scale",
            &[("min", CType::Int), ("max", CType::Int), ("draw", CType::Int)],
            CType::Int,
        ),
        ("elephc_random_csprng_failed", &[], CType::Int),
        ("elephc_random_next_up", &[("value", CType::Float)], CType::Float),
        (
            "elephc_random_next_down",
            &[("value", CType::Float)],
            CType::Float,
        ),
        ("elephc_random_float_repr", &[("value", CType::Float)], CType::Str),
    ];

    externs
        .iter()
        .map(|(name, params, return_type)| {
            let mut builder = extern_fn(name, "elephc_random").returns(return_type.clone());
            for (param, c_type) in *params {
                builder = builder.param(param, c_type.clone());
            }
            builder.build()
        })
        .collect()
}

/// Builds the procedural `mt_*`, `lcg_value` and `random_bytes` functions.
fn procedural_functions() -> Program {
    vec![
        function("mt_srand")
            .param_default("seed", t_nullable(TypeExpr::Int), e_null())
            .param_default("mode", TypeExpr::Int, e_int(0))
            .returns(TypeExpr::Void)
            .body(vec![
                if_then(
                    bin(var("mode"), BinOp::StrictEq, e_int(1)),
                    vec![expr_stmt(call(
                        "__elephc_builtin_deprecated",
                        vec![e_str("The MT_RAND_PHP variant of Mt19937 is deprecated")],
                    ))],
                ),
                expr_stmt(bridge(
                    "elephc_random_mt_seed",
                    vec![
                        e_null_coalesce(var("seed"), e_int(0)),
                        e_ternary(
                            bin(var("seed"), BinOp::StrictEq, e_null()),
                            e_int(1),
                            e_int(0),
                        ),
                        var("mode"),
                    ],
                )),
            ])
            .build(),
        function("srand")
            .param_default("seed", t_nullable(TypeExpr::Int), e_null())
            .param_default("mode", TypeExpr::Int, e_int(0))
            .returns(TypeExpr::Void)
            .body(vec![expr_stmt(call(
                "mt_srand",
                vec![var("seed"), var("mode")],
            ))])
            .build(),
        function("mt_getrandmax")
            .returns(TypeExpr::Int)
            .returning(e_int(2147483647))
            .build(),
        function("getrandmax")
            .returns(TypeExpr::Int)
            .returning(e_int(2147483647))
            .build(),
        // php-src parses `Mt19937`'s seed with `Z_PARAM_LONG_OR_NULL`: weak mode coerces
        // int-convertible scalars (with PHP's lossy-conversion deprecations), strict mode
        // rejects them, and `null` still requests a random seed. The constructing file's
        // `strict_types` state is known at the construction site, so it is published into the
        // runtime slot `__elephc_call_strict_types()` reads and passed in here for every call
        // shape (direct, named, spread, dynamically constructed class).
        function("__elephc_random_mt19937_seed")
            .param("seed", t_mixed())
            .param("strict", TypeExpr::Bool)
            .returns(t_mixed())
            .body(vec![
                if_then(
                    bin(var("seed"), BinOp::StrictEq, e_null()),
                    vec![ret(e_null())],
                ),
                if_then(
                    call("is_int", vec![var("seed")]),
                    vec![ret(var("seed"))],
                ),
                assign("_type", call("gettype", vec![var("seed")])),
                if_chain(
                    bin(var("_type"), BinOp::StrictEq, e_str("double")),
                    vec![assign("_type", e_str("float"))],
                    vec![(
                        bin(var("_type"), BinOp::StrictEq, e_str("boolean")),
                        vec![assign(
                            "_type",
                            e_ternary(var("seed"), e_str("true"), e_str("false")),
                        )],
                    )],
                    Some(vec![if_then(
                        call("is_object", vec![var("seed")]),
                        vec![assign("_type", call("get_class", vec![var("seed")]))],
                    )]),
                ),
                if_then(
                    var("strict"),
                    vec![throw_(mt19937_seed_type_error())],
                ),
                if_then(
                    call("is_bool", vec![var("seed")]),
                    vec![ret(cast(CastType::Int, var("seed")))],
                ),
                if_then(
                    call("is_float", vec![var("seed")]),
                    vec![
                        if_then(
                            mt19937_seed_float_out_of_range(var("seed")),
                            vec![throw_(mt19937_seed_type_error())],
                        ),
                        if_then(
                            bin(
                                cast(CastType::Float, cast(CastType::Int, var("seed"))),
                                BinOp::StrictNotEq,
                                var("seed"),
                            ),
                            vec![expr_stmt(mt19937_seed_deprecation(e_concat_all(vec![
                                e_str("Implicit conversion from float "),
                                bridge("elephc_random_float_repr", vec![var("seed")]),
                                e_str(" to int loses precision"),
                            ])))],
                        ),
                        ret(cast(CastType::Int, var("seed"))),
                    ],
                ),
                if_then(
                    call("is_string", vec![var("seed")]),
                    vec![
                        if_then(
                            not(call("is_numeric", vec![var("seed")])),
                            vec![throw_(mt19937_seed_type_error())],
                        ),
                        if_chain(
                            mt19937_seed_float_string(var("seed")),
                            vec![
                                assign("_number", cast(CastType::Float, var("seed"))),
                                if_then(
                                    mt19937_seed_float_out_of_range(var("_number")),
                                    vec![throw_(mt19937_seed_type_error())],
                                ),
                                if_then(
                                    bin(
                                        cast(CastType::Float, cast(CastType::Int, var("seed"))),
                                        BinOp::StrictNotEq,
                                        var("_number"),
                                    ),
                                    vec![expr_stmt(mt19937_seed_deprecation(
                                        e_concat_all(vec![
                                            e_str("Implicit conversion from float-string \""),
                                            var("seed"),
                                            e_str("\" to int loses precision"),
                                        ]),
                                    ))],
                                ),
                            ],
                            vec![],
                            Some(vec![
                                // Integer strings bind through zend_strtol, so the range check
                                // must not go through a double: `9223372036854775808` rounds to
                                // PHP_INT_MAX when compared as floats and would be accepted.
                                assign("_text", call("trim", vec![var("seed")])),
                                assign(
                                    "_negative",
                                    bin(
                                        call(
                                            "substr",
                                            vec![var("_text"), int(0), int(1)],
                                        ),
                                        BinOp::StrictEq,
                                        e_str("-"),
                                    ),
                                ),
                                assign(
                                    "_digits",
                                    call(
                                        "ltrim",
                                        vec![
                                            call("ltrim", vec![var("_text"), e_str("+-")]),
                                            e_str("0"),
                                        ],
                                    ),
                                ),
                                if_then(
                                    bin(var("_digits"), BinOp::StrictEq, e_str("")),
                                    vec![assign("_digits", e_str("0"))],
                                ),
                                assign(
                                    "_limit",
                                    e_ternary(
                                        var("_negative"),
                                        e_str("9223372036854775808"),
                                        e_str("9223372036854775807"),
                                    ),
                                ),
                                if_then(
                                    bin(
                                        bin(
                                            call("strlen", vec![var("_digits")]),
                                            BinOp::Gt,
                                            int(19),
                                        ),
                                        BinOp::Or,
                                        bin(
                                            bin(
                                                call("strlen", vec![var("_digits")]),
                                                BinOp::StrictEq,
                                                int(19),
                                            ),
                                            BinOp::And,
                                            bin(
                                                call(
                                                    "strcmp",
                                                    vec![var("_digits"), var("_limit")],
                                                ),
                                                BinOp::Gt,
                                                int(0),
                                            ),
                                        ),
                                    ),
                                    vec![
                                        // An integer string that overflows zend_long falls back to
                                        // the double binding: `-9223372036854775809` rounds to
                                        // PHP_INT_MIN and binds, while `9223372036854775808`
                                        // rounds to 2^63 and is rejected.
                                        assign("_number", cast(CastType::Float, var("seed"))),
                                        if_then(
                                            mt19937_seed_float_out_of_range(var("_number")),
                                            vec![throw_(mt19937_seed_type_error())],
                                        ),
                                        ret(cast(CastType::Int, var("seed"))),
                                    ],
                                ),
                            ]),
                        ),
                        ret(cast(CastType::Int, var("seed"))),
                    ],
                ),
                throw_(mt19937_seed_type_error()),
            ])
            .build(),
        function("lcg_value")
            .returns(TypeExpr::Float)
            .body(vec![
                expr_stmt(call(
                    "__elephc_builtin_deprecated",
                    vec![e_str(
                        "Function lcg_value() is deprecated since 8.4, use \\Random\\Randomizer::getFloat() instead",
                    )],
                )),
                ret(bridge("elephc_random_lcg_value", vec![])),
            ])
            .build(),

        function("__elephc_array_rand")
            .param("array", t_mixed())
            .param_default("num", TypeExpr::Int, e_int(1))
            .returns(t_mixed())
            .body(vec![
                // A direct call, a string callable and the runtime descriptor wrapper all
                // reach this one helper. The parameter is `mixed` so a dynamically selected
                // name hands over whatever the caller passed; the array check is PHP's own
                // parameter binding, raised here because the wrapper bypasses the checker.
                if_then(
                    not(call("is_array", vec![var("array")])),
                    vec![
                        // php-src's parameter error uses zend_zval_type_name: gettype's
                        // spellings for scalars differ, objects report their class, and
                        // resources stay "resource".
                        assign("_given", call("gettype", vec![var("array")])),
                        if_then(
                            bin(var("_given"), BinOp::StrictEq, e_str("integer")),
                            vec![assign("_given", e_str("int"))],
                        ),
                        if_then(
                            bin(var("_given"), BinOp::StrictEq, e_str("double")),
                            vec![assign("_given", e_str("float"))],
                        ),
                        if_then(
                            bin(var("_given"), BinOp::StrictEq, e_str("boolean")),
                            vec![assign("_given", e_str("bool"))],
                        ),
                        if_then(
                            bin(var("_given"), BinOp::StrictEq, e_str("NULL")),
                            vec![assign("_given", e_str("null"))],
                        ),
                        if_then(
                            call("is_object", vec![var("array")]),
                            vec![assign("_given", call("get_class", vec![var("array")]))],
                        ),
                        throw_(e_new_fq(
                            "\\TypeError",
                            vec![e_concat_all(vec![
                                e_str("array_rand(): Argument #1 ($array) must be of type array, "),
                                var("_given"),
                                e_str(" given"),
                            ])],
                        )),
                    ],
                ),
                assign("array", cast(CastType::Array, var("array"))),
                assign("_available", count(var("array"))),
                if_then(
                    bin(var("_available"), BinOp::StrictEq, int(0)),
                    vec![throw_(e_new_fq(
                        "\\ValueError",
                        vec![e_str(
                            "array_rand(): Argument #1 ($array) must not be empty",
                        )],
                    ))],
                ),
                if_then(
                    bin(
                        bin(var("num"), BinOp::Lt, int(1)),
                        BinOp::Or,
                        bin(var("num"), BinOp::Gt, var("_available")),
                    ),
                    vec![throw_(e_new_fq(
                        "\\ValueError",
                        vec![e_str(
                            "array_rand(): Argument #2 ($num) must be between 1 and the number of elements in argument #1 ($array)",
                        )],
                    ))],
                ),
                assign("_keys", call("array_keys", vec![var("array")])),
                if_then(
                    bin(var("num"), BinOp::StrictEq, int(1)),
                    vec![ret(index(
                        var("_keys"),
                        bridge("elephc_random_mt_uniform", vec![var("_available")]),
                    ))],
                ),
                assign("_negative", e_bool(false)),
                assign("_sample", var("num")),
                if_then(
                    bin(
                        var("num"),
                        BinOp::Gt,
                        bin(var("_available"), BinOp::ShiftRight, int(1)),
                    ),
                    vec![
                        assign("_negative", e_bool(true)),
                        assign(
                            "_sample",
                            bin(var("_available"), BinOp::Sub, var("num")),
                        ),
                    ],
                ),
                assign("_selected", e_array(vec![])),
                assign("_remaining", var("_sample")),
                assign("_failures", int(0)),
                s_while(
                    bin(var("_remaining"), BinOp::Gt, int(0)),
                    vec![
                        assign(
                            "_index",
                            bridge("elephc_random_mt_uniform", vec![var("_available")]),
                        ),
                        if_chain(
                            call("isset", vec![index(var("_selected"), var("_index"))]),
                            vec![
                                add_assign("_failures", int(1)),
                                if_then(
                                    bin(var("_failures"), BinOp::Gt, int(50)),
                                    vec![throw_broken()],
                                ),
                            ],
                            vec![],
                            Some(vec![
                                s_array_assign("_selected", var("_index"), e_bool(true)),
                                add_assign("_remaining", int(-1)),
                                assign("_failures", int(0)),
                            ]),
                        ),
                    ],
                ),
                assign("_result", e_array(vec![])),
                s_for(
                    Some(assign("_i", int(0))),
                    Some(bin(var("_i"), BinOp::Lt, var("_available"))),
                    Some(add_assign("_i", int(1))),
                    vec![if_then(
                        bin(
                            call("isset", vec![index(var("_selected"), var("_i"))]),
                            BinOp::StrictNotEq,
                            var("_negative"),
                        ),
                        vec![push("_result", index(var("_keys"), var("_i")))],
                    )],
                ),
                ret(var("_result")),
            ])
            .build(),
        // An internal function reports its argument count at CALL time, not at compile time, so
        // every route PHP can take -- a direct call, a callable string, a first-class callable,
        // `call_user_func*`, `array_map` -- has to reach the same runtime `ArgumentCountError`.
        // A fixed one-parameter signature makes the compiler reject the short and long forms,
        // so the shim takes an optional `$length` plus a variadic tail and validates the count
        // itself, exactly like ZPP does inside php-src. The tail also carries PHP's unknown
        // named parameters, which are `Error`s and are checked before the count.
        function("random_bytes")
            .param_default("length", TypeExpr::Int, e_int(0))
            .variadic("extra", Some(t_mixed()))
            .returns(TypeExpr::Str)
            .body(vec![
                assign("_argc", call("func_num_args", vec![])),
                s_foreach(
                    var("extra"),
                    Some("_name"),
                    "_value",
                    vec![if_then(
                        call("is_string", vec![var("_name")]),
                        vec![throw_(e_new_fq(
                            "\\Error",
                            vec![e_concat_all(vec![
                                e_str("Unknown named parameter $"),
                                var("_name"),
                            ])],
                        ))],
                    )],
                ),
                if_then(
                    bin(var("_argc"), BinOp::StrictNotEq, e_int(1)),
                    vec![throw_(e_new_fq(
                        "\\ArgumentCountError",
                        vec![e_concat_all(vec![
                            e_str("random_bytes() expects exactly 1 argument, "),
                            var("_argc"),
                            e_str(" given"),
                        ])],
                    ))],
                ),
                if_then(
                    bin(var("length"), BinOp::Lt, e_int(1)),
                    vec![throw_(e_new_fq(
                        "ValueError",
                        vec![e_str(
                            "random_bytes(): Argument #1 ($length) must be greater than 0",
                        )],
                    ))],
                ),
                assign(
                    "_bytes",
                    call(
                        "hex2bin",
                        vec![bridge("elephc_random_bytes_hex", vec![var("length")])],
                    ),
                ),
                if_then(
                    bin(
                        bridge("elephc_random_csprng_failed", vec![]),
                        BinOp::StrictNotEq,
                        e_int(0),
                    ),
                    vec![throw_(e_new_fq(
                        "\\Random\\RandomException",
                        vec![e_str("Failed to generate random bytes")],
                    ))],
                ),
                ret(e_ternary(
                    bin(var("_bytes"), BinOp::StrictEq, e_bool(false)),
                    e_str(""),
                    var("_bytes"),
                )),
            ])
            .build(),
    ]
}

/// Returns whether `program` references the PHP `random` surface.
#[allow(dead_code)]
pub fn program_uses_random(program: &[Stmt]) -> bool {
    crate::compiler_stack::with_compiler_stack(|| program_uses_random_on_compiler_stack(program))
}

/// The body of [`program_uses_random`], run on the stack `with_compiler_stack` sized.
fn program_uses_random_on_compiler_stack(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    const FUNCTIONS: &[&str] = &[
        "mt_srand",
        "srand",
        "mt_getrandmax",
        "getrandmax",
        "random_bytes",
        "lcg_value",
        "array_rand",
    ];
    if usage
        .functions
        .iter()
        .any(|name| FUNCTIONS.contains(&name.as_str()))
    {
        return true;
    }
    // A dynamic call such as `$f = 'random_bytes'; $f(16)` records the name as a
    // string literal, not as a function reference. The prelude has to be present for
    // that call to resolve at run time.
    if usage.literals.iter().any(|literal| {
        FUNCTIONS
            .iter()
            .any(|name| literal.eq_ignore_ascii_case(name))
    }) {
        return true;
    }
    if usage.classes.iter().any(|name| is_random_class(name)) {
        return true;
    }
    if usage.constants.contains("MT_RAND_PHP") {
        return true;
    }
    usage
        .literals
        .iter()
        .any(|literal| literal.contains("random\\"))
}

/// Returns whether a normalized class reference names the random extension's surface.
fn is_random_class(name: &str) -> bool {
    const CLASSES: &[&str] = &[
        "random",
        "random\\engine",
        "random\\cryptosafeengine",
        "random\\randomizer",
        "random\\intervalboundary",
        "random\\randomerror",
        "random\\brokenrandomengineerror",
        "random\\randomexception",
    ];
    let trimmed = name.trim_start_matches('\\');
    CLASSES.contains(&trimmed) || trimmed.starts_with("random\\engine\\")
}

/// Injects the random prelude when the program references its surface.
pub fn inject_if_used(
    program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        if !force && !program_uses_random_on_compiler_stack(&program) {
            return program;
        }
        let mut declarations = random_declarations();
        inventory.record_program(PRELUDE_GROUP, &declarations);
        // `array_rand`'s string-callable forms reach the prelude helper from EIR lowering, with
        // no AST reference that would keep it alive during reachability pruning.
        if force || program_references_array_rand(&program) {
            inventory.record_required_function(PRELUDE_GROUP, ARRAY_RAND_HELPER);
        }
        declarations.extend(program);
        declarations
    })
}

/// Returns whether the program can call `array_rand` through any AOT-visible form.
fn program_references_array_rand(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    usage
        .functions
        .iter()
        .any(|name| name.eq_ignore_ascii_case("array_rand"))
        || usage
            .literals
            .iter()
            .any(|literal| literal.eq_ignore_ascii_case("array_rand"))
}

// ---------------------------------------------------------------------------
// Shared AST constructors used by the engine and Randomizer builders
// ---------------------------------------------------------------------------

/// Builds an ordinary free-function call.
pub(super) fn call(name: &str, args: Vec<Expr>) -> Expr {
    e_call(name, args)
}

/// Builds a call to one of the bridge extern symbols.
pub(super) fn bridge(name: &str, args: Vec<Expr>) -> Expr {
    e_call(name, args)
}

/// Builds a call to a `self::` method.
pub(super) fn self_call(name: &str, args: Vec<Expr>) -> Expr {
    e_self_call(name, args)
}

/// Builds a call to a `$this->` method.
pub(super) fn method_call(object: Expr, name: &str, args: Vec<Expr>) -> Expr {
    e_method_call(object, name, args)
}


/// Reads the engine object's opaque native handle.
pub(super) fn handle() -> Expr {
    e_this_prop("__elephc_handle")
}

/// Builds a binary operation.
pub(super) fn bin(left: Expr, op: BinOp, right: Expr) -> Expr {
    e_binop(left, op, right)
}

/// Builds a variable read.
pub(super) fn var(name: &str) -> Expr {
    e_var(name)
}

/// Builds an array index read.
pub(super) fn index(array: Expr, at: Expr) -> Expr {
    e_index(array, at)
}

/// Builds a root-anchored object construction with a distinct synthetic span.
///
/// Every builder node otherwise carries `Span::dummy()`. Metadata that is keyed by span (the
/// exception class most visibly) then cannot tell two `new` expressions apart, so a throw of
/// `\ValueError` in one branch and `\TypeError` in another resolved to whichever was recorded
/// first. A unique span per construction keeps each throw attached to its own class.
pub(super) fn unique_new(class_name: &str, args: Vec<Expr>) -> Expr {
    let mut value = e_new_fq(class_name, args);
    value.span = Span::synthetic();
    value
}

/// Reads one limb from a numeric pair as an integer.
pub(super) fn limb(array: Expr, at: i64) -> Expr {
    e_cast(CastType::Int, e_index(array, e_int(at)))
}

/// Builds PHP's `TypeError` for a non-bindable `Mt19937` seed.
fn mt19937_seed_type_error() -> Expr {
    e_new_fq(
        "\\TypeError",
        vec![e_concat_all(vec![
            e_str("Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, "),
            var("_type"),
            e_str(" given"),
        ])],
    )
}

/// Returns whether a numeric value is NaN, infinite, or outside the signed 64-bit range.
fn mt19937_seed_float_out_of_range(value: Expr) -> Expr {
    bin(
        bin(
            call("is_nan", vec![value.clone()]),
            BinOp::Or,
            call("is_infinite", vec![value.clone()]),
        ),
        BinOp::Or,
        bin(
            bin(value.clone(), BinOp::GtEq, float(9_223_372_036_854_775_808.0)),
            BinOp::Or,
            bin(value, BinOp::Lt, float(-9_223_372_036_854_775_808.0)),
        ),
    )
}

/// Returns whether a numeric string is PHP's "float-string" form.
fn mt19937_seed_float_string(value: Expr) -> Expr {
    bin(
        call("str_contains", vec![value.clone(), e_str(".")]),
        BinOp::Or,
        bin(
            call("stripos", vec![value, e_str("e")]),
            BinOp::StrictNotEq,
            e_bool(false),
        ),
    )
}

/// Builds the engine-level `E_DEPRECATED` notice PHP emits for a lossy seed coercion.
fn mt19937_seed_deprecation(message: Expr) -> Expr {
    call("__elephc_builtin_deprecated", vec![message])
}

/// Builds an integer literal.
pub(super) fn int(value: i64) -> Expr {
    e_int(value)
}


/// Builds a float literal.
pub(super) fn float(value: f64) -> Expr {
    e_float(value)
}

/// Builds a cast expression.
pub(super) fn cast(target: CastType, value: Expr) -> Expr {
    e_cast(target, value)
}

/// Builds a logical negation.
pub(super) fn not(value: Expr) -> Expr {
    e_not(value)
}


/// Builds `$var = value`.
pub(super) fn assign(name: &str, value: Expr) -> Stmt {
    s_assign(name, value)
}

/// Builds `$var = $var . value`.
pub(super) fn append(name: &str, value: Expr) -> Stmt {
    s_assign(name, e_concat(var(name), value))
}

/// Builds `$this->property = value`.
pub(super) fn set_property(name: &str, value: Expr) -> Stmt {
    s_prop_assign(e_this(), name, value)
}

/// Builds an expression statement.
pub(super) fn expr_stmt(value: Expr) -> Stmt {
    s_expr(value)
}

/// Builds `return value`.
pub(super) fn ret(value: Expr) -> Stmt {
    s_return(value)
}

/// Builds `throw exception` with a distinct synthetic span.
///
/// The throw itself carries the span the compiler uses to pick the exception class, so two
/// throws in one method must not share `Span::dummy()` or the second inherits the first's
/// class.
pub(super) fn throw_(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Throw(value), Span::synthetic())
}

/// Builds `if (condition) { body }`.
pub(super) fn if_then(condition: Expr, body: Vec<Stmt>) -> Stmt {
    s_if(condition, body, Vec::new(), None)
}

/// Builds `if (condition) { then } elseif (...) { ... } else { ... }`.
pub(super) fn if_chain(
    condition: Expr,
    then_body: Vec<Stmt>,
    arms: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
) -> Stmt {
    s_if(condition, then_body, arms, else_body)
}


/// Builds `$var += amount`.
pub(super) fn add_assign(name: &str, amount: Expr) -> Stmt {
    s_assign(name, bin(var(name), BinOp::Add, amount))
}

/// Builds `$array[] = value`.
pub(super) fn push(array: &str, value: Expr) -> Stmt {
    s_array_push(array, value)
}

/// Builds a list of integer-typed local helpers' return type.
pub(super) fn int_array() -> TypeExpr {
    t_array()
}

/// Calls `strlen(value)`.
pub(super) fn len(value: Expr) -> Expr {
    call("strlen", vec![value])
}

/// Calls `count(value)`.
pub(super) fn count(value: Expr) -> Expr {
    call("count", vec![value])
}

/// Calls `chr(value)`.
pub(super) fn chr(value: Expr) -> Expr {
    call("chr", vec![value])
}

/// Calls `ord(value)`.
pub(super) fn ord(value: Expr) -> Expr {
    call("ord", vec![value])
}

/// Builds `$value & 0xFF`.
pub(super) fn low_byte(value: Expr) -> Expr {
    bin(value, BinOp::BitAnd, e_int(0xFF))
}

/// Throws a `Random\BrokenRandomEngineError` with php-src's exact message.
pub(super) fn throw_broken() -> Stmt {
    throw_(e_new_fq(
        "\\Random\\BrokenRandomEngineError",
        vec![e_str(
            "Failed to generate an acceptable random number in 50 attempts",
        )],
    ))
}

/// Throws the empty-engine-value error used by every consumer.
pub(super) fn throw_empty_engine() -> Stmt {
    throw_(e_new_fq(
        "\\Random\\BrokenRandomEngineError",
        vec![e_str("A random engine must return a non-empty string")],
    ))
}


#[cfg(test)]
mod tests {
    use super::*;

    /// `--with-random` forces the declaration set even for a program that never mentions it.
    #[test]
    fn forcing_random_injects_the_surface() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let injected = inject_if_used(Vec::new(), true, &mut inventory);
        assert!(!injected.is_empty());
        assert!(inventory.groups.contains_key(PRELUDE_GROUP));
    }

    /// An unrelated program carries no random declarations and no bridge externs.
    #[test]
    fn unused_random_surface_is_not_injected() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let injected = inject_if_used(Vec::new(), false, &mut inventory);
        assert!(injected.is_empty());
        assert!(!inventory.groups.contains_key(PRELUDE_GROUP));
    }

    /// Detection covers every entry point that needs the injected surface.
    #[test]
    fn detection_covers_the_random_surface() {
        for (source, expected) in [
            ("<?php mt_srand(1);", true),
            ("<?php $f = 'random_bytes'; echo $f(1);", true),
            (
                "<?php $r = new \\Random\\Randomizer(new \\Random\\Engine\\Secure());",
                true,
            ),
            ("<?php echo MT_RAND_PHP;", true),
            ("<?php $a = [1, 2]; echo array_rand($a);", true),
            ("<?php echo strlen('x');", false),
        ] {
            let tokens = crate::lexer::tokenize(source).expect("tokenize the fixture");
            let program = crate::parser::parse(&tokens).expect("parse the fixture");
            assert_eq!(program_uses_random(&program), expected, "{source}");
        }
    }
}
