//! Purpose:
//! Injects PHP 8.6's `Openssl\OpensslException` and `Openssl\Psk` value classes.
//!
//! Called from:
//! - `crate::pipeline::compile()` after the core and time preludes, before name resolution.
//!
//! Key details:
//! - The classes are declared only for the 8.6 profile, where php-src introduces them.
//! - Injection is demand-driven: any `openssl_*` call or `Openssl\` class reference opts the
//!   program in, matching the extension-loaded surface.
//! - `Psk` is a pure PHP-level value object; the TLS session and PSK transport plumbing lives
//!   in the separately tracked `elephc-tls` work.

use crate::optimize::reachability::PreludeInventory;
use crate::parser::ast::{BinOp, Program, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// The prelude group id used by declaration reachability.
pub(crate) const PRELUDE_GROUP: &str = "openssl";

/// Injects the 8.6 `Openssl` classes when the program references the OpenSSL surface.
pub fn inject_if_used(
    mut program: Program,
    force: bool,
    inventory: &mut PreludeInventory,
) -> Program {
    if crate::codegen_support::compile_php_version() < crate::php_version::PhpVersion::Php86 {
        return program;
    }
    if !force && !program_uses_openssl_classes(&program) {
        return program;
    }
    let mut declarations = declarations();
    inventory.record_program(PRELUDE_GROUP, &declarations);
    declarations.append(&mut program);
    declarations
}

/// Returns whether the program names an `Openssl\` class or an `openssl_*` function.
fn program_uses_openssl_classes(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    usage.classes.iter().any(|name| name.starts_with("openssl\\"))
        || usage.functions.iter().any(|name| name.starts_with("openssl_"))
        || usage
            .literals
            .iter()
            .any(|literal| literal.to_ascii_lowercase().contains("openssl\\"))
}

/// Builds the `Openssl` namespace declarations.
fn declarations() -> Vec<Stmt> {
    crate::source::with_parse_mode(
        crate::source::SourceProfile::new(crate::source::SourceMode::Php),
        || {
            vec![s_namespace(
                "Openssl",
                vec![
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
                ],
            )]
        },
    )
}
