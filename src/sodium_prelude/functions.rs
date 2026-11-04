//! Purpose:
//! Builds every PHP-visible `ext/sodium` helper as a synthetic AST function over the
//! `elephc_sodium` bridge.
//!
//! Called from:
//! - `crate::sodium_prelude::declarations`.
//!
//! Key details:
//! - Most helpers are generated from `spec.rs`: coerce each parameter, apply php-src's
//!   length checks, call the bridge with hexadecimal operands, then decode the packet.
//! - Specially shaped helpers own incremental states, password hashing, base64 variants,
//!   padding and secretstream framing because their observable result is not one packet
//!   value.
//! - No PHP source is parsed.

use super::dsl::*;
use super::spec::{Default, Param, ParamKind, ResultKind, Shape, Spec};
use crate::parser::ast::{BinOp, Expr, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Builds every helper declaration.
pub(super) fn declarations() -> Vec<Stmt> {
    super::spec::specs()
        .into_iter()
        .map(|spec| build(&spec))
        .collect()
}

/// Builds one helper from its specification.
fn build(spec: &Spec) -> Stmt {
    let helper = helper_name(spec.name);
    let mut builder = function(&helper).param("strict", TypeExpr::Bool);
    for param in &spec.params {
        builder = match param.default {
            Some(Default::Str(value)) => builder.param_default(param.name, t_mixed(), s(value)),
            Some(Default::Int(value)) => builder.param_default(param.name, t_mixed(), i(value)),
            None => builder.param(param.name, t_mixed()),
        };
    }
    let mut body = vec![assign("_function", s(spec.name))];
    for (index, param) in spec.params.iter().enumerate() {
        // php-src's increment/add/memzero read the first argument as an lvalue and reject a
        // non-string with `SodiumException`, not by coercing it through a `string` parameter.
        if index == 0
            && matches!(spec.shape, Shape::Add | Shape::Increment | Shape::Memzero)
        {
            continue;
        }
        body.push(assign(&local(index), coerce(param, index)));
    }
    body.extend(shape_body(spec, helper_result_type(spec)));
    builder.returns(helper_result_type(spec)).body(body).build()
}

/// Returns the helper name for one public function.
fn helper_name(name: &str) -> String {
    format!("__elephc_sodium_{}", name.strip_prefix("sodium_").unwrap_or(name))
}

/// Returns the local variable holding one parameter.
fn local(index: usize) -> String {
    format!("_a{index}")
}

/// Builds one parameter's coercion expression.
fn coerce(param: &Param, index: usize) -> Expr {
    match param.kind {
        ParamKind::Str => call(
            "__elephc_sodium_string_param",
            vec![
                v(param.name),
                v("strict"),
                v("_function"),
                i(index as i64 + 1),
                s(param.name),
            ],
        ),
        ParamKind::Int => call(
            "__elephc_sodium_int_param",
            vec![
                v(param.name),
                v("strict"),
                v("_function"),
                i(index as i64 + 1),
                s(param.name),
            ],
        ),
    }
}

/// Builds the result type declared by one helper.
///
/// Mutating shapes return the advanced string (or `string|false`) even when the public
/// function is `void` or `array`, because the lowering performs the write-back and the
/// public shaping itself.
fn helper_result_type(spec: &Spec) -> TypeExpr {
    match spec.shape {
        Shape::Add
        | Shape::Increment
        | Shape::Memzero
        | Shape::SecretstreamPush
        | Shape::SecretstreamRekey
        | Shape::GenerichashUpdate
        | Shape::GenerichashFinal => return TypeExpr::Str,
        Shape::SecretstreamPull => return t_union(vec![TypeExpr::Str, TypeExpr::Bool]),
        Shape::SecretstreamInitPush | Shape::KxSessionKeys => return t_array(),
        _ => {}
    }
    match spec.result {
        ResultKind::Str => TypeExpr::Str,
        ResultKind::StrOrFalse => t_union(vec![TypeExpr::Str, TypeExpr::Bool]),
        ResultKind::Bool => TypeExpr::Bool,
        ResultKind::Int => TypeExpr::Int,
        ResultKind::Void => TypeExpr::Void,
        ResultKind::Array => t_array(),
    }
}

/// Builds the body after parameter coercion.
fn shape_body(spec: &Spec, _result: TypeExpr) -> Vec<Stmt> {
    let _ = _result;
    match spec.shape {
        Shape::Plain => plain_body(spec),
        Shape::Add => add_body(spec),
        Shape::Increment => increment_body(spec),
        Shape::Memzero => memzero_body(),
        Shape::Compare | Shape::Memcmp => compare_body(spec),
        Shape::Base642bin => base64_decode_body(spec),
        Shape::Bin2base64 => base64_encode_body(spec),
        Shape::Hex2bin => hex_decode_body(spec),
        Shape::Pad => pad_body(spec),
        Shape::Unpad => unpad_body(spec),
        Shape::Generichash => generichash_body(spec),
        Shape::GenerichashInit => generichash_init_body(spec),
        Shape::GenerichashUpdate => generichash_update_body(spec),
        Shape::GenerichashFinal => generichash_final_body(spec),
        Shape::KxSessionKeys => kx_session_keys_body(spec),
        Shape::Pwhash => pwhash_body(spec),
        Shape::PwhashStr => pwhash_str_body(spec),
        Shape::PwhashStrVerify => pwhash_str_verify_body(spec),
        Shape::PwhashStrNeedsRehash => plain_body(spec),
        Shape::Scrypt => scrypt_body(spec),
        Shape::ScryptStr => scrypt_str_body(spec),
        Shape::ScryptStrVerify => scrypt_str_verify_body(spec),
        Shape::SecretstreamInitPush => secretstream_init_push_body(spec),
        Shape::SecretstreamPush => secretstream_push_body(spec),
        Shape::SecretstreamPull => secretstream_pull_body(spec),
        Shape::SecretstreamRekey => secretstream_rekey_body(spec),
    }
}

/// Emits the standard length checks for one spec.
fn length_checks(spec: &Spec) -> Vec<Stmt> {
    spec.lens
        .iter()
        .map(|check| {
            if_then(
                bin(
                    call("strlen", vec![v(&local(check.param))]),
                    BinOp::StrictNotEq,
                    i(check.len),
                ),
                vec![expr(call(
                    "__elephc_sodium_arg_error",
                    vec![
                        v("_function"),
                        i(check.param as i64 + 1),
                        s(spec.params[check.param].name),
                        s(&format!("must be {} bytes long", check.symbol)),
                    ],
                ))],
            )
        })
        .collect()
}

/// Builds the bridge call for one spec.
fn bridge_call(spec: &Spec) -> Expr {
    if spec.name == "sodium_crypto_pwhash" {
        return call(
            "elephc_sodium_pwhash",
            vec![
                call("bin2hex", vec![v(&local(1))]),
                call("bin2hex", vec![v(&local(2))]),
                v(&local(0)),
                v(&local(3)),
                v(&local(4)),
                v(&local(5)),
            ],
        );
    }
    let mut strings = Vec::new();
    let mut integers = Vec::new();
    for (index, param) in spec.params.iter().enumerate() {
        match param.kind {
            ParamKind::Str => strings.push(call("bin2hex", vec![v(&local(index))])),
            ParamKind::Int => integers.push(v(&local(index))),
        }
    }
    while strings.len() < 4 {
        strings.push(s(""));
    }
    while integers.len() < 3 {
        integers.push(i(0));
    }
    let mut args = vec![s(spec.name)];
    args.extend(strings);
    args.extend(integers);
    call("elephc_sodium_call", args)
}

/// Assigns the bridge packet to `$_packet`.
fn packet_statement(spec: &Spec) -> Stmt {
    assign("_packet", bridge_call(spec))
}

/// Builds a standard plain helper body.
fn plain_body(spec: &Spec) -> Vec<Stmt> {
    let mut body = domain_checks(spec);
    body.extend(length_checks(spec));
    body.push(packet_statement(spec));
    body.push(ret(result_expression(spec)));
    body
}

/// Adds php-src's integer domain checks that are not plain length checks.
fn domain_checks(spec: &Spec) -> Vec<Stmt> {
    match spec.name {
        "sodium_crypto_stream" | "sodium_crypto_stream_xchacha20" => vec![if_then(
            bin(v(&local(0)), BinOp::LtEq, i(0)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(1), s("length"), s("must be greater than 0")],
            ))],
        )],
        "sodium_crypto_kdf_derive_from_key" => vec![
            if_then(
                bin(v(&local(0)), BinOp::Lt, i(16)),
                vec![expr(call(
                    "__elephc_sodium_arg_error",
                    vec![
                        v("_function"),
                        i(1),
                        s("subkey_length"),
                        s("must be greater than or equal to SODIUM_CRYPTO_KDF_BYTES_MIN"),
                    ],
                ))],
            ),
            if_then(
                bin(v(&local(0)), BinOp::Gt, i(64)),
                vec![expr(call(
                    "__elephc_sodium_arg_error",
                    vec![
                        v("_function"),
                        i(1),
                        s("subkey_length"),
                        s("must be less than or equal to SODIUM_CRYPTO_KDF_BYTES_MAX"),
                    ],
                ))],
            ),
            if_then(
                bin(v(&local(1)), BinOp::Lt, i(0)),
                vec![expr(call(
                    "__elephc_sodium_arg_error",
                    vec![v("_function"), i(2), s("subkey_id"), s("must be greater than or equal to 0")],
                ))],
            ),
        ],
        _ => Vec::new(),
    }
}

