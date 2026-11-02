//! Purpose:
//! Builds php's `Random\Randomizer` class as prelude declarations, for programs that construct it.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the class when the program
//!   names `new Random\Randomizer` and declares no such class itself.
//!
//! Key details:
//! - The class is BUILT, never parsed: elephc does not parse PHP source outside tests.
//! - Every method draws from `random_int()`, the same CSPRNG php's default `Secure` engine uses,
//!   so a program that passes an engine (the constructor accepts and ignores it) still gets a
//!   uniformly random answer; seeded engines are not reproduced.
//! - `pickArrayKeys()` follows php-src's `Randomizer::pickArrayKeys`: it picks `$num` DISTINCT
//!   positions and returns their keys in the ORIGINAL array order, not in draw order. Laravel's
//!   `Arr::random()` / `Collection::random()` (`inspire`, every random pick) calls it.
//! - `shuffleArray()` returns a fresh list (keys are not preserved), as php does.
//! - The `ValueError` messages are php 8.5's own wording.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    class, e_array, e_binop, e_bool, e_call, e_cast, e_index, e_int, e_new_fq, e_not, e_null, e_str,
    e_var, internal_declarations, method, s_array_assign, s_assign, s_foreach, s_if,
    s_return, s_throw, s_while, t_array, t_mixed,
};

/// Canonical name of the class this module declares.
pub(crate) const RANDOMIZER_CLASS_NAME: &str = "Random\\Randomizer";

/// Returns the `Random\Randomizer` declaration for injection.
pub(crate) fn declarations() -> Program {
    internal_declarations(|| vec![decl_class_randomizer()])
}

/// Builds `throw new \ValueError($message);`.
fn throw_value_error(message: &str) -> Stmt {
    s_throw(e_new_fq("ValueError", vec![e_str(message)]))
}

/// Builds `random_int($low, $high)` over two expressions.
fn random_int(low: crate::parser::ast::Expr, high: crate::parser::ast::Expr) -> crate::parser::ast::Expr {
    e_call("random_int", vec![low, high])
}

