//! Purpose:
//! Home of the PHP `phpinfo` builtin: its declaration and backend-neutral EIR lowering.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - elephc binaries have no runtime extension registry, so `phpinfo()` renders the
//!   compile-time module set. The fileinfo row is always present because the compiler
//!   ships the extension surface on every supported target.
//! - The `$flags` argument is accepted and evaluated; the current implementation emits the
//!   complete table for every value, which is what the fileinfo compatibility corpus needs.

use crate::builtins::semantics::{
    BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering, BuiltinLoweringContext,
    BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy, BuiltinTargetSupport,
    BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Immediate, Op};
use crate::types::PhpType;

builtin! {
    contract: "phpinfo",
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        effects: BuiltinEffects::Static(crate::ir::Effects::OUTPUT),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: crate::builtins::semantics::BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly("phpinfo() accepts no callback"),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Renders the compile-time module table and returns PHP's boolean success value.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    if let Some(flags) = call.operands.first() {
        // PHP evaluates the argument even when the selected section is ignored here.
        let _ = flags;
    }
    let flags = call
        .operands
        .first()
        .copied()
        .and_then(|value| ctx.constant_i64(value));
    let text = crate::phpinfo_table::render(flags);
    let data = ctx.intern_string(&text);
    let message = ctx.emit_value(
        Op::ConstStr,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Str,
        Op::ConstStr.default_effects(),
        Some(call.span),
    );
    ctx.emit_void(
        Op::EchoValue,
        vec![message.value],
        None,
        Op::EchoValue.default_effects(),
        Some(call.span),
    );
    Ok(ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(true)),
        PhpType::Bool,
        Op::ConstBool.default_effects(),
        Some(call.span),
    ))
}
