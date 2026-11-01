//! Purpose:
//! Builds PHP's `hash_pbkdf2()` as a prelude declaration, for programs that name it.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declaration when the
//!   program references `hash_pbkdf2`.
//!
//! Key details:
//! - PBKDF2 (RFC 8018) over the backend's raw `hash_hmac()`, following php-src's
//!   `PHP_FUNCTION(hash_pbkdf2)`: the algorithm is checked first (a checksum or unknown name is
//!   php's ValueError for argument 1), then `$iterations > 0`, then `$length >= 0`. A zero
//!   `$length` means the digest size -- in bytes for raw output, in hex digits otherwise -- and
//!   hex output derives `ceil($length / 2)` bytes and keeps the first `$length` digits.
//! - `$options` (the seed of the murmur/xxh checksums) cannot apply to a cryptographic
//!   algorithm, so it is accepted and ignored, as php does for these.
//! - The interpreter has its own home (`interpreter::builtins::string::hash_pbkdf2`) over
//!   `elephc_crypto::pbkdf2`: Symfony's `Pbkdf2PasswordHasher` is interpreted and derives a key
//!   in its constructor, which every login builds.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_int, e_new_fq, e_str, e_ternary, e_var, function,
    php_semantics_declarations, s_assign, s_for, s_if, s_return, s_throw, s_try, t_array,
};

/// Returns the `hash_pbkdf2()` declaration for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![decl_fn_hash_pbkdf2()])
}

/// `throw new \ValueError('hash_pbkdf2(): Argument #n ($name) must ...')`.
fn value_error(message: &str) -> Stmt {
    s_throw(e_new_fq("ValueError", vec![e_str(&format!("hash_pbkdf2(): {message}"))]))
}

/// `$left <op> $right`.
fn bin(left: Expr, op: BinOp, right: Expr) -> Expr {
    e_binop(left, op, right)
}

/// `hash_hmac($algo, $data, $password, true)`.
fn hmac(data: Expr) -> Expr {
    e_call(
        "hash_hmac",
        vec![e_var("algo"), data, e_var("password"), e_bool(true)],
    )
}

/// `chr(($block >> shift) & 255)`, one byte of the big-endian block index.
fn block_byte(shift: i64) -> Expr {
    e_call(
        "chr",
        vec![bin(
            bin(e_var("block"), BinOp::ShiftRight, e_int(shift)),
            BinOp::BitAnd,
            e_int(255),
        )],
    )
}

/// Builds the declaration.
fn decl_fn_hash_pbkdf2() -> Stmt {
    let block_index = [24, 16, 8, 0]
        .into_iter()
        .map(block_byte)
        .fold(e_var("salt"), |acc, byte| bin(acc, BinOp::Concat, byte));
    let inner_loop = s_for(
        Some(s_assign("i", e_int(1))),
        Some(bin(e_var("i"), BinOp::Lt, e_var("iterations"))),
        Some(s_assign("i", bin(e_var("i"), BinOp::Add, e_int(1)))),
        vec![
            s_assign("round", hmac(e_var("round"))),
            s_assign("sum", bin(e_var("sum"), BinOp::BitXor, e_var("round"))),
        ],
    );
    let outer_loop = s_for(
        Some(s_assign("block", e_int(1))),
        Some(bin(e_var("block"), BinOp::LtEq, e_var("blocks"))),
        Some(s_assign("block", bin(e_var("block"), BinOp::Add, e_int(1)))),
        vec![
            s_assign("round", hmac(block_index)),
            s_assign("sum", e_var("round")),
            inner_loop,
            s_assign("derived", bin(e_var("derived"), BinOp::Concat, e_var("sum"))),
        ],
    );
    let body = vec![
        s_assign("algo", e_call("strtolower", vec![e_var("algo")])),
        s_try(
            vec![s_assign(
                "size",
                e_call(
                    "strlen",
                    vec![e_call(
                        "hash_hmac",
                        vec![e_var("algo"), e_str(""), e_str(""), e_bool(true)],
                    )],
                ),
            )],
            vec![(
                vec!["ValueError"],
                None,
                vec![value_error(
                    "Argument #1 ($algo) must be a valid cryptographic hashing algorithm",
                )],
            )],
            None,
        ),
        s_if(
            bin(e_var("iterations"), BinOp::LtEq, e_int(0)),
            vec![value_error("Argument #4 ($iterations) must be greater than 0")],
            vec![],
            None,
        ),
        s_if(
            bin(e_var("length"), BinOp::Lt, e_int(0)),
            vec![value_error(
                "Argument #5 ($length) must be greater than or equal to 0",
            )],
            vec![],
            None,
        ),
        s_if(
            bin(e_var("length"), BinOp::StrictEq, e_int(0)),
            vec![s_assign(
                "length",
                e_ternary(
                    e_var("binary"),
                    e_var("size"),
                    bin(e_var("size"), BinOp::Mul, e_int(2)),
                ),
            )],
            vec![],
            None,
        ),
        s_assign(
            "keyBytes",
            e_ternary(
                e_var("binary"),
                e_var("length"),
                e_call(
                    "intdiv",
                    vec![bin(e_var("length"), BinOp::Add, e_int(1)), e_int(2)],
                ),
            ),
        ),
        s_assign(
            "blocks",
            e_call(
                "intdiv",
                vec![
                    bin(
                        bin(e_var("keyBytes"), BinOp::Add, e_var("size")),
                        BinOp::Sub,
                        e_int(1),
                    ),
                    e_var("size"),
                ],
            ),
        ),
        s_assign("derived", e_str("")),
        outer_loop,
        s_assign(
            "derived",
            e_call("substr", vec![e_var("derived"), e_int(0), e_var("keyBytes")]),
        ),
        s_if(e_var("binary"), vec![s_return(e_var("derived"))], vec![], None),
        s_return(e_call(
            "substr",
            vec![
                e_call("bin2hex", vec![e_var("derived")]),
                e_int(0),
                e_var("length"),
            ],
        )),
    ];
    function("hash_pbkdf2")
        .param("algo", TypeExpr::Str)
        .param("password", TypeExpr::Str)
        .param("salt", TypeExpr::Str)
        .param("iterations", TypeExpr::Int)
        .param_default("length", TypeExpr::Int, e_int(0))
        .param_default("binary", TypeExpr::Bool, e_bool(false))
        .param_default("options", t_array(), e_array(vec![]))
        .returns(TypeExpr::Str)
        .body(body)
        .build()
}
