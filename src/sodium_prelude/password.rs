//! Purpose:
//! Declares the core `password` extension's Argon2 and bcrypt providers on top of the
//! `elephc_sodium` bridge.
//!
//! Called from:
//! - `crate::sodium_prelude::declarations`.
//!
//! Key details:
//! - `password_hash()`, `password_verify()`, `password_get_info()`,
//!   `password_needs_rehash()` and `password_algos()` are PHP-visible synthetic functions,
//!   not registry builtins: the sodium prelude is injected for password calls too.
//! - The Argon2i/Argon2id string formats, option validation and messages match php-src 8.5's
//!   sodium password adapter (`ext/sodium/sodium_pwhash.c`); bcrypt (`2y`) is implemented by
//!   the bridge's Rust bcrypt provider, so `PASSWORD_DEFAULT` is functional.
//! - `PASSWORD_ARGON2_PROVIDER` is `"sodium"` because the Argon2 implementation is libsodium.
//! - No PHP source is parsed.

use super::dsl::*;
use crate::names::Name;
use crate::parser::ast::{BinOp, Expr, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Builds the password extension declarations contributed by the sodium prelude.
pub(super) fn declarations() -> Vec<Stmt> {
    vec![
        identify_helper(),
        argon2_params_helper(),
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

/// `string|int|null` for the password algorithm selector.
fn algo_type() -> TypeExpr {
    t_union(vec![
        TypeExpr::Str,
        TypeExpr::Int,
        TypeExpr::Named(Name::unqualified("null")),
    ])
}

/// Returns `substr($value, $start, $length)`.
fn substr_expr(value: Expr, start: Expr, length: Expr) -> Expr {
    call("substr", vec![value, start, length])
}

/// Returns whether `$algo` is one of the supported algorithm names.
fn supported_algo() -> Expr {
    or(
        or(
            bin(v("algo_name"), BinOp::StrictEq, s("2y")),
            bin(v("algo_name"), BinOp::StrictEq, s("argon2i")),
        ),
        bin(v("algo_name"), BinOp::StrictEq, s("argon2id")),
    )
}

/// Normalizes a string/int/null algorithm selector into `$algo_name`.
fn normalize_algo() -> Vec<Stmt> {
    vec![
        assign("algo_name", v("algo")),
        if_then(
            call("is_int", vec![v("algo_name")]),
            vec![
                if_then(
                    or(
                        bin(v("algo_name"), BinOp::StrictEq, i(0)),
                        bin(v("algo_name"), BinOp::StrictEq, i(1)),
                    ),
                    vec![assign("algo_name", s("2y"))],
                ),
                if_then(
                    bin(v("algo_name"), BinOp::StrictEq, i(2)),
                    vec![assign("algo_name", s("argon2i"))],
                ),
                if_then(
                    bin(v("algo_name"), BinOp::StrictEq, i(3)),
                    vec![assign("algo_name", s("argon2id"))],
                ),
            ],
        ),
        if_then(
            call("is_null", vec![v("algo_name")]),
            vec![assign("algo_name", s("2y"))],
        ),
    ]
}

/// One option read, preserving PHP's `zval_get_long(null) == 0` behavior.
fn option(name: &str, default: i64) -> Expr {
    e_ternary(
        call("array_key_exists", vec![s(name), v("options")]),
        index(v("options"), s(name)),
        i(default),
    )
}

/// `throw new ValueError(...)` for an unknown algorithm selector.
fn invalid_algo() -> Stmt {
    throw_new(
        "ValueError",
        s("password_hash(): Argument #2 ($algo) must be a valid password hashing algorithm"),
    )
}

/// `password_hash(string $password, string|int|null $algo, array $options = []): string`
fn hash() -> Stmt {
    let mut body = normalize_algo();
    body.push(if_then(
        not(call("is_string", vec![v("algo_name")])),
        vec![invalid_algo()],
    ));
    body.push(s_if(
        bin(v("algo_name"), BinOp::StrictEq, s("2y")),
        bcrypt_body(),
        Vec::new(),
        Some(argon2_body()),
    ));
    function("password_hash")
        .param("password", TypeExpr::Str)
        .param("algo", algo_type())
        .param_default("options", t_array(), e_array(Vec::new()))
        .returns(TypeExpr::Str)
        .body(body)
        .build()
}

/// The `PASSWORD_BCRYPT` branch of `password_hash`.
fn bcrypt_body() -> Vec<Stmt> {
    vec![
        assign("cost", as_int(option("cost", 12))),
        if_then(
            bin(
                call("strpos", vec![v("password"), s("\0")]),
                BinOp::StrictNotEq,
                e_bool(false),
            ),
            vec![throw_new(
                "ValueError",
                s("Bcrypt password must not contain null character"),
            )],
        ),
        if_then(
            or(bin(v("cost"), BinOp::Lt, i(4)), bin(v("cost"), BinOp::Gt, i(31))),
            vec![throw_new(
                "ValueError",
                concat(
                    s("Invalid bcrypt cost parameter specified: "),
                    as_str(v("cost")),
                ),
            )],
        ),
        assign(
            "packet",
            call(
                "elephc_sodium_call",
                vec![
                    s("password_hash_bcrypt"),
                    call("bin2hex", vec![v("password")]),
                    s(""),
                    s(""),
                    s(""),
                    v("cost"),
                    i(0),
                    i(0),
                ],
            ),
        ),
        if_then(
            bin(index(v("packet"), i(0)), BinOp::StrictNotEq, s("O")),
            vec![throw_new("ValueError", s("Unexpected failure hashing password"))],
        ),
        ret(call(
            "hex2bin",
            vec![call("substr", vec![v("packet"), i(1)])],
        )),
    ]
}

/// The Argon2i/Argon2id branch of `password_hash`.
fn argon2_body() -> Vec<Stmt> {
    vec![
        if_then(
            not(or(
                bin(v("algo_name"), BinOp::StrictEq, s("argon2i")),
                bin(v("algo_name"), BinOp::StrictEq, s("argon2id")),
            )),
            vec![invalid_algo()],
        ),
        if_then(
            bin(call("strlen", vec![v("password")]), BinOp::GtEq, i(0xffff_ffff)),
            vec![throw_new("ValueError", s("Password is too long"))],
        ),
        assign("memory", as_int(option("memory_cost", 65536))),
        assign("time", as_int(option("time_cost", 4))),
        assign("threads", as_int(option("threads", 1))),
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
            bin(v("threads"), BinOp::StrictNotEq, i(1)),
            vec![throw_new(
                "ValueError",
                s("A thread value other than 1 is not supported by this implementation"),
            )],
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
                        bin(v("algo_name"), BinOp::StrictEq, s("argon2i")),
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
    ]
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
                    bin(call("strlen", vec![v("password")]), BinOp::GtEq, i(0xffff_ffff)),
                    bin(call("strlen", vec![v("hash")]), BinOp::GtEq, i(0xffff_ffff)),
                ),
                vec![ret(e_bool(false))],
            ),
            assign(
                "algo_name",
                call("__elephc_password_identify", vec![v("hash")]),
            ),
            if_then(
                bin(v("algo_name"), BinOp::StrictEq, s("2y")),
                vec![
                    assign(
                        "packet",
                        call(
                            "elephc_sodium_call",
                            vec![
                                s("password_verify_bcrypt"),
                                call("bin2hex", vec![v("password")]),
                                call("bin2hex", vec![v("hash")]),
                                s(""),
                                s(""),
                                i(0),
                                i(0),
                                i(0),
                            ],
                        ),
                    ),
                    if_then(
                        bin(index(v("packet"), i(0)), BinOp::StrictEq, s("M")),
                        vec![ret(bin(
                            as_int(call("substr", vec![v("packet"), i(1)])),
                            BinOp::StrictNotEq,
                            i(0),
                        ))],
                    ),
                    ret(e_bool(false)),
                ],
            ),
            if_then(
                or(
                    bin(v("algo_name"), BinOp::StrictEq, s("argon2id")),
                    bin(v("algo_name"), BinOp::StrictEq, s("argon2i")),
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
                "algo_name",
                call("__elephc_password_identify", vec![v("hash")]),
            ),
            if_then(
                bin(v("algo_name"), BinOp::StrictEq, s("2y")),
                vec![ret(assoc(vec![
                    (s("algo"), s("2y")),
                    (s("algoName"), s("bcrypt")),
                    (
                        s("options"),
                        assoc(vec![(
                            s("cost"),
                            as_int(call("substr", vec![v("hash"), i(4), i(2)])),
                        )]),
                    ),
                ]))],
            ),
            if_then(
                or(
                    bin(v("algo_name"), BinOp::StrictEq, s("argon2id")),
                    bin(v("algo_name"), BinOp::StrictEq, s("argon2i")),
                ),
                vec![
                    assign(
                        "params",
                        call("__elephc_password_argon2_params", vec![v("hash")]),
                    ),
                    ret(assoc(vec![
                        (s("algo"), v("algo_name")),
                        (s("algoName"), v("algo_name")),
                        (
                            s("options"),
                            assoc(vec![
                                (s("memory_cost"), index(v("params"), i(0))),
                                (s("time_cost"), index(v("params"), i(1))),
                                (s("threads"), index(v("params"), i(2))),
                            ]),
                        ),
                    ])),
                ],
            ),
            ret(assoc(vec![
                (s("algo"), e_null()),
                (s("algoName"), s("unknown")),
                (s("options"), e_array(Vec::new())),
            ])),
        ])
        .build()
}

