//! Purpose:
//! Declares PHP's `escapeshellarg` builtin and its typed shell-escaping runtime target.
//!
//! Called from:
//! - The builtin registry, checker, optimizer, and AST-to-EIR builtin lowering path.
//!
//! Key details:
//! - The helper is platform-aware: POSIX single-quotes arguments and Windows uses PHP's
//!   double-quoted command-line rule.
//! - Embedded NUL bytes raise PHP's catchable `ValueError`, so the operation may throw.

use crate::{
    builtins::spec::BuiltinCheckCtx,
    errors::CompileError,
    ir::{RuntimeCallTarget, UnaryStringRuntime},
    types::PhpType,
};

builtin! {
    name: "escapeshellarg",
    area: String,
    params: [arg: Str],
    returns: Str,
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::unary_string_runtime(
        RuntimeCallTarget::UnaryString(UnaryStringRuntime::EscapeShellArg),
        crate::ir::Effects::MAY_THROW,
    ),
    summary: "Quotes one argument for safe use in a shell command.",
    php_manual: "https://www.php.net/manual/en/function.escapeshellarg.php",
}

/// Accepts PHP's weak scalar string coercions while rejecting compound and object inputs.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !accepts_weak_scalar_string(&ty) {
        return Err(CompileError::new(
            cx.args[0].span,
            "escapeshellarg() argument #1 ($arg) must be string",
        ));
    }

    Ok(PhpType::Str)
}

/// Returns whether a statically known type can use PHP's weak scalar-to-string conversion.
///
/// `Mixed` remains deliberately rejected: its boxed payload can contain an array or object, and
/// generic mixed string casting cannot provide this builtin's TypeError contract. `TaggedScalar`
/// is safe because it is the codegen-only `int|null` representation (see `PhpType`).
fn accepts_weak_scalar_string(ty: &PhpType) -> bool {
    match ty {
        PhpType::Str
        | PhpType::Int
        | PhpType::Float
        | PhpType::Bool
        | PhpType::False
        | PhpType::Void
        | PhpType::Never
        | PhpType::TaggedScalar => true,
        PhpType::Union(members) => members.iter().all(accepts_weak_scalar_string),
        PhpType::Mixed
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Iterable
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_)
        | PhpType::Resource(_) => false,
    }
}
