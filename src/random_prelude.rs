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
        ("elephc_random_csprng_failed", &[], CType::Int),
        ("elephc_random_next_up", &[("value", CType::Float)], CType::Float),
        (
            "elephc_random_next_down",
            &[("value", CType::Float)],
            CType::Float,
        ),
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
                        "trigger_error",
                        vec![
                            e_str("The MT_RAND_PHP variant of Mt19937 is deprecated"),
                            e_const("E_USER_DEPRECATED"),
                        ],
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
        function("lcg_value")
            .returns(TypeExpr::Float)
            .body(vec![
                expr_stmt(call(
                    "trigger_error",
                    vec![
                        e_str("Function lcg_value() is deprecated since 8.4, use \\Random\\Randomizer::getFloat() instead"),
                        e_const("E_USER_DEPRECATED"),
                    ],
                )),
                ret(bridge("elephc_random_lcg_value", vec![])),
            ])
            .build(),

        function("__elephc_array_rand")
            .param("array", t_array())
            .param_default("num", TypeExpr::Int, e_int(1))
            .returns(t_mixed())
            .body(vec![
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
        declarations.extend(program);
        declarations
    })
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