/// Returns the expression decoding a standard packet according to the result kind.
fn result_expression(spec: &Spec) -> Expr {
    match spec.result {
        ResultKind::Str | ResultKind::Void => {
            call("__elephc_sodium_string_result", vec![v("_packet"), v("_function")])
        }
        ResultKind::StrOrFalse => call(
            "__elephc_sodium_string_or_false_result",
            vec![v("_packet"), v("_function")],
        ),
        ResultKind::Bool => call("__elephc_sodium_bool_result", vec![v("_packet")]),
        ResultKind::Int => call("__elephc_sodium_int_result", vec![v("_packet")]),
        ResultKind::Array => call("__elephc_sodium_string_result", vec![v("_packet"), v("_function")]),
    }
}

/// Builds `sodium_add`'s helper.
fn add_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        string_lvalue_check("string1", "PHP strings are required"),
        assign("_a0", v("string1")),
        if_then(
            bin(call("strlen", vec![v(&local(0))]), BinOp::StrictNotEq, call("strlen", vec![v(&local(1))])),
            vec![expr(call("__elephc_sodium_same_length", vec![v("_function")]))],
        ),
        packet_statement(spec),
        ret(call(
            "__elephc_sodium_string_result",
            vec![v("_packet"), v("_function")],
        )),
    ]
}

