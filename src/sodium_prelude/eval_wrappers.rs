//! Purpose:
//! Declares hidden by-reference wrappers used by Magician's direct eval dispatch.
//!
//! Called from:
//! - `crate::sodium_prelude::declarations`.
//!
//! Key details:
//! - Eval's declarative builtin values hook receives values, not caller lvalues. These
//!   ordinary prelude functions take the same source expression by reference, so eval's
//!   normal user-function call machinery mutates the caller variable exactly like AOT.
//! - The wrappers are hidden (`__elephc_` prefix), never part of the PHP-visible surface,
//!   and only registered in eval when the sodium prelude is present.
//! - No PHP source is parsed.

use super::dsl::*;
use crate::parser::ast::{BinOp, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Builds every hidden eval wrapper.
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

/// `__elephc_sodium_eval_add(&$string1, $string2): void`
fn add() -> Stmt {
    function("__elephc_sodium_eval_add")
        .param_by_ref("string1", None)
        .param("string2", t_mixed())
        .returns(TypeExpr::Void)
        .body(vec![assign(
            "string1",
            call("__elephc_sodium_add", vec![b(false), v("string1"), v("string2")]),
        )])
        .build()
}

/// `__elephc_sodium_eval_increment(&$string): void`
fn increment() -> Stmt {
    function("__elephc_sodium_eval_increment")
        .param_by_ref("string", None)
        .returns(TypeExpr::Void)
        .body(vec![assign(
            "string",
            call("__elephc_sodium_increment", vec![b(false), v("string")]),
        )])
        .build()
}

/// `__elephc_sodium_eval_memzero(&$string): void`
fn memzero() -> Stmt {
    function("__elephc_sodium_eval_memzero")
        .param_by_ref("string", None)
        .returns(TypeExpr::Void)
        .body(vec![
            expr(call("__elephc_sodium_memzero", vec![b(false), v("string")])),
            assign("string", e_null()),
        ])
        .build()
}

/// `__elephc_sodium_eval_crypto_generichash_update(&$state, $message): bool`
fn generichash_update() -> Stmt {
    function("__elephc_sodium_eval_crypto_generichash_update")
        .param_by_ref("state", None)
        .param("message", t_mixed())
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

/// `__elephc_sodium_eval_crypto_generichash_final(&$state, $length = 32): string`
fn generichash_final() -> Stmt {
    function("__elephc_sodium_eval_crypto_generichash_final")
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

/// `__elephc_sodium_eval_crypto_secretstream_xchacha20poly1305_push(&$state, $message, $ad = '', $tag = 0): string`
fn secretstream_push() -> Stmt {
    function("__elephc_sodium_eval_crypto_secretstream_xchacha20poly1305_push")
        .param_by_ref("state", None)
        .param("message", t_mixed())
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

/// `__elephc_sodium_eval_crypto_secretstream_xchacha20poly1305_pull(&$state, $ciphertext, $ad = ''): array|false`
fn secretstream_pull() -> Stmt {
    function("__elephc_sodium_eval_crypto_secretstream_xchacha20poly1305_pull")
        .param_by_ref("state", None)
        .param("ciphertext", t_mixed())
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

/// `__elephc_sodium_eval_crypto_secretstream_xchacha20poly1305_rekey(&$state): void`
fn secretstream_rekey() -> Stmt {
    function("__elephc_sodium_eval_crypto_secretstream_xchacha20poly1305_rekey")
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
