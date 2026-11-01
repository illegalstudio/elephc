//! Purpose:
//! Builds the prelude helper behind `array_splice()` over a receiver whose storage is not the
//! dense indexed array the native lowering assumes: a hash (`['id' => 1]`) or a boxed gradual
//! value that may hold either.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declaration when the
//!   program references `array_splice`.
//! - `crate::ir_lower::expr::compat_preludes::lower_backend_gap_builtin_shape`, which routes a
//!   hash or gradual receiver here.
//!
//! Key details:
//! - The native `array_splice` walks a dense slot array. On a hash it was a compile refusal
//!   (`array_pop for PHP type AssocArray`), and on a boxed receiver holding a hash a silent
//!   no-op. php's contract is positional: offsets count in INSERTION order, integer keys are
//!   renumbered from zero, string keys are kept, the replacement lands at the offset, and the
//!   removed entries come back as an array with the same key rule.
//! - The helper takes the receiver as `mixed &$array`, which is the one by-reference shape every
//!   caller representation (a hash local, a boxed cell, a by-reference parameter) can bind.
//! - The declaration is BUILT, never parsed: elephc does not parse PHP source outside tests.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_call, e_cast, e_int, e_null, e_var, function, php_semantics_declarations,
    s_array_assign, s_array_push, s_assign, s_foreach, s_if, s_return, t_mixed,
};

/// Reserved helper backing `array_splice()` over hash and gradual receivers.
pub(crate) const ARRAY_SPLICE_GRADUAL_NAME: &str = "__elephc_array_splice_gradual";

/// Returns the helper declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![splice_gradual()])
}

/// `$name = $left <op> $right;`.
fn assign_binop(name: &str, left: crate::parser::ast::Expr, op: BinOp, right: crate::parser::ast::Expr) -> Stmt {
    s_assign(name, e_binop(left, op, right))
}

/// Builds the splice walk.
fn splice_gradual() -> Stmt {
    // if ($offset < 0) { $offset = $count + $offset; if ($offset < 0) { $offset = 0; } }
    // elseif ($offset > $count) { $offset = $count; }
    let normalize_offset = s_if(
        e_binop(e_var("offset"), BinOp::Lt, e_int(0)),
        vec![
            assign_binop("offset", e_var("count"), BinOp::Add, e_var("offset")),
            s_if(
                e_binop(e_var("offset"), BinOp::Lt, e_int(0)),
                vec![s_assign("offset", e_int(0))],
                Vec::new(),
                None,
            ),
        ],
        vec![(
            e_binop(e_var("offset"), BinOp::Gt, e_var("count")),
            vec![s_assign("offset", e_var("count"))],
        )],
        None,
    );
    // if ($length === null) { $length = $count - $offset; }
    // elseif ($length < 0) { $length = $count + $length - $offset; if ($length < 0) { $length = 0; } }
    // elseif ($offset + $length > $count) { $length = $count - $offset; }
    let normalize_length = s_if(
        e_binop(e_var("length"), BinOp::StrictEq, e_null()),
        vec![assign_binop("length", e_var("count"), BinOp::Sub, e_var("offset"))],
        vec![
            (
                e_binop(e_var("length"), BinOp::Lt, e_int(0)),
                vec![
                    s_assign(
                        "length",
                        e_binop(
                            e_binop(e_var("count"), BinOp::Add, e_var("length")),
                            BinOp::Sub,
                            e_var("offset"),
                        ),
                    ),
                    s_if(
                        e_binop(e_var("length"), BinOp::Lt, e_int(0)),
                        vec![s_assign("length", e_int(0))],
                        Vec::new(),
                        None,
                    ),
                ],
            ),
            (
                e_binop(
                    e_binop(e_var("offset"), BinOp::Add, e_var("length")),
                    BinOp::Gt,
                    e_var("count"),
                ),
                vec![assign_binop("length", e_var("count"), BinOp::Sub, e_var("offset"))],
            ),
        ],
        None,
    );
    // foreach ($replacement as $item) { $kept[] = $item; }
    let insert_replacement =
        || s_foreach(e_var("replacement"), None, "item", vec![s_array_push("kept", e_var("item"))]);
    // if (is_string($key)) { $<target>[$key] = $value; } else { $<target>[] = $value; }
    let keep_entry = |target: &str| {
        s_if(
            e_call("is_string", vec![e_var("key")]),
            vec![s_array_assign(target, e_var("key"), e_var("value"))],
            Vec::new(),
            Some(vec![s_array_push(target, e_var("value"))]),
        )
    };
    function(ARRAY_SPLICE_GRADUAL_NAME)
        .param_by_ref("array", Some(t_mixed()))
        .param("offset", TypeExpr::Int)
        .param_default("length", t_mixed(), e_null())
        .param_default("replacement", t_mixed(), e_array(Vec::new()))
        .returns(t_mixed())
        .body(vec![
            s_assign("count", e_call("count", vec![e_var("array")])),
            normalize_offset,
            normalize_length,
            assign_binop("end", e_var("offset"), BinOp::Add, e_var("length")),
            s_assign("replacement", e_cast(CastType::Array, e_var("replacement"))),
            s_assign("kept", e_array(Vec::new())),
            s_assign("removed", e_array(Vec::new())),
            s_assign("position", e_int(0)),
            s_assign("inserted", crate::synthetic_class::e_bool(false)),
            s_foreach(
                e_var("array"),
                Some("key"),
                "value",
                vec![
                    s_if(
                        e_binop(e_var("position"), BinOp::StrictEq, e_var("offset")),
                        vec![
                            insert_replacement(),
                            s_assign("inserted", crate::synthetic_class::e_bool(true)),
                        ],
                        Vec::new(),
                        None,
                    ),
                    s_if(
                        e_binop(
                            e_binop(e_var("position"), BinOp::GtEq, e_var("offset")),
                            BinOp::And,
                            e_binop(e_var("position"), BinOp::Lt, e_var("end")),
                        ),
                        vec![keep_entry("removed")],
                        Vec::new(),
                        Some(vec![keep_entry("kept")]),
                    ),
                    assign_binop("position", e_var("position"), BinOp::Add, e_int(1)),
                ],
            ),
            s_if(
                e_binop(
                    e_var("inserted"),
                    BinOp::StrictEq,
                    crate::synthetic_class::e_bool(false),
                ),
                vec![insert_replacement()],
                Vec::new(),
                None,
            ),
            s_assign("array", e_var("kept")),
            s_return(e_var("removed")),
        ])
        .build()
}