/// Builds `sodium_increment`'s helper.
fn increment_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        string_lvalue_check("string", "a PHP string is required"),
        assign("_a0", v("string")),
        packet_statement(spec),
        ret(call(
            "__elephc_sodium_string_result",
            vec![v("_packet"), v("_function")],
        )),
    ]
}

/// Builds `sodium_memzero`'s helper: the lowering turns the empty result into null.
fn memzero_body() -> Vec<Stmt> {
    vec![
        string_lvalue_check("string", "a PHP string is required"),
        ret(s("")),
    ]
}

/// Emits the `SodiumException` php-src raises for a non-string lvalue argument.
fn string_lvalue_check(name: &str, message: &str) -> Stmt {
    if_then(
        not(call("is_string", vec![v(name)])),
        vec![throw_new("SodiumException", s(message))],
    )
}

/// Builds `sodium_compare`/`sodium_memcmp`.
fn compare_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        if_then(
            bin(call("strlen", vec![v(&local(0))]), BinOp::StrictNotEq, call("strlen", vec![v(&local(1))])),
            vec![expr(call("__elephc_sodium_same_length", vec![v("_function")]))],
        ),
        packet_statement(spec),
        ret(call("__elephc_sodium_int_result", vec![v("_packet")])),
    ]
}

/// Builds `sodium_base642bin`.
fn base64_decode_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        variant_check(),
        packet_statement(spec),
        if_then(
            bin(index(v("_packet"), i(0)), BinOp::StrictEq, s("B")),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(1), s("string"), s("must be a valid base64 string")],
            ))],
        ),
        ret(call(
            "__elephc_sodium_string_result",
            vec![v("_packet"), v("_function")],
        )),
    ]
}

/// Builds `sodium_bin2base64`.
fn base64_encode_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        variant_check(),
        packet_statement(spec),
        ret(call(
            "__elephc_sodium_string_result",
            vec![v("_packet"), v("_function")],
        )),
    ]
}

/// Emits the shared base64-variant validation.
fn variant_check() -> Stmt {
    if_then(
        not(call(
            "in_array",
            vec![v(&local(1)), array(vec![i(1), i(3), i(5), i(7)]), b(true)],
        )),
        vec![expr(call(
            "__elephc_sodium_arg_error",
            vec![
                v("_function"),
                i(2),
                s("id"),
                s("must be a valid base64 variant identifier"),
            ],
        ))],
    )
}

