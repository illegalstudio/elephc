//! Purpose:
//! Home of the PHP `chgrp` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Bool` and requires the `group` argument to be `Int` or `Str`
//!   (a numeric GID or a group name), emitting the diagnostic at that argument's span.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "chgrp",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::Chgrp,
    ),
}

/// Returns `Bool`, rejecting only a `user` argument php itself rejects.
///
/// The parameter is declared `string|int`, and php is COERCIVE about it. Measured on `php -n`
/// 8.5.6: a float, a bool and `null` are all accepted and coerced, and only an array or an object
/// raises `chown(): Argument #2 ($user) must be of type string|int, array given`.
///
/// `Int|Str` alone was too strict to be usable: `fileowner()` and `filegroup()` are declared
/// `int|false`, so `chown($p, fileowner($p))` — the no-op every ownership-preserving copy writes —
/// was refused at compile time for a program php runs, and answers `bool(true)` for.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let principal_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if !principal_type_is_coercible(&principal_ty) {
        return Err(CompileError::new(
            cx.args[1].span,
            &format!("{}() owner/group must be int or string", cx.name),
        ));
    }
    Ok(PhpType::Bool)
}

/// Whether php's coercive `string|int` boundary would take this type.
///
/// A UNION is taken when every member is, which is what admits the `int|false` the file-owner
/// queries return. An array or an object is the only shape php refuses, and it refuses those at
/// RUNTIME with a `TypeError`; refusing them here is the same answer, earlier and louder.
fn principal_type_is_coercible(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Object(_)
        | PhpType::Iterable
        | PhpType::Never => false,
        PhpType::Union(members) => members.iter().all(principal_type_is_coercible),
        _ => true,
    }
}
