//! Purpose:
//! Declares PHP's `sodium_crypto_box_*` sealed-box surface and `SodiumException` as AST-built
//! declarations over the internal `__elephc_sodium_box` / `__elephc_sodium_status` builtins.
//!
//! Called from:
//! - `crate::pipeline::compile()` twice: beside the hash prelude, before name resolution, so a
//!   differently cased or namespaced call resolves onto the declared function; and in the late
//!   compat-prelude phase, for references only an autoloaded class makes. The second call
//!   injects nothing the first already declared.
//!
//! Key details:
//! - The cryptography is `elephc_crypto::sodium` (libsodium-compatible X25519 +
//!   XSalsa20-Poly1305 sealed boxes). The internal builtin returns the output bytes and records
//!   a status; these wrappers turn a status into PHP's `SodiumException` or `false`, so neither
//!   backend emits a throw from assembly.
//! - Pay-for-use, in two halves. The functions are injected when the program names one (a call
//!   or a literal such as `function_exists('sodium_crypto_box_seal')`). The exception class is
//!   injected with them, and ALSO whenever code the compiler cannot read may raise or name it
//!   (`eval`, a runtime `include`): Magician implements the four functions itself and throws
//!   `SodiumException` by name, so the host program has to carry the class.
//! - A program that declares its own class or function of the same name keeps it.

use crate::parser::ast::{BinOp, Program, Stmt, StmtKind, TypeExpr};
use crate::prelude_prune::usage;
use crate::synthetic_class::{
    class, e_binop, e_call, e_int, e_new_fq, e_str, e_var, function, php_semantics_declarations,
    s_assign, s_if, s_return, s_throw, t_union,
};

/// Prelude inventory group holding the declarations.
pub(crate) const GROUP: &str = "sodium";

/// The exception class the wrappers throw.
const EXCEPTION_CLASS: &str = "SodiumException";

/// Operation codes shared with `elephc_crypto::sodium::SODIUM_OP_*`.
const OP_BOX_KEYPAIR: i64 = 1;
const OP_BOX_PUBLICKEY: i64 = 2;
const OP_BOX_SEAL: i64 = 3;
const OP_BOX_SEAL_OPEN: i64 = 4;

/// Statuses shared with `elephc_crypto::sodium::SODIUM_ERR_*`.
const STATUS_ARG1_LENGTH: i64 = -1;
const STATUS_ARG2_LENGTH: i64 = -2;

/// The PHP functions this prelude declares.
pub(crate) const FUNCTIONS: &[&str] = &[
    "sodium_crypto_box_keypair",
    "sodium_crypto_box_publickey",
    "sodium_crypto_box_seal",
    "sodium_crypto_box_seal_open",
];

/// Builds `$out = __elephc_sodium_box(op, a, b); $status = __elephc_sodium_status();`.
fn call_box(op: i64, first: &str, second: Option<&str>) -> Vec<Stmt> {
    let second = second.map(e_var).unwrap_or_else(|| e_str(""));
    let first = if first.is_empty() { e_str("") } else { e_var(first) };
    vec![
        s_assign("out", e_call("__elephc_sodium_box", vec![e_int(op), first, second])),
        s_assign("status", e_call("__elephc_sodium_status", vec![])),
    ]
}

/// Builds `if ($status === status) { throw new \SodiumException(message); }`.
fn throw_on(status: i64, message: &str) -> Stmt {
    s_if(
        e_binop(e_var("status"), BinOp::StrictEq, e_int(status)),
        vec![s_throw(e_new_fq(EXCEPTION_CLASS, vec![e_str(message)]))],
        Vec::new(),
        None,
    )
}

/// Builds the remaining-failure guard: any other nonzero status is libsodium's "internal error".
fn throw_on_other_failure() -> Stmt {
    s_if(
        e_binop(e_var("status"), BinOp::StrictNotEq, e_int(0)),
        vec![s_throw(e_new_fq(EXCEPTION_CLASS, vec![e_str("internal error")]))],
        Vec::new(),
        None,
    )
}

/// Builds `class SodiumException extends Exception {}`.
fn exception_declaration() -> Stmt {
    class(EXCEPTION_CLASS).extends("Exception").build()
}

