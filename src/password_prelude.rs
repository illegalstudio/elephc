//! Purpose:
//! Declares PHP's password hashing API (`password_hash`, `password_verify`,
//! `password_needs_rehash`, `password_get_info`, `password_algos`) as AST-built declarations
//! over two `elephc_crypto` bridge externs.
//!
//! Called from:
//! - `crate::pipeline::compile()` in the compat-prelude phase, after autoload expansion, so a
//!   call only an autoloaded class makes (Symfony's `NativePasswordHasher`) is seen.
//!
//! Key details:
//! - The algorithm is bcrypt (`$2y$`), the default PHP ships and the one `password_hash()`
//!   produces with `PASSWORD_DEFAULT`/`PASSWORD_BCRYPT`. Verification and hashing run in
//!   `elephc_crypto::password`; the bookkeeping php does in C (reading the cost back out of a
//!   hash, validating options) is written here in PHP terms.
//! - Argon2 is not provided: `PASSWORD_ARGON2I`/`PASSWORD_ARGON2ID` stay undefined, which is
//!   how php itself reports a build without libargon2, and `password_verify()` answers false for
//!   an Argon2 hash rather than guessing.
//! - Pay-for-use: injected only when the program names one of the functions, and never over a
//!   declaration of the same name the program makes itself.

use crate::parser::ast::{BinOp, CType, CastType, Program, Stmt, StmtKind, TypeExpr};
use crate::synthetic_class::{
    e_array, e_array_assoc, e_binop, e_call, e_cast, e_index, e_int, e_new_fq, e_null,
    e_null_coalesce, e_str, e_var, extern_fn, function, php_semantics_declarations, s_assign, s_if,
    s_return, s_throw, t_array, t_mixed,
};

/// Prelude inventory group holding the declarations.
pub(crate) const GROUP: &str = "password";

/// The bridge the two externs live in.
const BRIDGE: &str = "elephc_crypto";

/// The PHP functions this prelude declares.
pub(crate) const FUNCTIONS: &[&str] = &[
    "password_algos",
    "password_get_info",
    "password_hash",
    "password_needs_rehash",
    "password_verify",
];

/// php's default bcrypt cost (`PASSWORD_BCRYPT_DEFAULT_COST`), 12 since php 8.4.
const DEFAULT_COST: i64 = 12;

/// `$algo` names bcrypt when it is `null` or the legacy integer `0` (php's default algorithm),
/// `"2y"`, or the legacy integer `1` -- php's `php_password_algo_find_zval`.
fn names_bcrypt(algo: &str) -> crate::parser::ast::Expr {
    e_binop(
        e_binop(
            e_binop(e_var(algo), BinOp::StrictEq, e_null()),
            BinOp::Or,
            e_binop(e_var(algo), BinOp::StrictEq, e_str("2y")),
        ),
        BinOp::Or,
        e_binop(
            e_binop(e_var(algo), BinOp::StrictEq, e_int(1)),
            BinOp::Or,
            e_binop(e_var(algo), BinOp::StrictEq, e_int(0)),
        ),
    )
}

/// `throw new \ValueError(message);`
fn throw_value_error(message: &str) -> Stmt {
    s_throw(e_new_fq("ValueError", vec![e_str(message)]))
}

