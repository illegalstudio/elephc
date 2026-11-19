//! Purpose:
//! Declares getenv semantics and its argument-dependent boxed result type.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through the builtin registry.
//!
//! Key details:
//! - Omitted and null names return an environment array; non-null names return string|false.
//! - Nullable or erased operands retain a Mixed result because their runtime value selects the mode.

use crate::builtins::semantics::{
    runtime_fn_semantics, BuiltinArgumentLowering, BuiltinEffects, BuiltinResultType, BuiltinSemanticInput,
    BuiltinSemantics,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "getenv",
    check: check,
    semantics: getenv_semantics(),
}

/// Preserves nullable names and shares result typing between direct and callable EIR lowering.
const fn getenv_semantics() -> BuiltinSemantics {
    let mut semantics = runtime_fn_semantics(crate::ir::RuntimeFnId::Getenv);
    semantics.argument_lowering = BuiltinArgumentLowering::Getenv;
    semantics.result_type = BuiltinResultType::Shared(eir_result_type);
    semantics.effects = BuiltinEffects::Shared(effects);
    semantics
}

/// Includes string-conversion effects when an erased name may invoke user code or allocate scratch.
fn effects(input: &BuiltinSemanticInput<'_>) -> crate::ir::Effects {
    let intrinsic = crate::ir::RuntimeFnId::Getenv.intrinsic_effects();
    if input.arg_types.is_empty() || input.arg_types.first() == Some(&PhpType::Void) {
        intrinsic
    } else {
        // AST callable analysis can supply the declared string type for an unknown argument.
        // Keep that case conservative too. Nested user I/O publishes its own monitoring events.
        crate::ir::Op::Call.default_effects().difference(
            crate::ir::Effects::BLOCKING_IO | crate::ir::Effects::NETWORK_IO,
        )
    }
}

/// Resolves from operand types, which remain available when the AST arguments are absent.
fn eir_result_type(input: &BuiltinSemanticInput<'_>) -> PhpType {
    result_type(input.arg_types.first())
}

/// Keeps the array alternative whenever the name may hold null at runtime.
fn result_type(name: Option<&PhpType>) -> PhpType {
    match name {
        None => PhpType::Mixed,
        Some(ty) if may_be_null(ty) => PhpType::Mixed,
        Some(_) => PhpType::Union(vec![PhpType::Str, PhpType::False]),
    }
}

/// Recognizes statically null and dynamically nullable operand representations.
fn may_be_null(ty: &PhpType) -> bool {
    match ty {
        PhpType::Void | PhpType::Mixed | PhpType::TaggedScalar => true,
        PhpType::Union(types) => types.iter().any(may_be_null),
        _ => false,
    }
}

/// Types the name already normalized by the shared named/positional/spread planner.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    reject_nul_name(cx)?;
    let name = cx.args.first()
        .map(|arg| cx.checker.infer_type(arg, cx.env))
        .transpose()?;
    Ok(result_type(name.as_ref()))
}

/// Rejects a literal NUL byte in `$name` on the PHP 8.6 profile.
fn reject_nul_name(cx: &BuiltinCheckCtx) -> Result<(), CompileError> {
    if crate::codegen_support::compile_php_version() < crate::php_version::PhpVersion::Php86 {
        return Ok(());
    }
    let Some(arg) = cx.args.first() else { return Ok(()); };
    if matches!(&arg.kind, ExprKind::StringLiteral(value) if value.contains('\0')) {
        return Err(CompileError::new(
            arg.span,
            "getenv(): Argument #1 ($name) must not contain any null bytes",
        ));
    }
    Ok(())
}
