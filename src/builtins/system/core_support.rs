//! Purpose:
//! Shares typed EIR semantics across PHP Core runtime and introspection builtin homes.
//!
//! Called from:
//! - The individual Core builtin modules under `crate::builtins::system`.
//!
//! Key details:
//! - User error handlers retain both their PHP-visible callback value and normalized descriptor.
//! - Trigger operations carry source file and line metadata as ordinary typed EIR operands.

use crate::builtins::semantics::{
    callable_accepts_any_source, BuiltinArgumentLowering, BuiltinCallablePolicy,
    BuiltinEffects, BuiltinLowerFn, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport,
    BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{CoreBuiltinOp, Immediate, Op};
use crate::types::PhpType;

/// Builds the common semantic descriptor for one typed Core builtin operation.
pub const fn core_builtin_semantics(
    operation: CoreBuiltinOp,
    lower: BuiltinLowerFn,
) -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(operation.effects()),
        result_ownership: result_ownership(operation),
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirPrimitive,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::Dynamic(callable_accepts_any_source),
        lowering: BuiltinLowering::Eir(lower),
    }
}

/// Builds Core semantics whose backend result is always a boxed Mixed cell.
pub const fn boxed_core_builtin_semantics(
    operation: CoreBuiltinOp,
    lower: BuiltinLowerFn,
) -> BuiltinSemantics {
    let mut semantics = core_builtin_semantics(operation, lower);
    semantics.result_type = BuiltinResultType::Shared(boxed_result_type);
    semantics
}

/// Builds Core semantics for a PHP builtin that forbids generic dynamic invocation.
pub const fn core_builtin_direct_only_semantics(
    operation: CoreBuiltinOp,
    lower: BuiltinLowerFn,
    reason: &'static str,
) -> BuiltinSemantics {
    let mut semantics = core_builtin_semantics(operation, lower);
    semantics.callable = BuiltinCallablePolicy::DirectOnly(reason);
    semantics
}

/// Selects the physical boxed representation used by container-or-false Core results.
fn boxed_result_type(
    _input: &crate::builtins::semantics::BuiltinSemanticInput<'_>,
) -> PhpType {
    PhpType::Mixed
}

/// Returns the checker-facing associative map used by constant and variable introspection.
pub fn check_string_mixed_hash(
    _cx: &mut crate::builtins::spec::BuiltinCheckCtx,
) -> Result<PhpType, crate::errors::CompileError> {
    Ok(PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    })
}

/// Returns the checker-facing two-list schema of `get_defined_functions()`.
pub fn check_defined_functions(
    _cx: &mut crate::builtins::spec::BuiltinCheckCtx,
) -> Result<PhpType, crate::errors::CompileError> {
    Ok(PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Array(Box::new(PhpType::Str))),
    })
}

/// Returns the checker-facing string-array schema used by include introspection.
pub fn check_string_array(
    _cx: &mut crate::builtins::spec::BuiltinCheckCtx,
) -> Result<PhpType, crate::errors::CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}

/// Returns the checker-facing array-or-false result of extension function lookup.
pub fn check_extension_functions(
    cx: &mut crate::builtins::spec::BuiltinCheckCtx,
) -> Result<PhpType, crate::errors::CompileError> {
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::Array(Box::new(PhpType::Str)),
        PhpType::False,
    ]))
}

/// Returns the checker-facing indexed Mixed array used by traces and resource inventories.
pub fn check_mixed_array(
    _cx: &mut crate::builtins::spec::BuiltinCheckCtx,
) -> Result<PhpType, crate::errors::CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Mixed)))
}

/// Returns the checker-facing integer-keyed resource inventory.
pub fn check_resource_hash(
    _cx: &mut crate::builtins::spec::BuiltinCheckCtx,
) -> Result<PhpType, crate::errors::CompileError> {
    Ok(PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: Box::new(PhpType::Mixed),
    })
}

/// Returns the ownership contract of a Core builtin result.
const fn result_ownership(operation: CoreBuiltinOp) -> BuiltinResultOwnership {
    match operation {
        CoreBuiltinOp::DebugBacktrace
        | CoreBuiltinOp::SetErrorHandler
        | CoreBuiltinOp::SetExceptionHandler
        | CoreBuiltinOp::GetDefinedConstants
        | CoreBuiltinOp::GetDefinedFunctions
        | CoreBuiltinOp::GetDefinedVars
        | CoreBuiltinOp::GetExtensionFuncs
        | CoreBuiltinOp::GetIncludedFiles
        | CoreBuiltinOp::GetMangledObjectVars
        | CoreBuiltinOp::GetResources => BuiltinResultOwnership::Fresh,
        CoreBuiltinOp::DebugPrintBacktrace
        | CoreBuiltinOp::ErrorReporting
        | CoreBuiltinOp::RestoreErrorHandler
        | CoreBuiltinOp::RestoreExceptionHandler
        | CoreBuiltinOp::TriggerError => BuiltinResultOwnership::NonHeap,
    }
}