/// Builds the extern block and the five functions.
fn function_declarations() -> Vec<Stmt> {
    let verify_extern = extern_fn("elephc_crypto_password_verify", BRIDGE)
        .param("password", CType::Str)
        .param("hash", CType::Str)
        .returns(CType::Int)
        .build();
    let hash_extern = extern_fn("elephc_crypto_password_hash_bcrypt", BRIDGE)
        .param("password", CType::Str)
        .param("cost", CType::Int)
        .returns(CType::Str)
        .build();

    // password_verify(string $password, string $hash): bool
    let verify = function("password_verify")
        .param("password", TypeExpr::Str)
        .param("hash", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .returning(e_binop(
            e_call(
                "elephc_crypto_password_verify",
                vec![e_var("password"), e_var("hash")],
            ),
            BinOp::StrictEq,
            e_int(1),
        ))
        .build();

    // password_get_info(string $hash): array — bcrypt is `$2y$` + two cost digits, 60 bytes.
    let is_bcrypt_hash = e_binop(
        e_binop(
            e_call("strlen", vec![e_var("hash")]),
            BinOp::StrictEq,
            e_int(60),
        ),
        BinOp::And,
        e_binop(
            e_call("substr", vec![e_var("hash"), e_int(0), e_int(4)]),
            BinOp::StrictEq,
            e_str("$2y$"),
        ),
    );
    let get_info = function("password_get_info")
        .param("hash", TypeExpr::Str)
        .returns(t_array())
        .body(vec![
            s_if(
                is_bcrypt_hash,
                vec![s_return(e_array_assoc(vec![
                    (e_str("algo"), e_str("2y")),
                    (e_str("algoName"), e_str("bcrypt")),
                    (
                        e_str("options"),
                        e_array_assoc(vec![(
                            e_str("cost"),
                            e_cast(
                                CastType::Int,
                                e_call("substr", vec![e_var("hash"), e_int(4), e_int(2)]),
                            ),
                        )]),
                    ),
                ]))],
                Vec::new(),
                None,
            ),
            s_return(e_array_assoc(vec![
                (e_str("algo"), e_null()),
                (e_str("algoName"), e_str("unknown")),
                (e_str("options"), e_array(Vec::new())),
            ])),
        ])
        .build();

    // password_needs_rehash(string $hash, string|int|null $algo, array $options = []): bool
    let needs_rehash = function("password_needs_rehash")
        .param("hash", TypeExpr::Str)
        .param("algo", t_mixed())
        .param_default("options", t_array(), e_array(Vec::new()))
        .returns(TypeExpr::Bool)
        .body(vec![
            // php: an unknown new algorithm never prompts a rehash.
            s_if(
                crate::synthetic_class::e_not(names_bcrypt("algo")),
                vec![s_return(crate::synthetic_class::e_bool(false))],
                Vec::new(),
                None,
            ),
            s_assign("info", e_call("password_get_info", vec![e_var("hash")])),
            s_if(
                e_binop(
                    e_index(e_var("info"), e_str("algo")),
                    BinOp::StrictNotEq,
                    e_str("2y"),
                ),
                vec![s_return(crate::synthetic_class::e_bool(true))],
                Vec::new(),
                None,
            ),
            s_return(e_binop(
                e_index(e_index(e_var("info"), e_str("options")), e_str("cost")),
                BinOp::StrictNotEq,
                e_cast(
                    CastType::Int,
                    e_null_coalesce(e_index(e_var("options"), e_str("cost")), e_int(DEFAULT_COST)),
                ),
            )),
        ])
        .build();

    // password_hash(string $password, string|int|null $algo, array $options = []): string
    let hash = function("password_hash")
        .param("password", TypeExpr::Str)
        .param("algo", t_mixed())
        .param_default("options", t_array(), e_array(Vec::new()))
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                crate::synthetic_class::e_not(names_bcrypt("algo")),
                vec![throw_value_error(
                    "password_hash(): Argument #2 ($algo) must be a valid password hashing algorithm",
                )],
                Vec::new(),
                None,
            ),
            s_assign(
                "cost",
                e_cast(
                    CastType::Int,
                    e_null_coalesce(e_index(e_var("options"), e_str("cost")), e_int(DEFAULT_COST)),
                ),
            ),
            s_if(
                e_binop(
                    e_binop(e_var("cost"), BinOp::Lt, e_int(4)),
                    BinOp::Or,
                    e_binop(e_var("cost"), BinOp::Gt, e_int(31)),
                ),
                vec![s_throw(e_new_fq(
                    "ValueError",
                    vec![crate::synthetic_class::e_concat(
                        e_str("Invalid bcrypt cost parameter specified: "),
                        e_var("cost"),
                    )],
                ))],
                Vec::new(),
                None,
            ),
            s_return(e_call(
                "elephc_crypto_password_hash_bcrypt",
                vec![e_var("password"), e_var("cost")],
            )),
        ])
        .build();

    // password_algos(): array
    let algos = function("password_algos")
        .returns(t_array())
        .returning(e_array(vec![e_str("2y")]))
        .build();

    vec![verify_extern, hash_extern, algos, get_info, hash, needs_rehash, verify]
}

/// Returns the declarations to inject.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(function_declarations)
}

/// Returns whether a top-level, namespace-less, guarded or synthetic statement declares the
/// function `name`.
fn program_declares_function(program: &[Stmt], name: &str) -> bool {
    program.iter().any(|stmt| match &stmt.kind {
        StmtKind::FunctionDecl { name: declared, .. } => declared.eq_ignore_ascii_case(name),
        StmtKind::NamespaceBlock { name: None, body }
        | StmtKind::Synthetic(body)
        | StmtKind::IncludeOnceGuard { body, .. } => program_declares_function(body, name),
        _ => false,
    })
}

/// Prepends the password declarations when the program names one of the functions and
/// declares none of them itself.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    let used = crate::prelude_prune::usage::collect(&program);
    let names_function = FUNCTIONS.iter().any(|name| {
        crate::opcache_prelude::detect::program_references(&program, name)
            || used.literals.contains(&crate::names::php_symbol_key(name))
    });
    if !names_function
        || FUNCTIONS
            .iter()
            .any(|name| program_declares_function(&program, name))
    {
        return program;
    }
    let mut combined = declarations();
    inventory.record_program(GROUP, &combined);
    combined.extend(program);
    combined
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The surface is fixed: two externs bound to `elephc_crypto`, then the five functions.
    #[test]
    fn declares_two_crypto_externs_and_five_functions() {
        let mut externs = Vec::new();
        let mut functions = Vec::new();
        for stmt in declarations() {
            match stmt.kind {
                StmtKind::ExternFunctionDecl { name, library, .. } => {
                    assert_eq!(library.as_deref(), Some(BRIDGE));
                    externs.push(name);
                }
                StmtKind::FunctionDecl { name, .. } => functions.push(name),
                _ => {}
            }
        }
        assert_eq!(
            externs,
            vec!["elephc_crypto_password_verify", "elephc_crypto_password_hash_bcrypt"]
        );
        let mut sorted = functions.clone();
        sorted.sort();
        assert_eq!(sorted, FUNCTIONS);
    }
}
