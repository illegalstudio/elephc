//! Purpose:
//! Proves that a tail transition can reuse a frame without hidden cleanup or coercion.
//!
//! Called from:
//! - `super::eliminate_tail_recursion` and tail-site discovery.
//!
//! Key details:
//! - Owning locals, references, handlers and implicit frame readers retain ordinary calls.
//! - Backtrace or eval use anywhere in the module keeps the physical activation chain intact.

use crate::ir::{CoreBuiltinOp, Function, Immediate, Instruction, IrType, LocalKind, LocalSlotId, Module, Op};
use crate::types::PhpType;
use super::numeric;

/// Returns whether the PHP type has a plain scalar representation with no lifetime obligations.
fn scalar_type(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Void)
}

/// Looks up the concrete parameter slots initialized by the normal ABI prologue.
pub(super) fn parameter_slots(function: &Function) -> Option<Vec<LocalSlotId>> {
    function.params.iter().map(|param| {
        function.locals.iter().find(|local| {
            local.name.as_deref() == Some(param.name.as_str())
                && local.kind == LocalKind::PhpLocal
                && local.ir_type == param.ir_type && local.php_type == param.php_type
        }).map(|local| local.id)
    }).collect()
}

/// Rejects frames with ownership, aliasing, suspension or implicit local-access obligations.
pub(super) fn frame_is_safe(function: &Function, module: &Module) -> bool {
    let roots = numeric::balanced_roots(function);
    let flags = &function.flags;
    !flags.is_main && !flags.is_method && !flags.is_closure && !flags.is_generator
        && !flags.is_fiber_wrapper && !flags.is_callback_wrapper
        && !flags.is_runtime_callable_invoker && !flags.by_ref_return && !flags.is_synthetic
        && function.lexical_class.is_none()
        && scalar_type(&function.return_php_type)
        && function.params.iter().all(|param| {
            !param.by_ref && !param.variadic && scalar_type(&param.php_type)
                && param.php_type != PhpType::Void
                && !matches!(param.name.as_str(), crate::func_args::HIDDEN_ARGS_PARAM | crate::func_args::HIDDEN_ARGC_PARAM)
        })
        && function.locals.iter().all(|local| {
            roots.contains(&local.id) || (scalar_type(&local.php_type)
                && matches!(local.ir_type, IrType::I64 | IrType::F64)
                && matches!(local.kind, LocalKind::PhpLocal | LocalKind::HiddenTemp | LocalKind::OwnedTemp | LocalKind::BorrowedTemp | LocalKind::NamedArgTemp))
        })
        && parameter_slots(function).is_some()
        && !super::super::cfg::has_exception_handlers(function)
        && !super::super::local_scope::has_dynamic_eval(function)
        && calls_are_value_only(function, module)
        && function.instructions.iter().all(|inst| {
            !inst.op.name().starts_with("eval_") && inst.op != Op::LanguageConstructCall
                && match inst.immediate {
                    Some(Immediate::LocalSlot(slot)) => matches!(inst.op,
                        Op::LoadLocal | Op::StoreLocal | Op::ZeroLocalSlot | Op::UnsetLocal)
                        || (roots.contains(&slot)
                            && matches!(inst.op, Op::PushCallOperandOwner | Op::PopCallOperandOwner | Op::ReleaseLocalSlot)),
                    Some(Immediate::LocalSlotPair { .. } | Immediate::IterStart(_)) => false,
                    _ => true,
                }
        })
        && function.blocks.iter().all(|block| !matches!(block.terminator,
            Some(crate::ir::Terminator::GeneratorSuspend { .. })))
}

/// Requires matching return storage and exactly reproducible scalar argument conversions.
pub(super) fn call_can_rebind(host: &Function, call: &Instruction, callee: &Function) -> bool {
    host.return_type == callee.return_type
        && host.return_php_type == callee.return_php_type
        && (callee.return_type == IrType::Void || call.result_type == callee.return_type)
        && call.result_php_type == callee.return_php_type
        && call.operands.len() == callee.params.len()
        && call.operands.iter().zip(&callee.params).all(|(&arg, param)| {
            host.value(arg).is_some_and(|value|
                (value.ir_type == param.ir_type && value.php_type == param.php_type)
                    || (param.php_type == PhpType::Int && value.php_type == PhpType::Mixed
                        && numeric::is_numeric(host, arg)))
        })
}

/// Detects module-wide observers of the physical PHP activation chain.
pub(super) fn activation_chain_is_observable(module: &Module) -> bool {
    module.required_runtime_features.eval_bridge || module.functions.iter()
        .chain(&module.class_methods).chain(&module.closures).chain(&module.fiber_wrappers)
        .chain(&module.callback_wrappers).chain(&module.extern_callback_trampolines)
        .chain(&module.runtime_callable_invokers).any(|function| {
            function.params.iter().any(|param| matches!(param.name.as_str(),
                crate::func_args::HIDDEN_ARGS_PARAM | crate::func_args::HIDDEN_ARGC_PARAM))
                || function.instructions.iter().any(|inst| {
                    inst.op == Op::CoreBuiltin && matches!(inst.immediate,
                        Some(Immediate::I64(selector)) if matches!(CoreBuiltinOp::from_i64(selector),
                            Some(CoreBuiltinOp::DebugBacktrace | CoreBuiltinOp::DebugPrintBacktrace)))
                })
        })
}

/// Rejects implicit address-taking consumers, resolving ordinary calls against their signatures.
fn calls_are_value_only(function: &Function, module: &Module) -> bool {
    let local_reads: std::collections::HashSet<_> = function.instructions.iter()
        .filter(|inst| inst.op == Op::LoadLocal).filter_map(|inst| inst.result).collect();
    function.instructions.iter().all(|inst| {
        if inst.op == Op::Call {
            let Some(Immediate::Data(id)) = inst.immediate else { return false; };
            return module.data.function_names.get(id.as_raw() as usize)
                .and_then(|name| crate::ir::function_variants::function_by_php_name(module, name))
                .is_some_and(|callee| !callee.params.iter().any(|param| param.by_ref));
        }
        super::super::by_ref_alias::op_is_value_only_consumer(inst.op)
            || !inst.operands.iter().any(|value| local_reads.contains(value))
    })
}