/// Builds the four sealed-box functions.
fn function_declarations() -> Vec<Stmt> {
    let keypair = function("sodium_crypto_box_keypair")
        .returns(TypeExpr::Str)
        .body(
            [
                call_box(OP_BOX_KEYPAIR, "", None),
                vec![throw_on_other_failure(), s_return(e_var("out"))],
            ]
            .concat(),
        )
        .build();
    let publickey = function("sodium_crypto_box_publickey")
        .param("key_pair", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(
            [
                call_box(OP_BOX_PUBLICKEY, "key_pair", None),
                vec![
                    throw_on(
                        STATUS_ARG1_LENGTH,
                        "sodium_crypto_box_publickey(): Argument #1 ($key_pair) must be SODIUM_CRYPTO_BOX_KEYPAIRBYTES bytes long",
                    ),
                    throw_on_other_failure(),
                    s_return(e_var("out")),
                ],
            ]
            .concat(),
        )
        .build();
    let seal = function("sodium_crypto_box_seal")
        .param("message", TypeExpr::Str)
        .param("public_key", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(
            [
                call_box(OP_BOX_SEAL, "message", Some("public_key")),
                vec![
                    throw_on(
                        STATUS_ARG2_LENGTH,
                        "sodium_crypto_box_seal(): Argument #2 ($public_key) must be SODIUM_CRYPTO_BOX_PUBLICKEYBYTES bytes long",
                    ),
                    throw_on_other_failure(),
                    s_return(e_var("out")),
                ],
            ]
            .concat(),
        )
        .build();
    let seal_open = function("sodium_crypto_box_seal_open")
        .param("ciphertext", TypeExpr::Str)
        .param("key_pair", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(
            [
                call_box(OP_BOX_SEAL_OPEN, "ciphertext", Some("key_pair")),
                vec![
                    throw_on(
                        STATUS_ARG2_LENGTH,
                        "sodium_crypto_box_seal_open(): Argument #2 ($key_pair) must be SODIUM_CRYPTO_BOX_KEYPAIRBYTES bytes long",
                    ),
                    s_if(
                        e_binop(e_var("status"), BinOp::StrictNotEq, e_int(0)),
                        vec![s_return(crate::synthetic_class::e_bool(false))],
                        Vec::new(),
                        None,
                    ),
                    s_return(e_var("out")),
                ],
            ]
            .concat(),
        )
        .build();
    vec![keypair, publickey, seal, seal_open]
}

/// Returns the declarations to inject: the class when wanted, then the functions when wanted.
pub(crate) fn declarations(with_class: bool, with_functions: bool) -> Program {
    php_semantics_declarations(|| {
        let mut out = Vec::new();
        if with_class {
            out.push(exception_declaration());
        }
        if with_functions {
            out.extend(function_declarations());
        }
        out
    })
}

/// Returns whether a top-level, namespace-less, guarded, synthetic or conditional statement
/// declares `name` as a class (`is_class`) or as a function.
fn program_declares(program: &[Stmt], name: &str, is_class: bool) -> bool {
    program.iter().any(|stmt| match &stmt.kind {
        StmtKind::ClassDecl { name: declared, .. } if is_class => declared.eq_ignore_ascii_case(name),
        StmtKind::FunctionDecl { name: declared, .. } if !is_class => {
            declared.eq_ignore_ascii_case(name)
        }
        StmtKind::NamespaceBlock { name: None, body }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => program_declares(body, name, is_class),
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            program_declares(then_body, name, is_class)
                || elseif_clauses
                    .iter()
                    .any(|(_, arm)| program_declares(arm, name, is_class))
                || else_body
                    .as_deref()
                    .is_some_and(|body| program_declares(body, name, is_class))
        }
        _ => false,
    })
}

/// Prepends the sodium declarations the program may reach and does not declare itself.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    let used = usage::collect(&program);
    // The shared detector matches a call's LAST segment, so `sodium_crypto_box_seal()` written
    // inside a namespace (Symfony's `SodiumVault`) still counts; a literal covers
    // `function_exists('sodium_crypto_box_seal')`.
    let names_function = FUNCTIONS.iter().any(|name| {
        crate::opcache_prelude::detect::program_references(&program, name)
            || used.literals.contains(&crate::names::php_symbol_key(name))
    });
    let with_functions = names_function
        && !FUNCTIONS
            .iter()
            .any(|name| program_declares(&program, name, false));
    let class_key = crate::names::php_symbol_key(EXCEPTION_CLASS);
    let may_reach_class = with_functions
        || used.classes.contains(&class_key)
        || used.literals.contains(&class_key)
        || used.introspects
        || used.includes_runtime_php
        || used.constructs_dynamic_class;
    let with_class = may_reach_class && !program_declares(&program, EXCEPTION_CLASS, true);
    if !with_class && !with_functions {
        return program;
    }
    let mut combined = declarations(with_class, with_functions);
    inventory.record_program(GROUP, &combined);
    combined.extend(program);
    combined
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared_names(program: &Program) -> Vec<String> {
        program
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::ClassDecl { name, .. } | StmtKind::FunctionDecl { name, .. } => {
                    Some(name.clone())
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn declares_the_exception_and_the_four_functions() {
        assert_eq!(
            declared_names(&declarations(true, true)),
            vec![
                "SodiumException",
                "sodium_crypto_box_keypair",
                "sodium_crypto_box_publickey",
                "sodium_crypto_box_seal",
                "sodium_crypto_box_seal_open",
            ]
        );
        assert_eq!(declared_names(&declarations(true, false)), vec!["SodiumException"]);
    }
}