/// `password_needs_rehash(string $hash, string|int|null $algo, array $options = []): bool`
fn needs_rehash() -> Stmt {
    let mut body = normalize_algo();
    body.push(if_then(
        not(supported_algo()),
        vec![ret(e_bool(false))],
    ));
    body.extend([
        assign("old_algo", call("__elephc_password_identify", vec![v("hash")])),
        if_then(
            bin(v("old_algo"), BinOp::StrictEq, s("unknown")),
            vec![ret(e_bool(true))],
        ),
        if_then(
            bin(v("old_algo"), BinOp::StrictNotEq, v("algo_name")),
            vec![ret(e_bool(true))],
        ),
        if_then(
            bin(v("algo_name"), BinOp::StrictEq, s("2y")),
            vec![ret(bin(
                as_int(call("substr", vec![v("hash"), i(4), i(2)])),
                BinOp::StrictNotEq,
                as_int(option("cost", 12)),
            ))],
        ),
        assign(
            "params",
            call("__elephc_password_argon2_params", vec![v("hash")]),
        ),
        ret(or(
            or(
                bin(index(v("params"), i(0)), BinOp::StrictNotEq, as_int(option("memory_cost", 65536))),
                bin(index(v("params"), i(1)), BinOp::StrictNotEq, as_int(option("time_cost", 4))),
            ),
            bin(index(v("params"), i(2)), BinOp::StrictNotEq, as_int(option("threads", 1))),
        )),
    ]);
    function("password_needs_rehash")
        .param("hash", TypeExpr::Str)
        .param("algo", algo_type())
        .param_default("options", t_array(), e_array(Vec::new()))
        .returns(TypeExpr::Bool)
        .body(body)
        .build()
}