/// Builds the `Random\Randomizer` class.
fn decl_class_randomizer() -> Stmt {
    class(RANDOMIZER_CLASS_NAME)
        .final_()
        .method(
            method("__construct")
                .param_default("engine", t_mixed(), e_null())
                .body(vec![]),
        )
        .method(
            method("getInt")
                .param("min", TypeExpr::Int)
                .param("max", TypeExpr::Int)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_binop(e_var("max"), BinOp::Lt, e_var("min")),
                        vec![throw_value_error(
                            "Random\\Randomizer::getInt(): Argument #2 ($max) must be greater than or equal to argument #1 ($min)",
                        )],
                        vec![],
                        None,
                    ),
                    s_return(random_int(e_var("min"), e_var("max"))),
                ]),
        )
        .method(
            method("getBytesFromString")
                .param("string", TypeExpr::Str)
                .param("length", TypeExpr::Int)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_binop(e_var("string"), BinOp::StrictEq, e_str("")),
                        vec![throw_value_error(
                            "Random\\Randomizer::getBytesFromString(): Argument #1 ($string) must not be empty",
                        )],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("length"), BinOp::Lt, e_int(1)),
                        vec![throw_value_error(
                            "Random\\Randomizer::getBytesFromString(): Argument #2 ($length) must be greater than 0",
                        )],
                        vec![],
                        None,
                    ),
                    s_assign("result", e_str("")),
                    s_assign(
                        "maximum",
                        e_binop(e_call("strlen", vec![e_var("string")]), BinOp::Sub, e_int(1)),
                    ),
                    s_assign("position", e_int(0)),
                    s_while(
                        e_binop(e_var("position"), BinOp::Lt, e_var("length")),
                        vec![
                            s_assign(
                                "result",
                                e_binop(
                                    e_var("result"),
                                    BinOp::Concat,
                                    e_index(
                                        e_var("string"),
                                        random_int(e_int(0), e_cast(CastType::Int, e_var("maximum"))),
                                    ),
                                ),
                            ),
                            s_assign("position", e_binop(e_var("position"), BinOp::Add, e_int(1))),
                        ],
                    ),
                    s_return(e_var("result")),
                ]),
        )
        .method(
            method("pickArrayKeys")
                .param("array", t_array())
                .param("num", TypeExpr::Int)
                .returns(t_array())
                .body(vec![
                    s_assign("count", e_call("count", vec![e_var("array")])),
                    s_if(
                        e_binop(e_var("count"), BinOp::StrictEq, e_int(0)),
                        vec![throw_value_error(
                            "Random\\Randomizer::pickArrayKeys(): Argument #1 ($array) must not be empty",
                        )],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(
                            e_binop(e_var("num"), BinOp::Lt, e_int(1)),
                            BinOp::Or,
                            e_binop(e_var("num"), BinOp::Gt, e_var("count")),
                        ),
                        vec![throw_value_error(
                            "Random\\Randomizer::pickArrayKeys(): Argument #2 ($num) must be between 1 and the number of elements in argument #1 ($array)",
                        )],
                        vec![],
                        None,
                    ),
                    // Draw DISTINCT positions; the walk below emits their keys in array order.
                    s_assign("selected", e_array(vec![])),
                    s_assign("remaining", e_var("num")),
                    s_while(
                        e_binop(e_var("remaining"), BinOp::Gt, e_int(0)),
                        vec![
                            s_assign(
                                "draw",
                                random_int(
                                    e_int(0),
                                    e_binop(e_var("count"), BinOp::Sub, e_int(1)),
                                ),
                            ),
                            s_if(
                                e_not(e_call(
                                    "array_key_exists",
                                    vec![e_var("draw"), e_var("selected")],
                                )),
                                vec![
                                    s_array_assign("selected", e_var("draw"), e_bool(true)),
                                    s_assign(
                                        "remaining",
                                        e_binop(e_var("remaining"), BinOp::Sub, e_int(1)),
                                    ),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                    ),
                    // Explicit-key writes only: a push beside a keyed write would let the checker
                    // infer two layouts for one result (see the backend-gap runbook).
                    s_assign("keys", e_array(vec![])),
                    s_assign("position", e_int(0)),
                    s_assign("written", e_int(0)),
                    s_foreach(
                        e_var("array"),
                        Some("key"),
                        "ignored",
                        vec![
                            s_if(
                                e_call(
                                    "array_key_exists",
                                    vec![e_var("position"), e_var("selected")],
                                ),
                                vec![
                                    s_array_assign("keys", e_var("written"), e_var("key")),
                                    s_assign("written", e_binop(e_var("written"), BinOp::Add, e_int(1))),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("position", e_binop(e_var("position"), BinOp::Add, e_int(1))),
                        ],
                    ),
                    s_return(e_var("keys")),
                ]),
        )
        .method(
            method("shuffleArray")
                .param("array", t_array())
                .returns(t_array())
                .body(vec![
                    s_assign("values", e_call("array_values", vec![e_var("array")])),
                    s_assign("index", e_binop(e_call("count", vec![e_var("values")]), BinOp::Sub, e_int(1))),
                    // Fisher-Yates over the value list, php's own shuffle order of draws.
                    s_while(
                        e_binop(e_var("index"), BinOp::Gt, e_int(0)),
                        vec![
                            s_assign("swap", random_int(e_int(0), e_var("index"))),
                            s_assign("held", e_index(e_var("values"), e_var("index"))),
                            s_array_assign("values", e_var("index"), e_index(e_var("values"), e_var("swap"))),
                            s_array_assign("values", e_var("swap"), e_var("held")),
                            s_assign("index", e_binop(e_var("index"), BinOp::Sub, e_int(1))),
                        ],
                    ),
                    s_return(e_var("values")),
                ]),
        )
        .build()
}
