//! Purpose:
//! Declares the Argon2 provider of PHP's core `password` extension on top of the
//! `elephc_sodium` bridge.
//!
//! Called from:
//! - `crate::sodium_prelude::declarations`.
//!
//! Key details:
//! - `password_hash()`, `password_verify()`, `password_get_info()`,
//!   `password_needs_rehash()` and `password_algos()` are PHP-visible synthetic functions,
//!   not registry builtins: the sodium prelude is injected for password calls too.
//! - The Argon2i/Argon2id formats, option validation and messages match php-src 8.5's
//!   sodium password adapter (`ext/sodium/sodium_pwhash.c`).
//! - bcrypt (`2y`) is reported by `password_algos()`/`password_get_info()` but not
//!   implemented; hashing or verifying a bcrypt hash is outside this provider.
//! - No PHP source is parsed.

use super::dsl::*;
use crate::parser::ast::{BinOp, Expr, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Builds the password extension declarations contributed by the sodium prelude.
pub(super) fn declarations() -> Vec<Stmt> {
    vec![
        algos(),
        hash(),
        verify(),
        get_info(),
        needs_rehash(),
    ]
}

/// `password_algos(): array`
fn algos() -> Stmt {
    function("password_algos")
        .returns(t_array())
        .returning(array(vec![s("2y"), s("argon2i"), s("argon2id")]))
        .build()
}

/// Returns `substr($value, $start, $length)`.
fn substr_expr(value: Expr, start: Expr, length: Expr) -> Expr {
    call("substr", vec![value, start, length])
}

/// One option read with its PHP default.
fn option(name: &str, default: i64) -> Expr {
    e_null_coalesce(index(v("options"), s(name)), i(default))
}

/// `password_hash(string $password, string $algo = PASSWORD_DEFAULT, array $options = []): string|false`
fn hash() -> Stmt {
    function("password_hash")
        .param("password", TypeExpr::Str)
        .param_default("algo", TypeExpr::Str, e_const("PASSWORD_DEFAULT"))
        .param_default("options", t_array(), e_array(Vec::new()))
        .returns(TypeExpr::Str)
        .body(vec![
            if_then(
                not(or(
                    bin(v("algo"), BinOp::StrictEq, s("argon2i")),
                    bin(v("algo"), BinOp::StrictEq, s("argon2id")),
                )),
                vec![throw_new(
                    "ValueError",
                    s("password_hash(): Argument #2 ($algo) must be a valid password hashing algorithm"),
                )],
            ),
            assign("memory", as_int(option("memory_cost", 65536))),
            assign("time", as_int(option("time_cost", 4))),
            assign("threads", as_int(option("threads", 1))),
            if_then(
                bin(v("threads"), BinOp::StrictNotEq, i(1)),
                vec![throw_new(
                    "ValueError",
                    s("A thread value other than 1 is not supported by this implementation"),
                )],
            ),
            if_then(
                or(
                    bin(v("memory"), BinOp::Lt, i(8)),
                    bin(v("memory"), BinOp::Gt, i(4_294_967_295)),
                ),
                vec![throw_new(
                    "ValueError",
                    s("Memory cost is outside of allowed memory range"),
                )],
            ),
            if_then(
                or(
                    bin(v("time"), BinOp::Lt, i(1)),
                    bin(v("time"), BinOp::Gt, i(4_294_967_295)),
                ),
                vec![throw_new(
                    "ValueError",
                    s("Time cost is outside of allowed time range"),
                )],
            ),
            if_then(
                bin(call("strlen", vec![v("password")]), BinOp::GtEq, i(0xffff_ffff)),
                vec![throw_new("ValueError", s("Password is too long"))],
            ),
            assign(
                "packet",
                call(
                    "elephc_sodium_pwhash_str_alg",
                    vec![
                        call("bin2hex", vec![v("password")]),
                        v("time"),
                        bin(v("memory"), BinOp::Mul, i(1024)),
                        e_ternary(
                            bin(v("algo"), BinOp::StrictEq, s("argon2i")),
                            i(1),
                            i(2),
                        ),
                    ],
                ),
            ),
            if_then(
                bin(substr_expr(v("packet"), i(0), i(1)), BinOp::StrictNotEq, s("O")),
                vec![throw_new("ValueError", s("Unexpected failure hashing password"))],
            ),
            ret(call(
                "hex2bin",
                vec![call("substr", vec![v("packet"), i(1)])],
            )),
        ])
        .build()
}

/// `password_verify(string $password, string $hash): bool`
fn verify() -> Stmt {
    function("password_verify")
        .param("password", TypeExpr::Str)
        .param("hash", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            if_then(
                or(
                    bin(call("substr", vec![v("hash"), i(0), i(10)]), BinOp::StrictEq, s("$argon2id$")),
                    bin(call("substr", vec![v("hash"), i(0), i(9)]), BinOp::StrictEq, s("$argon2i$")),
                ),
                vec![ret(call(
                    "__elephc_sodium_crypto_pwhash_str_verify",
                    vec![b(false), v("hash"), v("password")],
                ))],
            ),
            ret(e_bool(false)),
        ])
        .build()
}