/// Lowers one normalized Core call, adding callback and source metadata where required.
pub fn lower_core_builtin(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    operation: CoreBuiltinOp,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let operands = match operation {
        CoreBuiltinOp::DebugBacktrace => {
            let mut operands = vec![
                operand_or_const_int(ctx, call, 0, 1),
                operand_or_const_int(ctx, call, 1, 0),
            ];
            operands.extend(ctx.current_frame_arguments(call.span));
            operands
        }
        CoreBuiltinOp::DebugPrintBacktrace => {
            let mut operands = vec![
                operand_or_const_int(ctx, call, 0, 0),
                operand_or_const_int(ctx, call, 1, 0),
            ];
            operands.extend(ctx.current_frame_arguments(call.span));
            operands
        }
        CoreBuiltinOp::ErrorReporting => {
            vec![operand_or_const_null(ctx, call, 0)]
        }
        CoreBuiltinOp::SetErrorHandler => lower_handler_operands(ctx, call, true)?,
        CoreBuiltinOp::SetExceptionHandler => lower_handler_operands(ctx, call, false)?,
        CoreBuiltinOp::TriggerError => lower_trigger_operands(ctx, call)?,
        CoreBuiltinOp::GetDefinedConstants => {
            vec![operand_or_const_bool(ctx, call, 0, false)]
        }
        CoreBuiltinOp::GetResources => vec![operand_or_const_null(ctx, call, 0)],
        CoreBuiltinOp::GetDefinedFunctions | CoreBuiltinOp::GetIncludedFiles => Vec::new(),
        _ => call.operands.to_vec(),
    };
    Ok(ctx.emit_value(
        Op::CoreBuiltin,
        operands,
        Some(Immediate::I64(operation.as_i64())),
        call.result_type.clone(),
        operation.effects(),
        Some(call.span),
    ))
}

/// Returns one supplied operand or emits the integer default required by the Core signature.
fn operand_or_const_int(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    index: usize,
    default: i64,
) -> crate::ir::ValueId {
    call.operands.get(index).copied().unwrap_or_else(|| {
        ctx.emit_value(
            Op::ConstI64,
            Vec::new(),
            Some(Immediate::I64(default)),
            PhpType::Int,
            Op::ConstI64.default_effects(),
            Some(call.span),
        )
        .value
    })
}

/// Returns one supplied operand or emits the boolean default required by the Core signature.
fn operand_or_const_bool(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    index: usize,
    default: bool,
) -> crate::ir::ValueId {
    call.operands.get(index).copied().unwrap_or_else(|| {
        ctx.emit_value(
            Op::ConstBool,
            Vec::new(),
            Some(Immediate::Bool(default)),
            PhpType::Bool,
            Op::ConstBool.default_effects(),
            Some(call.span),
        )
        .value
    })
}

/// Returns one supplied operand or emits PHP null for a nullable optional parameter.
fn operand_or_const_null(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    index: usize,
) -> crate::ir::ValueId {
    call.operands.get(index).copied().unwrap_or_else(|| {
        ctx.emit_value(
            Op::ConstNull,
            Vec::new(),
            None,
            PhpType::Void,
            Op::ConstNull.default_effects(),
            Some(call.span),
        )
        .value
    })
}

/// Preserves a handler's original PHP value and also produces a uniform callable descriptor.
fn lower_handler_operands(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
    with_mask: bool,
) -> Result<Vec<crate::ir::ValueId>, BuiltinLoweringError> {
    let callback = call.operand(0)?;
    let callback_type = ctx.value_php_type(callback).codegen_repr();
    let original = ctx.emit_value(
        Op::MixedBox,
        vec![callback],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(call.span),
    );
    let descriptor = if matches!(callback_type, PhpType::Void | PhpType::Never) {
        ctx.emit_value(
            Op::ConstI64,
            Vec::new(),
            Some(Immediate::I64(0)),
            PhpType::Callable,
            Op::ConstI64.default_effects(),
            Some(call.span),
        )
    } else {
        ctx.emit_value(
            Op::NormalizeCallable,
            vec![callback],
            None,
            PhpType::Callable,
            Op::NormalizeCallable.default_effects(),
            Some(call.span),
        )
    };
    let mut operands = vec![original.value, descriptor.value];
    if with_mask {
        operands.push(operand_or_const_int(ctx, call, 1, 32_767));
    }
    Ok(operands)
}

