//! Purpose:
//! Joins the `proc_open()` family's shared contracts to the AOT implementation: each
//! public builtin lowers to a call of its `__elephc_proc_*` twin declared by the injected
//! proc prelude (`crate::proc_prelude`), and the internal resource helpers that prelude
//! composes lower to typed runtime calls.
//!
//! Called from:
//! - Each `crate::builtins::proc::*` home file through `builtin!`.
//! - Reachability's call scan through [`prelude_helpers_for`].
//!
//! Key details:
//! - WHY REGISTRY BUILTINS AND NOT PRELUDE FUNCTIONS. `proc_open()`'s `$pipes` is a
//!   write-only by-reference output that PHP code never predeclares, which only a
//!   builtin's checker hook can admit (the `xml_parse_into_struct()` model), and the
//!   iOS sandbox refusal must point at the user's call, not into the prelude.
//! - Operands are boxed to `Mixed` before the helper call so php-src's TypeErrors and
//!   ValueErrors live in one PHP implementation, exactly like GMP.
//! - `proc_open()` then calls `__elephc_proc_open_pipes($process, $previous)` and stores
//!   its answer into the caller's `$pipes` with `store_operand_local`: the new pipes
//!   array, or — when the spawn failed — the variable's previous value, so a failed call
//!   leaves it as PHP does.

use crate::builtins::semantics::{
    callable_accepts_any_source, BuiltinArgumentLowering, BuiltinCallablePolicy,
    BuiltinEffects, BuiltinLowering, BuiltinLoweringContext, BuiltinLoweringError,
    BuiltinRequirement, BuiltinRequirements, BuiltinResultOwnership, BuiltinResultType,
    BuiltinRuntimeFunctions, BuiltinSemanticInput, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::{Effects, Immediate, Op, ValueId};
use crate::types::PhpType;
use elephc_builtin_contract::DefaultSpec;

pub mod __elephc_fd_stream;
pub mod __elephc_proc_resource;
pub mod __elephc_resource_close;
pub mod __elephc_resource_kind;
pub mod __elephc_resource_payload;
pub mod proc_close;
pub mod proc_get_status;
pub mod proc_nice;
pub mod proc_open;
pub mod proc_terminate;

/// The bridge required by every public proc builtin.
const PROC_REQUIREMENTS: &[BuiltinRequirement] = &[BuiltinRequirement::Bridge("elephc_proc")];

/// The prelude helper `proc_open()` calls to collect the pipes array it stores.
pub(crate) const OPEN_PIPES_HELPER: &str = "__elephc_proc_open_pipes";

/// Every prelude helper a public proc builtin's lowering may call directly.
pub(crate) const HELPERS: &[&str] = &[
    "__elephc_proc_close",
    "__elephc_proc_get_status",
    "__elephc_proc_nice",
    "__elephc_proc_open",
    OPEN_PIPES_HELPER,
    "__elephc_proc_terminate",
];

/// Returns the prelude helpers to root when `name` is a public proc builtin.
pub(crate) fn prelude_helpers_for(name: &str) -> impl Iterator<Item = &'static str> {
    let is_proc = elephc_builtin_contract::PROC_FUNCTIONS.contains(&name);
    HELPERS.iter().copied().filter(move |_| is_proc)
}

/// Builds the shared descriptor for one public proc builtin.
pub(crate) const fn semantics(
    result_type: crate::builtins::semantics::ResultTypeFn,
    ownership: BuiltinResultOwnership,
    argument_lowering: BuiltinArgumentLowering,
    callable: BuiltinCallablePolicy,
) -> BuiltinSemantics {
    BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Shared(result_type),
        effects: BuiltinEffects::Static(Effects::all()),
        result_ownership: ownership,
        requirements: BuiltinRequirements::Static(PROC_REQUIREMENTS),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::HostOnly,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering,
        callable,
        lowering: BuiltinLowering::Eir(lower),
    }
}

/// Semantics for a scalar-returning builtin that dynamic callables may name.
pub(crate) const fn scalar_semantics(result_type: crate::builtins::semantics::ResultTypeFn) -> BuiltinSemantics {
    semantics(
        result_type,
        BuiltinResultOwnership::NonHeap,
        BuiltinArgumentLowering::PreserveUncoerced,
        BuiltinCallablePolicy::Dynamic(callable_accepts_any_source),
    )
}

/// Resolves a boxed `Mixed` backend result.
pub(crate) fn mixed_result(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Mixed
}

/// Resolves an integer result.
pub(crate) fn int_result(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Int
}

/// Resolves a boolean result.
pub(crate) fn bool_result(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::Bool
}

/// Resolves the string-keyed status array the prelude helper builds.
pub(crate) fn status_result(_input: &BuiltinSemanticInput<'_>) -> PhpType {
    PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    }
}

/// Refuses the call on a sandboxed target and returns `result`, the checker-facing type.
pub(crate) fn check_spawning(cx: &mut BuiltinCheckCtx, result: PhpType) -> Result<PhpType, CompileError> {
    crate::builtins::spec::reject_if_process_spawn_forbidden(cx)?;
    Ok(result)
}