/// Builds `sodium_hex2bin`.
fn hex_decode_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        packet_statement(spec),
        if_then(
            bin(index(v("_packet"), i(0)), BinOp::StrictEq, s("H")),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(1),
                    s("string"),
                    s("must be a valid hexadecimal string"),
                ],
            ))],
        ),
        ret(call(
            "__elephc_sodium_string_result",
            vec![v("_packet"), v("_function")],
        )),
    ]
}

/// Builds `sodium_pad`.
fn pad_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        block_size_check(2, "block_size"),
        packet_statement(spec),
        ret(call(
            "__elephc_sodium_string_result",
            vec![v("_packet"), v("_function")],
        )),
    ]
}

/// Builds `sodium_unpad`.
fn unpad_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        block_size_check(2, "block_size"),
        if_then(
            bin(call("strlen", vec![v(&local(0))]), BinOp::Lt, v(&local(1))),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(1),
                    s("string"),
                    s("must be at least as long as the block size"),
                ],
            ))],
        ),
        packet_statement(spec),
        if_then(
            bin(index(v("_packet"), i(0)), BinOp::StrictEq, s("P")),
            vec![throw_new("SodiumException", s("invalid padding"))],
        ),
        ret(call(
            "__elephc_sodium_string_result",
            vec![v("_packet"), v("_function")],
        )),
    ]
}

/// Emits the shared positive block-size check.
fn block_size_check(position: i64, name: &str) -> Stmt {
    if_then(
        bin(v(&local(1)), BinOp::LtEq, i(0)),
        vec![expr(call(
            "__elephc_sodium_arg_error",
            vec![
                v("_function"),
                i(position),
                s(name),
                s("must be greater than 0"),
            ],
        ))],
    )
}

/// Builds `sodium_crypto_generichash`.
fn generichash_body(spec: &Spec) -> Vec<Stmt> {
    let mut body = generichash_range_checks(1, 2);
    body.push(packet_statement(spec));
    body.push(ret(call(
        "__elephc_sodium_string_result",
        vec![v("_packet"), v("_function")],
    )));
    body
}

/// Builds `sodium_crypto_generichash_init`.
fn generichash_init_body(spec: &Spec) -> Vec<Stmt> {
    let mut body = generichash_range_checks(0, 1);
    body.push(packet_statement(spec));
    body.push(ret(call(
        "__elephc_sodium_string_result",
        vec![v("_packet"), v("_function")],
    )));
    body
}

/// Emits output-length and key-length checks for the BLAKE2b family.
fn generichash_range_checks(key_index: usize, length_index: usize) -> Vec<Stmt> {
    vec![
        if_then(
            or(
                bin(v(&local(length_index)), BinOp::Lt, i(16)),
                bin(v(&local(length_index)), BinOp::Gt, i(64)),
            ),
            vec![throw_new("SodiumException", s("unsupported output length"))],
        ),
        if_then(
            and(
                bin(call("strlen", vec![v(&local(key_index))]), BinOp::StrictNotEq, i(0)),
                or(
                    bin(call("strlen", vec![v(&local(key_index))]), BinOp::Lt, i(16)),
                    bin(call("strlen", vec![v(&local(key_index))]), BinOp::Gt, i(64)),
                ),
            ),
            vec![throw_new("SodiumException", s("unsupported key length"))],
        ),
    ]
}

/// Emits the BLAKE2b output-length range check without a key check.
fn generichash_output_length_check(length_index: usize) -> Stmt {
    if_then(
        or(
            bin(v(&local(length_index)), BinOp::Lt, i(16)),
            bin(v(&local(length_index)), BinOp::Gt, i(64)),
        ),
        vec![throw_new("SodiumException", s("unsupported output length"))],
    )
}

/// Builds `sodium_crypto_generichash_update`.
fn generichash_update_body(spec: &Spec) -> Vec<Stmt> {
    let mut body = state_checks("incorrect state length");
    body.push(packet_statement(spec));
    body.push(ret(call(
        "__elephc_sodium_string_result",
        vec![v("_packet"), v("_function")],
    )));
    body
}