/// Appends the source file and line supplied to a PHP user error callback.
fn lower_trigger_operands(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<Vec<crate::ir::ValueId>, BuiltinLoweringError> {
    let file = ctx.source_path().unwrap_or("unknown").to_string();
    let file_data = ctx.intern_string(&file);
    let file = ctx.emit_value(
        Op::ConstStr,
        Vec::new(),
        Some(Immediate::Data(file_data)),
        PhpType::Str,
        Op::ConstStr.default_effects(),
        Some(call.span),
    );
    let line = ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(i64::from(call.span.line))),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(call.span),
    );
    Ok(vec![
        call.operand(0)?,
        operand_or_const_int(ctx, call, 1, 1_024),
        file.value,
        line.value,
    ])
}

macro_rules! core_builtin_home {
    ($contract:literal, $operation:ident, check: $check:path, no_first_class: $reason:literal) => {
        builtin! {
            contract: $contract,
            check: $check,
            semantics: crate::builtins::system::core_support::core_builtin_direct_only_semantics(
                crate::ir::CoreBuiltinOp::$operation,
                lower,
                $reason,
            ),
        }

        /// Lowers this PHP Core builtin through its typed EIR selector.
        fn lower(
            ctx: &mut dyn crate::builtins::semantics::BuiltinLoweringContext,
            call: &crate::builtins::semantics::NormalizedBuiltinCall<'_>,
        ) -> Result<
            crate::builtins::semantics::LoweredBuiltinValue,
            crate::builtins::semantics::BuiltinLoweringError,
        > {
            crate::builtins::system::core_support::lower_core_builtin(
                ctx,
                call,
                crate::ir::CoreBuiltinOp::$operation,
            )
        }
    };
    ($contract:literal, $operation:ident) => {
        builtin! {
            contract: $contract,
            semantics: crate::builtins::system::core_support::core_builtin_semantics(
                crate::ir::CoreBuiltinOp::$operation,
                lower,
            ),
        }

        /// Lowers this PHP Core builtin through its typed EIR selector.
        fn lower(
            ctx: &mut dyn crate::builtins::semantics::BuiltinLoweringContext,
            call: &crate::builtins::semantics::NormalizedBuiltinCall<'_>,
        ) -> Result<
            crate::builtins::semantics::LoweredBuiltinValue,
            crate::builtins::semantics::BuiltinLoweringError,
        > {
            crate::builtins::system::core_support::lower_core_builtin(
                ctx,
                call,
                crate::ir::CoreBuiltinOp::$operation,
            )
        }
    };
    ($contract:literal, $operation:ident, check: $check:path) => {
        builtin! {
            contract: $contract,
            check: $check,
            semantics: crate::builtins::system::core_support::core_builtin_semantics(
                crate::ir::CoreBuiltinOp::$operation,
                lower,
            ),
        }

        /// Lowers this PHP Core builtin through its typed EIR selector.
        fn lower(
            ctx: &mut dyn crate::builtins::semantics::BuiltinLoweringContext,
            call: &crate::builtins::semantics::NormalizedBuiltinCall<'_>,
        ) -> Result<
            crate::builtins::semantics::LoweredBuiltinValue,
            crate::builtins::semantics::BuiltinLoweringError,
        > {
            crate::builtins::system::core_support::lower_core_builtin(
                ctx,
                call,
                crate::ir::CoreBuiltinOp::$operation,
            )
        }
    };
    ($contract:literal, $operation:ident, check_boxed: $check:path) => {
        builtin! {
            contract: $contract,
            check: $check,
            semantics: crate::builtins::system::core_support::boxed_core_builtin_semantics(
                crate::ir::CoreBuiltinOp::$operation,
                lower,
            ),
        }

        /// Lowers this PHP Core builtin through its typed boxed EIR selector.
        fn lower(
            ctx: &mut dyn crate::builtins::semantics::BuiltinLoweringContext,
            call: &crate::builtins::semantics::NormalizedBuiltinCall<'_>,
        ) -> Result<
            crate::builtins::semantics::LoweredBuiltinValue,
            crate::builtins::semantics::BuiltinLoweringError,
        > {
            crate::builtins::system::core_support::lower_core_builtin(
                ctx,
                call,
                crate::ir::CoreBuiltinOp::$operation,
            )
        }
    };
}

pub(crate) use core_builtin_home;