/// `password_get_info(string $hash): array`
fn get_info() -> Stmt {
    function("password_get_info")
        .param("hash", TypeExpr::Str)
        .returns(t_array())
        .body(vec![
            assign(
                "info",
                assoc(vec![
                    (s("algo"), e_null()),
                    (s("algoName"), s("unknown")),
                    (s("options"), e_array(Vec::new())),
                ]),
            ),
            assign("parts", call("explode", vec![s("$"), v("hash")])),
            if_then(
                bin(call("substr", vec![v("hash"), i(0), i(10)]), BinOp::StrictEq, s("$argon2id$")),
                {
                    let mut body = argon2_options();
                    body.push(ret(argon2_info("argon2id")));
                    body
                },
            ),
            if_then(
                bin(call("substr", vec![v("hash"), i(0), i(9)]), BinOp::StrictEq, s("$argon2i$")),
                {
                    let mut body = argon2_options();
                    body.push(ret(argon2_info("argon2i")));
                    body
                },
            ),
            if_then(
                bin(call("substr", vec![v("hash"), i(0), i(4)]), BinOp::StrictEq, s("$2y$")),
                vec![ret(assoc(vec![
                    (s("algo"), s("2y")),
                    (s("algoName"), s("bcrypt")),
                    (
                        s("options"),
                        assoc(vec![(s("cost"), as_int(index(v("parts"), i(2))))]),
                    ),
                ]))],
            ),
            ret(v("info")),
        ])
        .build()
}

/// Parses the `m=...,t=...,p=...` Argon2 option list into locals.
fn argon2_options() -> Vec<Stmt> {
    vec![
        assign("memory", i(65536)),
        assign("time", i(4)),
        assign("threads", i(1)),
        s_foreach(
            call("explode", vec![s(","), index(v("parts"), i(3))]),
            None,
            "pair",
            vec![
                assign("kv", call("explode", vec![s("="), v("pair")])),
                if_then(
                    bin(index(v("kv"), i(0)), BinOp::StrictEq, s("m")),
                    vec![assign("memory", as_int(index(v("kv"), i(1))))],
                ),
                if_then(
                    bin(index(v("kv"), i(0)), BinOp::StrictEq, s("t")),
                    vec![assign("time", as_int(index(v("kv"), i(1))))],
                ),
                if_then(
                    bin(index(v("kv"), i(0)), BinOp::StrictEq, s("p")),
                    vec![assign("threads", as_int(index(v("kv"), i(1))))],
                ),
            ],
        ),
    ]
}

/// Builds the `password_get_info()` result for one Argon2 algorithm name.
fn argon2_info(algo: &str) -> Expr {
    assoc(vec![
        (s("algo"), s(algo)),
        (s("algoName"), s(algo)),
        (
            s("options"),
            assoc(vec![
                (s("memory_cost"), v("memory")),
                (s("time_cost"), v("time")),
                (s("threads"), v("threads")),
            ]),
        ),
    ])
}

/// `password_needs_rehash(string $hash, string $algo, array $options = []): bool`
fn needs_rehash() -> Stmt {
    function("password_needs_rehash")
        .param("hash", TypeExpr::Str)
        .param("algo", TypeExpr::Str)
        .param_default("options", t_array(), e_array(Vec::new()))
        .returns(TypeExpr::Bool)
        .body(vec![
            if_then(
                not(or(
                    bin(v("algo"), BinOp::StrictEq, s("argon2i")),
                    bin(v("algo"), BinOp::StrictEq, s("argon2id")),
                )),
                vec![ret(e_bool(false))],
            ),
            assign("memory", as_int(option("memory_cost", 65536))),
            assign("time", as_int(option("time_cost", 4))),
            assign("threads", as_int(option("threads", 1))),
            if_then(
                bin(v("threads"), BinOp::StrictNotEq, i(1)),
                vec![ret(e_bool(true))],
            ),
            if_then(
                or(
                    bin(v("memory"), BinOp::Lt, i(8)),
                    bin(v("memory"), BinOp::Gt, i(4_294_967_295)),
                ),
                vec![ret(e_bool(true))],
            ),
            if_then(
                or(
                    bin(v("time"), BinOp::Lt, i(1)),
                    bin(v("time"), BinOp::Gt, i(4_294_967_295)),
                ),
                vec![ret(e_bool(true))],
            ),
            ret(call(
                "__elephc_sodium_crypto_pwhash_str_needs_rehash",
                vec![b(false), v("hash"), v("time"), bin(v("memory"), BinOp::Mul, i(1024))],
            )),
        ])
        .build()
}
