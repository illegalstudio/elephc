//! Purpose:
//! Injects PHP 8.6's `Openssl\OpensslException`, `Openssl\Psk` and `Openssl\Session`
//! value classes, plus the session bridge externs.
//!
//! Called from:
//! - `crate::pipeline::compile()` after the core and time preludes, before name resolution.
//!
//! Key details:
//! - The classes and `OPENSSL_ENCODING_*` constants are 8.6-only; the session externs
//!   link `elephc_openssl`, whose managed package now supplies both libssl and libcrypto.
//! - Injection is demand-driven: any `openssl_*` call or `Openssl\` class reference opts
//!   the program in. The libssl-backed externs and `Session` class are emitted only when
//!   `Openssl\Session` is named, so Psk-only programs keep their pure-PHP link surface.
//! - `Psk` and `Session` are PHP-level value objects; TLS stream context integration
//!   (session/PSK/early-data options) remains in the separately tracked `elephc-tls` work.

use crate::optimize::reachability::PreludeInventory;
use crate::parser::ast::{BinOp, CType, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// The prelude group id used by declaration reachability.
pub(crate) const PRELUDE_GROUP: &str = "openssl";

/// Bridge field selector for `SSL_SESSION_get_timeout`.
const SESSION_FIELD_TIMEOUT: i64 = 0;
/// Bridge field selector for `SSL_SESSION_get_time_ex`.
const SESSION_FIELD_CREATED_AT: i64 = 1;
/// Bridge field selector for `SSL_SESSION_is_resumable`.
const SESSION_FIELD_RESUMABLE: i64 = 2;
/// Bridge field selector for `SSL_SESSION_has_ticket`.
const SESSION_FIELD_HAS_TICKET: i64 = 3;
/// Bridge field selector for `SSL_SESSION_get_ticket_lifetime_hint`.
const SESSION_FIELD_TICKET_LIFETIME_HINT: i64 = 4;
/// Bridge selector for `SSL_SESSION_get_protocol_version`.
const SESSION_KIND_PROTOCOL: i64 = 0;
/// Bridge selector for `SSL_SESSION_get0_cipher`.
const SESSION_KIND_CIPHER: i64 = 1;

/// Injects the 8.6 `Openssl` classes when the program references the OpenSSL surface.
pub fn inject_if_used(
    mut program: Program,
    force: bool,
    inventory: &mut PreludeInventory,
) -> Program {
    if crate::codegen_support::compile_php_version() < crate::php_version::PhpVersion::Php86 {
        return program;
    }
    if !force && !requested(&program) {
        return program;
    }
    let includes_session = force || program_uses_session(&program);
    let mut declarations = declarations(includes_session);
    inventory.record_program(PRELUDE_GROUP, &declarations);
    declarations.append(&mut program);
    declarations
}

/// Returns whether the user program names an `Openssl\` class or an `openssl_*` function.
///
/// Called from `crate::pipeline` before compiler preludes are injected, so a
/// stream-error helper mentioning `Openssl\Session` cannot force the libssl
/// bridge into every program that reads a file.
pub fn requested(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    usage.classes.iter().any(|name| name.starts_with("openssl\\"))
        || usage.functions.iter().any(|name| name.starts_with("openssl_"))
        || usage
            .literals
            .iter()
            .any(|literal| literal.to_ascii_lowercase().contains("openssl\\"))
}

/// Returns whether the program names `Openssl\Session` in a class reference or literal.
fn program_uses_session(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    usage.classes.iter().any(|name| names_session(name))
        || usage.literals.iter().any(|literal| names_session(literal))
}

/// Recognizes `Openssl\Session` regardless of a leading slash or ASCII case.
fn names_session(name: &str) -> bool {
    name.trim_start_matches('\\').eq_ignore_ascii_case("openssl\\session")
}

/// Builds the `Openssl` namespace declarations and, when requested, the session externs.
fn declarations(includes_session: bool) -> Vec<Stmt> {
    let mut declarations = Vec::new();
    if includes_session {
        declarations.extend(session_externs());
    }
    let mut classes = vec![
        class("OpensslException").extends("\\Exception").build(),
        class("Psk")
                .final_()
                .constant("MAX_PSK_LEN", e_int(256))
                .constant("MAX_IDENTITY_LEN", e_int(128))
                .readonly_prop("psk", TypeExpr::Str)
                .readonly_prop("identity", t_nullable(TypeExpr::Str))
                .method(
                    method("__construct")
                        .param("psk", TypeExpr::Str)
                        .param_default("identity", t_nullable(TypeExpr::Str), e_null())
                        .body(vec![
                            s_if(
                                e_binop(
                                    e_call("strlen", vec![e_var("psk")]),
                                    BinOp::StrictEq,
                                    e_int(0),
                                ),
                                vec![s_throw(e_new_fq(
                                    "ValueError",
                                    vec![e_str(
                                        "Openssl\\Psk::__construct(): Argument #1 ($psk) must not be empty",
                                    )],
                                ))],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(
                                    e_call("strlen", vec![e_var("psk")]),
                                    BinOp::Gt,
                                    e_int(256),
                                ),
                                vec![s_throw(e_new_fq(
                                    "ValueError",
                                    vec![e_str(
                                        "Openssl\\Psk::__construct(): Argument #1 ($psk) must not exceed 256 bytes",
                                    )],
                                ))],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(
                                    e_binop(
                                        e_var("identity"),
                                        BinOp::StrictNotEq,
                                        e_null(),
                                    ),
                                    BinOp::And,
                                    e_binop(
                                        e_call("strlen", vec![e_var("identity")]),
                                        BinOp::Gt,
                                        e_int(128),
                                    ),
                                ),
                                vec![s_throw(e_new_fq(
                                    "ValueError",
                                    vec![e_str(
                                        "Openssl\\Psk::__construct(): Argument #2 ($identity) must not exceed 128 bytes",
                                    )],
                                ))],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "psk", e_var("psk")),
                            s_prop_assign(e_this(), "identity", e_var("identity")),
                        ]),
                )
                .build(),
    ];
    if includes_session {
        classes.push(session_class());
    }
    declarations.push(s_namespace("Openssl", classes));
    declarations
}

/// Declares the libssl session entry points the `Openssl\Session` class calls.
fn session_externs() -> Vec<Stmt> {
    vec![
        extern_fn("elephc_openssl_session_import", "elephc_openssl")
            .param("data", CType::Str)
            .param("length", CType::Int)
            .param("format", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_openssl_session_export", "elephc_openssl")
            .param("der_hex", CType::Str)
            .param("length", CType::Int)
            .param("format", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_openssl_session_id", "elephc_openssl")
            .param("der_hex", CType::Str)
            .param("length", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_openssl_session_protocol", "elephc_openssl")
            .param("der_hex", CType::Str)
            .param("length", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_openssl_session_cipher", "elephc_openssl")
            .param("der_hex", CType::Str)
            .param("length", CType::Int)
            .returns(CType::Str)
            .build(),
        extern_fn("elephc_openssl_session_field", "elephc_openssl")
            .param("der_hex", CType::Str)
            .param("length", CType::Int)
            .param("field", CType::Int)
            .returns(CType::Int)
            .build(),
    ]
}

/// Builds the final `Openssl\Session` declaration.
fn session_class() -> Stmt {
    class("Session")
        .final_()
        .readonly_prop("id", TypeExpr::Str)
        .private_prop("__elephc_der", TypeExpr::Str, Some(e_str("")))
        .private_prop("__elephc_initialized", TypeExpr::Bool, Some(e_bool(false)))
        .method(session_constructor())
        .method(session_export())
        .method(session_import())
        .method(session_is_resumable())
        .method(session_get_timeout())
        .method(session_get_created_at())
        .method(session_get_protocol())
        .method(session_get_cipher())
        .method(session_has_ticket())
        .method(session_get_ticket_lifetime_hint())
        .method(session_serialize())
        .method(session_unserialize())
        .method(session_debug_info())
        .method(session_require_der())
        .method(session_read_int())
        .method(session_read_string())
        .build()
}

/// `Openssl\Session::__construct()` — PHP blocks direct construction.
fn session_constructor() -> MethodBuilder {
    method("__construct").body(vec![s_throw(e_new_fq(
        "Error",
        vec![e_str(
            "Cannot directly construct Openssl\\Session, use Openssl\\Session::import() or TLS session callbacks",
        )],
    ))])
}

/// Returns whether `format` is one of the two supported session encodings.
fn valid_session_format(var: &str) -> Expr {
    e_binop(
        e_binop(
            e_var(var),
            BinOp::StrictNotEq,
            e_const("OPENSSL_ENCODING_DER"),
        ),
        BinOp::And,
        e_binop(
            e_var(var),
            BinOp::StrictNotEq,
            e_const("OPENSSL_ENCODING_PEM"),
        ),
    )
}

/// `Openssl\Session::export(int $format = OPENSSL_ENCODING_PEM): string`.
fn session_export() -> MethodBuilder {
    method("export")
        .param_default("format", TypeExpr::Int, e_const("OPENSSL_ENCODING_PEM"))
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign(
                "_der",
                e_method_call(e_this(), "__elephc_require_der", vec![]),
            ),
            s_if(
                valid_session_format("format"),
                vec![s_throw(e_new_fq(
                    "ValueError",
                    vec![e_str(
                        "Openssl\\Session::export(): Argument #1 ($format) must be OPENSSL_ENCODING_DER or OPENSSL_ENCODING_PEM",
                    )],
                ))],
                vec![],
                None,
            ),
            s_assign(
                "_encoded",
                e_call(
                    "elephc_openssl_session_export",
                    vec![
                        e_var("_der"),
                        e_call("strlen", vec![e_var("_der")]),
                        e_var("format"),
                    ],
                ),
            ),
            s_if(
                e_binop(e_var("_encoded"), BinOp::StrictEq, e_str("")),
                vec![s_throw(e_new_fq(
                    "Openssl\\OpensslException",
                    vec![e_ternary(
                        e_binop(
                            e_var("format"),
                            BinOp::StrictEq,
                            e_const("OPENSSL_ENCODING_DER"),
                        ),
                        e_str("Failed to export session"),
                        e_str("Failed to export session as PEM"),
                    )],
                ))],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_var("format"),
                    BinOp::StrictEq,
                    e_const("OPENSSL_ENCODING_DER"),
                ),
                vec![s_return(e_call("hex2bin", vec![e_var("_encoded")]))],
                vec![],
                None,
            ),
            s_return(e_var("_encoded")),
        ])
}

/// `Openssl\Session::import(string $data, int $format = OPENSSL_ENCODING_PEM): Session`.
fn session_import() -> MethodBuilder {
    method("import")
        .static_()
        .param("data", TypeExpr::Str)
        .param_default("format", TypeExpr::Int, e_const("OPENSSL_ENCODING_PEM"))
        .returns(t_class("\\Openssl\\Session"))
        .body(vec![
            s_if(
                valid_session_format("format"),
                vec![s_throw(e_new_fq(
                    "ValueError",
                    vec![e_str(
                        "Openssl\\Session::import(): Argument #2 ($format) must be OPENSSL_ENCODING_DER or OPENSSL_ENCODING_PEM",
                    )],
                ))],
                vec![],
                None,
            ),
            s_assign(
                "_der",
                e_call(
                    "elephc_openssl_session_import",
                    vec![
                        e_var("data"),
                        e_call("strlen", vec![e_var("data")]),
                        e_var("format"),
                    ],
                ),
            ),
            s_if(
                e_binop(e_var("_der"), BinOp::StrictEq, e_str("")),
                vec![s_throw(e_new_fq(
                    "Openssl\\OpensslException",
                    vec![e_str("Failed to import session data")],
                ))],
                vec![],
                None,
            ),
            s_assign(
                "_reflection",
                e_new_fq("ReflectionClass", vec![e_self_class()]),
            ),
            s_assign(
                "_session",
                e_method_call(
                    e_var("_reflection"),
                    "newInstanceWithoutConstructor",
                    vec![],
                ),
            ),
            s_prop_assign(e_var("_session"), "__elephc_der", e_var("_der")),
            s_prop_assign(e_var("_session"), "__elephc_initialized", e_bool(true)),
            session_id_assignment(
                e_var("_session"),
                e_call(
                    "hex2bin",
                    vec![e_call(
                        "elephc_openssl_session_id",
                        vec![e_var("_der"), e_call("strlen", vec![e_var("_der")])],
                    )],
                ),
            ),
            s_return(e_var("_session")),
        ])
}

/// Assigns the readonly `id` while giving the runtime guard its own synthetic node span.
fn session_id_assignment(object: Expr, value: Expr) -> Stmt {
    let mut statement = s_prop_assign(object, "id", value);
    statement.span = crate::span::Span::synthetic();
    statement
}

/// `Openssl\Session::isResumable(): bool`.
fn session_is_resumable() -> MethodBuilder {
    method("isResumable").returns(TypeExpr::Bool).returning(e_binop(
        e_method_call(
            e_this(),
            "__elephc_session_int",
            vec![e_int(SESSION_FIELD_RESUMABLE)],
        ),
        BinOp::StrictEq,
        e_int(1),
    ))
}

/// `Openssl\Session::getTimeout(): int`.
fn session_get_timeout() -> MethodBuilder {
    method("getTimeout")
        .returns(TypeExpr::Int)
        .returning(e_method_call(
            e_this(),
            "__elephc_session_int",
            vec![e_int(SESSION_FIELD_TIMEOUT)],
        ))
}

/// `Openssl\Session::getCreatedAt(): int`.
fn session_get_created_at() -> MethodBuilder {
    method("getCreatedAt")
        .returns(TypeExpr::Int)
        .returning(e_method_call(
            e_this(),
            "__elephc_session_int",
            vec![e_int(SESSION_FIELD_CREATED_AT)],
        ))
}

/// `Openssl\Session::getProtocol(): ?string`.
fn session_get_protocol() -> MethodBuilder {
    method("getProtocol")
        .returns(t_nullable(TypeExpr::Str))
        .returning(e_method_call(
            e_this(),
            "__elephc_session_string",
            vec![e_int(SESSION_KIND_PROTOCOL)],
        ))
}

/// `Openssl\Session::getCipher(): ?string`.
fn session_get_cipher() -> MethodBuilder {
    method("getCipher")
        .returns(t_nullable(TypeExpr::Str))
        .returning(e_method_call(
            e_this(),
            "__elephc_session_string",
            vec![e_int(SESSION_KIND_CIPHER)],
        ))
}

/// `Openssl\Session::hasTicket(): bool`.
fn session_has_ticket() -> MethodBuilder {
    method("hasTicket").returns(TypeExpr::Bool).returning(e_binop(
        e_method_call(
            e_this(),
            "__elephc_session_int",
            vec![e_int(SESSION_FIELD_HAS_TICKET)],
        ),
        BinOp::StrictEq,
        e_int(1),
    ))
}

/// `Openssl\Session::getTicketLifetimeHint(): ?int`.
fn session_get_ticket_lifetime_hint() -> MethodBuilder {
    method("getTicketLifetimeHint")
        .returns(t_nullable(TypeExpr::Int))
        .body(vec![
            s_if(
                e_not(e_method_call(e_this(), "hasTicket", vec![])),
                vec![s_return(e_null())],
                vec![],
                None,
            ),
            s_return(e_method_call(
                e_this(),
                "__elephc_session_int",
                vec![e_int(SESSION_FIELD_TICKET_LIFETIME_HINT)],
            )),
        ])
}

/// `Openssl\Session::__serialize(): array` — php-src's single `pem` entry.
fn session_serialize() -> MethodBuilder {
    method("__serialize")
        .returns(TypeExpr::AssocArray {
            key: Box::new(TypeExpr::Str),
            value: Box::new(TypeExpr::Str),
        })
        .body(vec![s_return(
        e_array_assoc(vec![(
            e_str("pem"),
            e_method_call(e_this(), "export", vec![e_const("OPENSSL_ENCODING_PEM")]),
        )]),
    )])
}

/// `Openssl\Session::__unserialize(array $data): void`.
fn session_unserialize() -> MethodBuilder {
    method("__unserialize")
        .param("data", TypeExpr::Array(Box::new(t_mixed())))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_this_prop("__elephc_initialized"),
                vec![s_throw(e_new_fq(
                    "Error",
                    vec![e_str(
                        "Cannot call Openssl\\Session::__unserialize() on an already-initialized session",
                    )],
                ))],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_not(e_call(
                        "array_key_exists",
                        vec![e_str("pem"), e_var("data")],
                    )),
                    BinOp::Or,
                    e_not(e_call(
                        "is_string",
                        vec![e_index(e_var("data"), e_str("pem"))],
                    )),
                ),
                vec![s_throw(e_new_fq(
                    "Openssl\\OpensslException",
                    vec![e_str("Invalid serialization data")],
                ))],
                vec![],
                None,
            ),
            s_assign(
                "_pem",
                e_index(e_var("data"), e_str("pem")),
            ),
            s_assign(
                "_der",
                e_call(
                    "elephc_openssl_session_import",
                    vec![
                        e_var("_pem"),
                        e_call("strlen", vec![e_var("_pem")]),
                        e_const("OPENSSL_ENCODING_PEM"),
                    ],
                ),
            ),
            s_if(
                e_binop(e_var("_der"), BinOp::StrictEq, e_str("")),
                vec![s_throw(e_new_fq(
                    "Openssl\\OpensslException",
                    vec![e_str("Failed to unserialize session")],
                ))],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "__elephc_der", e_var("_der")),
            s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
            s_prop_assign(
                e_this(),
                "id",
                e_call(
                    "hex2bin",
                    vec![e_call(
                        "elephc_openssl_session_id",
                        vec![e_var("_der"), e_call("strlen", vec![e_var("_der")])],
                    )],
                ),
            ),
        ])
}