/// Builds `sodium_crypto_generichash_final`.
fn generichash_final_body(spec: &Spec) -> Vec<Stmt> {
    let mut body = state_checks("incorrect state length");
    body.push(generichash_output_length_check(1));
    body.push(packet_statement(spec));
    body.push(ret(call(
        "__elephc_sodium_string_result",
        vec![v("_packet"), v("_function")],
    )));
    body
}

/// Emits the shared incremental-state type and length check.
fn state_checks(message: &str) -> Vec<Stmt> {
    vec![
        if_then(
            not(call("is_string", vec![v(&local(0))])),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(1),
                    s("state"),
                    s("must be a reference to a state"),
                ],
            ))],
        ),
        if_then(
            bin(call("strlen", vec![v(&local(0))]), BinOp::StrictNotEq, i(384)),
            vec![throw_new("SodiumException", s(message))],
        ),
    ]
}

/// Builds key-exchange session keys.
fn kx_session_keys_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        packet_statement(spec),
        assign(
            "_keys",
            call(
                "__elephc_sodium_string_result",
                vec![v("_packet"), v("_function")],
            ),
        ),
        ret(array(vec![
            call("substr", vec![v("_keys"), i(0), i(32)]),
            call("substr", vec![v("_keys"), i(32), i(32)]),
        ])),
    ]
}

/// Builds `sodium_crypto_pwhash`.
fn pwhash_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        if_then(
            bin(v(&local(0)), BinOp::LtEq, i(0)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(1), s("length"), s("must be greater than 0")],
            ))],
        ),
        if_then(
            bin(v(&local(0)), BinOp::GtEq, i(0xffff_ffff)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(1), s("length"), s("is too large")],
            ))],
        ),
        if_then(
            bin(call("strlen", vec![v(&local(1))]), BinOp::GtEq, i(0xffff_ffff)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(2), s("password"), s("is too long")],
            ))],
        ),
        if_then(
            bin(v(&local(3)), BinOp::LtEq, i(0)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(4), s("opslimit"), s("must be greater than 0")],
            ))],
        ),
        if_then(
            bin(v(&local(4)), BinOp::LtEq, i(0)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(5), s("memlimit"), s("must be greater than 0")],
            ))],
        ),
        if_then(
            not(call(
                "in_array",
                vec![v(&local(5)), array(vec![i(1), i(2)]), b(true)],
            )),
            vec![throw_new("SodiumException", s("unsupported password hashing algorithm"))],
        ),
        empty_password_warning(1),
        if_then(
            bin(call("strlen", vec![v(&local(2))]), BinOp::StrictNotEq, i(16)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(3),
                    s("salt"),
                    s("must be SODIUM_CRYPTO_PWHASH_SALTBYTES bytes long"),
                ],
            ))],
        ),
        if_then(
            bin(v(&local(3)), BinOp::Lt, i(1)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(4),
                    s("opslimit"),
                    s("must be greater than or equal to 1"),
                ],
            ))],
        ),
        if_then(
            bin(v(&local(4)), BinOp::Lt, i(8192)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(5),
                    s("memlimit"),
                    s("must be greater than or equal to 8192"),
                ],
            ))],
        ),
        packet_statement(spec),
        ret(call(
            "__elephc_sodium_string_result",
            vec![v("_packet"), v("_function")],
        )),
    ]
}

/// Builds `sodium_crypto_pwhash_str`.
fn pwhash_str_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        if_then(
            bin(v(&local(1)), BinOp::LtEq, i(0)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(2), s("opslimit"), s("must be greater than 0")],
            ))],
        ),
        if_then(
            bin(v(&local(2)), BinOp::LtEq, i(0)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(3), s("memlimit"), s("must be greater than 0")],
            ))],
        ),
        if_then(
            bin(call("strlen", vec![v(&local(0))]), BinOp::GtEq, i(0xffff_ffff)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(1), s("password"), s("is too long")],
            ))],
        ),
        empty_password_warning(0),
        if_then(
            bin(v(&local(1)), BinOp::Lt, i(1)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(2),
                    s("opslimit"),
                    s("must be greater than or equal to 1"),
                ],
            ))],
        ),
        if_then(
            bin(v(&local(2)), BinOp::Lt, i(8192)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(3),
                    s("memlimit"),
                    s("must be greater than or equal to 8192"),
                ],
            ))],
        ),
        packet_statement(spec),
        ret(call(
            "__elephc_sodium_string_result",
            vec![v("_packet"), v("_function")],
        )),
    ]
}