/// `__elephc_password_identify(string $hash): string`
fn identify_helper() -> Stmt {
    function("__elephc_password_identify")
        .param("hash", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            if_then(
                bin(substr_expr(v("hash"), i(0), i(10)), BinOp::StrictEq, s("$argon2id$")),
                vec![ret(s("argon2id"))],
            ),
            if_then(
                bin(substr_expr(v("hash"), i(0), i(9)), BinOp::StrictEq, s("$argon2i$")),
                vec![ret(s("argon2i"))],
            ),
            if_then(
                and(
                    bin(call("strlen", vec![v("hash")]), BinOp::StrictEq, i(60)),
                    bin(substr_expr(v("hash"), i(0), i(3)), BinOp::StrictEq, s("$2y")),
                ),
                vec![ret(s("2y"))],
            ),
            ret(s("unknown")),
        ])
        .build()
}

/// `__elephc_password_argon2_params(string $hash): array{int,int,int}`
fn argon2_params_helper() -> Stmt {
    function("__elephc_password_argon2_params")
        .param("hash", TypeExpr::Str)
        .returns(t_array())
        .body(vec![
            assign("memory", i(65536)),
            assign("time", i(4)),
            assign("threads", i(1)),
            if_then(
                bin(substr_expr(v("hash"), i(0), i(10)), BinOp::StrictEq, s("$argon2id$")),
                vec![assign("rest", call("substr", vec![v("hash"), i(10)]))],
            ),
            if_then(
                bin(substr_expr(v("hash"), i(0), i(9)), BinOp::StrictEq, s("$argon2i$")),
                vec![assign("rest", call("substr", vec![v("hash"), i(9)]))],
            ),
            if_then(
                not(call("isset", vec![v("rest")])),
                vec![ret(array(vec![v("memory"), v("time"), v("threads")]))],
            ),
            assign("parts", call("explode", vec![s("$"), v("rest")])),
            assign(
                "options_text",
                e_ternary(
                    bin(substr_expr(index(v("parts"), i(0)), i(0), i(2)), BinOp::StrictEq, s("v=")),
                    index(v("parts"), i(1)),
                    index(v("parts"), i(0)),
                ),
            ),
            s_foreach(
                call("explode", vec![s(","), v("options_text")]),
                None,
                "pair",
                vec![
                    assign("kv", call("explode", vec![s("="), v("pair")])),
                    if_then(
                        bin(call("count", vec![v("kv")]), BinOp::GtEq, i(2)),
                        vec![
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
                ],
            ),
            ret(array(vec![v("memory"), v("time"), v("threads")])),
        ])
        .build()
}