/// `Openssl\Session::__debugInfo(): array` — hide the private bridge storage.
fn session_debug_info() -> MethodBuilder {
    method("__debugInfo")
        .returns(TypeExpr::AssocArray {
            key: Box::new(TypeExpr::Str),
            value: Box::new(TypeExpr::Str),
        })
        .body(vec![
        s_if(
            e_not(e_this_prop("__elephc_initialized")),
            vec![s_return(e_array(vec![]))],
            vec![],
            None,
        ),
        s_return(e_array_assoc(vec![(e_str("id"), e_this_prop("id"))])),
    ])
}

/// Hidden helper: returns the canonical DER hex or throws `Session is not valid`.
fn session_require_der() -> MethodBuilder {
    method("__elephc_require_der")
        .private()
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_this_prop("__elephc_initialized")),
                vec![s_throw(e_new_fq(
                    "Openssl\\OpensslException",
                    vec![e_str("Session is not valid")],
                ))],
                vec![],
                None,
            ),
            s_return(e_this_prop("__elephc_der")),
        ])
}

/// Hidden helper: reads one integer session metadata field.
fn session_read_int() -> MethodBuilder {
    method("__elephc_session_int")
        .private()
        .param("field", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign(
                "_der",
                e_method_call(e_this(), "__elephc_require_der", vec![]),
            ),
            s_assign(
                "_value",
                e_call(
                    "elephc_openssl_session_field",
                    vec![
                        e_var("_der"),
                        e_call("strlen", vec![e_var("_der")]),
                        e_var("field"),
                    ],
                ),
            ),
            s_if(
                e_binop(e_var("_value"), BinOp::StrictEq, e_const("PHP_INT_MIN")),
                vec![s_throw(e_new_fq(
                    "Openssl\\OpensslException",
                    vec![e_str("Session is not valid")],
                ))],
                vec![],
                None,
            ),
            s_return(e_var("_value")),
        ])
}

/// Hidden helper: reads the protocol label (`kind = 0`) or cipher name (`kind = 1`).
fn session_read_string() -> MethodBuilder {
    method("__elephc_session_string")
        .private()
        .param("kind", TypeExpr::Int)
        .returns(t_nullable(TypeExpr::Str))
        .body(vec![
            s_assign(
                "_der",
                e_method_call(e_this(), "__elephc_require_der", vec![]),
            ),
            s_assign(
                "_value",
                e_ternary(
                    e_binop(e_var("kind"), BinOp::StrictEq, e_int(SESSION_KIND_PROTOCOL)),
                    e_call(
                        "elephc_openssl_session_protocol",
                        vec![e_var("_der"), e_call("strlen", vec![e_var("_der")])],
                    ),
                    e_call(
                        "elephc_openssl_session_cipher",
                        vec![e_var("_der"), e_call("strlen", vec![e_var("_der")])],
                    ),
                ),
            ),
            s_if(
                e_binop(e_var("_value"), BinOp::StrictEq, e_str("")),
                vec![s_return(e_null())],
                vec![],
                None,
            ),
            s_return(e_var("_value")),
        ])
}