/// Lowers one public proc builtin by calling its prelude twin with boxed operands.
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let contract = elephc_builtin_contract::lookup(call.name).ok_or_else(|| {
        BuiltinLoweringError::new(format!("{} has no shared proc contract", call.name))
    })?;
    let mut supplied = call.operands.to_vec();
    // `emit_user_call` needs the helper's full ABI: materialize omitted trailing defaults.
    for index in supplied.len()..contract.params.len() {
        let default = contract.params[index].default.ok_or_else(|| {
            BuiltinLoweringError::new(format!(
                "{} lowering is missing required operand {index}", call.name
            ))
        })?;
        supplied.push(emit_default(ctx, default, call.span)?);
    }
    let pipes_target = (call.name == "proc_open").then(|| supplied.remove(2));
    let raw_streams = (call.name == "proc_open").then(|| {
        let raw = has_raw_resource_elements(&ctx.value_php_type(supplied[1]));
        ctx.emit_value(
            Op::ConstBool,
            Vec::new(),
            Some(Immediate::Bool(raw)),
            PhpType::Bool,
            Op::ConstBool.default_effects(),
            Some(call.span),
        )
        .value
    });
    let mut boxes = Vec::new();
    let operands: Vec<ValueId> = supplied
        .into_iter()
        .map(|operand| boxed_operand(ctx, operand, &mut boxes, call))
        .chain(raw_streams)
        .collect();
    let helper = format!("__elephc_{}", call.name);
    let result = ctx.emit_user_call(&helper, operands, call.result_type.clone(), Some(call.span));
    if let Some(target) = pipes_target {
        let previous = boxed_operand(ctx, target, &mut boxes, call);
        store_pipes(ctx, target, result.value, previous, call)?;
    }
    for boxed in boxes {
        ctx.emit_void(Op::Release, vec![boxed], None, Op::Release.default_effects(), Some(call.span));
    }
    Ok(result)
}

/// Returns whether a descriptor-spec array stores its resources as raw native
/// descriptors: a container whose static element type is `resource` keeps unboxed
/// payloads, so the helper must read an integer element as a stream descriptor.
fn has_raw_resource_elements(spec: &PhpType) -> bool {
    match spec.codegen_repr() {
        PhpType::Array(element) => matches!(*element, PhpType::Resource(_)),
        PhpType::AssocArray { value, .. } => matches!(*value, PhpType::Resource(_)),
        _ => false,
    }
}

/// Returns `operand` as a Mixed value, boxing (and recording the box for release) when
/// its representation is not already boxed.
fn boxed_operand(
    ctx: &mut dyn BuiltinLoweringContext,
    operand: ValueId,
    boxes: &mut Vec<ValueId>,
    call: &NormalizedBuiltinCall<'_>,
) -> ValueId {
    let representation = ctx.value_php_type(operand).codegen_repr();
    if matches!(representation, PhpType::Mixed | PhpType::Union(_)) {
        return operand;
    }
    let boxed = ctx.emit_value(
        Op::MixedBox,
        vec![operand],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(call.span),
    );
    boxes.push(boxed.value);
    boxed.value
}

/// Assigns `proc_open()`'s pipes array (or `$previous` after a failed spawn) to the
/// caller's `$pipes`.
fn store_pipes(
    ctx: &mut dyn BuiltinLoweringContext,
    target: ValueId,
    process: ValueId,
    previous: ValueId,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<(), BuiltinLoweringError> {
    let pipes = ctx.emit_user_call(
        OPEN_PIPES_HELPER,
        vec![process, previous],
        PhpType::Mixed,
        Some(call.span),
    );
    if ctx.store_operand_local(target, pipes.value, PhpType::Mixed, Some(call.span)) {
        return Ok(());
    }
    Err(BuiltinLoweringError::new(
        "proc_open() parameter $pipes must be a variable the lowering can store into",
    ))
}

/// Emits one omitted optional argument's default value for the helper ABI.
fn emit_default(
    ctx: &mut dyn BuiltinLoweringContext,
    default: DefaultSpec,
    span: crate::span::Span,
) -> Result<ValueId, BuiltinLoweringError> {
    let (op, immediate, php_type) = match default {
        DefaultSpec::Null => (Op::ConstNull, None, PhpType::Void),
        DefaultSpec::Int(value) => (Op::ConstI64, Some(Immediate::I64(value)), PhpType::Int),
        other => {
            return Err(BuiltinLoweringError::new(format!(
                "unsupported proc helper default {other:?}"
            )))
        }
    };
    Ok(ctx.emit_value(op, Vec::new(), immediate, php_type, op.default_effects(), Some(span)).value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every public proc contract roots every helper, and nothing else roots them.
    #[test]
    fn every_proc_contract_roots_its_helpers() {
        for name in elephc_builtin_contract::PROC_FUNCTIONS {
            let helpers: Vec<_> = prelude_helpers_for(name).collect();
            assert!(helpers.contains(&format!("__elephc_{name}").as_str()), "{name}");
        }
        assert!(prelude_helpers_for("popen").next().is_none());
    }
}
