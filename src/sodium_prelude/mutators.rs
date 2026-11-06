//! Purpose:
//! Declares the PHP-visible by-reference sodium mutators as synthetic prelude functions.
//!
//! Called from:
//! - `crate::sodium_prelude::declarations`.
//!
//! Key details:
//! - These are ordinary PHP functions with real `&$` parameters, so AOT and eval both use the
//!   compiler's normal by-reference call machinery for locals, array elements and properties.
//! - Their bodies delegate the cryptography to the hidden `__elephc_sodium_*` helpers, which
//!   remain by-value implementation details.
//! - No PHP source is parsed.

use super::dsl::*;
use crate::parser::ast::{BinOp, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Builds every PHP-visible by-reference sodium mutator.
pub(super) fn declarations() -> Vec<Stmt> {
    vec![
        add(),
        increment(),
        memzero(),
        generichash_update(),
        generichash_final(),
        secretstream_push(),
        secretstream_pull(),
        secretstream_rekey(),
    ]
}

/// `sodium_add(string &$string1, string $string2): void`
fn add() -> Stmt {
    function("sodium_add")
        .param_by_ref("string1", None)
        .param("string2", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![assign(
            "string1",
            call("__elephc_sodium_add", vec![b(false), v("string1"), v("string2")]),
        )])
        .build()
}

/// `sodium_increment(string &$string): void`
fn increment() -> Stmt {
    function("sodium_increment")
        .param_by_ref("string", None)
        .returns(TypeExpr::Void)
        .body(vec![assign(
            "string",
            call("__elephc_sodium_increment", vec![b(false), v("string")]),
        )])
        .build()
}

/// `sodium_memzero(string &$string): void`
fn memzero() -> Stmt {
    function("sodium_memzero")
        .param_by_ref("string", None)
        .returns(TypeExpr::Void)
        .body(vec![
            expr(call("__elephc_sodium_memzero", vec![b(false), v("string")])),
            expr(call("__elephc_str_zero", vec![v("string")])),
            assign("string", e_null()),
        ])
        .build()
}

/// `sodium_crypto_generichash_update(string &$state, string $message): bool`
fn generichash_update() -> Stmt {
    function("sodium_crypto_generichash_update")
        .param_by_ref("state", None)
        .param("message", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            assign(
                "state",
                call(
                    "__elephc_sodium_crypto_generichash_update",
                    vec![b(false), v("state"), v("message")],
                ),
            ),
            ret(b(true)),
        ])
        .build()
}

/// `sodium_crypto_generichash_final(string &$state, int $length = 32): string`
fn generichash_final() -> Stmt {
    function("sodium_crypto_generichash_final")
        .param_by_ref("state", None)
        .param_default("length", TypeExpr::Int, i(32))
        .returns(TypeExpr::Str)
        .body(vec![
            assign(
                "result",
                call(
                    "__elephc_sodium_crypto_generichash_final",
                    vec![b(false), v("state"), v("length")],
                ),
            ),
            assign("state", e_null()),
            ret(v("result")),
        ])
        .build()
}

/// `sodium_crypto_secretstream_xchacha20poly1305_push(string &$state, string $message, string $additional_data = '', int $tag = 0): string`
fn secretstream_push() -> Stmt {
    function("sodium_crypto_secretstream_xchacha20poly1305_push")
        .param_by_ref("state", None)
        .param("message", TypeExpr::Str)
        .param_default("additional_data", TypeExpr::Str, s(""))
        .param_default("tag", TypeExpr::Int, i(0))
        .returns(TypeExpr::Str)
        .body(vec![
            assign(
                "raw",
                call(
                    "__elephc_sodium_crypto_secretstream_xchacha20poly1305_push",
                    vec![b(false), v("state"), v("message"), v("additional_data"), v("tag")],
                ),
            ),
            assign("state", call("substr", vec![v("raw"), i(0), i(52)])),
            ret(call("substr", vec![v("raw"), i(52)])),
        ])
        .build()
}

/// `sodium_crypto_secretstream_xchacha20poly1305_pull(string &$state, string $ciphertext, string $additional_data = ''): array|false`
fn secretstream_pull() -> Stmt {
    function("sodium_crypto_secretstream_xchacha20poly1305_pull")
        .param_by_ref("state", None)
        .param("ciphertext", TypeExpr::Str)
        .param_default("additional_data", TypeExpr::Str, s(""))
        .returns(t_union(vec![t_array(), TypeExpr::Bool]))
        .body(vec![
            assign(
                "raw",
                call(
                    "__elephc_sodium_crypto_secretstream_xchacha20poly1305_pull",
                    vec![b(false), v("state"), v("ciphertext"), v("additional_data")],
                ),
            ),
            if_then(
                bin(v("raw"), BinOp::StrictEq, e_bool(false)),
                vec![ret(e_bool(false))],
            ),
            assign("state", call("substr", vec![v("raw"), i(0), i(52)])),
            ret(array(vec![
                call("substr", vec![v("raw"), i(52), e_neg(i(1))]),
                call("ord", vec![call("substr", vec![v("raw"), e_neg(i(1))])]),
            ])),
        ])
        .build()
}

/// `sodium_crypto_secretstream_xchacha20poly1305_rekey(string &$state): void`
fn secretstream_rekey() -> Stmt {
    function("sodium_crypto_secretstream_xchacha20poly1305_rekey")
        .param_by_ref("state", None)
        .returns(TypeExpr::Void)
        .body(vec![assign(
            "state",
            call(
                "__elephc_sodium_crypto_secretstream_xchacha20poly1305_rekey",
                vec![b(false), v("state")],
            ),
        )])
        .build()
}