/// Builds `sodium_crypto_pwhash_str_verify`.
fn pwhash_str_verify_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        if_then(
            bin(call("strlen", vec![v(&local(1))]), BinOp::GtEq, i(0xffff_ffff)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(2), s("password"), s("is too long")],
            ))],
        ),
        empty_password_warning(1),
        packet_statement(spec),
        ret(call("__elephc_sodium_bool_result", vec![v("_packet")])),
    ]
}

/// Builds `sodium_crypto_pwhash_scryptsalsa208sha256`.
fn scrypt_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        if_then(
            or(
                bin(v(&local(0)), BinOp::LtEq, i(0)),
                bin(v(&local(0)), BinOp::Gt, i(0x1ffff_ffe0)),
            ),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(1), s("length"), s("must be greater than 0")],
            ))],
        ),
        if_then(
            bin(v(&local(3)), BinOp::LtEq, i(0)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(4), s("opslimit"), s("must be greater than 0")],
            ))],
        ),
        if_then(
            bin(v(&local(4)), BinOp::LtEq, i(0)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(5), s("memlimit"), s("must be greater than 0")],
            ))],
        ),
        empty_password_warning(1),
        if_then(
            bin(call("strlen", vec![v(&local(2))]), BinOp::StrictNotEq, i(32)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(3),
                    s("salt"),
                    s("must be SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_SALTBYTES bytes long"),
                ],
            ))],
        ),
        if_then(
            bin(v(&local(3)), BinOp::Lt, i(524288)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(4),
                    s("opslimit"),
                    s("must be greater than or equal to 524288"),
                ],
            ))],
        ),
        if_then(
            bin(v(&local(4)), BinOp::Lt, i(16777216)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(5),
                    s("memlimit"),
                    s("must be greater than or equal to 16777216"),
                ],
            ))],
        ),
        packet_statement(spec),
        ret(call(
            "__elephc_sodium_string_result",
            vec![v("_packet"), v("_function")],
        )),
    ]
}

/// Builds `sodium_crypto_pwhash_scryptsalsa208sha256_str`.
fn scrypt_str_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        if_then(
            bin(v(&local(1)), BinOp::LtEq, i(0)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(2), s("opslimit"), s("must be greater than 0")],
            ))],
        ),
        if_then(
            bin(v(&local(2)), BinOp::LtEq, i(0)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(3), s("memlimit"), s("must be greater than 0")],
            ))],
        ),
        empty_password_warning(0),
        if_then(
            bin(v(&local(1)), BinOp::Lt, i(524288)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(2),
                    s("opslimit"),
                    s("must be greater than or equal to 524288"),
                ],
            ))],
        ),
        if_then(
            bin(v(&local(2)), BinOp::Lt, i(16777216)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(3),
                    s("memlimit"),
                    s("must be greater than or equal to 16777216"),
                ],
            ))],
        ),
        packet_statement(spec),
        ret(call(
            "__elephc_sodium_string_result",
            vec![v("_packet"), v("_function")],
        )),
    ]
}

/// Builds `sodium_crypto_pwhash_scryptsalsa208sha256_str_verify`.
fn scrypt_str_verify_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        empty_password_warning(1),
        if_then(
            bin(call("strlen", vec![v(&local(0))]), BinOp::StrictNotEq, i(101)),
            vec![
                expr(call(
                    "__elephc_sodium_warning",
                    vec![v("_function"), s("wrong size for the hashed password")],
                )),
                ret(e_bool(false)),
            ],
        ),
        packet_statement(spec),
        ret(call("__elephc_sodium_bool_result", vec![v("_packet")])),
    ]
}

/// Emits php-src's empty-password warning for one string parameter.
fn empty_password_warning(index: usize) -> Stmt {
    if_then(
        bin(call("strlen", vec![v(&local(index))]), BinOp::StrictEq, i(0)),
        vec![expr(call(
            "__elephc_sodium_warning",
            vec![v("_function"), s("empty password")],
        ))],
    )
}

/// Builds `sodium_crypto_secretstream_xchacha20poly1305_init_push`.
fn secretstream_init_push_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        packet_statement(spec),
        assign(
            "_raw",
            call(
                "__elephc_sodium_string_result",
                vec![v("_packet"), v("_function")],
            ),
        ),
        ret(array(vec![
            call("substr", vec![v("_raw"), i(0), i(52)]),
            call("substr", vec![v("_raw"), i(52), i(24)]),
        ])),
    ]
}

/// Builds the secretstream push helper, which returns `state || ciphertext`.
fn secretstream_push_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        if_then(
            bin(call("strlen", vec![v(&local(0))]), BinOp::StrictNotEq, i(52)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(1),
                    s("state"),
                    s("must have a correct length"),
                ],
            ))],
        ),
        if_then(
            bin(call("strlen", vec![v(&local(1))]), BinOp::Gt, i(274_877_906_816)),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![
                    v("_function"),
                    i(2),
                    s("message"),
                    s("must be at most SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_MESSAGEBYTES_MAX bytes long"),
                ],
            ))],
        ),
        if_then(
            or(bin(v(&local(3)), BinOp::Lt, i(0)), bin(v(&local(3)), BinOp::Gt, i(255))),
            vec![expr(call(
                "__elephc_sodium_arg_error",
                vec![v("_function"), i(4), s("tag"), s("must be in the range of 0-255")],
            ))],
        ),
        packet_statement(spec),
        ret(call(
            "__elephc_sodium_string_result",
            vec![v("_packet"), v("_function")],
        )),
    ]
}

/// Builds the secretstream pull helper, which returns `state || plaintext || tag` or false.
fn secretstream_pull_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        if_then(
            bin(call("strlen", vec![v(&local(0))]), BinOp::StrictNotEq, i(52)),
            vec![throw_new("SodiumException", s("incorrect state length"))],
        ),
        packet_statement(spec),
        ret(call(
            "__elephc_sodium_string_or_false_result",
            vec![v("_packet"), v("_function")],
        )),
    ]
}

/// Builds the secretstream rekey helper, which returns the advanced state.
fn secretstream_rekey_body(spec: &Spec) -> Vec<Stmt> {
    vec![
        if_then(
            bin(call("strlen", vec![v(&local(0))]), BinOp::StrictNotEq, i(52)),
            vec![throw_new("SodiumException", s("incorrect state length"))],
        ),
        packet_statement(spec),
        ret(call(
            "__elephc_sodium_string_result",
            vec![v("_packet"), v("_function")],
        )),
    ]
}

/// Builds the split helpers used by the lowering to unpack secretstream framing.
pub(super) fn framing_helpers() -> Vec<Stmt> {
    vec![
        function("__elephc_sodium_take_state")
            .param("raw", TypeExpr::Str)
            .returns(TypeExpr::Str)
            .returning(call("substr", vec![v("raw"), i(0), i(52)])),
        function("__elephc_sodium_take_payload")
            .param("raw", TypeExpr::Str)
            .returns(TypeExpr::Str)
            .returning(call("substr", vec![v("raw"), i(52)])),
        function("__elephc_sodium_pull_state")
            .param("raw", t_union(vec![TypeExpr::Str, TypeExpr::Bool]))
            .param("original", TypeExpr::Str)
            .returns(TypeExpr::Str)
            .body(vec![
                if_then(bin(v("raw"), BinOp::StrictEq, e_bool(false)), vec![ret(v("original"))]),
                ret(call("substr", vec![v("raw"), i(0), i(52)])),
            ]),
        function("__elephc_sodium_pull_result")
            .param("raw", t_union(vec![TypeExpr::Str, TypeExpr::Bool]))
            .returns(t_union(vec![t_array(), TypeExpr::Bool]))
            .body(vec![
                if_then(bin(v("raw"), BinOp::StrictEq, e_bool(false)), vec![ret(e_bool(false))]),
                ret(array(vec![
                    call("substr", vec![v("raw"), i(52), e_neg(i(1))]),
                    call("ord", vec![call("substr", vec![v("raw"), e_neg(i(1))])]),
                ])),
            ]),
    ]
    .into_iter()
    .map(|builder| builder.build())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every spec yields one helper with the expected name.
    #[test]
    fn every_spec_builds_one_helper() {
        let declarations = declarations();
        assert_eq!(declarations.len(), super::super::spec::specs().len());
        let mut names = std::collections::HashSet::new();
        for statement in &declarations {
            if let crate::parser::ast::StmtKind::FunctionDecl { name, .. } = &statement.kind {
                names.insert(name.clone());
            }
        }
        for spec in super::super::spec::specs() {
            assert!(names.contains(&helper_name(spec.name)), "missing {}", spec.name);
        }
    }
}
