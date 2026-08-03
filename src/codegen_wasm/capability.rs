//! Purpose:
//! Audits a complete EIR module for wasm32-wasi capability and, after the
//! aggregate static gate succeeds, returns its exact lowered plan.
//!
//! Called from:
//! - `crate::codegen_wasm::generate` as the sole capability-and-planning gate.
//!
//! Key details:
//! - Every EIR function collection is inspected. Collections the backend cannot
//!   yet emit are rejected explicitly instead of being silently omitted.
//! - Opcode and terminator classification is exhaustive, so a new enum variant
//!   cannot compile until its WASM status is decided.
//! - Diagnostics name the collection, function, block, and instruction.
//! - A successful static audit delegates once to `plan::plan_module`; no WAT
//!   lowering is repeated after the returned plan is accepted.

use super::calls::{classify_by_ref_source, resolve_direct_call, ByRefSource};
use super::plan::{self, LoweredWasmPlan};
use super::transfer;
use super::WasmError;
use crate::codegen::{literal_default_value, Emit, LiteralDefaultValue};
use crate::ir::{
    BlockId, Function, Immediate, Instruction, IrHeapKind, IrType, LocalKind, Module,
    Op, Ownership, RuntimeCallTarget, RuntimeFnId, Terminator, UnaryStringRuntime,
    ValueDef, ValueId,
};
use crate::parser::ast::Visibility;
use crate::types::PhpType;
use std::collections::HashSet;

/// Compile-time provenance of ref-bound local slots within one EIR function.
///
/// `owned` means the slot points at a promotable kind-7 heap cell. `borrowed`
/// means the slot can point at caller-owned or interior array storage. A slot in
/// both sets is conservatively non-escapable.
#[derive(Default)]
struct RefCellProvenance {
    owned: HashSet<u32>,
    borrowed: HashSet<u32>,
}

/// Computes the block ids reachable from the function entry through EIR edges.
fn reachable_block_ids(function: &Function) -> HashSet<u32> {
    let mut reachable = HashSet::new();
    let mut pending = vec![function.entry];
    while let Some(block_id) = pending.pop() {
        if !reachable.insert(block_id.as_raw()) {
            continue;
        }
        let Some(block) = function.block(block_id) else {
            continue;
        };
        match block.terminator.as_ref() {
            Some(Terminator::Br { target, .. }) => pending.push(*target),
            Some(Terminator::CondBr {
                then_target,
                else_target,
                ..
            }) => {
                pending.push(*then_target);
                pending.push(*else_target);
            }
            Some(Terminator::Switch { cases, default, .. }) => {
                pending.push(*default);
                pending.extend(cases.iter().map(|case| case.target));
            }
            Some(Terminator::GeneratorSuspend { resume, .. }) => pending.push(*resume),
            Some(
                Terminator::Return { .. }
                | Terminator::Throw { .. }
                | Terminator::Fatal { .. }
                | Terminator::Unreachable,
            )
            | None => {}
        }
    }
    reachable
}

/// Audits every function collection and returns one aggregate diagnostic.
fn audit_module(module: &Module) -> Result<(), WasmError> {
    let mut issues = Vec::new();
    scan_functions(module, "functions", &module.functions, true, &mut issues);
    scan_functions(
        module,
        "class_methods",
        &module.class_methods,
        true,
        &mut issues,
    );
    scan_functions(module, "closures", &module.closures, true, &mut issues);
    scan_functions(
        module,
        "fiber_wrappers",
        &module.fiber_wrappers,
        false,
        &mut issues,
    );
    scan_functions(
        module,
        "callback_wrappers",
        &module.callback_wrappers,
        false,
        &mut issues,
    );
    scan_functions(
        module,
        "extern_callback_trampolines",
        &module.extern_callback_trampolines,
        false,
        &mut issues,
    );
    scan_functions(
        module,
        "runtime_callable_invokers",
        &module.runtime_callable_invokers,
        false,
        &mut issues,
    );
    if issues.is_empty() {
        Ok(())
    } else {
        Err(WasmError::Unsupported(format!(
            "WASM capability audit found {} issue(s):\n{}",
            issues.len(),
            issues
                .iter()
                .map(|issue| format!("  - {issue}"))
                .collect::<Vec<_>>()
                .join("\n")
        )))
    }
}

/// Audits and exactly lowers a module into the private plan consumed by generation.
///
/// Static capability issues retain their stable aggregate diagnostic and stop
/// before lowering. Once this function returns a plan, every fallible lowering
/// and identifier-consistency check has already succeeded; artifact publication
/// still owns WAT assembly and binary validation.
pub(super) fn validate_module(
    module: &Module,
    emit: Emit,
) -> Result<LoweredWasmPlan, WasmError> {
    audit_module(module)?;
    plan::plan_module(module, emit)
}

/// Scans one module function collection and records collection-level omission.
fn scan_functions(
    module: &Module,
    collection: &str,
    functions: &[Function],
    emitted: bool,
    issues: &mut Vec<String>,
) {
    for function in functions {
        if !emitted {
            issues.push(format!(
                "{collection}::{}: function collection is not emitted by wasm32-wasi",
                function.name
            ));
        }
        scan_function(module, collection, function, issues);
    }
}

/// Scans types, instructions, runtime targets, and terminators in one function.
fn scan_function(
    module: &Module,
    collection: &str,
    function: &Function,
    issues: &mut Vec<String>,
) {
    let ref_cell_provenance = collect_ref_cell_provenance(function);
    let reachable_blocks = reachable_block_ids(function);
    if function.flags.is_main
        && (function.return_type != IrType::Void
            || function.return_php_type.codegen_repr() != PhpType::Void)
    {
        issues.push(format!(
            "{collection}::{}: main must declare a void return, got {:?}/{:?}",
            function.name,
            function.return_type,
            function.return_php_type.codegen_repr()
        ));
    }
    check_type(
        function.return_type,
        &format!("{collection}::{} return", function.name),
        issues,
    );
    for (index, param) in function.params.iter().enumerate() {
        check_type(
            param.ir_type,
            &format!("{collection}::{} param#{index}", function.name),
            issues,
        );
    }
    for local in &function.locals {
        check_type(
            local.ir_type,
            &format!(
                "{collection}::{} local#{}",
                function.name,
                local.id.as_raw()
            ),
            issues,
        );
    }
    for block in &function.blocks {
        for (index, value_id) in block.params.iter().enumerate() {
            match function.value(*value_id) {
                Some(value) => check_type(
                    value.ir_type,
                    &format!(
                        "{collection}::{} block#{} param#{index}",
                        function.name,
                        block.id.as_raw()
                    ),
                    issues,
                ),
                None => issues.push(format!(
                    "{collection}::{} block#{} param#{index}: missing value {:?}",
                    function.name,
                    block.id.as_raw(),
                    value_id
                )),
            }
        }
        for inst_id in &block.instructions {
            let Some(instruction) = function.instruction(*inst_id) else {
                issues.push(format!(
                    "{collection}::{} block#{} instruction#{:?}: missing instruction",
                    function.name,
                    block.id.as_raw(),
                    inst_id
                ));
                continue;
            };
            if !op_is_supported(instruction.op) {
                issues.push(format!(
                    "{collection}::{} block#{} instruction#{}: unsupported op {}",
                    function.name,
                    block.id.as_raw(),
                    inst_id.as_raw(),
                    instruction.op.name()
                ));
            }
            // `exit`/`die` cannot unwind a caller's WASM frames, so they stay confined to
            // main. `isset` has no such constraint — it only reads a tag — so it is exempt.
            let construct_name = if instruction.op == Op::LanguageConstructCall {
                instruction.immediate.as_ref().and_then(|immediate| match immediate {
                    Immediate::Data(data) => {
                        module.data.function_names.get(data.as_raw() as usize).cloned()
                    }
                    _ => None,
                })
            } else {
                None
            };
            if instruction.op == Op::LanguageConstructCall
                && !function.flags.is_main
                && construct_name.as_deref() != Some("isset")
            {
                issues.push(format!(
                    "{collection}::{} block#{} instruction#{}: exit/die outside main cannot unwind caller-owned WASM frames",
                    function.name,
                    block.id.as_raw(),
                    inst_id.as_raw()
                ));
            }
            check_instruction_shape(
                module,
                collection,
                function,
                block.id.as_raw(),
                inst_id.as_raw(),
                instruction,
                &ref_cell_provenance,
                issues,
            );
            check_type(
                instruction.result_type,
                &format!(
                    "{collection}::{} block#{} instruction#{} result",
                    function.name,
                    block.id.as_raw(),
                    inst_id.as_raw()
                ),
                issues,
            );
            if instruction.op == Op::RuntimeCall {
                check_runtime_call(
                    module,
                    collection,
                    function,
                    block.id.as_raw(),
                    inst_id.as_raw(),
                    instruction,
                    issues,
                );
            }
        }
        match block.terminator.as_ref() {
            Some(terminator) if !terminator_is_supported(terminator) => issues.push(format!(
                "{collection}::{} block#{}: unsupported terminator {}",
                function.name,
                block.id.as_raw(),
                terminator_name(terminator)
            )),
            None => issues.push(format!(
                "{collection}::{} block#{}: missing terminator",
                function.name,
                block.id.as_raw()
            )),
            Some(Terminator::Return { value: Some(_) }) if function.flags.is_main => {
                issues.push(format!(
                    "{collection}::{} block#{}: main return value would be discarded before proc_exit(0)",
                    function.name,
                    block.id.as_raw()
                ));
            }
            Some(Terminator::Unreachable)
                if reachable_blocks.contains(&block.id.as_raw())
                    && !unreachable_has_noreturn_proof(module, function, block) =>
            {
                issues.push(format!(
                    "{collection}::{} block#{}: reachable EIR unreachable terminator lacks a no-return proof",
                    function.name,
                    block.id.as_raw()
                ));
            }
            _ => {}
        }
        if let Some(terminator) = block.terminator.as_ref() {
            if let Some(issue) = terminator_transfer_shape_issue(function, terminator) {
                issues.push(format!(
                    "{collection}::{} block#{}: unsupported terminator transfer shape: {issue}",
                    function.name,
                    block.id.as_raw()
                ));
            }
        }
    }
}

/// Validates return and control-flow value transfers before WAT emission.
fn terminator_transfer_shape_issue(
    function: &Function,
    terminator: &Terminator,
) -> Option<String> {
    let check_edge = |target: BlockId, args: &[ValueId]| -> Option<String> {
        let Some(block) = function.block(target) else {
            return Some(format!("branch target {target:?} is missing"));
        };
        if block.params.len() != args.len() {
            return Some(format!(
                "branch target {target:?} expects {} arguments, got {}",
                block.params.len(),
                args.len()
            ));
        }
        for (index, (argument, parameter)) in args.iter().zip(&block.params).enumerate() {
            let Some(source) = function.value(*argument) else {
                return Some(format!("branch argument #{index} is missing"));
            };
            let Some(destination) = function.value(*parameter) else {
                return Some(format!("branch parameter #{index} is missing"));
            };
            if let Some(issue) = value_transfer_shape_issue(
                source.ir_type,
                source.php_type.codegen_repr(),
                destination.ir_type,
                destination.php_type.codegen_repr(),
            ) {
                return Some(format!("branch argument #{index}: {issue}"));
            }
        }
        None
    };
    match terminator {
        Terminator::Br { target, args } => check_edge(*target, args),
        Terminator::CondBr {
            then_target,
            then_args,
            else_target,
            else_args,
            ..
        } => check_edge(*then_target, then_args)
            .or_else(|| check_edge(*else_target, else_args)),
        Terminator::Switch {
            cases,
            default,
            default_args,
            ..
        } => cases
            .iter()
            .find_map(|case| check_edge(case.target, &case.args))
            .or_else(|| check_edge(*default, default_args)),
        Terminator::GeneratorSuspend {
            resume,
            resume_args,
            ..
        } => check_edge(*resume, resume_args),
        Terminator::Return { value: Some(value) } => {
            let Some(source) = function.value(*value) else {
                return Some("return value is missing from the value table".to_string());
            };
            if source.ir_type != function.return_type {
                return Some(format!(
                    "return storage {:?}/{:?} differs from function storage {:?}/{:?}",
                    source.ir_type,
                    source.php_type.codegen_repr(),
                    function.return_type,
                    function.return_php_type.codegen_repr()
                ));
            }
            None
        }
        Terminator::Return { value: None }
        | Terminator::Throw { .. }
        | Terminator::Fatal { .. }
        | Terminator::Unreachable => None,
    }
}

/// Records PHP-sensitive lowerer-shape defects before constructing the exact plan.
///
/// Generic arity, immediate, result, and value-table contracts are enforced by
/// EIR validation; this pass adds the target-specific semantic restrictions that
/// decide whether an otherwise valid instruction can be lowered faithfully.
fn check_instruction_shape(
    module: &Module,
    collection: &str,
    function: &Function,
    block: u32,
    instruction: u32,
    inst: &Instruction,
    ref_cell_provenance: &RefCellProvenance,
    issues: &mut Vec<String>,
) {
    let issue = match inst.op {
        Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul => {
            checked_int_binop_shape_issue(function, inst)
        }
        Op::FToI => Some(
            "implicit float-to-int coercion requires exact profile-specific warning and deprecation diagnostics"
                .to_string(),
        ),
        Op::LoadLocal | Op::StoreLocal => local_transfer_shape_issue(function, inst),
        Op::LoadGlobal => load_global_shape_issue(module, inst),
        Op::StoreRefCell => store_ref_cell_shape_issue(function, inst),
        Op::Move | Op::Borrow | Op::Acquire => forward_transfer_shape_issue(function, inst),
        Op::UnsetLocal => unset_owned_temp_shape_issue(function, inst),
        Op::Cast => cast_shape_issue(function, inst),
        Op::IToStr => int_like_to_string_shape_issue(function, inst),
        Op::StrictEq | Op::StrictNotEq => strict_compare_shape_issue(function, inst),
        Op::IsTruthy => truthiness_shape_issue(function, inst),
        Op::ArraySet => array_store_shape_issue(function, inst, 2, false),
        Op::ArrayPush => array_store_shape_issue(function, inst, 1, true),
        Op::ArrayToMixed => array_to_mixed_shape_issue(function, inst),
        Op::LooseEq | Op::LooseNotEq => loose_eq_shape_issue(function, inst),
        Op::IterStart => iter_start_shape_issue(function, inst),
        Op::IterCurrentValueRef => iter_current_value_ref_shape_issue(function, inst),
        Op::ArrayGet | Op::ArrayGetSilent => {
            array_get_shape_issue(module, function, block, inst)
        }
        Op::HashGet | Op::HashGetSilent => {
            hash_get_shape_issue(module, function, block, inst)
        }
        Op::HashSet => hash_key_diagnostic_issue(function, inst, 1)
            .or_else(|| hash_store_value_diagnostic_issue(function, inst, 2)),
        Op::HashIsset | Op::HashUnset => hash_key_diagnostic_issue(function, inst, 1),
        Op::LoadStaticProperty | Op::StoreStaticProperty => {
            static_property_shape_issue(module, function, inst)
        }
        Op::ScopedConstantGet => scoped_constant_shape_issue(module, inst),
        Op::HashAppend => hash_store_value_diagnostic_issue(function, inst, 1),
        Op::ArrayToHash => array_to_hash_shape_issue(function, inst),
        Op::Call => {
            direct_call_shape_issue(module, function, inst, ref_cell_provenance)
        }
        Op::MethodCall | Op::NullsafeMethodCall => {
            method_call_shape_issue(module, function, block, inst)
        }
        Op::ObjectNew => object_new_shape_issue(module, function, inst),
        Op::PropGet | Op::NullsafePropGet => {
            property_get_shape_issue(module, function, inst)
        }
        Op::PropSet => property_set_shape_issue(module, function, inst),
        Op::Warn => array_offset_on_null_warning_shape_issue(module, inst),
        Op::ThrowError => method_call_on_null_error_shape_issue(module, inst),
        Op::StaticMethodCall => static_method_call_shape_issue(module, function, inst),
        Op::ClosureCall => closure_call_shape_issue(module, function, inst),
        Op::CallableDescriptorInvoke => {
            callable_descriptor_invoke_shape_issue(module, function, inst)
        }
        Op::ClosureNew => {
            closure_new_by_ref_capture_issue(module, function, inst, ref_cell_provenance)
        }
        Op::FirstClassCallableNew => {
            first_class_callable_new_shape_issue(module, inst)
        }
        _ => None,
    };
    if let Some(issue) = issue {
        issues.push(format!(
            "{collection}::{} block#{block} instruction#{instruction}: unsupported {} shape: {issue}",
            function.name,
            inst.op.name()
        ));
    }
}

/// Returns whether a reachable EIR `Unreachable` immediately follows the one
/// admitted static PHP fatal boundary.
fn unreachable_has_noreturn_proof(
    module: &Module,
    function: &Function,
    block: &crate::ir::BasicBlock,
) -> bool {
    let Some(instruction) = block
        .instructions
        .last()
        .and_then(|inst_id| function.instruction(*inst_id))
    else {
        return false;
    };
    instruction.op == Op::ThrowError
        && method_call_on_null_error_shape_issue(module, instruction).is_none()
}

/// Validates the only static `ThrowError` form supported by the public command
/// backend: the uncaught `Error` produced by an ordinary method call on `null`.
fn method_call_on_null_error_shape_issue(
    module: &Module,
    inst: &Instruction,
) -> Option<String> {
    if !module
        .functions
        .iter()
        .any(|function| function.flags.is_main)
    {
        return Some(
            "method-on-null errors require the public WASI command runtime".to_string(),
        );
    }
    if !inst.operands.is_empty() {
        return Some(format!(
            "method-on-null error expects no operands, got {}",
            inst.operands.len()
        ));
    }
    if inst.result.is_some()
        || inst.result_type != IrType::Void
        || inst.result_php_type.codegen_repr() != PhpType::Void
        || inst.result_ownership != Ownership::NonHeap
    {
        return Some(
            "method-on-null error must have a result-free Void/NonHeap shape".to_string(),
        );
    }
    let Some(message) = data_string(module, inst) else {
        return Some("method-on-null error requires a valid Data immediate".to_string());
    };
    if super::inst::method_call_on_null_name_range(message).is_none() {
        return Some(format!(
            "unsupported static Error message {message:?}; only method calls on null are admitted"
        ));
    }
    None
}

/// Validates the only static warning form admitted by the public command backend.
fn array_offset_on_null_warning_shape_issue(
    module: &Module,
    inst: &Instruction,
) -> Option<String> {
    if !module
        .functions
        .iter()
        .any(|function| function.flags.is_main)
    {
        return Some(
            "array-offset-on-null warnings require the public WASI command runtime".to_string(),
        );
    }
    if !inst.operands.is_empty() {
        return Some(format!(
            "array-offset-on-null warning expects no operands, got {}",
            inst.operands.len()
        ));
    }
    if inst.result.is_some()
        || inst.result_type != IrType::Void
        || inst.result_php_type.codegen_repr() != PhpType::Void
        || inst.result_ownership != Ownership::NonHeap
    {
        return Some(
            "array-offset-on-null warning must have a result-free Void/NonHeap shape".to_string(),
        );
    }
    let Some(message) = data_string(module, inst) else {
        return Some(
            "array-offset-on-null warning requires a valid Data immediate".to_string(),
        );
    };
    if message != crate::codegen_support::runtime::array_offset_on_null_warning() {
        return Some(format!(
            "unsupported static warning {message:?}; only array-offset-on-null is admitted"
        ));
    }
    None
}

/// Admits only the single-use `OwnedTemp` clear used after moving a branch
/// result out of its hidden merge slot.
fn unset_owned_temp_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    if !inst.operands.is_empty() {
        return Some(format!(
            "owned-temp unset must not carry operands, got {}",
            inst.operands.len()
        ));
    }
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return Some("owned-temp unset requires a local-slot immediate".to_string());
    };
    let Some(local) = function.locals.get(slot.as_raw() as usize) else {
        return Some(format!("owned-temp unset references missing slot {slot:?}"));
    };
    if local.kind != LocalKind::OwnedTemp {
        return Some(format!(
            "only OwnedTemp slots may be cleared, got {:?}",
            local.kind
        ));
    }
    if inst.result.is_some()
        || inst.result_type != IrType::Void
        || inst.result_php_type.codegen_repr() != PhpType::Void
    {
        return Some("owned-temp unset must not materialize a result".to_string());
    }
    None
}

/// Collects conservative owned/borrowed ref-binding provenance for one function.
///
/// Promoted cells are owned; by-ref parameters and foreach element bindings are
/// borrowed. Alias edges propagate both classifications to a fixed point. Ambiguous
/// control-flow bindings therefore carry `borrowed` and cannot escape in a closure.
fn collect_ref_cell_provenance(function: &Function) -> RefCellProvenance {
    let mut provenance = RefCellProvenance::default();
    let mut aliases = Vec::new();
    for (index, param) in function.params.iter().enumerate() {
        if param.by_ref {
            provenance.borrowed.insert(index as u32);
        }
    }
    for inst in &function.instructions {
        match (inst.op, &inst.immediate) {
            (Op::PromoteLocalRefCell, Some(Immediate::LocalSlotPair { first, second })) => {
                provenance.owned.insert(first.as_raw());
                provenance.owned.insert(second.as_raw());
            }
            (Op::AliasLocalRefCell, Some(Immediate::LocalSlotPair { first, second })) => {
                aliases.push((first.as_raw(), second.as_raw()));
            }
            (Op::IterCurrentValueRef, Some(Immediate::LocalSlot(slot))) => {
                provenance.borrowed.insert(slot.as_raw());
            }
            _ => {}
        }
    }
    loop {
        let mut changed = false;
        for (target, source) in &aliases {
            if provenance.owned.contains(source) {
                changed |= provenance.owned.insert(*target);
            }
            if provenance.borrowed.contains(source) {
                changed |= provenance.borrowed.insert(*target);
            }
        }
        if !changed {
            break;
        }
    }
    provenance
}

/// Reports a non-escapable by-ref `ClosureNew` operand before WAT emission.
///
/// Fresh `LoadLocal` operands are promotable. `LoadRefCell` operands require an
/// unambiguous owned provenance; borrowed by-ref parameters, foreach element
/// addresses, and aliases derived from either are rejected.
fn closure_new_by_ref_capture_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
    provenance: &RefCellProvenance,
) -> Option<String> {
    let Some(name) = data_string(module, inst) else {
        return Some("missing closure-name data immediate".to_string());
    };
    let closures: Vec<&Function> = module
        .closures
        .iter()
        .filter(|closure| closure.name == name)
        .collect();
    let closure = match closures.as_slice() {
        [closure] => *closure,
        [] => return Some(format!("closure body {name:?} is missing")),
        _ => {
            return Some(format!(
                "closure body {name:?} is ambiguous across {} definitions",
                closures.len()
            ))
        }
    };
    let capture_count = inst.operands.len();
    if capture_count != closure.flags.closure_capture_count {
        return Some(format!(
            "operand count {capture_count} does not match closure capture_count {}",
            closure.flags.closure_capture_count
        ));
    }
    let Some(visible_count) = closure.params.len().checked_sub(capture_count) else {
        return Some(format!(
            "capture count {capture_count} exceeds closure parameter count {}",
            closure.params.len()
        ));
    };
    if let Some(issue) = callable_wrapper_signature_issue(closure, visible_count) {
        return Some(issue);
    }
    if !callable_return_is_boxable(closure) {
        return Some(format!(
            "callable return {:?}/{:?} cannot be boxed by the WASM wrapper",
            closure.return_type,
            closure.return_php_type.codegen_repr()
        ));
    }
    for (operand, param) in inst
        .operands
        .iter()
        .zip(&closure.params[visible_count..])
    {
        if !param.by_ref {
            continue;
        }
        let Some(value) = function.value(*operand) else {
            return Some(format!(
                "by-ref capture {} operand {:?} is missing",
                param.name, operand
            ));
        };
        let ValueDef::Instruction { inst: source_id, .. } = value.def else {
            return Some(format!(
                "by-ref capture {} source is not a local load",
                param.name
            ));
        };
        let Some(source) = function.instruction(source_id) else {
            return Some(format!(
                "by-ref capture {} source instruction {:?} is missing",
                param.name, source_id
            ));
        };
        match (source.op, &source.immediate) {
            (Op::LoadLocal, Some(Immediate::LocalSlot(_))) => {}
            (Op::LoadRefCell, Some(Immediate::LocalSlot(slot)))
                if provenance.owned.contains(&slot.as_raw())
                    && !provenance.borrowed.contains(&slot.as_raw()) => {}
            (Op::LoadRefCell, Some(Immediate::LocalSlot(slot))) => {
                return Some(format!(
                    "by-ref capture {} cannot escape non-owned ref-bound local#{}",
                    param.name,
                    slot.as_raw()
                ));
            }
            _ => {
                return Some(format!(
                    "by-ref capture {} source must be LoadLocal or an owned LoadRefCell",
                    param.name
                ));
            }
        }
    }
    None
}

/// Validates one free-function first-class callable against PHP name folding,
/// collision rules, and the wrapper's supported parameter/result shapes.
fn first_class_callable_new_shape_issue(
    module: &Module,
    inst: &Instruction,
) -> Option<String> {
    let Some(name) = data_string(module, inst) else {
        return Some("missing callable-name Data immediate".to_string());
    };
    let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
    let matches: Vec<&Function> = module
        .functions
        .iter()
        .filter(|function| {
            !function.flags.is_main
                && crate::names::php_symbol_key(function.name.trim_start_matches('\\')) == key
        })
        .collect();
    let function = match matches.as_slice() {
        [function] => *function,
        [] => return Some(format!("free-function callable target {name:?} is missing")),
        _ => {
            return Some(format!(
                "free-function callable target {name:?} is ambiguous across {} definitions",
                matches.len()
            ))
        }
    };
    if let Some(issue) = callable_wrapper_signature_issue(function, function.params.len()) {
        return Some(issue);
    }
    if !callable_return_is_boxable(function) {
        return Some(format!(
            "callable return {:?}/{:?} cannot be boxed by the WASM wrapper",
            function.return_type,
            function.return_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates the two-integer to owned-Mixed contract of checked arithmetic.
fn checked_int_binop_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let [lhs, rhs] = inst.operands.as_slice() else {
        return Some(format!(
            "expected two integer operands, got {}",
            inst.operands.len()
        ));
    };
    for (label, operand) in [("lhs", lhs), ("rhs", rhs)] {
        let Some(value) = function.value(*operand) else {
            return Some(format!("{label} operand is missing from the value table"));
        };
        if value.ir_type != IrType::I64 || value.php_type.codegen_repr() != PhpType::Int {
            return Some(format!(
                "{label} must be int/I64, got {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if inst.result.is_none()
        || inst.result_type != IrType::Heap(IrHeapKind::Mixed)
        || inst.result_php_type.codegen_repr() != PhpType::Mixed
    {
        return Some(format!(
            "checked arithmetic must materialize a Mixed cell, got {:?}/{:?}",
            inst.result_type,
            inst.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Reports a conversion that would silently unbox a dynamic Mixed cell.
///
/// Boxing and exact copies preserve the source value. `UnboxMixed` delegates to
/// runtime scalar casts whose per-tag warnings and failures are not complete.
fn value_transfer_shape_issue(
    source_ir: IrType,
    source_php: PhpType,
    destination_ir: IrType,
    destination_php: PhpType,
) -> Option<String> {
    match transfer::classify_transfer(
        source_ir,
        source_php,
        destination_ir,
        destination_php,
    ) {
        // An implicit Mixed-to-scalar transfer unboxes through the same `__rt_mixed_cast_*`
        // helpers the NATIVE backend uses for the identical coercion, so the two targets answer
        // alike. It is what a local widened by checked arithmetic needs — `$i = $i + 1` types
        // `$i` Mixed, and every later read of it is one of these — so refusing it turned away
        // ordinary counting loops.
        //
        // The known gap it inherits is the EIR's, not this lowering's: a load is typed `int` from
        // the slot's type BEFORE the loop's widening store, so once an add really does overflow
        // into a float the read still claims an integer. The native backend narrows there too and
        // prints a saturated `9223372036854775807` where PHP prints `9.2233720368548E+18`.
        Ok(transfer::TransferKind::UnboxMixed) => None,
        Ok(
            transfer::TransferKind::Copy
            | transfer::TransferKind::BoxMixed
            | transfer::TransferKind::WidenArrayToMixed { .. }
            | transfer::TransferKind::NullPointer,
        ) => None,
        Err(error) => Some(error.to_string()),
    }
}

/// Returns whether a cast's result is used ONLY where PHP renders a value as a string.
///
/// `"v=" . $mixed` and `echo $mixed` reach an implicit `Str` cast, and PHP's conversion there
/// is the SAME operation `(string)` performs — the array warning and the object fatal
/// included, with no `TypeError` in sight. So the implicit cast is exact in that context,
/// unlike one at a typed parameter or return, where PHP raises instead of converting.
///
/// The check is over every consumer, so a result that also flows somewhere else stays refused.
fn cast_feeds_string_context(function: &Function, inst: &Instruction) -> bool {
    let Some(result) = inst.result else {
        return false;
    };
    let mut consumed = false;
    for candidate in &function.instructions {
        if !candidate.operands.contains(&result) {
            continue;
        }
        // Ownership bookkeeping is not a USE: the EIR releases the cast's temporary right
        // after the concat consumes it, and that release says nothing about the context.
        if matches!(
            candidate.op,
            Op::Acquire | Op::Release | Op::Move | Op::Borrow
        ) {
            continue;
        }
        consumed = true;
        if !matches!(
            candidate.op,
            Op::StrConcat | Op::EchoValue | Op::StrInterpolate | Op::WriteStrStdout | Op::StrLen
        ) {
            return false;
        }
    }
    consumed
}

/// Returns whether this value is a Mixed that INTEGER arithmetic produced, rather than one PHP
/// would coerce.
///
/// `$a + $b` on two ints is typed Mixed only because an overflow would promote it to a float —
/// PHP itself performs no conversion, so narrowing the result back is exact for every value that
/// did not overflow. Chained arithmetic reaches the same shape through `MixedNumericBinop`, whose
/// left operand is the previous Mixed, so the walk is transitive: every operand must itself be a
/// plain integer or another widened integer computation.
///
/// That transitivity is the whole point. `function f(mixed $m): int { return $m + 1; }` also
/// emits a `MixedNumericBinop`, but its operand is a genuine `mixed` — PHP really does coerce
/// there, with a diagnostic this target cannot raise yet — so the walk refuses it.
fn value_is_widened_integer_arithmetic(
    function: &Function,
    value: crate::ir::ValueId,
    visiting: &mut HashSet<crate::ir::ValueId>,
) -> bool {
    // A loop-carried counter is CYCLIC: the boxing store reads the very slot it writes. Treating
    // a value already on the walk as satisfied is what makes the cycle converge — it adds no
    // source of its own, so the answer rests on the other stores into the slot.
    if !visiting.insert(value) {
        return true;
    }
    if let Some(defined) = function.value(value) {
        if defined.ir_type == IrType::I64 && matches!(defined.php_type.codegen_repr(), PhpType::Int)
        {
            return true;
        }
    }
    let Some(instruction) = function
        .instructions
        .iter()
        .find(|instruction| instruction.result == Some(value))
    else {
        return false;
    };
    match instruction.op {
        Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul | Op::MixedNumericBinop => instruction
            .operands
            .iter()
            .all(|operand| value_is_widened_integer_arithmetic(function, *operand, visiting)),
        // Boxing a number is how a loop-carried counter gets into its Mixed slot: the checker
        // reports the widened storage and lowering boxes the local at loop entry.
        Op::MixedBox => instruction
            .operands
            .iter()
            .all(|operand| value_is_widened_integer_arithmetic(function, *operand, visiting)),
        // Reading a local is integer-derived when EVERY store into that slot is. A `mixed`
        // PARAMETER has no store at all, so it fails this and keeps its refusal — which is the
        // case PHP really coerces, with a diagnostic this target cannot raise yet. So does a slot
        // fed by a `mixed`-returning call.
        Op::LoadLocal => {
            let Some(Immediate::LocalSlot(slot)) = instruction.immediate else {
                return false;
            };
            let mut stored_at_least_once = false;
            for store in function.instructions.iter().filter(|candidate| {
                candidate.op == Op::StoreLocal
                    && matches!(
                        candidate.immediate,
                        Some(Immediate::LocalSlot(candidate_slot)) if candidate_slot == slot
                    )
            }) {
                stored_at_least_once = true;
                let Some(source) = store.operands.first() else {
                    return false;
                };
                if !value_is_widened_integer_arithmetic(function, *source, visiting) {
                    return false;
                }
            }
            stored_at_least_once
        }
        // `acquire` forwards its operand unchanged.
        Op::Acquire => instruction
            .operands
            .first()
            .is_some_and(|operand| {
                value_is_widened_integer_arithmetic(function, *operand, visiting)
            }),
        _ => false,
    }
}

/// Validates the typed transfer performed by a local load or store.
fn local_transfer_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return Some("local transfer requires a LocalSlot immediate".to_string());
    };
    let Some(local) = function
        .locals
        .iter()
        .find(|local| local.id == slot)
    else {
        return Some(format!("local transfer references missing slot {slot:?}"));
    };
    match inst.op {
        Op::LoadLocal => {
            if inst.result.is_none() {
                return Some("local load must materialize a result".to_string());
            }
            value_transfer_shape_issue(
                local.ir_type,
                local.php_type.codegen_repr(),
                inst.result_type,
                inst.result_php_type.codegen_repr(),
            )
        }
        Op::StoreLocal => {
            let [source] = inst.operands.as_slice() else {
                return Some(format!(
                    "local store expects one operand, got {}",
                    inst.operands.len()
                ));
            };
            let Some(source) = function.value(*source) else {
                return Some("local store source is missing from the value table".to_string());
            };
            value_transfer_shape_issue(
                source.ir_type,
                source.php_type.codegen_repr(),
                local.ir_type,
                local.php_type.codegen_repr(),
            )
        }
        _ => None,
    }
}

/// Admits only `$argc` and `$argv` with the exact source shapes built by WASI.
fn load_global_shape_issue(module: &Module, inst: &Instruction) -> Option<String> {
    let Some(Immediate::GlobalName(name)) = inst.immediate else {
        return Some("global load requires a GlobalName immediate".to_string());
    };
    let Some(name) = module.data.global_names.get(name.as_raw() as usize) else {
        return Some("global load references an unknown name".to_string());
    };
    if inst.result.is_none() {
        return Some(format!("global ${name} load must materialize a result"));
    }
    let (source_ir, source_php) = match name.as_str() {
        "argc" => (IrType::I64, PhpType::Int),
        "argv" => (
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Str)),
        ),
        _ => return Some(format!("global ${name} is not implemented by the WASI runtime")),
    };
    value_transfer_shape_issue(
        source_ir,
        source_php,
        inst.result_type,
        inst.result_php_type.codegen_repr(),
    )
}

/// Requires a ref-cell store to preserve the cell payload representation.
///
/// The current lowerer only has a partial Mixed narrowing path and otherwise
/// writes raw operand bits. Any source/target type drift can therefore either
/// omit PHP coercion diagnostics or corrupt the referenced payload layout.
fn store_ref_cell_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let [source] = inst.operands.as_slice() else {
        return Some(format!(
            "ref-cell store expects one operand, got {}",
            inst.operands.len()
        ));
    };
    let Some(source) = function.value(*source) else {
        return Some("ref-cell store source is missing from the value table".to_string());
    };
    let source_php = source.php_type.codegen_repr();
    let target_php = inst.result_php_type.codegen_repr();
    if source_php != target_php
        || transfer::validate_storage_pair(source.ir_type, &source.php_type).is_err()
    {
        return Some(format!(
            "ref-cell store value {:?}/{source_php:?} must exactly match payload {target_php:?}",
            source.ir_type
        ));
    }
    None
}

/// Validates the typed transfer performed by ownership-only forwarders.
fn forward_transfer_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let [source] = inst.operands.as_slice() else {
        return Some(format!(
            "{} expects one operand, got {}",
            inst.op.name(),
            inst.operands.len()
        ));
    };
    let Some(source) = function.value(*source) else {
        return Some("forwarded source is missing from the value table".to_string());
    };
    if inst.result.is_none() {
        return Some(format!("{} must materialize a result", inst.op.name()));
    }
    value_transfer_shape_issue(
        source.ir_type,
        source.php_type.codegen_repr(),
        inst.result_type,
        inst.result_php_type.codegen_repr(),
    )
}

/// Validates the exact source/target pairs implemented by `lower_cast`.
fn cast_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let [operand] = inst.operands.as_slice() else {
        return Some(format!(
            "expected one source operand, got {}",
            inst.operands.len()
        ));
    };
    let Some(source) = function.value(*operand) else {
        return Some("cast source is missing from the value table".to_string());
    };
    let (target, explicit) = match inst.immediate {
        Some(Immediate::CastTarget(target)) => (target, false),
        Some(Immediate::ExplicitCastTarget(target)) => (target, true),
        _ => {
            return Some(
                "missing CastTarget or ExplicitCastTarget immediate".to_string(),
            )
        }
    };
    if inst.result.is_none() || target != inst.result_type {
        return Some(format!(
            "cast target {target:?} must equal the materialized result {:?}",
            inst.result_type
        ));
    }

    let source_php = source.php_type.codegen_repr();
    let result_php = inst.result_php_type.codegen_repr();
    let exact_mixed_scalar = source.ir_type == IrType::Heap(IrHeapKind::Mixed)
        && source_php == PhpType::Mixed
        && matches!(target, IrType::I64 | IrType::F64 | IrType::Str)
        && mixed_scalar_cast_source_is_exact(
            function,
            *operand,
            target,
            &result_php,
            explicit,
        );
    // An EXPLICIT `(int)` / `(float)` runs `__rt_mixed_cast_int` / `__rt_mixed_cast_float`,
    // whose per-tag answers are measured against php-src 8.5 down to the array-yields-one rule
    // and the object diagnostic. An IMPLICIT coercion is a different operation entirely — the
    // same value that `(int)` turns into 0 or a wrapped integer makes a declared `int` return
    // raise a `TypeError` — so it stays refused until that path carries its own diagnostics.
    // `(string)` is refused for both: its array and object arms still answer the empty string
    // where PHP produces "Array" with a notice, or dispatches `__toString`.
    // A Mixed produced by CHECKED ARITHMETIC is a widening artefact, not a value PHP ever
    // coerces: `Math::square(int $x): int { return $x * $x; }` multiplies two ints, and the
    // result is typed Mixed only because an overflow would promote it to a float. PHP performs no
    // conversion there — `square(7)` is just 49 — so narrowing it back is exact for every value
    // that did not overflow, and refusing it turned away arithmetic in any typed function.
    //
    // This is deliberately narrower than "implicit": a genuinely Mixed value reaching a typed
    // parameter or return IS coerced by PHP, with a `TypeError` or a `Deprecated` this target
    // cannot yet raise, and that stays refused.
    let widened_by_checked_arithmetic =
        value_is_widened_integer_arithmetic(function, *operand, &mut HashSet::new());
    // An EXPLICIT `(string)` is now exact for every tag too: the array arm warns and yields
    // "Array", and the object arm raises PHP's fatal naming the class. What stays refused is
    // the IMPLICIT coercion, a different operation — the same value `(int)` turns into 0
    // makes a declared `int` return raise a `TypeError` this target cannot yet produce.
    let admitted_mixed_scalar = (explicit || widened_by_checked_arithmetic)
        && matches!(target, IrType::I64 | IrType::F64)
        || ((explicit || cast_feeds_string_context(function, inst))
            && target == IrType::Str);
    if source.ir_type == IrType::Heap(IrHeapKind::Mixed)
        && source_php == PhpType::Mixed
        && matches!(target, IrType::I64 | IrType::F64 | IrType::Str)
        && !exact_mixed_scalar
        && !admitted_mixed_scalar
    {
        return Some(
            "Mixed-to-scalar casts require exact per-tag PHP values and diagnostics"
                .to_string(),
        );
    }
    if source.ir_type == IrType::F64
        && source_php == PhpType::Float
        && target == IrType::I64
        && result_php == PhpType::Int
        && !explicit
    {
        return Some(
            "implicit float-to-int casts require exact profile-specific out-of-range diagnostics"
                .to_string(),
        );
    }
    let supported = match (source.ir_type, target) {
        (IrType::TaggedScalar, IrType::I64) => {
            source_php == PhpType::TaggedScalar
                && matches!(result_php, PhpType::Int | PhpType::Bool)
        }
        (IrType::TaggedScalar, IrType::F64) => {
            source_php == PhpType::TaggedScalar && result_php == PhpType::Float
        }
        (IrType::I64, IrType::I64) => {
            matches!(source_php, PhpType::Int | PhpType::Bool) && source_php == result_php
        }
        (IrType::F64, IrType::F64) => {
            source_php == PhpType::Float && result_php == PhpType::Float
        }
        (IrType::Str, IrType::Str) => {
            source_php == PhpType::Str && result_php == PhpType::Str
        }
        (IrType::I64, IrType::F64) => {
            source_php == PhpType::Int && result_php == PhpType::Float
        }
        // Only the explicit `(int)` cast is admitted: the implicit coercion is
        // rejected above because its diagnostics differ from this one.
        (IrType::F64, IrType::I64) => {
            explicit && source_php == PhpType::Float && result_php == PhpType::Int
        }
        (IrType::Heap(IrHeapKind::Mixed), _) if exact_mixed_scalar => true,
        // An explicit `(int)` / `(float)` over an arbitrary Mixed: the runtime dispatches on the
        // cell's tag and reproduces php-src's answer for each, diagnostics included.
        (IrType::Heap(IrHeapKind::Mixed), IrType::I64) => {
            admitted_mixed_scalar && matches!(result_php, PhpType::Int | PhpType::Bool)
        }
        (IrType::Heap(IrHeapKind::Mixed), IrType::F64) => {
            admitted_mixed_scalar && result_php == PhpType::Float
        }
        // `(string)` reaches every tag exactly too: "Array" with a warning for a container,
        // and PHP's fatal naming the class for an object.
        (IrType::Heap(IrHeapKind::Mixed), IrType::Str) => {
            admitted_mixed_scalar && result_php == PhpType::Str
        }
        _ => false,
    };
    if !supported {
        return Some(format!(
            "unsupported conversion {:?}/{source_php:?} to {target:?}/{result_php:?}",
            source.ir_type
        ));
    }
    None
}

/// Returns whether a boxed nullable array/hash read proves one exact scalar tag.
///
/// Nullable scalar reads use a Mixed cell to retain the hit/miss distinction.
/// A subsequent cast may unbox the statically declared element tag without a
/// dynamic PHP coercion; arbitrary Mixed producers remain outside this proof.
fn mixed_scalar_cast_source_is_exact(
    function: &Function,
    source: ValueId,
    target: IrType,
    result_php: &PhpType,
    explicit: bool,
) -> bool {
    let mut current = source;
    for _ in 0..=function.values.len() {
        let Some(value) = function.value(current) else {
            return false;
        };
        let ValueDef::Instruction { inst, .. } = value.def else {
            return false;
        };
        let Some(defining) = function.instruction(inst) else {
            return false;
        };
        match defining.op {
            Op::Move | Op::Borrow | Op::Acquire => {
                let Some(forwarded) = defining.operands.first() else {
                    return false;
                };
                current = *forwarded;
            }
            Op::ArrayGet | Op::ArrayGetSilent => {
                let Some(container) = defining.operands.first().and_then(|id| function.value(*id))
                else {
                    return false;
                };
                let PhpType::Array(element) = container.php_type.codegen_repr() else {
                    return false;
                };
                return exact_scalar_cast_pair(
                    &element.codegen_repr(),
                    target,
                    result_php,
                    explicit,
                );
            }
            Op::HashGet | Op::HashGetSilent => {
                let Some(container) = defining.operands.first().and_then(|id| function.value(*id))
                else {
                    return false;
                };
                let PhpType::AssocArray { value, .. } = container.php_type.codegen_repr() else {
                    return false;
                };
                return exact_scalar_cast_pair(
                    &value.codegen_repr(),
                    target,
                    result_php,
                    explicit,
                );
            }
            _ => return false,
        }
    }
    false
}

/// Returns whether one declared container element has exact cast semantics for the target.
fn exact_scalar_cast_pair(
    element: &PhpType,
    target: IrType,
    result_php: &PhpType,
    explicit: bool,
) -> bool {
    let identity = matches!(
        (element, target, result_php),
        (PhpType::Bool | PhpType::False, IrType::I64, PhpType::Bool)
            | (PhpType::Float, IrType::F64, PhpType::Float)
            | (PhpType::Str, IrType::Str, PhpType::Str)
    );
    identity
        || (explicit
            && matches!(
                (element, target, result_php),
                (
                    PhpType::Bool | PhpType::False,
                    IrType::I64,
                    PhpType::Int | PhpType::Bool
                ) | (
                    PhpType::Str,
                    IrType::I64,
                    PhpType::Int | PhpType::Bool
                ) | (PhpType::Str, IrType::Str, PhpType::Str)
            ))
}

/// Rejects float and dynamic-Mixed truthiness until warning behavior is exact.
fn truthiness_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let [operand] = inst.operands.as_slice() else {
        return None;
    };
    let value = function.value(*operand)?;
    if value.ir_type == IrType::F64
        || value.ir_type == IrType::Heap(IrHeapKind::Mixed)
        || matches!(value.php_type.codegen_repr(), PhpType::Float | PhpType::Mixed)
    {
        return Some(
            "float or Mixed truthiness requires exact profile-specific NAN diagnostics"
                .to_string(),
        );
    }
    None
}

/// Admits only the stable integer-backed forms implemented by the WASM lowerer.
///
/// The shared EIR uses `MaybeOwned` for strings, while the WASM lowerer
/// materializes an owned heap copy before publishing the result.
fn int_like_to_string_shape_issue(
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let [operand] = inst.operands.as_slice() else {
        return Some(format!(
            "IToStr expects one operand, got {}",
            inst.operands.len()
        ));
    };
    if inst.immediate.is_some() {
        return Some("IToStr does not accept an immediate".to_string());
    }
    let Some(source) = function.value(*operand) else {
        return Some("IToStr source is missing from the value table".to_string());
    };
    if source.ir_type != IrType::I64
        || !matches!(source.php_type.codegen_repr(), PhpType::Int | PhpType::Bool)
    {
        return Some(format!(
            "IToStr requires I64/Int or I64/Bool, got {:?}/{:?}",
            source.ir_type,
            source.php_type.codegen_repr()
        ));
    }
    if inst.result.is_none()
        || inst.result_type != IrType::Str
        || inst.result_php_type.codegen_repr() != PhpType::Str
        || inst.result_ownership != Ownership::MaybeOwned
    {
        return Some(format!(
            "IToStr requires the EIR MaybeOwned Str/String contract, got {:?}/{:?}/{:?}",
            inst.result_type,
            inst.result_php_type.codegen_repr(),
            inst.result_ownership
        ));
    }
    None
}

/// Admits only exact scalar, string, and object shapes for strict comparison.
fn strict_compare_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let [lhs, rhs] = inst.operands.as_slice() else {
        return Some(format!(
            "{} expects two operands, got {}",
            inst.op.name(),
            inst.operands.len()
        ));
    };
    if inst.immediate.is_some() {
        return Some(format!("{} does not accept an immediate", inst.op.name()));
    }
    if inst.result.is_none()
        || inst.result_type != IrType::I64
        || inst.result_php_type != PhpType::Bool
        || inst.result_ownership != Ownership::NonHeap
    {
        return Some(format!(
            "{} requires an I64/Bool/NonHeap result, got {:?}/{:?}/{:?}",
            inst.op.name(),
            inst.result_type,
            inst.result_php_type,
            inst.result_ownership
        ));
    }
    let mut kinds = Vec::with_capacity(2);
    for (side, value_id) in [("lhs", lhs), ("rhs", rhs)] {
        let Some(value) = function.value(*value_id) else {
            return Some(format!(
                "{} {} is missing from the value table",
                inst.op.name(),
                side
            ));
        };
        let Some(kind) = super::strict::classify_strict_value(
            value.ir_type,
            &value.php_type,
            value.ownership,
        ) else {
            return Some(format!(
                "{} rejects {} shape {:?}/{:?}/{:?}",
                inst.op.name(),
                side,
                value.ir_type,
                value.php_type,
                value.ownership
            ));
        };
        kinds.push(kind);
    }
    // Each side being admissible is not enough: two runtime-tagged cells could both hold arrays,
    // whose identity is PHP's deep element-wise comparison rather than anything this lowers.
    if !super::strict::strict_pair_is_supported(kinds[0], kinds[1]) {
        return Some(format!(
            "{} rejects the pair {:?}/{:?}",
            inst.op.name(),
            kinds[0],
            kinds[1]
        ));
    }
    None
}

/// Validates `Op::LooseEq`/`Op::LooseNotEq` against the pairs the lowerer implements.
///
/// PHP 8's `==` table is much wider than this: anything involving a Mixed cell, an array, an
/// object, or a bool against a number needs rules this backend has not measured yet, and answering
/// those by guessing would be a silently wrong answer rather than a refusal.
fn loose_eq_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let [left, right] = inst.operands.as_slice() else {
        return Some(format!(
            "loose comparison takes two operands, got {}",
            inst.operands.len()
        ));
    };
    let mut kinds = Vec::new();
    for operand in [left, right] {
        let Some(value) = function.value(*operand) else {
            return Some("loose comparison operand is missing from the value table".to_string());
        };
        kinds.push((value.ir_type, value.php_type.codegen_repr()));
    }
    let admitted = match (&kinds[0], &kinds[1]) {
        ((IrType::I64, left), (IrType::I64, right)) => {
            // Two ints or two bools compare as machine words; a bool against a number is a
            // DIFFERENT rule (PHP casts the number to bool) and is not lowered.
            matches!(
                (left, right),
                (PhpType::Int, PhpType::Int)
                    | (PhpType::Bool | PhpType::False, PhpType::Bool | PhpType::False)
            )
        }
        ((IrType::F64, PhpType::Float), (IrType::F64, PhpType::Float)) => true,
        ((IrType::Str, PhpType::Str), (IrType::Str, PhpType::Str)) => true,
        ((IrType::I64, PhpType::Int), (IrType::F64, PhpType::Float)) => true,
        ((IrType::F64, PhpType::Float), (IrType::I64, PhpType::Int)) => true,
        _ => false,
    };
    if !admitted {
        return Some(format!(
            "loose comparison of {:?}/{:?} against {:?}/{:?} is not lowered",
            kinds[0].0, kinds[0].1, kinds[1].0, kinds[1].1
        ));
    }
    None
}

/// Validates `Op::ArrayToMixed`: an indexed array whose element type has a lowered copy.
///
/// This is EIR's own widening instruction, which it emits when a concrete array is stored where
/// `array<mixed>` is expected. It shares `__rt_array_widen_to_mixed` with the call-argument
/// conversion; unlike that one, the result is an EIR value the EIR itself releases.
fn array_to_mixed_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let Some(source) = inst.operands.first().and_then(|id| function.value(*id)) else {
        return Some("array_to_mixed source is missing from the value table".to_string());
    };
    if source.ir_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "array_to_mixed takes an indexed array, got {:?}",
            source.ir_type
        ));
    }
    let PhpType::Array(element) = source.php_type.codegen_repr() else {
        return Some(format!(
            "array_to_mixed takes an indexed array, got {:?}",
            source.php_type.codegen_repr()
        ));
    };
    if super::transfer::array_widen_shape(&element).is_none() {
        return Some(format!(
            "array_to_mixed has no lowered element copy for {element:?}"
        ));
    }
    None
}

/// Validates indexed-array write storage against the helper selected by WASM.
///
/// The current runtime has exact layouts only for int/bool-like, float, string and
/// object arrays. `ArrayPush` additionally supports an already-boxed Mixed/Union cell,
/// and BOXES a concrete scalar when the destination stores Mixed cells — which is
/// what a heterogeneous array literal emits, since EIR pushes raw scalars into an
/// `array<mixed>` and leaves the boxing to the backend. `ArraySet` has no boxing
/// setter yet, so it still refuses that.
fn array_store_shape_issue(
    function: &Function,
    inst: &Instruction,
    value_index: usize,
    is_push: bool,
) -> Option<String> {
    let Some(array) = inst.operands.first().and_then(|id| function.value(*id)) else {
        return Some("array write source is missing from the value table".to_string());
    };
    let PhpType::Array(element) = array.php_type.codegen_repr() else {
        return Some(format!(
            "array write requires Array<T>/Heap(Array), got {:?}/{:?}",
            array.ir_type,
            array.php_type.codegen_repr()
        ));
    };
    if array.ir_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "array write requires Heap(Array), got {:?}",
            array.ir_type
        ));
    }
    if inst.op == Op::ArraySet {
        let Some(index) = inst.operands.get(1).and_then(|id| function.value(*id)) else {
            return Some("array set index is missing from the value table".to_string());
        };
        if index.ir_type != IrType::I64
            || !matches!(
                index.php_type.codegen_repr(),
                PhpType::Int | PhpType::Bool | PhpType::False
            )
        {
            return Some(format!(
                "array set index must be int-like/I64, got {:?}/{:?}",
                index.ir_type,
                index.php_type.codegen_repr()
            ));
        }
    }
    let Some(source) = inst
        .operands
        .get(value_index)
        .and_then(|id| function.value(*id))
    else {
        return Some("array write value is missing from the value table".to_string());
    };
    let element = element.codegen_repr();
    let source_php = source.php_type.codegen_repr();
    let exact = match (&element, source.ir_type, &source_php) {
        (
            PhpType::Int | PhpType::Bool | PhpType::False,
            IrType::I64,
            PhpType::Int | PhpType::Bool | PhpType::False,
        ) => element == source_php,
        (PhpType::Str, IrType::Str, PhpType::Str) => true,
        // A float shares the int slot width; the array's value_type 2 records which it is.
        (PhpType::Float, IrType::F64, PhpType::Float) => true,
        // An object slot holds its pointer; value_type 4 is what makes the deep free reach it.
        (PhpType::Object(element), IrType::Heap(IrHeapKind::Object), PhpType::Object(source)) => {
            *element == *source
        }
        // A nested indexed array is the same pointer slot under value_type 5. The inner
        // element types must agree exactly — a slot is just a pointer, so nothing here can
        // convert one layout to another, and admitting a mismatch would read the child with
        // the wrong stride. An inner `Never` (a literal `[]`) has no decided layout yet, so
        // it is interchangeable with any.
        (PhpType::Array(element), IrType::Heap(IrHeapKind::Array), PhpType::Array(source)) => {
            element.codegen_repr() == source.codegen_repr()
                || matches!(source.codegen_repr(), PhpType::Void)
                || matches!(element.codegen_repr(), PhpType::Void)
        }
        // An associative array element is the same pointer slot under value_type 6. The two
        // key/value shapes must agree exactly for the same reason: a slot is just a pointer.
        (
            PhpType::AssocArray { key, value },
            IrType::Heap(IrHeapKind::Hash),
            PhpType::AssocArray {
                key: source_key,
                value: source_value,
            },
        ) => {
            key.codegen_repr() == source_key.codegen_repr()
                && value.codegen_repr() == source_value.codegen_repr()
        }
        (
            PhpType::Void | PhpType::Never,
            IrType::I64,
            PhpType::Int | PhpType::Bool | PhpType::False,
        ) => true,
        (PhpType::Void | PhpType::Never, IrType::F64, PhpType::Float) => true,
        (PhpType::Void | PhpType::Never, IrType::Str, PhpType::Str) => true,
        // An `array<never>` — a literal `[]` — has no decided slot layout, so the FIRST push
        // is what fixes it. A container element shapes it to pointer slots the same way a
        // scalar shapes it to 8-byte ones; this is the accumulator `$out = []; $out[] = $row;`.
        (
            PhpType::Void | PhpType::Never,
            IrType::Heap(IrHeapKind::Array | IrHeapKind::Object),
            PhpType::Array(_) | PhpType::Object(_),
        ) if is_push => true,
        (PhpType::Mixed, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed)
            if is_push =>
        {
            true
        }
        // A raw scalar boxed into a Mixed cell at the push site. Each of these has an
        // exact tag and payload; a heap container has neither, so it stays refused.
        (
            PhpType::Mixed,
            IrType::I64,
            PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Void,
        ) if is_push => true,
        (PhpType::Mixed, IrType::F64, PhpType::Float) if is_push => true,
        (PhpType::Mixed, IrType::Str, PhpType::Str) if is_push => true,
        // An object boxes into a cell under tag 6, which is what an array of interface
        // implementors is: the checker types it `array<mixed>` because the classes differ.
        (PhpType::Mixed, IrType::Heap(IrHeapKind::Object), PhpType::Object(_)) if is_push => true,
        // A nested array boxes under tag 4 — what `[["a"], []]` needs, since inner element
        // types that differ make the checker type the outer literal `array<mixed>`.
        (PhpType::Mixed, IrType::Heap(IrHeapKind::Array), PhpType::Array(_)) if is_push => true,
        (
            PhpType::Void | PhpType::Never,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        ) if is_push => true,
        (PhpType::Union(_), IrType::Heap(IrHeapKind::Union), PhpType::Union(_))
            if is_push =>
        {
            element == source_php
        }
        _ => false,
    };
    if !exact {
        return Some(format!(
            "array write value {:?}/{source_php:?} does not match supported element storage {element:?}",
            source.ir_type
        ));
    }
    None
}

/// Validates one static-property access against the placement `codegen_wasm::statics` built.
///
/// A static is one 16-byte slot in static memory, shaped like an instance property slot, so
/// the supported types are the ones that fit two words directly: int, bool, float, string.
/// A Mixed or container static needs a heap cell the data region cannot express, and a
/// `static::`/`self::` receiver picks its slot from the CALLED class at runtime, which a
/// compile-time address cannot follow — both stay refused rather than guessed.
fn static_property_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return Some("static property access without an interned label".to_string());
    };
    let Some(label) = module.data.strings.get(data.as_raw() as usize) else {
        return Some("static property label is missing from the data pool".to_string());
    };
    let slots = match super::statics::plan_static_slots_for_audit(module) {
        Some(slots) => slots,
        None => return Some("static property placement is unavailable".to_string()),
    };
    let Some(slot) = super::statics::resolve_label(module, &slots, label) else {
        return Some(format!("static property {label} has no lowered slot"));
    };
    if !matches!(
        slot.php_type.codegen_repr(),
        PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Float | PhpType::Str
    ) {
        return Some(format!(
            "static property {label} typed {:?} has no WASM slot shape",
            slot.php_type.codegen_repr()
        ));
    }
    if inst.op == Op::StoreStaticProperty {
        let Some(value) = inst.operands.first().and_then(|id| function.value(*id)) else {
            return Some("static property store is missing its value".to_string());
        };
        // A Mixed source narrows into a concrete slot, exactly as an instance property store
        // does: `K::$count = K::$count + 1` widens through the checked add and the slot stays
        // an int.
        let narrows = value.ir_type == IrType::Heap(IrHeapKind::Mixed)
            && value.php_type.codegen_repr() == PhpType::Mixed;
        if !narrows && value.php_type.codegen_repr() != slot.php_type.codegen_repr() {
            return Some(format!(
                "static property {label} takes {:?}, got {:?}",
                slot.php_type.codegen_repr(),
                value.php_type.codegen_repr()
            ));
        }
    }
    None
}

/// Validates a scoped constant against the enum-case placement.
///
/// Only an ENUM CASE is lowered here: it is the form PHP models as a class constant holding
/// a singleton object, and the one this backend can materialize. An ordinary class constant
/// is folded to its literal by the emitter long before this, so anything still arriving as a
/// `ScopedConstantGet` that is not a case has no shape to give it.
fn scoped_constant_shape_issue(module: &Module, inst: &Instruction) -> Option<String> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return Some("scoped constant without an interned label".to_string());
    };
    let Some(label) = module.data.strings.get(data.as_raw() as usize) else {
        return Some("scoped constant label is missing from the data pool".to_string());
    };
    let Some(slots) = super::statics::plan_static_slots_for_audit(module) else {
        return Some("enum case placement is unavailable".to_string());
    };
    let Some((_, enum_name, case)) = super::statics::resolve_enum_case(module, &slots, label)
    else {
        return Some(format!("scoped constant {label} is not an enum case"));
    };
    let Some(class_info) = module.class_infos.get(enum_name) else {
        return Some(format!("enum {enum_name} has no class shape to materialize"));
    };
    // The materializer writes `name`, and `value` for a backed case; both must be real slots
    // whose type the scalar-default writer can render.
    let mut required = vec![("name", PhpType::Str)];
    if let Some(value) = &case.value {
        required.push((
            "value",
            match value {
                crate::types::EnumCaseValue::Int(_) => PhpType::Int,
                crate::types::EnumCaseValue::Str(_) => PhpType::Str,
            },
        ));
    }
    for (property, expected) in required {
        match class_info
            .properties
            .iter()
            .find(|(name, _)| name == property)
        {
            Some((_, declared)) if declared.codegen_repr() == expected => {}
            Some((_, declared)) => {
                return Some(format!(
                    "enum {enum_name} case property ${property} is {:?}, not {expected:?}",
                    declared.codegen_repr()
                ))
            }
            None => {
                return Some(format!(
                    "enum {enum_name} has no ${property} property to materialize"
                ))
            }
        }
    }
    None
}

/// Validates iterator creation against the concrete layouts implemented by WASM.
fn iter_start_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let [source] = inst.operands.as_slice() else {
        return Some(format!(
            "iterator start expects one source, got {} operands",
            inst.operands.len()
        ));
    };
    let Some(source) = function.value(*source) else {
        return Some("iterator source is missing from the value table".to_string());
    };
    let source_shape = match (source.ir_type, source.php_type.codegen_repr()) {
        (IrType::Heap(IrHeapKind::Array), PhpType::Array(element)) => {
            if matches!(
                element.codegen_repr(),
                PhpType::Int
                    | PhpType::Bool
                    | PhpType::False
                    | PhpType::Str
                    | PhpType::Float
                    | PhpType::Object(_)
                    | PhpType::Array(_)
                    | PhpType::AssocArray { .. }
                    | PhpType::Mixed
                    // An empty array's element type: the body never runs, so any contract does.
                    | PhpType::Void
            ) {
                None
            } else {
                Some(format!(
                    "indexed foreach element {:?} has no exact WASM load contract",
                    element.codegen_repr()
                ))
            }
        }
        (IrType::Heap(IrHeapKind::Hash), PhpType::AssocArray { value, .. }) => {
            // A hash entry is tagged storage already (tag at +40, payload at +24/+32), so a Mixed
            // value needs no widening — the read boxes the three words it finds into a cell.
            if matches!(
                value.codegen_repr(),
                PhpType::Int
                    | PhpType::Bool
                    | PhpType::False
                    | PhpType::Str
                    | PhpType::Float
                    | PhpType::Mixed
                    // An empty hash's value type: the body never runs, so any contract does.
                    | PhpType::Void
            ) {
                None
            } else {
                Some(format!(
                    "associative foreach value {:?} has no exact WASM load contract",
                    value.codegen_repr()
                ))
            }
        }
        (ir_type, php_type) => Some(format!(
            "foreach requires a concrete indexed or associative array, got {ir_type:?}/{php_type:?}"
        )),
    };
    if source_shape.is_some() {
        return source_shape;
    }
    iterator_alias_mutation_issue(function, inst, *inst.operands.first()?)
}

/// Rejects mutations that can invalidate a live iterator snapshot.
fn iterator_alias_mutation_issue(
    function: &Function,
    start: &Instruction,
    source: ValueId,
) -> Option<String> {
    let start_id = start
        .result
        .and_then(|result| function.value(result))
        .and_then(|value| match value.def {
            ValueDef::Instruction { inst, .. } => Some(inst),
            _ => None,
        })?;
    let source_slot = value_local_origin(function, source);
    for candidate in function
        .instructions
        .iter()
        .skip(start_id.as_raw() as usize + 1)
    {
        if matches!(
            candidate.op,
            Op::StoreLocal | Op::UnsetLocal | Op::StoreRefCell
        ) {
            if let (
                Some(source_slot),
                Some(Immediate::LocalSlot(candidate_slot)),
            ) = (source_slot, candidate.immediate.as_ref())
            {
                if source_slot == candidate_slot.as_raw() {
                    return Some(format!(
                        "{} may replace or release the iterated container without retaining a PHP snapshot",
                        candidate.op.name()
                    ));
                }
            }
        }
        let mutates_container = matches!(
            candidate.op,
            Op::ArraySet
                | Op::ArrayPush
                | Op::HashSet
                | Op::HashUnset
                | Op::HashAppend
                | Op::ArrayToHash
        );
        let sorts_container = matches!(
            candidate.immediate.as_ref(),
            Some(Immediate::RuntimeCall(
                RuntimeCallTarget::Function(RuntimeFnId::Usort)
                    | RuntimeCallTarget::ProfiledFunction {
                        target: RuntimeFnId::Usort,
                        ..
                    }
            ))
        );
        if !mutates_container && !sorts_container {
            continue;
        }
        let Some(target) = candidate.operands.first().copied() else {
            continue;
        };
        let same_value = target == source;
        let same_slot = source_slot.is_some() && value_local_origin(function, target) == source_slot;
        if same_value || same_slot {
            let mutation = if sorts_container {
                "usort"
            } else {
                candidate.op.name()
            };
            return Some(format!(
                "{mutation} may mutate the iterated container without PHP snapshot/COW semantics"
            ));
        }
    }
    None
}

/// Traces a value through ownership forwarders to its originating local slot.
fn value_local_origin(function: &Function, mut value: ValueId) -> Option<u32> {
    for _ in 0..=function.values.len() {
        let value_data = function.value(value)?;
        let ValueDef::Instruction { inst, .. } = value_data.def else {
            return None;
        };
        let instruction = function.instruction(inst)?;
        match instruction.op {
            Op::LoadLocal => {
                let Some(Immediate::LocalSlot(slot)) = instruction.immediate else {
                    return None;
                };
                return Some(slot.as_raw());
            }
            Op::Move | Op::Borrow | Op::Acquire => {
                value = *instruction.operands.first()?;
            }
            _ => return None,
        }
    }
    None
}

/// Rejects by-reference associative iteration before the indexed-only lowerer.
fn iter_current_value_ref_shape_issue(
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let [iterator] = inst.operands.as_slice() else {
        return Some(format!(
            "by-reference iterator value expects one iterator, got {} operands",
            inst.operands.len()
        ));
    };
    let Some(iterator) = function.value(*iterator) else {
        return Some("iterator state is missing from the value table".to_string());
    };
    let ValueDef::Instruction { inst: defining, .. } = iterator.def else {
        return Some("iterator state is not produced by IterStart".to_string());
    };
    let Some(start) = function.instruction(defining) else {
        return Some("iterator start instruction is missing".to_string());
    };
    if start.op != Op::IterStart {
        return Some(format!(
            "iterator state is produced by {}, not IterStart",
            start.op.name()
        ));
    }
    let Some(source) = start
        .operands
        .first()
        .and_then(|source| function.value(*source))
    else {
        return Some("iterator source is missing from the value table".to_string());
    };
    match source.php_type.codegen_repr() {
        PhpType::Array(element)
            if matches!(
                element.codegen_repr(),
                PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Str
            ) =>
        {
            None
        }
        PhpType::AssocArray { .. } => Some(
            "by-reference foreach over associative arrays has no addressable WASM value-cell contract"
                .to_string(),
        ),
        other => Some(format!(
            "by-reference foreach requires a concrete supported indexed array, got {other:?}"
        )),
    }
}

/// Validates null-capable indexed int/bool/string reads supported by `lower_array_get`.
///
/// `ArrayGet` additionally requires a main-bearing command module because its
/// warning path writes through WASI; `ArrayGetSilent` remains import-free.
fn array_get_shape_issue(
    module: &Module,
    function: &Function,
    block: u32,
    inst: &Instruction,
) -> Option<String> {
    if inst.op == Op::ArrayGet
        && !module
            .functions
            .iter()
            .any(|candidate| candidate.flags.is_main)
    {
        return Some(
            "warning-producing indexed read requires a main-bearing command module"
                .to_string(),
        );
    }
    let [array, index] = inst.operands.as_slice() else {
        return Some(format!(
            "expected an indexed array and integer index, got {} operands",
            inst.operands.len()
        ));
    };
    let Some(array_value) = function.value(*array) else {
        return Some("array operand is missing from the value table".to_string());
    };
    let element_type = match (array_value.ir_type, &array_value.php_type) {
        (IrType::Heap(IrHeapKind::Array), PhpType::Array(element)) => {
            element.codegen_repr()
        }
        (IrType::Heap(IrHeapKind::Array), php_type)
            if guarded_nullable_container_source(function, block, *array, php_type) =>
        {
            let Some(PhpType::Array(element)) = exact_nullable_container_member(php_type)
            else {
                return Some("guarded nullable source is not an indexed array".to_string());
            };
            element.codegen_repr()
        }
        (ir_type, php_type) => {
            return Some(format!(
                "source must be an indexed array or a proven non-null nullable array, got {ir_type:?}/{php_type:?}"
            ))
        }
    };
    let Some(index_value) = function.value(*index) else {
        return Some("index operand is missing from the value table".to_string());
    };
    if index_value.ir_type != IrType::I64
        || index_value.php_type.codegen_repr() != PhpType::Int
    {
        return Some(format!(
            "index must be int/I64, got {:?}/{:?}",
            index_value.ir_type,
            index_value.php_type.codegen_repr()
        ));
    }
    if inst.result.is_none() {
        return Some("array_get must materialize its result".to_string());
    }
    let result_php = inst.result_php_type.codegen_repr();
    let supported_result = match &element_type {
        PhpType::Int => {
            inst.result_type == IrType::TaggedScalar && result_php == PhpType::TaggedScalar
        }
        PhpType::Bool | PhpType::Str => {
            inst.result_type == IrType::Heap(IrHeapKind::Mixed) && result_php == PhpType::Mixed
        }
        _ => false,
    };
    if !supported_result {
        return Some(format!(
            "element {element_type:?} cannot lower into {:?}/{result_php:?}",
            inst.result_type
        ));
    }
    let ownership_is_supported = match &element_type {
        PhpType::Int => inst.result_ownership == Ownership::NonHeap,
        PhpType::Bool | PhpType::Str => matches!(
            inst.result_ownership,
            Ownership::Owned | Ownership::MaybeOwned
        ),
        _ => false,
    };
    if !ownership_is_supported {
        return Some(format!(
            "element {element_type:?} result has incompatible ownership {:?}",
            inst.result_ownership
        ));
    }
    None
}

/// Validates null-capable associative-array reads supported by `lower_hash_get`.
///
/// `HashGet` requires the command runtime for its warning path. Silent reads
/// remain import-free. The gate excludes dynamic Mixed keys until every PHP key
/// tag has an exact warning/fatal path, and excludes legacy result shapes that
/// cannot distinguish a missing key from an in-range PHP value.
fn hash_get_shape_issue(
    module: &Module,
    function: &Function,
    block: u32,
    inst: &Instruction,
) -> Option<String> {
    if inst.op == Op::HashGet
        && !module
            .functions
            .iter()
            .any(|candidate| candidate.flags.is_main)
    {
        return Some(
            "warning-producing associative read requires a main-bearing command module"
                .to_string(),
        );
    }
    let [hash, key] = inst.operands.as_slice() else {
        return Some(format!(
            "expected an associative array and key, got {} operands",
            inst.operands.len()
        ));
    };
    if inst.immediate.is_some() {
        return Some("hash_get must not carry an immediate".to_string());
    }
    let Some(hash_value) = function.value(*hash) else {
        return Some("hash operand is missing from the value table".to_string());
    };
    let value_type = match (hash_value.ir_type, &hash_value.php_type) {
        (IrType::Heap(IrHeapKind::Hash), PhpType::AssocArray { value, .. }) => {
            value.codegen_repr()
        }
        (IrType::Heap(IrHeapKind::Hash), php_type)
            if guarded_nullable_container_source(function, block, *hash, php_type) =>
        {
            let Some(PhpType::AssocArray { value, .. }) =
                exact_nullable_container_member(php_type)
            else {
                return Some("guarded nullable source is not an associative array".to_string());
            };
            value.codegen_repr()
        }
        (ir_type, php_type) => {
            return Some(format!(
                "source must be an associative array or a proven non-null nullable hash, got {ir_type:?}/{php_type:?}"
            ))
        }
    };
    let Some(key_value) = function.value(*key) else {
        return Some("key operand is missing from the value table".to_string());
    };
    if let Some(issue) = hash_key_diagnostic_issue(function, inst, 1) {
        return Some(issue);
    }
    let key_php = key_value.php_type.codegen_repr();
    let supported_key = matches!(
        (key_value.ir_type, &key_php),
        (IrType::I64, PhpType::Int | PhpType::Bool | PhpType::False)
            | (IrType::Str, PhpType::Str)
    );
    if !supported_key {
        return Some(format!(
            "key must be a statically normalizable int/bool/string value, got {:?}/{key_php:?}",
            key_value.ir_type
        ));
    }
    if inst.result.is_none() {
        return Some("hash_get must materialize its result".to_string());
    }
    let result_php = inst.result_php_type.codegen_repr();
    let tagged_int = value_type == PhpType::Int
        && inst.result_type == IrType::TaggedScalar
        && result_php == PhpType::TaggedScalar;
    let boxed_nullable = matches!(
        &value_type,
        PhpType::Bool
            | PhpType::False
            | PhpType::Float
            | PhpType::Str
            | PhpType::Callable
            | PhpType::Resource(_)
            | PhpType::Mixed
            | PhpType::Union(_)
    ) && inst.result_type == IrType::Heap(IrHeapKind::Mixed)
        && result_php == PhpType::Mixed;
    let exact_nullable_container = matches!(
        &value_type,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_)
    ) && inst.result_type == IrType::from_php(&value_type)
        && php_type_is_exact_nullable_container(&inst.result_php_type, &value_type);
    if !(tagged_int || boxed_nullable || exact_nullable_container) {
        return Some(format!(
            "element {value_type:?} cannot lower into {:?}/{result_php:?}",
            inst.result_type
        ));
    }
    let ownership_is_supported = if tagged_int {
        inst.result_ownership == Ownership::NonHeap
    } else {
        matches!(
            inst.result_ownership,
            Ownership::Owned | Ownership::MaybeOwned
        )
    };
    if !ownership_is_supported {
        return Some(format!(
            "element {value_type:?} result has incompatible ownership {:?}",
            inst.result_ownership
        ));
    }
    None
}

/// Returns whether PHP result metadata is exactly `container|null` for the
/// declared associative element type.
fn php_type_is_exact_nullable_container(actual: &PhpType, container: &PhpType) -> bool {
    exact_nullable_container_member(actual).is_some_and(|member| member == container)
}

/// Returns the concrete member of an exact two-member `container|null` union.
fn exact_nullable_container_member(actual: &PhpType) -> Option<&PhpType> {
    let PhpType::Union(members) = actual else {
        return None;
    };
    if members.len() != 2
        || !members
            .iter()
            .any(|member| matches!(member, PhpType::Void | PhpType::Never))
    {
        return None;
    }
    members
        .iter()
        .find(|member| {
            matches!(
                member,
                PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_)
            )
        })
}

/// Proves that a nullable container source is consumed only after the false
/// edge of `IsNull(source)` and that edge dominates the consumer block.
fn guarded_nullable_container_source(
    function: &Function,
    consumer_block: u32,
    source: ValueId,
    php_type: &PhpType,
) -> bool {
    if exact_nullable_container_member(php_type).is_none() {
        return false;
    }
    function.blocks.iter().any(|guard| {
        let Some(Terminator::CondBr {
            cond,
            then_target: _,
            else_target,
            ..
        }) = guard.terminator.as_ref()
        else {
            return false;
        };
        if !condition_is_null_probe_of(function, *cond, source) {
            return false;
        }
        let predecessors = block_predecessors(function, *else_target);
        predecessors.len() == 1
            && predecessors[0] == guard.id
            && block_dominates(function, *else_target, BlockId::from_raw(consumer_block))
    })
}

/// Returns whether a condition value is defined by `IsNull(source)`.
fn condition_is_null_probe_of(function: &Function, condition: ValueId, source: ValueId) -> bool {
    let Some(value) = function.value(condition) else {
        return false;
    };
    let ValueDef::Instruction { inst, .. } = value.def else {
        return false;
    };
    let Some(instruction) = function.instruction(inst) else {
        return false;
    };
    instruction.op == Op::IsNull && instruction.operands.as_slice() == [source]
}

/// Lists all CFG predecessors of one block in deterministic function order.
fn block_predecessors(function: &Function, target: BlockId) -> Vec<BlockId> {
    function
        .blocks
        .iter()
        .filter(|block| terminator_successors(block.terminator.as_ref()).contains(&target))
        .map(|block| block.id)
        .collect()
}

/// Returns the successor blocks named by a terminator.
fn terminator_successors(terminator: Option<&Terminator>) -> Vec<BlockId> {
    match terminator {
        Some(Terminator::Br { target, .. }) => vec![*target],
        Some(Terminator::CondBr {
            then_target,
            else_target,
            ..
        }) => vec![*then_target, *else_target],
        Some(Terminator::Switch { cases, default, .. }) => {
            let mut successors = cases.iter().map(|case| case.target).collect::<Vec<_>>();
            successors.push(*default);
            successors
        }
        Some(Terminator::GeneratorSuspend { resume, .. }) => vec![*resume],
        Some(
            Terminator::Return { .. }
            | Terminator::Throw { .. }
            | Terminator::Fatal { .. }
            | Terminator::Unreachable,
        )
        | None => Vec::new(),
    }
}

/// Returns whether every entry-to-target path crosses `dominator`.
fn block_dominates(function: &Function, dominator: BlockId, target: BlockId) -> bool {
    if dominator == target {
        return true;
    }
    let mut visited = HashSet::new();
    let mut pending = vec![function.entry];
    while let Some(block_id) = pending.pop() {
        if block_id == dominator || !visited.insert(block_id.as_raw()) {
            continue;
        }
        if block_id == target {
            return false;
        }
        let Some(block) = function.block(block_id) else {
            continue;
        };
        pending.extend(terminator_successors(block.terminator.as_ref()));
    }
    reachable_block_ids(function).contains(&target.as_raw())
}

/// Rejects associative keys whose PHP diagnostics are not implemented exactly.
///
/// Dynamic Mixed keys need per-tag coercion and fatal behavior. Float keys need
/// profile-specific precision-loss deprecations and out-of-range warnings.
/// This guard applies equally to reads, writes, and `unset`: silent reads
/// suppress only undefined-key warnings, not key-conversion diagnostics.
fn hash_key_diagnostic_issue(
    function: &Function,
    inst: &Instruction,
    key_index: usize,
) -> Option<String> {
    let Some(key) = inst.operands.get(key_index) else {
        return None;
    };
    let Some(key_value) = function.value(*key) else {
        return None;
    };
    let key_php = key_value.php_type.codegen_repr();
    if key_value.ir_type == IrType::Heap(IrHeapKind::Mixed) || key_php == PhpType::Mixed {
        return Some(
            "dynamic Mixed associative keys require exact per-tag PHP diagnostics".to_string(),
        );
    }
    if key_value.ir_type == IrType::F64 && key_php == PhpType::Float {
        return Some(
            "float associative keys require exact profile-specific implicit-conversion diagnostics"
                .to_string(),
        );
    }
    None
}

/// Validates hash element storage against the tag and payload emitted by WASM.
///
/// Generic Mixed/Iterable storage preserves the source runtime tag. Concrete
/// storage must receive the exact same PHP/storage representation; otherwise
/// the current lowerer either silently casts a Mixed cell or stamps mismatched
/// raw bits with the destination tag.
fn hash_store_value_diagnostic_issue(
    function: &Function,
    inst: &Instruction,
    value_index: usize,
) -> Option<String> {
    let hash = inst.operands.first().and_then(|id| function.value(*id))?;
    let PhpType::AssocArray { value: storage, .. } = hash.php_type.codegen_repr() else {
        return None;
    };
    let source = inst
        .operands
        .get(value_index)
        .and_then(|id| function.value(*id))?;
    let source_php = source.php_type.codegen_repr();
    let storage = storage.codegen_repr();
    if matches!(storage, PhpType::Mixed | PhpType::Iterable) {
        return transfer::validate_storage_pair(source.ir_type, &source.php_type)
            .err()
            .map(|error| format!("hash write source has invalid storage: {error}"));
    }
    if storage != source_php
        || transfer::validate_storage_pair(source.ir_type, &source.php_type).is_err()
    {
        return Some(
            format!(
                "hash write value {:?}/{source_php:?} must exactly match concrete storage {storage:?}",
                source.ir_type
            ),
        );
    }
    None
}

/// Validates the consuming indexed-array to associative-hash promotion contract.
///
/// The runtime consumes an owned/maybe-owned indexed source and returns a
/// release-tracked hash. Concrete element storage may be preserved exactly or
/// widened to Mixed, as required by array spread and contextual hash promotion;
/// the empty-literal `array<never>` placeholder may adopt its contextual value type.
fn array_to_hash_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let [array] = inst.operands.as_slice() else {
        return Some(format!(
            "expected one indexed-array operand, got {}",
            inst.operands.len()
        ));
    };
    if inst.immediate.is_some() {
        return Some("promotion must not carry an immediate".to_string());
    }
    let Some(source) = function.value(*array) else {
        return Some("array operand is missing from the value table".to_string());
    };
    let source_element = match (source.ir_type, source.php_type.codegen_repr()) {
        (IrType::Heap(IrHeapKind::Array), PhpType::Array(element)) => {
            element.codegen_repr()
        }
        (ir_type, php_type) => {
            return Some(format!(
                "source must be an indexed array, got {ir_type:?}/{php_type:?}"
            ))
        }
    };
    if !matches!(source.ownership, Ownership::Owned | Ownership::MaybeOwned) {
        return Some(format!(
            "consumed source must own a releasable reference, got {:?}",
            source.ownership
        ));
    }
    // A promoted array's EXISTING entries carry integer keys, so a `Str`-keyed result is only
    // honest when the source is proven empty — which is exactly `$h = []; $h["k"] = ...`, the
    // ordinary way a string-keyed map is built. A non-empty source claiming string keys would
    // be a type the runtime contradicts.
    let result_value = match inst.result_php_type.codegen_repr() {
        PhpType::AssocArray { key, value }
            if matches!(key.codegen_repr(), PhpType::Int | PhpType::Mixed)
                || (matches!(key.codegen_repr(), PhpType::Str)
                    && source_element == PhpType::Void) =>
        {
            value.codegen_repr()
        }
        php_type => {
            return Some(format!(
                "result must be AssocArray<Int|Mixed, T> (or Str-keyed from an empty source), \
                 got {:?}/{php_type:?}",
                inst.result_type
            ))
        }
    };
    if inst.result.is_none() || inst.result_type != IrType::Heap(IrHeapKind::Hash) {
        return Some(format!(
            "result must materialize Heap(Hash), got {:?}",
            inst.result_type
        ));
    }
    if !matches!(
        inst.result_ownership,
        Ownership::Owned | Ownership::MaybeOwned
    ) {
        return Some(format!(
            "result must own a releasable hash reference, got {:?}",
            inst.result_ownership
        ));
    }
    if source_element != PhpType::Void
        && result_value != source_element
        && result_value != PhpType::Mixed
    {
        return Some(format!(
            "result value {result_value:?} must preserve {source_element:?} or widen to Mixed"
        ));
    }
    None
}

/// Validates one direct-call target, its lowered arity, storage transfers, and
/// by-reference source forms against the exact resolver used by emission.
fn direct_call_shape_issue(
    module: &Module,
    owner: &Function,
    inst: &Instruction,
    ref_cell_provenance: &RefCellProvenance,
) -> Option<String> {
    let target = match resolve_direct_call(module, inst) {
        Ok(target) => target,
        Err(error) => return Some(error.to_string()),
    };
    // A variadic parameter is ALREADY PACKED by the EIR: the call site builds the array and
    // the callee's signature carries one `array<T>` parameter, so the call is an ordinary
    // direct one. Only a variadic the emitter did NOT pack — arity or storage disagreeing
    // with the packed form — is outside the contract.
    let packed_variadic = target
        .function
        .params
        .iter()
        .filter(|param| param.variadic)
        .all(|param| matches!(param.ir_type, IrType::Heap(IrHeapKind::Array)))
        && inst.operands.len() == target.function.params.len();
    if !packed_variadic && target.function.params.iter().any(|param| param.variadic) {
        return Some(format!(
            "target {:?} has a variadic parameter outside the L1 direct-call contract",
            target.name
        ));
    }
    if target.function.params.len() != inst.operands.len() {
        return Some(format!(
            "target {:?} expects {} lowered operands, got {}",
            target.name,
            target.function.params.len(),
            inst.operands.len()
        ));
    }
    for (index, (operand, parameter)) in inst
        .operands
        .iter()
        .zip(&target.function.params)
        .enumerate()
    {
        let Some(value) = owner.value(*operand) else {
            return Some(format!("argument #{index} is missing from the value table"));
        };
        if parameter.by_ref {
            let source = classify_by_ref_source(owner, *operand);
            if let Some(issue) =
                by_ref_source_shape_issue(owner, value, parameter, source)
            {
                return Some(format!("by-reference argument #{index}: {issue}"));
            }
            match source {
                ByRefSource::NonLocal => {
                    return Some(format!(
                        "by-reference argument #{index} is not a supported local storage load"
                    ))
                }
                ByRefSource::AlreadyRefBound(slot)
                    if !ref_cell_provenance.owned.contains(&slot)
                        && !ref_cell_provenance.borrowed.contains(&slot) =>
                {
                    return Some(format!(
                        "by-reference argument #{index} reads unregistered ref-cell local#{slot}"
                    ))
                }
                ByRefSource::AlreadyRefBound(_) | ByRefSource::FreshLocal(_) => {}
            }
            if value.ir_type != parameter.ir_type
                || value.php_type.codegen_repr() != parameter.php_type.codegen_repr()
            {
                return Some(format!(
                    "by-reference argument #{index} storage {:?}/{:?} differs from parameter {:?}/{:?}",
                    value.ir_type,
                    value.php_type.codegen_repr(),
                    parameter.ir_type,
                    parameter.php_type.codegen_repr()
                ));
            }
        } else if let Some(issue) = value_transfer_shape_issue(
            value.ir_type,
            value.php_type.codegen_repr(),
            parameter.ir_type,
            parameter.php_type.codegen_repr(),
        ) {
            return Some(format!("argument #{index}: {issue}"));
        }
    }
    if let Some(result) = inst.result {
        let Some(value) = owner.value(result) else {
            return Some("call result is missing from the value table".to_string());
        };
        if let Some(issue) = call_result_shape_issue(
            target.function.return_type,
            target.function.return_php_type.codegen_repr(),
            value,
        ) {
            return Some(format!("result: {issue}"));
        }
    }
    None
}

/// Validates the destination a call's result value is bound to.
///
/// A VOID callee leaves nothing on the stack: PHP gives the call expression the value `null`,
/// and `transfer::emit_store_call_result` materializes that as the i64 null sentinel before
/// transferring it. The audit describes the source the same way — `I64`/`Void` rather than
/// `Void`/`Void` — so it classifies the transfer the emitter actually performs. Describing it as
/// a `Void` SOURCE would model a zero-component value and refuse both shapes EIR emits for a
/// void call: the `I64`/`null` result of a statement call, and the boxed Mixed result of one
/// whose value flows into a `mixed` slot.
fn call_result_shape_issue(
    return_type: IrType,
    return_php_type: PhpType,
    value: &crate::ir::Value,
) -> Option<String> {
    let (source_ir, source_php) = if return_type == IrType::Void {
        (IrType::I64, PhpType::Void)
    } else {
        (return_type, return_php_type)
    };
    value_transfer_shape_issue(
        source_ir,
        source_php,
        value.ir_type,
        value.php_type.codegen_repr(),
    )
}

/// Validates the exact local/value/parameter metadata behind one by-reference
/// direct-call argument before emission takes or promotes its cell address.
fn by_ref_source_shape_issue(
    owner: &Function,
    value: &crate::ir::Value,
    parameter: &crate::ir::FunctionParam,
    source: ByRefSource,
) -> Option<String> {
    let slot_raw = match source {
        ByRefSource::AlreadyRefBound(slot) => slot,
        ByRefSource::FreshLocal(slot) => slot.as_raw(),
        ByRefSource::NonLocal => return None,
    };
    let Some(slot) = owner
        .locals
        .iter()
        .find(|slot| slot.id.as_raw() == slot_raw)
    else {
        return Some(format!("local#{slot_raw} is missing from the local table"));
    };
    let pair_is_valid = transfer::validate_storage_pair(slot.ir_type, &slot.php_type).is_ok()
        && transfer::validate_storage_pair(value.ir_type, &value.php_type).is_ok()
        && transfer::validate_storage_pair(parameter.ir_type, &parameter.php_type).is_ok();
    let exact_metadata = slot.ir_type == value.ir_type
        && value.ir_type == parameter.ir_type
        && slot.php_type == value.php_type
        && value.php_type == parameter.php_type;
    if !pair_is_valid || !exact_metadata {
        return Some(format!(
            "local#{slot_raw} metadata {:?}/{:?}, loaded value {:?}/{:?}, and parameter {:?}/{:?} must match exactly",
            slot.ir_type,
            slot.php_type,
            value.ir_type,
            value.php_type,
            parameter.ir_type,
            parameter.php_type
        ));
    }
    None
}

/// Validates the statically resolvable object and boxed Mixed/Union method paths.
///
/// A Mixed/Union receiver can still carry a non-object runtime tag or an object
/// outside the closed candidate set. Those dynamic cases route through the PHP
/// fatal helpers; this gate proves that every closed-world candidate shares the
/// argument and result contract needed before runtime selection.
fn method_call_shape_issue(
    module: &Module,
    function: &Function,
    block: u32,
    inst: &Instruction,
) -> Option<String> {
    let Some(method_name) = data_string(module, inst) else {
        return Some("missing or invalid method-name Data immediate".to_string());
    };
    let Some((receiver, arguments)) = inst.operands.split_first() else {
        return Some("missing receiver operand".to_string());
    };
    let Some(receiver_value) = function.value(*receiver) else {
        return Some("receiver operand is missing from the value table".to_string());
    };
    let method_key = crate::names::php_symbol_key(method_name);
    let receiver_declared_php = receiver_value.php_type.clone();
    let mut receiver_php = receiver_declared_php.codegen_repr();
    if inst.op == Op::MethodCall {
        if let Some(PhpType::Object(class_name)) =
            exact_nullable_container_member(&receiver_declared_php)
        {
            if receiver_value.ir_type == IrType::Heap(IrHeapKind::Object) {
                if !guarded_nullable_container_source(
                    function,
                    block,
                    *receiver,
                    &receiver_declared_php,
                ) {
                    return Some(
                        "exact nullable object receiver lacks a dominating IsNull false-edge proof"
                            .to_string(),
                    );
                }
                receiver_php = PhpType::Object(class_name.clone());
            }
        }
    }
    match receiver_php {
        PhpType::Object(class_name) => {
            if receiver_value.ir_type != IrType::Heap(IrHeapKind::Object) {
                return Some(format!(
                    "object receiver must use Heap(Object), got {:?}",
                    receiver_value.ir_type
                ));
            }
            let Some(class_info) = module.class_infos.get(&class_name) else {
                return Some(format!("unknown receiver class {class_name}"));
            };
            let Some(signature) = class_info.methods.get(&method_key) else {
                return Some(format!("unknown method {class_name}::{method_name}"));
            };
            if let Some(issue) = method_signature_shape_issue(
                function,
                arguments,
                signature,
                method_name,
            ) {
                return Some(issue);
            }
            let dynamic = class_info.vtable_slots.contains_key(&method_key)
                && !class_info.final_methods.contains(&method_key);
            let candidates = if dynamic {
                match dynamic_method_candidates(module, &class_name, &method_key) {
                    Ok(candidates) => candidates,
                    Err(issue) => return Some(issue),
                }
            } else {
                vec![(
                    class_name.clone(),
                    class_info
                        .method_impl_classes
                        .get(&method_key)
                        .cloned()
                        .unwrap_or_else(|| class_name.clone()),
                )]
            };
            // A Throwable accessor has a signature but no EIR body on either backend; the call is
            // open-coded against the object rather than dispatched, so it is audited against the
            // slot it reads instead of against a body that will never exist.
            if let Some(intrinsic) = super::objects::throwable_intrinsic(
                module,
                &class_name,
                &method_key,
                &candidates,
            ) {
                return throwable_intrinsic_shape_issue(
                    &class_name,
                    &method_name,
                    class_info,
                    intrinsic,
                    arguments,
                    inst,
                );
            }
            for (candidate, implementation) in candidates {
                let Some(candidate_info) = module.class_infos.get(&candidate) else {
                    return Some(format!("missing candidate class {candidate}"));
                };
                let Some(candidate_signature) = candidate_info.methods.get(&method_key) else {
                    return Some(format!(
                        "missing candidate signature {candidate}::{method_name}"
                    ));
                };
                let Some(body) = find_method_function(module, &implementation, &method_key) else {
                    return Some(format!(
                        "missing method body {implementation}::{method_name} for dynamic candidate {candidate}"
                    ));
                };
                if let Some(issue) = method_body_signature_shape_issue(
                    body,
                    candidate_signature,
                    IrType::Heap(IrHeapKind::Object),
                ) {
                    return Some(issue);
                }
                if let Some(issue) = method_body_argument_shape_issue(function, inst, body) {
                    return Some(issue);
                }
                if let Some(issue) =
                    direct_method_result_shape_issue(inst, body, &candidate_signature.return_type)
                {
                    return Some(issue);
                }
            }
            None
        }
        PhpType::Mixed | PhpType::Union(_) => {
            if !matches!(
                receiver_value.ir_type,
                IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union)
            ) {
                return Some(format!(
                    "dynamic receiver must use Heap(Mixed/Union), got {:?}",
                    receiver_value.ir_type
                ));
            }
            let result_php = inst.result_php_type.codegen_repr();
            if transfer::validate_storage_pair(inst.result_type, &inst.result_php_type).is_err()
                && !(inst.result.is_none()
                    && inst.result_type == IrType::Void
                    && result_php == PhpType::Void)
            {
                return Some(format!(
                    "dynamic mixed/union method dispatch has invalid result storage {:?}/{result_php:?}",
                    inst.result_type
                ));
            }
            let boxed_result = inst.result.is_some()
                && inst.result_type == IrType::Heap(IrHeapKind::Mixed)
                && result_php == PhpType::Mixed;
            if inst.op == Op::NullsafeMethodCall && !boxed_result {
                return Some(format!(
                    "dynamic nullsafe dispatch requires a boxed Mixed result, got {:?}/{result_php:?}",
                    inst.result_type
                ));
            }

            let candidates = super::classes::mixed_method_candidates(module, &method_key);
            if candidates.is_empty() {
                return Some(format!(
                    "no closed-world candidate for dynamic mixed/union method {method_name}"
                ));
            }
            for (_, class_name, implementation) in candidates {
                let Some(class_info) = module.class_infos.get(&class_name) else {
                    return Some(format!("missing candidate class {class_name}"));
                };
                let Some(signature) = class_info.methods.get(&method_key) else {
                    return Some(format!(
                        "missing candidate signature {class_name}::{method_name}"
                    ));
                };
                let visibility = class_info
                    .method_visibilities
                    .get(&method_key)
                    .unwrap_or(&Visibility::Public);
                if visibility != &Visibility::Public {
                    return Some(format!(
                        "{class_name}::{method_name} has unsupported {visibility:?} visibility for dynamic mixed/union dispatch"
                    ));
                }
                if let Some(issue) = method_signature_shape_issue(
                    function,
                    arguments,
                    signature,
                    method_name,
                ) {
                    return Some(format!("{class_name}: {issue}"));
                }
                // The dispatch ladder selects one exact runtime class per arm, so a Throwable
                // accessor is decided per candidate: a sibling that overrides it keeps its own
                // arm and its own body.
                if let Some(intrinsic) = super::objects::throwable_intrinsic(
                    module,
                    &class_name,
                    &method_key,
                    &[(class_name.clone(), implementation.clone())],
                ) {
                    if !arguments.is_empty() {
                        return Some(format!(
                            "{class_name}::{method_name} takes no arguments, got {}",
                            arguments.len()
                        ));
                    }
                    let storage =
                        match super::objects::throwable_intrinsic_storage(class_info, intrinsic) {
                            Ok(storage) => storage,
                            Err(error) => {
                                return Some(format!("{class_name}::{method_name}: {error}"))
                            }
                        };
                    // A boxed destination is filled by `box_call_result_into_mixed`, exactly as a
                    // real body's return would be, so the question is boxability rather than an
                    // exact match.
                    if boxed_result {
                        if !mixed_method_return_is_boxable(storage.0, &storage.1) {
                            return Some(format!(
                                "{class_name}::{method_name} accessor result {:?}/{:?} cannot be boxed",
                                storage.0, storage.1
                            ));
                        }
                    } else if let Some(issue) = value_transfer_shape_issue(
                        storage.0,
                        storage.1,
                        inst.result_type,
                        inst.result_php_type.codegen_repr(),
                    ) {
                        return Some(format!("{class_name}::{method_name} result: {issue}"));
                    }
                    continue;
                }
                let Some(body) = find_method_function(module, &implementation, &method_key) else {
                    return Some(format!(
                        "missing candidate body {implementation}::{method_name}"
                    ));
                };
                if let Some(issue) = method_body_signature_shape_issue(
                    body,
                    signature,
                    IrType::Heap(IrHeapKind::Object),
                ) {
                    return Some(format!("{class_name}: {issue}"));
                }
                if let Some(issue) = method_body_argument_shape_issue(function, inst, body) {
                    return Some(format!("{class_name}: {issue}"));
                }
                if boxed_result {
                    if !mixed_method_return_is_boxable(body.return_type, &body.return_php_type) {
                        return Some(format!(
                            "{class_name}::{method_name} return {:?}/{:?} cannot be boxed",
                            body.return_type,
                            body.return_php_type.codegen_repr()
                        ));
                    }
                } else if let Some(issue) =
                    direct_method_result_shape_issue(inst, body, &signature.return_type)
                {
                    return Some(format!("{class_name}: {issue}"));
                }
            }
            None
        }
        other => Some(format!(
            "receiver must be Object, Mixed, or Union, got {other:?}"
        )),
    }
}

/// Rejects object construction whose property defaults lack a WASM layout.
fn object_new_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let Some(Immediate::Data(class_data)) = inst.immediate else {
        return Some("object construction requires a class-name Data immediate".to_string());
    };
    let Some(class_name) = module
        .data
        .class_names
        .get(class_data.as_raw() as usize)
    else {
        return Some("object construction references an unknown class name".to_string());
    };
    let Some(class_info) = module.class_infos.get(class_name) else {
        return Some(format!("object construction references unknown class {class_name}"));
    };
    for (index, default) in class_info.defaults.iter().enumerate() {
        let Some(default) = default else {
            continue;
        };
        let Some((property, property_type)) = class_info.properties.get(index) else {
            return Some(format!(
                "class {class_name} default #{index} has no property metadata"
            ));
        };
        let literal = match literal_default_value(
            &format!("property ${property}"),
            property_type,
            &default.kind,
            "ObjectNew",
        ) {
            Ok(literal) => literal,
            Err(error) => return Some(error.to_string()),
        };
        // A `= []` default stays refused. It is a small emitter change on its own — allocate a
        // fresh empty array into the slot — but it uncovers a LATENT use-after-free: with the
        // slot holding the only reference, `__rt_array_push_*` reaches `__rt_array_grow`, which
        // frees the old block, while the property slot still points at it until the following
        // `prop_set` — whose lowering releases exactly that stale pointer. Measured on
        // `foreach (range(1,40)) { $s = new T(); $s->push(1); }`: correct for 8 pushes, then a
        // dispatch failure on an object allocated over the freed block. Disabling free-list
        // reuse makes all 40 pass, which is what identifies it as a use-after-free rather than
        // an overflow. Constructor promotion reaches the same slot WITHOUT the defect (verified
        // leak-free over 30000 iterations), so it is the default's allocation order that
        // exposes it. Closing it needs the push to write its result back into the PROPERTY
        // slot, the analogue of the `value_source_slot` write-back locals already get.
        if !matches!(
            &literal,
            LiteralDefaultValue::Int(_)
                | LiteralDefaultValue::Bool(_)
                | LiteralDefaultValue::Float(_)
                | LiteralDefaultValue::Null
                | LiteralDefaultValue::BoxedNull
                | LiteralDefaultValue::BoxedInt(_)
                | LiteralDefaultValue::BoxedBool(_)
                | LiteralDefaultValue::BoxedFloat(_)
                | LiteralDefaultValue::Str(_)
                | LiteralDefaultValue::BoxedStr(_)
        ) && !matches!(&literal, LiteralDefaultValue::Array { elements, .. } if elements.is_empty())
        {
            return Some(format!(
                "property ${property} has a non-scalar default unsupported by WASM object construction"
            ));
        }
    }
    let constructor_key = crate::names::php_symbol_key("__construct");
    match class_info.methods.get(&constructor_key) {
        None if !inst.operands.is_empty() => {
            return Some(format!(
                "class {class_name} has no __construct but received {} arguments",
                inst.operands.len()
            ));
        }
        None => {}
        Some(signature) => {
            if let Some(issue) =
                method_signature_shape_issue(function, &inst.operands, signature, "__construct")
            {
                return Some(issue);
            }
            let implementation = class_info
                .method_impl_classes
                .get(&constructor_key)
                .map(String::as_str)
                .unwrap_or(class_name);
            let Some(body) = find_method_function(module, implementation, &constructor_key) else {
                // A Throwable's constructor has a signature but no EIR body; construction
                // open-codes its field writes (see `objects::emit_open_coded_throwable_constructor`).
                return throwable_constructor_shape_issue(
                    module,
                    function,
                    class_name,
                    implementation,
                    class_info,
                    inst,
                );
            };
            if let Some(issue) = method_body_signature_shape_issue(
                body,
                signature,
                IrType::Heap(IrHeapKind::Object),
            ) {
                return Some(issue);
            }
            for (index, (argument, parameter)) in inst
                .operands
                .iter()
                .zip(body.params.iter().skip(1))
                .enumerate()
            {
                let Some(argument) = function.value(*argument) else {
                    return Some(format!(
                        "constructor argument #{index} is missing from the value table"
                    ));
                };
                // Same contract as a direct call's arguments: identical storage copies, and a
                // concrete value bound to a `mixed` parameter boxes. Construction pushes its
                // arguments through `transfer::emit_push_call_argument`, so requiring exact
                // equality here would refuse `new C("s")` for `__construct(mixed $v)` — a
                // transfer the backend performs — while adding no safety the classifier lacks.
                if let Some(issue) = value_transfer_shape_issue(
                    argument.ir_type,
                    argument.php_type.codegen_repr(),
                    parameter.ir_type,
                    parameter.php_type.codegen_repr(),
                ) {
                    return Some(format!("constructor argument #{index}: {issue}"));
                }
            }
        }
    }
    None
}

/// Validates one open-coded `Throwable` accessor against the storage it actually reads.
///
/// The property-backed accessors resolve a real slot, so they are audited exactly like the
/// `PropGet` they lower to. The synthetic ones (`getFile`, `getLine`, `getTrace`,
/// `getTraceAsString`) materialize a constant, so the only question is whether the destination
/// can hold it — checked through the same transfer contract, from the storage the emitter
/// pushes rather than from the signature's declared return type.
fn throwable_intrinsic_shape_issue(
    class_name: &str,
    method_name: &str,
    class_info: &crate::types::ClassInfo,
    intrinsic: super::objects::ThrowableIntrinsic,
    arguments: &[ValueId],
    inst: &Instruction,
) -> Option<String> {
    if !arguments.is_empty() {
        return Some(format!(
            "{class_name}::{method_name} takes no arguments, got {}",
            arguments.len()
        ));
    }
    if inst.result.is_none() {
        // A discarded accessor result is dropped by arity; nothing reaches a destination.
        return None;
    }
    let (source_ir, source_php) =
        match super::objects::throwable_intrinsic_storage(class_info, intrinsic) {
            Ok(storage) => storage,
            Err(error) => return Some(format!("{class_name}::{method_name}: {error}")),
        };
    value_transfer_shape_issue(
        source_ir,
        source_php,
        inst.result_type,
        inst.result_php_type.codegen_repr(),
    )
    .map(|issue| format!("{class_name}::{method_name} result: {issue}"))
}

/// Validates a construction whose constructor has a signature but no EIR body.
///
/// That combination is legitimate for exactly one family: `Throwable`. Its constructor is part
/// of the prelude, and both backends open-code its field writes rather than calling a body — the
/// native one in `lower_builtin_throwable_new`, the WASM one in
/// `objects::emit_open_coded_throwable_constructor`. Every argument is audited against the
/// property slot it will actually be written to, so this admits no more than a real constructor
/// call would; any other class reaching here is still refused for the missing body.
fn throwable_constructor_shape_issue(
    module: &Module,
    function: &Function,
    class_name: &str,
    implementation: &str,
    class_info: &crate::types::ClassInfo,
    inst: &Instruction,
) -> Option<String> {
    if !super::objects::needs_open_coded_throwable_constructor(module, class_name, implementation)
    {
        return Some(format!(
            "constructor body {implementation}::__construct is missing"
        ));
    }
    let properties = super::objects::THROWABLE_CONSTRUCTOR_PROPERTIES;
    if inst.operands.len() > properties.len() {
        return Some(format!(
            "{class_name}::__construct received {} arguments, expected at most {}",
            inst.operands.len(),
            properties.len()
        ));
    }
    for (index, (argument, property)) in inst.operands.iter().zip(properties.iter()).enumerate() {
        let Some(argument) = function.value(*argument) else {
            return Some(format!(
                "constructor argument #{index} is missing from the value table"
            ));
        };
        let Some(slot) = class_info
            .properties
            .iter()
            .find(|(name, _)| name == property)
        else {
            return Some(format!(
                "{class_name} declares no ${property} for its inherited Throwable constructor"
            ));
        };
        if let Some(issue) = property_write_shape_issue(
            argument.ir_type,
            &argument.php_type,
            &slot.1.codegen_repr(),
        ) {
            return Some(format!("constructor argument #{index} (${property}): {issue}"));
        }
    }
    None
}

/// Validates property-read result storage against the declared slot layout.
fn property_get_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    if inst.op == Op::NullsafePropGet {
        if inst.result.is_none()
            || inst.result_type != IrType::Heap(IrHeapKind::Mixed)
            || inst.result_php_type.codegen_repr() != PhpType::Mixed
        {
            return Some(format!(
                "nullsafe property reads require a boxed Mixed result, got {:?}/{:?}",
                inst.result_type,
                inst.result_php_type.codegen_repr()
            ));
        }
    }
    let [receiver] = inst.operands.as_slice() else {
        return Some(format!(
            "property get expects one receiver, got {} operands",
            inst.operands.len()
        ));
    };
    let Some(receiver) = function.value(*receiver) else {
        return Some("property receiver is missing from the value table".to_string());
    };
    let receiver_id = *inst.operands.first()?;
    if inst.op == Op::NullsafePropGet
        && nullsafe_receiver_is_definitely_null(function, receiver_id)
    {
        return None;
    }
    let receiver_php = receiver.php_type.clone();
    let class_name = match receiver_php {
        PhpType::Object(class_name) => class_name,
        PhpType::Union(variants) if inst.op == Op::NullsafePropGet => {
            let object_classes = variants
                .into_iter()
                .filter_map(|variant| match variant {
                    PhpType::Object(class_name) => Some(class_name),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [class_name] = object_classes.as_slice() else {
                return Some(
                    "nullsafe property receiver union must contain exactly one concrete object class"
                        .to_string(),
                );
            };
            class_name.clone()
        }
        other => {
            return Some(format!(
                "property receiver must resolve to a concrete object, got {:?}/{other:?}",
                receiver.ir_type
            ));
        }
    };
    let Some(class_info) = module.class_infos.get(&class_name) else {
        return Some(format!("property receiver class {class_name} is missing"));
    };
    let Some(property) = data_string(module, inst) else {
        return Some("property get requires a valid property-name immediate".to_string());
    };
    let Some((property_index, (_, property_type))) = class_info
        .properties
        .iter()
        .enumerate()
        .find(|(_, (name, _))| name == property)
    else {
        if !class_info.allow_dynamic_properties
            && !class_name
                .trim_start_matches('\\')
                .eq_ignore_ascii_case("stdClass")
        {
            return Some(format!(
                "class {class_name} does not provide dynamic property storage for ${property}"
            ));
        }
        if dynamic_property_initialized_before_read(
            function,
            inst,
            receiver_id,
            inst.immediate.as_ref(),
        ) {
            if inst.result.is_some()
                && inst.result_type == IrType::Heap(IrHeapKind::Mixed)
                && inst.result_php_type.codegen_repr() == PhpType::Mixed
                && matches!(
                    inst.result_ownership,
                    Ownership::Owned | Ownership::MaybeOwned
                )
            {
                return None;
            }
            return Some(format!(
                "dynamic property ${property} reads require an owned boxed Mixed result"
            ));
        }
        return Some(format!(
            "dynamic property ${property} reads require the exact PHP undefined-property warning"
        ));
    };
    // A declared typed property with no default can be read before it is ever assigned, which PHP
    // answers with `Error: Typed property C::$p must not be accessed before initialization`. The
    // object allocator zeroes the slot, and zero is a legitimate value for an int or a bool, so
    // there is no sentinel to test — implementing that check needs an initialization bitmap.
    //
    // A CONSTRUCTOR-PROMOTED property is the exception: the promotion assigns it from the
    // constructor's signature, before the constructor body runs, so no read can precede it.
    if class_info
        .property_declared_slots
        .get(property_index)
        .copied()
        .unwrap_or(false)
        && class_info
            .defaults
            .get(property_index)
            .map(|default| default.is_none())
            .unwrap_or(true)
        && !class_info.promoted_properties.contains(property)
        // An ENUM CASE is the other exception: `name` and `value` are written by the
        // materializer, and `scoped_constant_get` is the ONLY way to obtain a case object,
        // so no read can precede those writes.
        && !module.enum_infos.contains_key(&class_name)
    {
        return Some(format!(
            "typed property ${property} may be uninitialized and requires an exact PHP fatal check"
        ));
    }
    let property_type = property_type.codegen_repr();
    if property_type == PhpType::Iterable {
        return Some(
            "iterable property reads require runtime tag unboxing before use".to_string(),
        );
    }
    if inst.op == Op::NullsafePropGet {
        return None;
    }
    let result_php = inst.result_php_type.codegen_repr();
    if result_php != property_type
        || transfer::validate_storage_pair(inst.result_type, &inst.result_php_type).is_err()
    {
        return Some(format!(
            "property ${property} result {:?}/{result_php:?} must exactly match declared slot {property_type:?}",
            inst.result_type
        ));
    }
    None
}

/// Returns whether a nullable receiver is statically a boxed null value.
fn nullsafe_receiver_is_definitely_null(function: &Function, mut receiver: ValueId) -> bool {
    for _ in 0..=function.values.len() {
        let Some(value) = function.value(receiver) else {
            return false;
        };
        let ValueDef::Instruction { inst, .. } = value.def else {
            return false;
        };
        let Some(instruction) = function.instruction(inst) else {
            return false;
        };
        match instruction.op {
            Op::ConstNull => return true,
            Op::Move | Op::Borrow | Op::Acquire | Op::MixedBox => {
                let Some(source) = instruction.operands.first() else {
                    return false;
                };
                receiver = *source;
            }
            _ => return false,
        }
    }
    false
}

/// Proves an undeclared dynamic property was written earlier in the same block.
fn dynamic_property_initialized_before_read(
    function: &Function,
    read: &Instruction,
    receiver: ValueId,
    property: Option<&Immediate>,
) -> bool {
    let Some(read_id) = read
        .result
        .and_then(|result| function.value(result))
        .and_then(|value| match value.def {
            ValueDef::Instruction { inst, .. } => Some(inst),
            _ => None,
        })
    else {
        return false;
    };
    let Some(Immediate::Data(property)) = property else {
        return false;
    };
    let receiver_root = property_receiver_origin(function, receiver);
    let receiver_slot = value_local_origin(function, receiver_root);
    let Some((block, read_index)) = function.blocks.iter().find_map(|block| {
        block
            .instructions
            .iter()
            .position(|candidate| *candidate == read_id)
            .map(|index| (block, index))
    }) else {
        return false;
    };
    let mut initialized = false;
    for candidate_id in block.instructions.iter().take(read_index) {
        let Some(candidate) = function.instruction(*candidate_id) else {
            return false;
        };
        if initialized
            && matches!(candidate.op, Op::StoreLocal | Op::UnsetLocal)
            && matches!(
                (receiver_slot, candidate.immediate.as_ref()),
                (Some(slot), Some(Immediate::LocalSlot(candidate_slot)))
                    if slot == candidate_slot.as_raw()
            )
        {
            initialized = false;
        }
        if candidate.op != Op::PropSet
            || !matches!(candidate.immediate, Some(Immediate::Data(candidate_property)) if candidate_property == *property)
        {
            continue;
        }
        let Some(candidate_receiver) = candidate.operands.first().copied() else {
            continue;
        };
        let candidate_root = property_receiver_origin(function, candidate_receiver);
        initialized = candidate_root == receiver_root
            || (receiver_slot.is_some()
                && value_local_origin(function, candidate_root) == receiver_slot);
    }
    initialized
}

/// Traces nullable boxing and ownership forwarders to the underlying object receiver.
fn property_receiver_origin(function: &Function, mut receiver: ValueId) -> ValueId {
    for _ in 0..=function.values.len() {
        let Some(value) = function.value(receiver) else {
            return receiver;
        };
        let ValueDef::Instruction { inst, .. } = value.def else {
            return receiver;
        };
        let Some(instruction) = function.instruction(inst) else {
            return receiver;
        };
        if !matches!(
            instruction.op,
            Op::Move | Op::Borrow | Op::Acquire | Op::MixedBox
        ) {
            return receiver;
        }
        let Some(source) = instruction.operands.first() else {
            return receiver;
        };
        receiver = *source;
    }
    receiver
}

/// Validates a declared property write against its exact runtime slot layout.
///
/// Mixed properties use the audited boxing path. Concrete properties currently
/// store the operand representation directly, so any implicit type conversion
/// must fail closed before WAT generation.
fn property_set_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let [receiver, source] = inst.operands.as_slice() else {
        return Some(format!(
            "property set expects receiver and value, got {} operands",
            inst.operands.len()
        ));
    };
    let Some(receiver) = function.value(*receiver) else {
        return Some("property receiver is missing from the value table".to_string());
    };
    let PhpType::Object(class_name) = receiver.php_type.codegen_repr() else {
        return Some(format!(
            "property receiver must be a concrete object, got {:?}/{:?}",
            receiver.ir_type,
            receiver.php_type.codegen_repr()
        ));
    };
    let Some(class_info) = module.class_infos.get(&class_name) else {
        return Some(format!("property receiver class {class_name} is missing"));
    };
    let Some(property) = data_string(module, inst) else {
        return Some("property set requires a valid property-name immediate".to_string());
    };
    let source_id = *source;
    let Some(source) = function.value(source_id) else {
        return Some("property value is missing from the value table".to_string());
    };
    if source.ir_type == IrType::Heap(IrHeapKind::Mixed)
        && matches!(
            source.php_type.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable
        )
        && !matches!(
            source.ownership,
            Ownership::Owned | Ownership::Borrowed | Ownership::Persistent
        )
        && !(source.ownership == Ownership::MaybeOwned
            && value_local_origin(function, source_id).is_some())
    {
        return Some(format!(
            "mixed property writes require owned, borrowed, persistent, or local-load provenance, got {:?}",
            source.ownership
        ));
    }
    let Some((_, property_type)) = class_info
        .properties
        .iter()
        .find(|(name, _)| name == property)
    else {
        return None;
    };
    let property_type = property_type.codegen_repr();
    property_write_shape_issue(
        source.ir_type,
        &source.php_type,
        &property_type,
    )
    .map(|issue| format!("property ${property}: {issue}"))
}

/// Validates one value against the DECLARED property slot it will be written into.
///
/// A Mixed/Union/Iterable slot holds a Mixed cell, so the write is the ordinary boxing transfer.
/// A concrete slot is stored raw, so it demands an exact storage match — a narrower rule than the
/// transfer contract, and deliberately so: there is no conversion step on that path.
/// Shared by `PropSet` and by the inherited Throwable constructor that construction open-codes,
/// so both are audited by the rule the emitter actually implements.
fn property_write_shape_issue(
    source_ir: IrType,
    source_php: &PhpType,
    property_type: &PhpType,
) -> Option<String> {
    if matches!(
        property_type,
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable
    ) {
        return value_transfer_shape_issue(
            source_ir,
            source_php.codegen_repr(),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        )
        .map(|issue| format!("mixed slot: {issue}"));
    }
    // A Mixed source narrows into a concrete SCALAR slot, the same coercion a local load
    // performs — `$this->n = $this->n + 1` widens through the checked add and the slot stays an
    // int. Only scalars: a container slot has no narrowing to perform.
    if source_ir == IrType::Heap(IrHeapKind::Mixed)
        && source_php.codegen_repr() == PhpType::Mixed
        && matches!(
            property_type,
            PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Float | PhpType::Str
        )
    {
        return None;
    }
    // A literal `[]` is `array<never>`: no element, and no slot layout decided until the first
    // push fixes it. Its pointer is therefore interchangeable with any element type's, which is
    // what `public array $items = [];` assigns into an `array<mixed>` slot.
    let source_repr = source_php.codegen_repr();
    if source_ir == IrType::Heap(IrHeapKind::Array)
        && matches!(property_type, PhpType::Array(_))
        && matches!(
            &source_repr,
            PhpType::Array(element) if matches!(element.codegen_repr(), PhpType::Void)
        )
    {
        return None;
    }
    if &source_repr != property_type
        || transfer::validate_storage_pair(source_ir, source_php).is_err()
    {
        return Some(format!(
            "value {source_ir:?}/{source_repr:?} must exactly match concrete slot {property_type:?}"
        ));
    }
    None
}

/// Returns whether `class_name` belongs to the class subtree rooted at `ancestor`.
fn class_descends_from(module: &Module, class_name: &str, ancestor: &str) -> bool {
    let mut current = Some(class_name);
    let mut visited = HashSet::new();
    while let Some(name) = current {
        if name == ancestor {
            return true;
        }
        if !visited.insert(name.to_string()) {
            return false;
        }
        current = module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Enumerates every concrete implementation required by one generated virtual stub.
pub(super) fn dynamic_method_candidates(
    module: &Module,
    receiver_class: &str,
    method_key: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut introducer = receiver_class.to_string();
    let mut visited = HashSet::new();
    while visited.insert(introducer.clone()) {
        let Some(parent) = module
            .class_infos
            .get(&introducer)
            .and_then(|class_info| class_info.parent.as_ref())
        else {
            break;
        };
        let Some(parent_info) = module.class_infos.get(parent) else {
            return Err(format!("missing parent class {parent}"));
        };
        if !parent_info.vtable_slots.contains_key(method_key) {
            break;
        }
        introducer = parent.clone();
    }

    let mut candidates = module
        .class_infos
        .iter()
        .filter(|(class_name, class_info)| {
            !class_info.is_abstract
                && class_info.vtable_slots.contains_key(method_key)
                && class_descends_from(module, class_name, &introducer)
        })
        .map(|(class_name, class_info)| {
            (
                class_info.class_id,
                class_name.clone(),
                class_info
                    .method_impl_classes
                    .get(method_key)
                    .cloned()
                    .unwrap_or_else(|| class_name.clone()),
            )
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
    });
    if candidates.is_empty() {
        return Err(format!(
            "virtual method {receiver_class}::{method_key} has no concrete candidate"
        ));
    }
    Ok(candidates
        .into_iter()
        .map(|(_, class_name, implementation)| (class_name, implementation))
        .collect())
}

/// Validates true-static and lexical `self::`/`parent::` method calls against
/// the exact hidden-parameter ABI emitted by `methods::lower_static_method_call`.
fn static_method_call_shape_issue(
    module: &Module,
    owner: &Function,
    inst: &Instruction,
) -> Option<String> {
    let Some(target) = data_string(module, inst) else {
        return Some("missing or invalid static-target Data immediate".to_string());
    };
    let Some((receiver_label, method_name)) = target.rsplit_once("::") else {
        return Some(format!("malformed static target {target:?}"));
    };
    if receiver_label == "static" {
        return Some(format!(
            "static::{method_name} late-bound dispatch is outside the L1 call contract"
        ));
    }
    let current_class = owner
        .name
        .rsplit_once("::")
        .map(|(class_name, _)| class_name.to_string());
    let receiver_class = match receiver_label {
        "self" => match current_class.clone() {
            Some(class_name) => class_name,
            None => return Some("self:: outside a method".to_string()),
        },
        "parent" => {
            let Some(class_name) = current_class.as_ref() else {
                return Some("parent:: outside a method".to_string());
            };
            let Some(parent) = module
                .class_infos
                .get(class_name)
                .and_then(|class| class.parent.clone())
            else {
                return Some(format!("class {class_name} has no parent"));
            };
            parent
        }
        named => named.to_string(),
    };
    let Some(class_info) = module.class_infos.get(&receiver_class) else {
        return Some(format!("unknown class {receiver_class}"));
    };
    let method_key = crate::names::php_symbol_key(method_name);
    let true_static = class_info.static_methods.contains_key(&method_key);
    let lexical_instance = !true_static
        && matches!(receiver_label, "self" | "parent")
        && owner.flags.is_method
        && !owner.flags.is_static
        && class_info.methods.contains_key(&method_key);
    let (signature, implementation, hidden_ir) = if true_static {
        let Some(signature) = class_info.static_methods.get(&method_key) else {
            return Some(format!(
                "missing static signature {receiver_class}::{method_name}"
            ));
        };
        let implementation = class_info
            .static_method_impl_classes
            .get(&method_key)
            .map(String::as_str)
            .unwrap_or(receiver_class.as_str());
        (signature, implementation, IrType::I64)
    } else if lexical_instance {
        let Some(signature) = class_info.methods.get(&method_key) else {
            return Some(format!(
                "missing instance signature {receiver_class}::{method_name}"
            ));
        };
        let implementation = class_info
            .method_impl_classes
            .get(&method_key)
            .map(String::as_str)
            .unwrap_or(receiver_class.as_str());
        (signature, implementation, IrType::Heap(IrHeapKind::Object))
    } else {
        return Some(format!(
            "target {target:?} is neither a true static method nor an applicable lexical instance method"
        ));
    };
    if let Some(issue) =
        method_signature_shape_issue(owner, &inst.operands, signature, method_name)
    {
        return Some(issue);
    }
    let Some(body) = find_method_function(module, implementation, &method_key) else {
        return Some(format!(
            "missing method body {implementation}::{method_name}"
        ));
    };
    if let Some(issue) = method_body_signature_shape_issue(body, signature, hidden_ir) {
        return Some(issue);
    }
    for (index, (operand, parameter)) in
        inst.operands.iter().zip(body.params.iter().skip(1)).enumerate()
    {
        let Some(value) = owner.value(*operand) else {
            return Some(format!("argument #{index} is missing from the value table"));
        };
        if value.ir_type != parameter.ir_type
            || value.php_type.codegen_repr() != parameter.php_type.codegen_repr()
        {
            return Some(format!(
                "argument #{index} storage {:?}/{:?} differs from {} parameter {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr(),
                body.name,
                parameter.ir_type,
                parameter.php_type.codegen_repr()
            ));
        }
    }
    direct_method_result_shape_issue(inst, body, &signature.return_type)
}

/// Validates user-argument arity and by-reference state against the checker-owned signature.
///
/// Argument STORAGE is deliberately not checked here. The signature carries PHP types but no
/// EIR types, so the strongest thing this function could assert is PHP-type equality — which is
/// both weaker and stricter than the real contract: weaker because it cannot see the `IrType`
/// the callee actually receives, and stricter because a concrete argument bound to a `mixed`
/// parameter is a legal boxing transfer, not a mismatch. Every caller follows this with a body
/// check (`method_body_argument_shape_issue`, or the constructor loop in
/// `object_new_shape_issue`) that has both types and applies the transfer contract, so the
/// argument audit lives there in full rather than half here.
fn method_signature_shape_issue(
    owner: &Function,
    arguments: &[ValueId],
    signature: &crate::types::FunctionSig,
    method_name: &str,
) -> Option<String> {
    if signature.ref_params.iter().any(|by_ref| *by_ref) {
        return Some(format!("{method_name} has a by-reference parameter"));
    }
    // A variadic the EIR already packed arrives as one ordinary `array<T>` argument, so the
    // arity check below is what decides: the packed form has exactly `params.len()` operands.
    // A variadic the emitter did NOT pack shows up as an arity mismatch and is refused there.
    if signature.params.len() != arguments.len() {
        return Some(format!(
            "{method_name} expects {} arguments, got {}",
            signature.params.len(),
            arguments.len()
        ));
    }
    for (index, argument) in arguments.iter().enumerate() {
        if owner.value(*argument).is_none() {
            return Some(format!("argument #{index} is missing from the value table"));
        }
    }
    None
}

/// Validates one concrete method body's hidden ABI parameter, user parameter
/// metadata, and return metadata against the checker-owned signature.
fn method_body_signature_shape_issue(
    body: &Function,
    signature: &crate::types::FunctionSig,
    hidden_ir: IrType,
) -> Option<String> {
    if body.params.len() != signature.params.len() + 1 {
        return Some(format!(
            "method body {} expects {} total parameters including its hidden receiver, signature requires {}",
            body.name,
            body.params.len(),
            signature.params.len() + 1
        ));
    }
    let Some(hidden) = body.params.first() else {
        return Some(format!("method body {} has no hidden parameter", body.name));
    };
    let hidden_php = hidden.php_type.codegen_repr();
    let hidden_php_matches = match hidden_ir {
        IrType::I64 => hidden_php == PhpType::Int,
        IrType::Heap(IrHeapKind::Object) => matches!(hidden_php, PhpType::Object(_)),
        _ => false,
    };
    if hidden.ir_type != hidden_ir
        || !hidden_php_matches
        || transfer::validate_storage_pair(hidden.ir_type, &hidden.php_type).is_err()
    {
        return Some(format!(
            "method body {} hidden parameter must be {:?} with matching PHP metadata, got {:?}/{hidden_php:?}",
            body.name, hidden_ir, hidden.ir_type
        ));
    }
    for (index, (parameter, (_, declared_php))) in body
        .params
        .iter()
        .skip(1)
        .zip(&signature.params)
        .enumerate()
    {
        let parameter_php = parameter.php_type.codegen_repr();
        let declared_php = declared_php.codegen_repr();
        if parameter_php != declared_php
            || transfer::validate_storage_pair(parameter.ir_type, &parameter.php_type).is_err()
        {
            return Some(format!(
                "method body {} parameter #{index} {:?}/{parameter_php:?} differs from signature {declared_php:?}",
                body.name, parameter.ir_type
            ));
        }
    }
    let body_return_php = body.return_php_type.codegen_repr();
    let declared_return_php = signature.return_type.codegen_repr();
    if body_return_php != declared_return_php
        || transfer::validate_storage_pair(body.return_type, &body.return_php_type).is_err()
    {
        return Some(format!(
            "method body {} return {:?}/{body_return_php:?} differs from signature {declared_return_php:?}",
            body.name, body.return_type
        ));
    }
    None
}

/// Validates argument storage against the concrete method body's WASM signature.
fn method_body_argument_shape_issue(
    owner: &Function,
    inst: &Instruction,
    body: &Function,
) -> Option<String> {
    if body.params.len() != inst.operands.len() {
        return Some(format!(
            "method body {} expects {} total operands, got {}",
            body.name,
            body.params.len(),
            inst.operands.len()
        ));
    }
    if body
        .params
        .first()
        .is_none_or(|parameter| parameter.ir_type != IrType::Heap(IrHeapKind::Object))
    {
        return Some(format!(
            "method body {} has no Heap(Object) receiver parameter",
            body.name
        ));
    }
    for (index, (operand, parameter)) in inst
        .operands
        .iter()
        .skip(1)
        .zip(body.params.iter().skip(1))
        .enumerate()
    {
        let Some(value) = owner.value(*operand) else {
            return Some(format!(
                "operand #{} is missing from the value table",
                index + 1
            ));
        };
        if value.ir_type != parameter.ir_type
            || value.php_type.codegen_repr() != parameter.php_type.codegen_repr()
        {
            return Some(format!(
                "operand #{} storage {:?}/{:?} differs from {} parameter {:?}/{:?}",
                index + 1,
                value.ir_type,
                value.php_type.codegen_repr(),
                body.name,
                parameter.ir_type,
                parameter.php_type.codegen_repr()
            ));
        }
    }
    None
}

/// Validates the direct object-call result against the concrete method body.
fn direct_method_result_shape_issue(
    inst: &Instruction,
    body: &Function,
    declared_php: &PhpType,
) -> Option<String> {
    let declared_php = declared_php.codegen_repr();
    let body_php = body.return_php_type.codegen_repr();
    if body_php != declared_php
        || transfer::validate_storage_pair(body.return_type, &body.return_php_type).is_err()
    {
        return Some(format!(
            "method body {} return {:?}/{body_php:?} differs from declared {declared_php:?}",
            body.name, body.return_type
        ));
    }
    let has_exact_result = match body.return_type {
        // A `void` body returns nothing, but PHP still gives its CALL EXPRESSION the value
        // null — so the EIR materializes an `I64 php=null` result whenever it is used. Both
        // shapes are exact; the emitter supplies the null the callee did not push.
        IrType::Void => {
            (inst.result.is_none()
                && inst.result_type == IrType::Void
                && inst.result_php_type.codegen_repr() == PhpType::Void)
                || (inst.result_type == IrType::I64
                    && inst.result_php_type.codegen_repr() == PhpType::Void)
        }
        return_type => {
            inst.result.is_some()
                && inst.result_type == return_type
                && inst.result_php_type.codegen_repr() == declared_php
        }
    };
    if !has_exact_result {
        return Some(format!(
            "result {:?}/{:?} differs from method body {:?}/{declared_php:?}",
            inst.result_type,
            inst.result_php_type.codegen_repr(),
            body.return_type
        ));
    }
    None
}

/// Finds a concrete class-method body using the lowerer's name-matching rules.
fn find_method_function<'a>(
    module: &'a Module,
    implementation: &str,
    method_key: &str,
) -> Option<&'a Function> {
    module
        .class_methods
        .iter()
        .find(|function| match function.name.rsplit_once("::") {
            Some((class_name, method_name)) => {
                class_name == implementation
                    && crate::names::php_symbol_key(method_name) == method_key
            }
            None => false,
        })
}

/// Returns whether dynamic dispatch can box the concrete method return.
fn mixed_method_return_is_boxable(ir_type: IrType, php_type: &PhpType) -> bool {
    transfer::classify_transfer(
        ir_type,
        php_type.codegen_repr(),
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    )
    .is_ok()
}

/// Records a storage type the current wasm32 ABI cannot materialize safely.
fn check_type(ir_type: IrType, context: &str, issues: &mut Vec<String>) {
    let supported = match ir_type {
        IrType::I64
        | IrType::F64
        | IrType::Str
        | IrType::TaggedScalar
        | IrType::Heap(IrHeapKind::Array)
        | IrType::Heap(IrHeapKind::Hash)
        | IrType::Heap(IrHeapKind::Object)
        | IrType::Heap(IrHeapKind::Mixed)
        | IrType::Heap(IrHeapKind::Iterable)
        | IrType::Heap(IrHeapKind::Union)
        | IrType::Void => true,
        IrType::Heap(IrHeapKind::Buffer) => false,
    };
    if !supported {
        issues.push(format!("{context}: unsupported storage type {ir_type:?}"));
    }
}

/// Validates the typed target carried by one `RuntimeCall`.
fn check_runtime_call(
    module: &Module,
    collection: &str,
    function: &Function,
    block: u32,
    instruction: u32,
    call: &Instruction,
    issues: &mut Vec<String>,
) {
    let context = format!(
        "{collection}::{} block#{block} instruction#{instruction}",
        function.name
    );
    match call.immediate.clone() {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
            | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => {
            if !runtime_function_is_supported(target) {
                issues.push(format!(
                    "{context}: unsupported runtime function {}",
                    target.as_eir()
                ));
                return;
            }
            if let Some(issue) = runtime_function_shape_issue(module, function, call, target) {
                issues.push(format!(
                    "{context}: unsupported runtime function {} shape: {issue}",
                    target.as_eir()
                ));
            }
        }
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWrite)) => {
            issues.push(format!(
                "{context}: unsupported runtime target array.fetch_for_write"
            ));
        }
        Some(Immediate::RuntimeCall(RuntimeCallTarget::UnaryString(target))) => {
            if !super::builtins::unary_string_is_supported(target) {
                issues.push(format!(
                    "{context}: unsupported unary string runtime {}",
                    unary_string_name(target)
                ));
                return;
            }
            if let Some(issue) = super::builtins::unary_string_shape_issue(function, call, target) {
                issues.push(format!(
                    "{context}: unsupported unary string runtime {} shape: {issue}",
                    unary_string_name(target)
                ));
            }
        }
        _ => issues.push(format!("{context}: missing typed runtime target")),
    }
}

/// Returns a diagnostic when an admitted runtime function does not match the
/// exact operand, callback, and result subset implemented by its lowerer.
fn runtime_function_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    if super::builtins::is_direct_builtin(target) {
        return super::builtins::direct_builtin_shape_issue(module, function, call, target);
    }
    match target {
        RuntimeFnId::GetClass => get_class_shape_issue(function, call),
        RuntimeFnId::ArrayMap => array_map_shape_issue(module, function, call),
        RuntimeFnId::Usort => usort_shape_issue(module, function, call),
        RuntimeFnId::ArrayReduce => array_reduce_shape_issue(module, function, call),
        _ => Some("the runtime function has no audited WASM shape contract".to_string()),
    }
}

/// Validates the exact supported `get_class(object): string` form.
fn get_class_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one object operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("object operand is missing from the value table".to_string());
    };
    if value.ir_type != IrType::Heap(IrHeapKind::Object)
        || !matches!(value.php_type.codegen_repr(), PhpType::Object(_))
    {
        return Some(format!(
            "expected a statically object-typed Heap(Object) operand, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "expected a string result, got {:?}/{:?}",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates the exact single-array `array_map` subset and its static callback.
fn array_map_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
) -> Option<String> {
    let [callback, array] = call.operands.as_slice() else {
        return Some(format!(
            "expected (callable, indexed array), got {} operands",
            call.operands.len()
        ));
    };
    let Some(callback_value) = function.value(*callback) else {
        return Some("callback operand is missing from the value table".to_string());
    };
    if callback_value.php_type.codegen_repr() != PhpType::Callable {
        return Some(format!(
            "callback must be Callable, got {:?}",
            callback_value.php_type.codegen_repr()
        ));
    }
    let Some(array_value) = function.value(*array) else {
        return Some("array operand is missing from the value table".to_string());
    };
    let element_type = match (
        array_value.ir_type,
        array_value.php_type.codegen_repr(),
    ) {
        (IrType::Heap(IrHeapKind::Array), PhpType::Array(element)) => {
            element.codegen_repr()
        }
        (ir_type, php_type) => {
            return Some(format!(
                "source must be an indexed array, got {ir_type:?}/{php_type:?}"
            ))
        }
    };
    if !matches!(element_type, PhpType::Int | PhpType::Str) {
        return Some(format!(
            "source element type {element_type:?} is not represented exactly by the map runtime"
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::Heap(IrHeapKind::Array)
        || !matches!(
            call.result_php_type.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. }
        )
    {
        return Some(format!(
            "array_map must materialize an indexed-array result, got {:?}/{:?}",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    let Some((callback_function, visible_count)) =
        static_callable_contract(module, function, *callback)
    else {
        return Some(
            "callback must resolve statically to a direct closure or user-function callable"
                .to_string(),
        );
    };
    if let Some(issue) = callable_wrapper_issue(callback_function, visible_count, 1) {
        return Some(issue);
    }
    if let Some(param) = callback_function.params.first() {
        let param_type = param.php_type.codegen_repr();
        let compatible = match element_type {
            PhpType::Int => param_type == PhpType::Int,
            PhpType::Str => param_type == PhpType::Str,
            _ => false,
        };
        if !compatible {
            return Some(format!(
                "callback parameter {:?} cannot receive source element {:?} without PHP coercion",
                param_type, element_type
            ));
        }
    }
    if !callable_return_is_boxable(callback_function) {
        return Some(format!(
            "callback return {:?}/{:?} cannot be boxed by the WASM callable wrapper",
            callback_function.return_type,
            callback_function.return_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates the exact `usort(array<int>, callable): bool` subset.
fn usort_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
) -> Option<String> {
    let [array, callback] = call.operands.as_slice() else {
        return Some(format!(
            "expected (array<int>, callable), got {} operands",
            call.operands.len()
        ));
    };
    if !value_is_int_array(function, *array) {
        return Some("source must be an indexed array<int>".to_string());
    }
    if function
        .value(*callback)
        .is_none_or(|value| value.php_type.codegen_repr() != PhpType::Callable)
    {
        return Some("comparator must be Callable".to_string());
    }
    if call.result.is_some()
        && (call.result_type != IrType::I64
            || call.result_php_type.codegen_repr() != PhpType::Bool)
    {
        return Some(format!(
            "used usort result must be bool/I64, got {:?}/{:?}",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    let Some((callback_function, visible_count)) =
        static_callable_contract(module, function, *callback)
    else {
        return Some(
            "comparator must resolve statically to a direct closure or user-function callable"
                .to_string(),
        );
    };
    if let Some(issue) = callable_wrapper_issue(callback_function, visible_count, 2) {
        return Some(issue);
    }
    if callback_function.params[..visible_count]
        .iter()
        .any(|param| param.php_type.codegen_repr() != PhpType::Int)
    {
        return Some("comparator parameters must be int".to_string());
    }
    if callback_function.return_type != IrType::I64
        || callback_function.return_php_type.codegen_repr() != PhpType::Int
    {
        return Some(format!(
            "comparator must return int/I64, got {:?}/{:?}",
            callback_function.return_type,
            callback_function.return_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates the exact integer-carry `array_reduce` subset.
fn array_reduce_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
) -> Option<String> {
    let [array, callback, initial] = call.operands.as_slice() else {
        return Some(format!(
            "expected (array<int>, callable, int), got {} operands",
            call.operands.len()
        ));
    };
    if !value_is_int_array(function, *array) {
        return Some("source must be an indexed array<int>".to_string());
    }
    if function
        .value(*callback)
        .is_none_or(|value| value.php_type.codegen_repr() != PhpType::Callable)
    {
        return Some("callback must be Callable".to_string());
    }
    if function.value(*initial).is_none_or(|value| {
        value.ir_type != IrType::I64 || value.php_type.codegen_repr() != PhpType::Int
    }) {
        return Some("initial carry must be an int/I64 value".to_string());
    }
    if call.result.is_some() {
        let result_is_supported = match call.result_type {
            IrType::I64 => call.result_php_type.codegen_repr() == PhpType::Int,
            IrType::Heap(IrHeapKind::Mixed) => {
                call.result_php_type.codegen_repr() == PhpType::Mixed
            }
            IrType::Heap(IrHeapKind::Union) => {
                matches!(call.result_php_type.codegen_repr(), PhpType::Union(_))
            }
            _ => false,
        };
        if !result_is_supported {
            return Some(format!(
                "result must be int, Mixed, Union, or unused; got {:?}/{:?}",
                call.result_type,
                call.result_php_type.codegen_repr()
            ));
        }
    }
    let Some((callback_function, visible_count)) =
        static_callable_contract(module, function, *callback)
    else {
        return Some(
            "callback must resolve statically to a direct closure or user-function callable"
                .to_string(),
        );
    };
    if let Some(issue) = callable_wrapper_issue(callback_function, visible_count, 2) {
        return Some(issue);
    }
    if callback_function.params[..visible_count]
        .iter()
        .any(|param| param.php_type.codegen_repr() != PhpType::Int)
    {
        return Some("callback parameters must be int".to_string());
    }
    if callback_function.return_type != IrType::I64
        || callback_function.return_php_type.codegen_repr() != PhpType::Int
    {
        return Some(format!(
            "callback must return int/I64, got {:?}/{:?}",
            callback_function.return_type,
            callback_function.return_php_type.codegen_repr()
        ));
    }
    None
}

/// Returns whether a value is exactly an indexed `array<int>`.
fn value_is_int_array(function: &Function, value: ValueId) -> bool {
    function.value(value).is_some_and(|value| {
        value.ir_type == IrType::Heap(IrHeapKind::Array)
            && matches!(
                value.php_type.codegen_repr(),
                PhpType::Array(element) if element.codegen_repr() == PhpType::Int
            )
    })
}

/// Validates a direct `ClosureCall` argument buffer and its result unboxing.
///
/// The descriptor must resolve to one statically proven wrapper contract.
/// Mutable or multi-definition descriptors fail closed until runtime
/// descriptors carry and validate their full parameter signature.
fn closure_call_shape_issue(
    module: &Module,
    owner: &Function,
    inst: &Instruction,
) -> Option<String> {
    let Some((callable, arguments)) = inst.operands.split_first() else {
        return Some("missing callable descriptor operand".to_string());
    };
    if !value_is_callable_descriptor(owner, *callable) {
        return Some("callable operand must be Callable/I64".to_string());
    }
    for (index, argument) in arguments.iter().enumerate() {
        let Some(value) = owner.value(*argument) else {
            return Some(format!("argument #{index} is missing from the value table"));
        };
        if !closure_argument_is_boxable(value.ir_type, &value.php_type) {
            return Some(format!(
                "argument #{index} cannot be boxed from {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if let Some(issue) = closure_result_shape_issue(owner, inst) {
        return Some(issue);
    }
    let Some((target, visible_count)) = static_callable_contract(module, owner, *callable) else {
        return Some(
            "callable descriptor signature is not statically provable before wrapper dispatch"
                .to_string(),
        );
    };
    if let Some(issue) = callable_wrapper_issue(target, visible_count, arguments.len()) {
        return Some(issue);
    }
    if let Some(issue) = callable_argument_contract_issue(owner, target, visible_count, arguments) {
        return Some(issue);
    }
    if !callable_return_is_boxable(target) {
        return Some(format!(
            "callable return {:?}/{:?} cannot be boxed by the WASM wrapper",
            target.return_type,
            target.return_php_type.codegen_repr()
        ));
    }
    if let Some(issue) = callable_result_contract_issue(owner, inst, target) {
        return Some(issue);
    }
    None
}

/// Validates descriptor invocation through a pre-built positional
/// `array<mixed>` and the result-cell conversion performed by the lowerer.
fn callable_descriptor_invoke_shape_issue(
    module: &Module,
    owner: &Function,
    inst: &Instruction,
) -> Option<String> {
    let [callable, arguments] = inst.operands.as_slice() else {
        return Some(format!(
            "expected callable and positional array, got {} operands",
            inst.operands.len()
        ));
    };
    if !value_is_callable_descriptor(owner, *callable) {
        return Some("callable operand must be Callable/I64".to_string());
    }
    let Some(argument_value) = owner.value(*arguments) else {
        return Some("argument container is missing from the value table".to_string());
    };
    let is_mixed_array = argument_value.ir_type == IrType::Heap(IrHeapKind::Array)
        && matches!(
            argument_value.php_type.codegen_repr(),
            PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed
        );
    if !is_mixed_array {
        return Some(format!(
            "argument container must be array<mixed>/Heap(Array), got {:?}/{:?}",
            argument_value.ir_type,
            argument_value.php_type.codegen_repr()
        ));
    }
    if let Some(issue) = closure_result_shape_issue(owner, inst) {
        return Some(issue);
    }
    let Some((target, visible_count)) = static_callable_contract(module, owner, *callable) else {
        return Some(
            "callable descriptor signature is not statically provable before array dispatch"
                .to_string(),
        );
    };
    if let Some(issue) = callable_wrapper_signature_issue(target, visible_count) {
        return Some(issue);
    }
    if visible_count != 0 {
        return Some(format!(
            "array<mixed> descriptor arguments cannot prove the {} visible parameter tag(s)",
            visible_count
        ));
    }
    if !callable_return_is_boxable(target) {
        return Some(format!(
            "callable return {:?}/{:?} cannot be boxed by the WASM wrapper",
            target.return_type,
            target.return_php_type.codegen_repr()
        ));
    }
    if let Some(issue) = callable_result_contract_issue(owner, inst, target) {
        return Some(issue);
    }
    None
}

/// Requires each wrapper-consumed argument to match its parameter exactly.
fn callable_argument_contract_issue(
    owner: &Function,
    target: &Function,
    visible_count: usize,
    arguments: &[ValueId],
) -> Option<String> {
    for (index, (argument, parameter)) in arguments
        .iter()
        .take(visible_count)
        .zip(&target.params[..visible_count])
        .enumerate()
    {
        let Some(source) = owner.value(*argument) else {
            return Some(format!("argument #{index} is missing from the value table"));
        };
        if source.ir_type != parameter.ir_type
            || source.php_type.codegen_repr() != parameter.php_type.codegen_repr()
        {
            return Some(format!(
                "argument #{index} {:?}/{:?} requires an implicit wrapper conversion to {:?}/{:?}",
                source.ir_type,
                source.php_type.codegen_repr(),
                parameter.ir_type,
                parameter.php_type.codegen_repr()
            ));
        }
    }
    None
}

/// Requires a used wrapper result to preserve the statically proven return tag.
fn callable_result_contract_issue(
    owner: &Function,
    inst: &Instruction,
    target: &Function,
) -> Option<String> {
    let Some(result) = inst.result else {
        return None;
    };
    let Some(destination) = owner.value(result) else {
        return Some("callable result is missing from the value table".to_string());
    };
    if destination.ir_type == IrType::Heap(IrHeapKind::Mixed)
        && destination.php_type.codegen_repr() == PhpType::Mixed
    {
        return None;
    }
    if destination.ir_type != target.return_type
        || destination.php_type.codegen_repr() != target.return_php_type.codegen_repr()
    {
        return Some(format!(
            "used callable result {:?}/{:?} requires an implicit wrapper conversion from {:?}/{:?}",
            destination.ir_type,
            destination.php_type.codegen_repr(),
            target.return_type,
            target.return_php_type.codegen_repr()
        ));
    }
    None
}

/// Returns whether one EIR value carries the descriptor representation consumed
/// by `__rt_closure_call`.
fn value_is_callable_descriptor(function: &Function, value: ValueId) -> bool {
    function.value(value).is_some_and(|value| {
        value.ir_type == IrType::I64 && value.php_type.codegen_repr() == PhpType::Callable
    })
}

/// Returns whether `objects::emit_box_value_into_mixed` implements this source.
fn closure_argument_is_boxable(ir_type: IrType, php_type: &PhpType) -> bool {
    if transfer::validate_storage_pair(ir_type, php_type).is_err() {
        return false;
    }
    let php_type = php_type.codegen_repr();
    match ir_type {
        IrType::I64 => matches!(
            php_type,
            PhpType::Int | PhpType::Bool | PhpType::Callable
        ),
        IrType::F64 => php_type == PhpType::Float,
        IrType::Str => php_type == PhpType::Str,
        IrType::Heap(IrHeapKind::Array) => matches!(php_type, PhpType::Array(_)),
        IrType::Heap(IrHeapKind::Hash) => matches!(php_type, PhpType::AssocArray { .. }),
        IrType::Heap(IrHeapKind::Object) => matches!(php_type, PhpType::Object(_)),
        IrType::Heap(
            IrHeapKind::Mixed | IrHeapKind::Iterable | IrHeapKind::Union | IrHeapKind::Buffer,
        )
        | IrType::TaggedScalar
        | IrType::Void => false,
    }
}

/// Returns the first unsupported closure result-cell destination shape.
fn closure_result_shape_issue(owner: &Function, inst: &Instruction) -> Option<String> {
    let Some(result) = inst.result else {
        return None;
    };
    let Some(value) = owner.value(result) else {
        return Some("result is missing from the value table".to_string());
    };
    if value.ir_type == IrType::I64
        && value.php_type.codegen_repr() == PhpType::Callable
        && transfer::validate_storage_pair(value.ir_type, &value.php_type).is_ok()
    {
        return None;
    }
    transfer::classify_transfer(
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
        value.ir_type,
        value.php_type.codegen_repr(),
    )
    .err()
    .map(|error| format!("result cell cannot be stored: {error}"))
}

/// Resolves a direct closure/FCC descriptor through ownership-only SSA moves.
///
/// Dynamic callables, block parameters, and values loaded from mutable locals
/// remain rejected until callable descriptors carry a runtime signature.
fn static_callable_contract<'a>(
    module: &'a Module,
    owner: &'a Function,
    value: ValueId,
) -> Option<(&'a Function, usize)> {
    static_callable_contract_inner(module, owner, value, 0)
}

/// Resolves a callable contract with a bounded interprocedural capture walk.
fn static_callable_contract_inner<'a>(
    module: &'a Module,
    owner: &'a Function,
    value: ValueId,
    depth: usize,
) -> Option<(&'a Function, usize)> {
    if depth > 16 {
        return None;
    }
    let mut current = value;
    for _ in 0..=owner.values.len() {
        let value = owner.value(current)?;
        let ValueDef::Instruction { inst, .. } = value.def else {
            return None;
        };
        let defining = owner.instruction(inst)?;
        match defining.op {
            Op::Move | Op::Borrow | Op::Acquire => {
                let [source] = defining.operands.as_slice() else {
                    return None;
                };
                current = *source;
            }
            Op::LoadLocal => {
                let Some(Immediate::LocalSlot(slot)) = defining.immediate else {
                    return None;
                };
                let load_block = owner.blocks.iter().find(|block| {
                    block.instructions.iter().any(|candidate| *candidate == inst)
                })?;
                let stores = owner
                    .instructions
                    .iter()
                    .enumerate()
                    .filter(|(_, candidate)| {
                        candidate.op == Op::StoreLocal
                            && candidate.immediate == Some(Immediate::LocalSlot(slot))
                    })
                    .collect::<Vec<_>>();
                let (store_index, store) = match stores.as_slice() {
                    [(store_index, store)] => (*store_index, *store),
                    [] => {
                        return captured_callable_contract(
                            module,
                            owner,
                            slot,
                            depth + 1,
                        )
                    }
                    _ => return None,
                };
                let store_block = owner.blocks.iter().find(|block| {
                    block
                        .instructions
                        .iter()
                        .any(|candidate| candidate.as_raw() as usize == store_index)
                })?;
                let ordered = if store_block.id == load_block.id {
                    let store_position = store_block
                        .instructions
                        .iter()
                        .position(|candidate| candidate.as_raw() as usize == store_index)?;
                    let load_position = load_block
                        .instructions
                        .iter()
                        .position(|candidate| *candidate == inst)?;
                    store_position < load_position
                } else {
                    block_dominates(owner, store_block.id, load_block.id)
                };
                if !ordered {
                    return None;
                }
                let [source] = store.operands.as_slice() else {
                    return None;
                };
                current = *source;
            }
            Op::ClosureNew => {
                let name = data_string(module, defining)?;
                let function = module
                    .closures
                    .iter()
                    .find(|function| function.name == name)?;
                let visible = function
                    .params
                    .len()
                    .checked_sub(function.flags.closure_capture_count)?;
                return Some((function, visible));
            }
            Op::FirstClassCallableNew => {
                let name = data_string(module, defining)?;
                let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
                let function = module.functions.iter().find(|function| {
                    crate::names::php_symbol_key(function.name.trim_start_matches('\\')) == key
                })?;
                return Some((function, function.params.len()));
            }
            Op::Call => {
                let target = resolve_direct_call(module, defining).ok()?;
                return returned_callable_contract(
                    module,
                    target.function,
                    depth + 1,
                );
            }
            _ => return None,
        }
    }
    None
}

/// Resolves a direct callee whose every reachable return yields one callable contract.
fn returned_callable_contract<'a>(
    module: &'a Module,
    function: &'a Function,
    depth: usize,
) -> Option<(&'a Function, usize)> {
    if depth > 16 {
        return None;
    }
    let reachable = reachable_block_ids(function);
    let mut resolved: Option<(&'a Function, usize)> = None;
    for block in &function.blocks {
        if !reachable.contains(&block.id.as_raw()) {
            continue;
        }
        let Some(Terminator::Return { value: Some(value) }) =
            block.terminator.as_ref()
        else {
            if matches!(
                block.terminator.as_ref(),
                Some(Terminator::Return { value: None })
            ) {
                return None;
            }
            continue;
        };
        let contract =
            static_callable_contract_inner(module, function, *value, depth)?;
        if let Some((expected, expected_visible)) = resolved {
            if !std::ptr::eq(expected, contract.0)
                || expected_visible != contract.1
            {
                return None;
            }
        } else {
            resolved = Some(contract);
        }
    }
    resolved
}

/// Resolves a by-value callable capture when every creator supplies one contract.
fn captured_callable_contract<'a>(
    module: &'a Module,
    closure: &'a Function,
    slot: crate::ir::LocalSlotId,
    depth: usize,
) -> Option<(&'a Function, usize)> {
    if !closure.flags.is_closure {
        return None;
    }
    let visible_count = closure
        .params
        .len()
        .checked_sub(closure.flags.closure_capture_count)?;
    let parameter_index = slot.as_raw() as usize;
    if parameter_index < visible_count || parameter_index >= closure.params.len() {
        return None;
    }
    let capture_index = parameter_index - visible_count;
    let mut resolved: Option<(&'a Function, usize)> = None;
    for creator in module
        .functions
        .iter()
        .chain(&module.class_methods)
        .chain(&module.closures)
    {
        for candidate in &creator.instructions {
            if candidate.op != Op::ClosureNew
                || data_string(module, candidate) != Some(closure.name.as_str())
            {
                continue;
            }
            let operand = *candidate.operands.get(capture_index)?;
            let contract =
                static_callable_contract_inner(module, creator, operand, depth)?;
            if let Some((expected, expected_visible)) = resolved {
                if !std::ptr::eq(expected, contract.0) || expected_visible != contract.1 {
                    return None;
                }
            } else {
                resolved = Some(contract);
            }
        }
    }
    resolved
}

/// Resolves an ordinary or source-profiled data entry in the shared string pool.
fn data_string<'a>(module: &'a Module, instruction: &Instruction) -> Option<&'a str> {
    let data = match instruction.immediate {
        Some(Immediate::Data(data)) | Some(Immediate::ProfiledData { data, .. }) => data,
        _ => return None,
    };
    module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Returns the first wrapper-shape defect for a statically resolved callable.
fn callable_wrapper_issue(
    function: &Function,
    visible_count: usize,
    supplied_args: usize,
) -> Option<String> {
    if visible_count > supplied_args {
        return Some(format!(
            "callback declares {visible_count} visible parameters but the runtime supplies {supplied_args}"
        ));
    }
    callable_wrapper_signature_issue(function, visible_count)
}

/// Returns the first parameter-shape defect in a callable wrapper without
/// making a claim about the runtime argument-array length.
fn callable_wrapper_signature_issue(
    function: &Function,
    visible_count: usize,
) -> Option<String> {
    for param in &function.params[..visible_count] {
        if param.by_ref || param.variadic {
            return Some(format!(
                "callback parameter {} is by-reference or variadic",
                param.name
            ));
        }
        if !callable_param_is_unboxable(param) {
            return Some(format!(
                "callback parameter {} has unsupported storage {:?}/{:?}",
                param.name,
                param.ir_type,
                param.php_type.codegen_repr()
            ));
        }
    }
    None
}

/// Returns whether the callable wrapper can materialize one argument type.
fn callable_param_is_unboxable(param: &crate::ir::FunctionParam) -> bool {
    if transfer::validate_storage_pair(param.ir_type, &param.php_type).is_err() {
        return false;
    }
    let php_type = param.php_type.codegen_repr();
    match param.ir_type {
        IrType::I64 => matches!(
            php_type,
            PhpType::Int | PhpType::Bool | PhpType::Callable
        ),
        IrType::F64 => php_type == PhpType::Float,
        IrType::Str => php_type == PhpType::Str,
        IrType::Heap(IrHeapKind::Array) => matches!(php_type, PhpType::Array(_)),
        IrType::Heap(IrHeapKind::Hash) => matches!(php_type, PhpType::AssocArray { .. }),
        IrType::Heap(IrHeapKind::Object) => matches!(php_type, PhpType::Object(_)),
        // The call's argument buffer is already an array of CELLS, so a Mixed parameter is
        // the simplest case of all: the cell passes straight through with nothing to unbox.
        IrType::Heap(IrHeapKind::Mixed) => php_type == PhpType::Mixed,
        IrType::Heap(
            IrHeapKind::Iterable | IrHeapKind::Union | IrHeapKind::Buffer,
        )
        | IrType::TaggedScalar
        | IrType::Void => false,
    }
}

/// Returns whether the callable wrapper can box one callback result.
fn callable_return_is_boxable(function: &Function) -> bool {
    transfer::classify_transfer(
        function.return_type,
        function.return_php_type.codegen_repr(),
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    )
    .is_ok()
}

/// Returns whether a typed runtime function is safe to admit through the WASM gate.
///
/// A partial lowering stays rejected when its accepted subset changes PHP-visible
/// behavior. This keeps the capability audit conservative until the full public
/// contract is implemented and differentially tested.
pub(super) fn runtime_function_is_supported(target: RuntimeFnId) -> bool {
    match target {
        RuntimeFnId::GetClass
        | RuntimeFnId::ArrayMap
        | RuntimeFnId::Usort
        | RuntimeFnId::ArrayReduce
        | RuntimeFnId::Abs
        | RuntimeFnId::Floor
        | RuntimeFnId::Round
        | RuntimeFnId::Ceil
        | RuntimeFnId::Sqrt
        | RuntimeFnId::Count
        | RuntimeFnId::ArrayIsList
        | RuntimeFnId::ArrayKeys
        | RuntimeFnId::ArrayValues
        | RuntimeFnId::InArray
        | RuntimeFnId::ArrayReverse
        | RuntimeFnId::ArraySum
        | RuntimeFnId::ArrayProduct
        | RuntimeFnId::Max
        | RuntimeFnId::Min
        | RuntimeFnId::Intdiv
        | RuntimeFnId::ArrayFill
        | RuntimeFnId::StrContains
        | RuntimeFnId::StrStartsWith
        | RuntimeFnId::StrEndsWith
        | RuntimeFnId::Chr
        | RuntimeFnId::Ord
        | RuntimeFnId::Ucfirst
        | RuntimeFnId::Lcfirst
        | RuntimeFnId::Ucwords
        | RuntimeFnId::Strcmp
        | RuntimeFnId::Strcasecmp
        | RuntimeFnId::Trim
        | RuntimeFnId::Ltrim
        | RuntimeFnId::Rtrim
        | RuntimeFnId::Substr
        | RuntimeFnId::StrRepeat
        | RuntimeFnId::Strpos
        | RuntimeFnId::Strrpos
        | RuntimeFnId::Implode
        | RuntimeFnId::ArraySlice
        | RuntimeFnId::ArrayMerge
        | RuntimeFnId::Range
        | RuntimeFnId::ArrayKeyExists
        | RuntimeFnId::Sort
        | RuntimeFnId::Rsort
        | RuntimeFnId::ArraySearch
        | RuntimeFnId::Explode
        | RuntimeFnId::StrSplit
        | RuntimeFnId::Wordwrap
        | RuntimeFnId::Sprintf
        | RuntimeFnId::Printf
        | RuntimeFnId::Strstr
        | RuntimeFnId::StrPad
        | RuntimeFnId::StrReplace
        | RuntimeFnId::Crc32
        | RuntimeFnId::Sha1
        | RuntimeFnId::Md5
        | RuntimeFnId::Htmlspecialchars => true,
        RuntimeFnId::ArrayFilter
        | RuntimeFnId::Uasort
        | RuntimeFnId::Uksort
        | RuntimeFnId::ArrayWalk
        | RuntimeFnId::ArrayAll
        | RuntimeFnId::ArrayAny
        | RuntimeFnId::ArrayChunk
        | RuntimeFnId::ArrayColumn
        | RuntimeFnId::ArrayCombine
        | RuntimeFnId::ArrayDiff
        | RuntimeFnId::ArrayDiffAssoc
        | RuntimeFnId::ArrayDiffKey
        | RuntimeFnId::ArrayFillKeys
        | RuntimeFnId::ArrayFind
        | RuntimeFnId::ArrayFlip
        | RuntimeFnId::ArrayIntersect
        | RuntimeFnId::ArrayIntersectAssoc
        | RuntimeFnId::ArrayIntersectKey
        | RuntimeFnId::ArrayKeyFirst
        | RuntimeFnId::ArrayKeyLast
        | RuntimeFnId::ArrayMergeRecursive
        | RuntimeFnId::ArrayMultisort
        | RuntimeFnId::ArrayPad
        | RuntimeFnId::ArrayPop
        | RuntimeFnId::ArrayPush
        | RuntimeFnId::ArrayRand
        | RuntimeFnId::ArrayReplace
        | RuntimeFnId::ArrayReplaceRecursive
        | RuntimeFnId::ArrayShift
        | RuntimeFnId::ArraySplice
        | RuntimeFnId::ArrayUdiff
        | RuntimeFnId::ArrayUintersect
        | RuntimeFnId::ArrayUnique
        | RuntimeFnId::ArrayUnshift
        | RuntimeFnId::ArrayWalkRecursive
        | RuntimeFnId::Arsort
        | RuntimeFnId::Asort
        | RuntimeFnId::Krsort
        | RuntimeFnId::Ksort
        | RuntimeFnId::Natcasesort
        | RuntimeFnId::Natsort
        | RuntimeFnId::Shuffle
        | RuntimeFnId::CallUserFunc
        | RuntimeFnId::CallUserFuncArray
        | RuntimeFnId::ClassAlias
        | RuntimeFnId::ClassExists
        | RuntimeFnId::ClassImplements
        | RuntimeFnId::ClassParents
        | RuntimeFnId::ClassUses
        | RuntimeFnId::EnumExists
        | RuntimeFnId::FunctionExists
        | RuntimeFnId::GetDeclaredClasses
        | RuntimeFnId::GetDeclaredInterfaces
        | RuntimeFnId::GetDeclaredTraits
        | RuntimeFnId::GetLoadedExtensions
        | RuntimeFnId::GetParentClass
        | RuntimeFnId::InterfaceExists
        | RuntimeFnId::IsA
        | RuntimeFnId::IsSubclassOf
        | RuntimeFnId::MethodExists
        | RuntimeFnId::PregReplaceCallback
        | RuntimeFnId::PropertyExists
        | RuntimeFnId::TraitExists
        | RuntimeFnId::ElephcPharBzip2Archive
        | RuntimeFnId::ElephcPharDecompressArchive
        | RuntimeFnId::ElephcPharGetFileMetadata
        | RuntimeFnId::ElephcPharGetMetadata
        | RuntimeFnId::ElephcPharGetSignatureHash
        | RuntimeFnId::ElephcPharGetSignatureType
        | RuntimeFnId::ElephcPharGetStub
        | RuntimeFnId::ElephcPharGzipArchive
        | RuntimeFnId::ElephcPharListEntries
        | RuntimeFnId::ElephcPharSetCompression
        | RuntimeFnId::ElephcPharSetFileMetadata
        | RuntimeFnId::ElephcPharSetMetadata
        | RuntimeFnId::ElephcPharSetStub
        | RuntimeFnId::ElephcPharSetZipPassword
        | RuntimeFnId::ElephcPharSignHash
        | RuntimeFnId::ElephcPharSignOpenssl
        | RuntimeFnId::Basename
        | RuntimeFnId::Chdir
        | RuntimeFnId::Chgrp
        | RuntimeFnId::Chmod
        | RuntimeFnId::Chown
        | RuntimeFnId::Clearstatcache
        | RuntimeFnId::Closedir
        | RuntimeFnId::Copy
        | RuntimeFnId::Dirname
        | RuntimeFnId::DiskFreeSpace
        | RuntimeFnId::DiskTotalSpace
        | RuntimeFnId::Fclose
        | RuntimeFnId::Fdatasync
        | RuntimeFnId::Feof
        | RuntimeFnId::Fflush
        | RuntimeFnId::Fgetc
        | RuntimeFnId::Fgetcsv
        | RuntimeFnId::Fgets
        | RuntimeFnId::File
        | RuntimeFnId::FileExists
        | RuntimeFnId::FileGetContents
        | RuntimeFnId::FilePutContents
        | RuntimeFnId::Fileatime
        | RuntimeFnId::Filectime
        | RuntimeFnId::Filegroup
        | RuntimeFnId::Fileinode
        | RuntimeFnId::Filemtime
        | RuntimeFnId::Fileowner
        | RuntimeFnId::Fileperms
        | RuntimeFnId::Filesize
        | RuntimeFnId::Filetype
        | RuntimeFnId::Flock
        | RuntimeFnId::Fnmatch
        | RuntimeFnId::Fopen
        | RuntimeFnId::Fpassthru
        | RuntimeFnId::Fprintf
        | RuntimeFnId::Fputcsv
        | RuntimeFnId::Fread
        | RuntimeFnId::Fscanf
        | RuntimeFnId::Fseek
        | RuntimeFnId::Fsockopen
        | RuntimeFnId::Fstat
        | RuntimeFnId::Fsync
        | RuntimeFnId::Ftell
        | RuntimeFnId::Ftruncate
        | RuntimeFnId::Fwrite
        | RuntimeFnId::Getcwd
        | RuntimeFnId::Gethostbyaddr
        | RuntimeFnId::Gethostbyname
        | RuntimeFnId::Gethostname
        | RuntimeFnId::Getprotobyname
        | RuntimeFnId::Getprotobynumber
        | RuntimeFnId::Getservbyname
        | RuntimeFnId::Getservbyport
        | RuntimeFnId::Glob
        | RuntimeFnId::HashFile
        | RuntimeFnId::IsDir
        | RuntimeFnId::IsExecutable
        | RuntimeFnId::IsFile
        | RuntimeFnId::IsLink
        | RuntimeFnId::IsReadable
        | RuntimeFnId::IsWritable
        | RuntimeFnId::IsWriteable
        | RuntimeFnId::Lchgrp
        | RuntimeFnId::Lchown
        | RuntimeFnId::Link
        | RuntimeFnId::Linkinfo
        | RuntimeFnId::Lstat
        | RuntimeFnId::Mkdir
        | RuntimeFnId::ObClean
        | RuntimeFnId::ObEndClean
        | RuntimeFnId::ObEndFlush
        | RuntimeFnId::ObFlush
        | RuntimeFnId::ObGetClean
        | RuntimeFnId::ObGetContents
        | RuntimeFnId::ObGetFlush
        | RuntimeFnId::ObGetLength
        | RuntimeFnId::ObGetLevel
        | RuntimeFnId::ObGetStatus
        | RuntimeFnId::ObImplicitFlush
        | RuntimeFnId::ObListHandlers
        | RuntimeFnId::ObStart
        | RuntimeFnId::Opendir
        | RuntimeFnId::Pathinfo
        | RuntimeFnId::Pclose
        | RuntimeFnId::Pfsockopen
        | RuntimeFnId::Popen
        | RuntimeFnId::PrintR
        | RuntimeFnId::Readdir
        | RuntimeFnId::Readfile
        | RuntimeFnId::Readline
        | RuntimeFnId::Readlink
        | RuntimeFnId::Realpath
        | RuntimeFnId::RealpathCacheGet
        | RuntimeFnId::RealpathCacheSize
        | RuntimeFnId::Rename
        | RuntimeFnId::Rewind
        | RuntimeFnId::Rewinddir
        | RuntimeFnId::Rmdir
        | RuntimeFnId::Scandir
        | RuntimeFnId::Stat
        | RuntimeFnId::StreamBucketAppend
        | RuntimeFnId::StreamBucketMakeWriteable
        | RuntimeFnId::StreamBucketNew
        | RuntimeFnId::StreamBucketPrepend
        | RuntimeFnId::StreamContextCreate
        | RuntimeFnId::StreamContextGetDefault
        | RuntimeFnId::StreamContextGetOptions
        | RuntimeFnId::StreamContextGetParams
        | RuntimeFnId::StreamContextSetDefault
        | RuntimeFnId::StreamContextSetOption
        | RuntimeFnId::StreamContextSetParams
        | RuntimeFnId::StreamCopyToStream
        | RuntimeFnId::StreamFilterAppend
        | RuntimeFnId::StreamFilterPrepend
        | RuntimeFnId::StreamFilterRegister
        | RuntimeFnId::StreamFilterRemove
        | RuntimeFnId::StreamGetContents
        | RuntimeFnId::StreamGetFilters
        | RuntimeFnId::StreamGetLine
        | RuntimeFnId::StreamGetMetaData
        | RuntimeFnId::StreamGetTransports
        | RuntimeFnId::StreamGetWrappers
        | RuntimeFnId::StreamIsLocal
        | RuntimeFnId::StreamIsatty
        | RuntimeFnId::StreamResolveIncludePath
        | RuntimeFnId::StreamSelect
        | RuntimeFnId::StreamSetBlocking
        | RuntimeFnId::StreamSetChunkSize
        | RuntimeFnId::StreamSetReadBuffer
        | RuntimeFnId::StreamSetTimeout
        | RuntimeFnId::StreamSetWriteBuffer
        | RuntimeFnId::StreamSocketAccept
        | RuntimeFnId::StreamSocketClient
        | RuntimeFnId::StreamSocketEnableCrypto
        | RuntimeFnId::StreamSocketGetName
        | RuntimeFnId::StreamSocketPair
        | RuntimeFnId::StreamSocketRecvfrom
        | RuntimeFnId::StreamSocketSendto
        | RuntimeFnId::StreamSocketServer
        | RuntimeFnId::StreamSocketShutdown
        | RuntimeFnId::StreamSupportsLock
        | RuntimeFnId::StreamWrapperRegister
        | RuntimeFnId::StreamWrapperRestore
        | RuntimeFnId::StreamWrapperUnregister
        | RuntimeFnId::Symlink
        | RuntimeFnId::SysGetTempDir
        | RuntimeFnId::Tempnam
        | RuntimeFnId::Tmpfile
        | RuntimeFnId::Touch
        | RuntimeFnId::Umask
        | RuntimeFnId::Unlink
        | RuntimeFnId::VarDump
        | RuntimeFnId::Vfprintf
        | RuntimeFnId::Acos
        | RuntimeFnId::Asin
        | RuntimeFnId::Atan
        | RuntimeFnId::Atan2
        | RuntimeFnId::Clamp
        | RuntimeFnId::Cos
        | RuntimeFnId::Cosh
        | RuntimeFnId::Deg2rad
        | RuntimeFnId::Exp
        | RuntimeFnId::Fdiv
        | RuntimeFnId::Fmod
        | RuntimeFnId::Hypot
        | RuntimeFnId::Log
        | RuntimeFnId::Log10
        | RuntimeFnId::Log2
        | RuntimeFnId::MtRand
        | RuntimeFnId::Pi
        | RuntimeFnId::Pow
        | RuntimeFnId::Rad2deg
        | RuntimeFnId::Rand
        | RuntimeFnId::RandomInt
        | RuntimeFnId::Sin
        | RuntimeFnId::Sinh
        | RuntimeFnId::Tan
        | RuntimeFnId::Tanh
        | RuntimeFnId::ElephcPtrIsNull
        | RuntimeFnId::ElephcPtrReadString
        | RuntimeFnId::ElephcPtrWriteString
        | RuntimeFnId::BufferFree
        | RuntimeFnId::BufferLen
        | RuntimeFnId::Ptr
        | RuntimeFnId::PtrGet
        | RuntimeFnId::PtrIsNull
        | RuntimeFnId::PtrNull
        | RuntimeFnId::PtrOffset
        | RuntimeFnId::PtrRead16
        | RuntimeFnId::PtrRead32
        | RuntimeFnId::PtrRead8
        | RuntimeFnId::PtrReadString
        | RuntimeFnId::PtrSet
        | RuntimeFnId::PtrSizeof
        | RuntimeFnId::PtrWrite16
        | RuntimeFnId::PtrWrite32
        | RuntimeFnId::PtrWrite8
        | RuntimeFnId::PtrWriteString
        | RuntimeFnId::ZvalFree
        | RuntimeFnId::ZvalPack
        | RuntimeFnId::ZvalType
        | RuntimeFnId::ZvalUnpack
        | RuntimeFnId::IteratorApply
        | RuntimeFnId::IteratorCount
        | RuntimeFnId::IteratorToArray
        | RuntimeFnId::SplAutoload
        | RuntimeFnId::SplAutoloadCall
        | RuntimeFnId::SplAutoloadExtensions
        | RuntimeFnId::SplAutoloadFunctions
        | RuntimeFnId::SplAutoloadRegister
        | RuntimeFnId::SplAutoloadUnregister
        | RuntimeFnId::SplClasses
        | RuntimeFnId::SplObjectHash
        | RuntimeFnId::SplObjectId
        | RuntimeFnId::Chop
        | RuntimeFnId::CtypeAlnum
        | RuntimeFnId::CtypeAlpha
        | RuntimeFnId::CtypeDigit
        | RuntimeFnId::CtypeSpace
        | RuntimeFnId::GraphemeStrrev
        | RuntimeFnId::Gzcompress
        | RuntimeFnId::Gzdeflate
        | RuntimeFnId::Gzinflate
        | RuntimeFnId::Gzuncompress
        | RuntimeFnId::Hash
        | RuntimeFnId::HashAlgos
        | RuntimeFnId::HashCopy
        | RuntimeFnId::HashEquals
        | RuntimeFnId::HashFinal
        | RuntimeFnId::HashHmac
        | RuntimeFnId::HashInit
        | RuntimeFnId::HashUpdate
        | RuntimeFnId::Htmlentities
        | RuntimeFnId::InetNtop
        | RuntimeFnId::InetPton
        | RuntimeFnId::Ip2long
        | RuntimeFnId::Long2ip
        | RuntimeFnId::MbEregMatch
        | RuntimeFnId::MbStrlen
        | RuntimeFnId::NumberFormat
        | RuntimeFnId::Sscanf
        | RuntimeFnId::StrIreplace
        | RuntimeFnId::SubstrReplace
        | RuntimeFnId::Vprintf
        | RuntimeFnId::Vsprintf
        | RuntimeFnId::ElephcGmmktimeRaw
        | RuntimeFnId::ElephcMktimeRaw
        | RuntimeFnId::ElephcStrtotimeRaw
        | RuntimeFnId::Checkdate
        | RuntimeFnId::ClassAttributeArgs
        | RuntimeFnId::ClassAttributeNames
        | RuntimeFnId::ClassGetAttributes
        | RuntimeFnId::Date
        | RuntimeFnId::DateDefaultTimezoneGet
        | RuntimeFnId::DateDefaultTimezoneSet
        | RuntimeFnId::Define
        | RuntimeFnId::Defined
        | RuntimeFnId::Exec
        | RuntimeFnId::ExtensionLoaded
        | RuntimeFnId::Getdate
        | RuntimeFnId::Getenv
        | RuntimeFnId::Gmdate
        | RuntimeFnId::Gmmktime
        | RuntimeFnId::Header
        | RuntimeFnId::Hrtime
        | RuntimeFnId::HttpResponseCode
        | RuntimeFnId::JsonDecode
        | RuntimeFnId::JsonEncode
        | RuntimeFnId::JsonLastError
        | RuntimeFnId::JsonLastErrorMsg
        | RuntimeFnId::JsonValidate
        | RuntimeFnId::Localtime
        | RuntimeFnId::Microtime
        | RuntimeFnId::Mktime
        | RuntimeFnId::Passthru
        | RuntimeFnId::PhpUname
        | RuntimeFnId::Phpversion
        | RuntimeFnId::PregMatch
        | RuntimeFnId::PregMatchAll
        | RuntimeFnId::PregReplace
        | RuntimeFnId::PregSplit
        | RuntimeFnId::Putenv
        | RuntimeFnId::Serialize
        | RuntimeFnId::ShellExec
        | RuntimeFnId::Sleep
        | RuntimeFnId::Strtotime
        | RuntimeFnId::System
        | RuntimeFnId::Time
        | RuntimeFnId::Unserialize
        | RuntimeFnId::Usleep
        | RuntimeFnId::GetResourceId
        | RuntimeFnId::GetResourceType
        | RuntimeFnId::Gettype
        | RuntimeFnId::IsCallable
        | RuntimeFnId::IsFinite
        | RuntimeFnId::IsInfinite
        | RuntimeFnId::IsNan
        | RuntimeFnId::IsNumeric
        | RuntimeFnId::Settype
        => false,
    }
}

/// Returns the stable name of every unary-string runtime variant.
pub(super) fn unary_string_name(target: UnaryStringRuntime) -> &'static str {
    match target {
        UnaryStringRuntime::AddSlashes => "string.add_slashes",
        UnaryStringRuntime::Base64Decode => "string.base64_decode",
        UnaryStringRuntime::Base64Encode => "string.base64_encode",
        UnaryStringRuntime::BinToHex => "string.bin_to_hex",
        UnaryStringRuntime::HexToBin => "string.hex_to_bin",
        UnaryStringRuntime::HtmlEntityDecode => "string.html_entity_decode",
        UnaryStringRuntime::NlToBr => "string.nl_to_br",
        UnaryStringRuntime::RawUrlDecode => "string.raw_url_decode",
        UnaryStringRuntime::RawUrlEncode => "string.raw_url_encode",
        UnaryStringRuntime::StripSlashes => "string.strip_slashes",
        UnaryStringRuntime::StrReverse => "string.reverse",
        UnaryStringRuntime::StrToLower => "string.to_lower",
        UnaryStringRuntime::StrToUpper => "string.to_upper",
        UnaryStringRuntime::UrlDecode => "string.url_decode",
        UnaryStringRuntime::UrlEncode => "string.url_encode",
    }
}

/// Returns whether an EIR terminator has an active WASM lowering.
pub(super) fn terminator_is_supported(terminator: &Terminator) -> bool {
    match terminator {
        Terminator::Br { .. }
        | Terminator::CondBr { .. }
        | Terminator::Switch { .. }
        | Terminator::Return { .. }
        | Terminator::Throw { .. }
        | Terminator::Unreachable
        | Terminator::Fatal { .. } => true,
        Terminator::GeneratorSuspend { .. } => false,
    }
}

/// Returns the stable diagnostic name for every EIR terminator.
pub(super) fn terminator_name(terminator: &Terminator) -> &'static str {
    match terminator {
        Terminator::Br { .. } => "br",
        Terminator::CondBr { .. } => "cond_br",
        Terminator::Switch { .. } => "switch",
        Terminator::Return { .. } => "return",
        Terminator::Throw { .. } => "throw",
        Terminator::Fatal { .. } => "fatal",
        Terminator::GeneratorSuspend { .. } => "generator_suspend",
        Terminator::Unreachable => "unreachable",
    }
}

/// Returns whether an EIR opcode has an active WASM dispatch.
pub(super) fn op_is_supported(op: Op) -> bool {
    match op {
        Op::ConstI64
        | Op::ConstF64
        | Op::ConstStr
        | Op::ConstNull
        | Op::ConstBool
        | Op::LoadLocal
        | Op::StoreLocal
        | Op::UnsetLocal
        | Op::LoadRefCell
        | Op::StoreRefCell
        | Op::PromoteLocalRefCell
        | Op::AliasLocalRefCell
        | Op::ReleaseLocalRefCell
        | Op::LoadGlobal
        | Op::IAdd
        | Op::ISub
        | Op::IMul
        | Op::ICheckedAdd
        | Op::ICheckedSub
        | Op::ICheckedMul
        | Op::IDiv
        | Op::ISDiv
        | Op::ISMod
        | Op::INeg
        | Op::IBitAnd
        | Op::IBitOr
        | Op::IBitXor
        | Op::IBitNot
        | Op::IShl
        | Op::IShrA
        | Op::FAdd
        | Op::FSub
        | Op::FMul
        | Op::FDiv
        | Op::FNeg
        | Op::ICmp
        | Op::FCmp
        | Op::StrictEq
        | Op::StrictNotEq
        | Op::IsNull
        | Op::IsTruthy
        | Op::InstanceOf
        | Op::IToF
        | Op::IToStr
        | Op::Cast
        | Op::MixedBox
        | Op::MixedTagOf
        | Op::StrConcat
        | Op::StrLen
        | Op::StrPersist
        | Op::ArrayToMixed
        | Op::LooseEq
        | Op::LooseNotEq
        | Op::ConcatReset
        | Op::ArrayNew
        | Op::HashNew
        | Op::ArrayLen
        | Op::ArrayGet
        | Op::ArrayGetSilent
        | Op::HashGet
        | Op::HashGetSilent
        | Op::ArraySet
        | Op::HashSet
        | Op::HashUnset
        | Op::HashIsset
        | Op::GcCollect
        | Op::LoadStaticProperty
        | Op::StoreStaticProperty
        | Op::ScopedConstantGet
        | Op::ArrayPush
        | Op::ArrayToHash
        | Op::HashAppend
        | Op::ArrayUnion
        | Op::HashUnion
        | Op::ArrayHashUnion
        | Op::HashArrayUnion
        | Op::IterStart
        | Op::IterCurrentKey
        | Op::IterCurrentValue
        | Op::IterCurrentValueRef
        | Op::IterNext
        | Op::IterEnd
        | Op::ObjectNew
        | Op::PropGet
        | Op::PropSet
        | Op::NullsafePropGet
        | Op::NullsafeMethodCall
        | Op::MethodCall
        | Op::StaticMethodCall
        | Op::InstanceOfDynamic
        | Op::Call
        | Op::LanguageConstructCall
        | Op::RuntimeCall
        | Op::ClosureNew
        | Op::ClosureCapture
        | Op::ClosureCall
        | Op::FirstClassCallableNew
        | Op::CallableDescriptorInvoke
        | Op::EchoValue
        | Op::PrintValue
        | Op::Warn
        | Op::ThrowError
        | Op::Acquire
        | Op::Release
        | Op::Move
        | Op::Borrow
        | Op::MixedNumericBinop
        | Op::TryPushHandler
        | Op::TryPopHandler
        | Op::ThrowException
        | Op::ThrowErrorValue
        | Op::CatchCurrent
        | Op::CatchBind
        | Op::Nop => true,
        Op::ConstClassName
        | Op::ConstEnumCase
        | Op::LoadCalledClassId
        | Op::DataAddr
        | Op::ReleaseLocalSlot
        | Op::StoreGlobal
        | Op::LoadStaticLocal
        | Op::StoreStaticLocal
        | Op::InitStaticLocal
        | Op::LoadReflectionStaticProperty
        | Op::StoreReflectionStaticProperty
        | Op::ReflectionStaticPropertyInitialized
        | Op::IPow
        | Op::FPow
        | Op::StrEq
        | Op::StrCmp
        | Op::StrLooseEq
        | Op::Spaceship
        | Op::TypePredicate
        | Op::IsEmpty
        | Op::FToI
        | Op::FToStr
        | Op::BoolToStr
        | Op::StrToI
        | Op::StrToF
        | Op::StrToNumber
        | Op::ResourceToStr
        | Op::InvokerRefArg
        | Op::MixedUnbox
        | Op::HashToMixed
        | Op::MixedCastBool
        | Op::MixedCastInt
        | Op::MixedCastFloat
        | Op::MixedCastString
        | Op::StrCharAt
        | Op::StrInterpolate
        | Op::WriteStrStdout
        | Op::HashLen
        | Op::ArrayIsset
        | Op::ArrayElemAddr
        | Op::MixedArrayAppend
        | Op::ArrayEnsureUnique
        | Op::HashEnsureUnique
        | Op::ArrayCloneShallow
        | Op::HashCloneShallow
        | Op::HashSpread
        | Op::ArraySetMixedKey
        | Op::ArrayGetMixedKey
        | Op::ArrayGetMixedKeySilent
        | Op::ArrayKeyExists
        | Op::OffsetExists
        | Op::OffsetUnset
        | Op::ListUnpack
        | Op::IteratorMethodCall
        | Op::SplRuntimeCall
        | Op::EvalObjectNew
        | Op::ObjectCloneShallow
        | Op::DynamicObjectNew
        | Op::DynamicObjectNewMixed
        | Op::DynamicObjectNewWithoutConstructorMixed
        | Op::PropInitialized
        | Op::LoadPropRefCell
        | Op::LoadArrayElemRefCell
        | Op::BindRefCellPtr
        | Op::DynamicPropGet
        | Op::DynamicPropSet
        | Op::MethodLookup
        | Op::EvalStaticMethodCall
        | Op::EnumBackingStringToInt
        | Op::EnumBackingMixedToInt
        | Op::ClassConstant
        | Op::ClassAttrNames
        | Op::ClassAttrArgs
        | Op::ClassGetAttributes
        | Op::FunctionVariantCall
        | Op::ClosureBind
        | Op::EvalLiteralCall
        | Op::EvalScopeGet
        | Op::EvalScopeSet
        | Op::EvalFunctionCall
        | Op::EvalFunctionCallArray
        | Op::EvalFunctionExists
        | Op::EvalClassExists
        | Op::EvalConstantExists
        | Op::EvalConstantFetch
        | Op::ExternCall
        | Op::ExprCall
        | Op::CallableArrayNew
        | Op::PipeCall
        | Op::PtrCast
        | Op::PtrRead
        | Op::PtrWrite
        | Op::PtrReadString
        | Op::PtrWriteString
        | Op::PtrOffset
        | Op::PtrCheckNonnull
        | Op::BufferNew
        | Op::BufferLen
        | Op::BufferGet
        | Op::BufferSet
        | Op::BufferFree
        | Op::PackedFieldGet
        | Op::PackedFieldSet
        | Op::ExternGlobalLoad
        | Op::ExternGlobalStore
        | Op::WriteStdout
        | Op::VarDump
        | Op::PrintR
        | Op::ErrorSuppressBegin
        | Op::ErrorSuppressEnd
        | Op::FinallyEnter
        | Op::FinallyExit
        | Op::FiberRuntimeCall
        | Op::GeneratorNew
        | Op::GeneratorYield
        | Op::GeneratorYieldFrom
        | Op::GeneratorReturn
        | Op::IncludeOnceMark
        | Op::IncludeOnceGuard
        | Op::FunctionVariantMark
        | Op::FunctionVariantDispatch
        | Op::EnsureOwned => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{loose_eq_shape_issue, validate_module as validate_and_plan, LoweredWasmPlan, WasmError};
    use crate::codegen::platform::Target;
    use crate::codegen::Emit;
    use crate::ir::{
        Builder, DataId, Function, FunctionParam, Immediate, IrHeapKind, IrType, LocalKind, Module,
        Op, Ownership, RuntimeCallTarget, RuntimeFnId, Terminator,
    };
    use crate::parser::ast::Visibility;
    use crate::span::Span;
    use crate::types::{ClassInfo, FunctionSig, PhpType};
    use std::collections::{HashMap, HashSet};

    /// `Op::LooseEq` admits only the operand pairs whose rule was MEASURED against php-src.
    ///
    /// PHP 8's `==` table is much wider than what is lowered: a bool against a number casts the
    /// number to bool, a Mixed cell dispatches on its tag, and arrays compare element-wise.
    /// Answering those by guessing would be a silently wrong answer rather than a refusal, so the
    /// gate keeps them out.
    #[test]
    fn loose_equality_admits_only_measured_pairs() {
        let probe_op = |op: Op, left: (IrType, PhpType), right: (IrType, PhpType)| {
            let mut function =
                Function::new("probe".to_string(), IrType::Void, PhpType::Void);
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let mut operands = Vec::new();
                for (index, (ir, php)) in [left.clone(), right.clone()].into_iter().enumerate() {
                    let slot = builder.add_local(
                        Some(format!("a{index}")),
                        ir,
                        php.clone(),
                        LocalKind::PhpLocal,
                    );
                    operands.push(builder.emit_load_local(slot, ir, php));
                }
                builder.emit(
                    op,
                    operands,
                    None,
                    IrType::I64,
                    PhpType::Bool,
                    Ownership::NonHeap,
                );
                builder.terminate(Terminator::Return { value: None });
            }
            let inst = function
                .instructions
                .last()
                .expect("the probe emitted a comparison")
                .clone();
            loose_eq_shape_issue(&function, &inst)
        };
        // `!=` is the same gate negated, so both opcodes go through the same predicate.
        let probe = |left: (IrType, PhpType), right: (IrType, PhpType)| {
            let eq = probe_op(Op::LooseEq, left.clone(), right.clone());
            let ne = probe_op(Op::LooseNotEq, left, right);
            assert_eq!(eq.is_some(), ne.is_some(), "== and != must gate alike");
            eq
        };

        let int = (IrType::I64, PhpType::Int);
        let boolean = (IrType::I64, PhpType::Bool);
        let float = (IrType::F64, PhpType::Float);
        let string = (IrType::Str, PhpType::Str);

        for pair in [
            (int.clone(), int.clone()),
            (boolean.clone(), boolean.clone()),
            (float.clone(), float.clone()),
            (string.clone(), string.clone()),
            (int.clone(), float.clone()),
            (float.clone(), int.clone()),
        ] {
            assert_eq!(probe(pair.0.clone(), pair.1.clone()), None, "{pair:?}");
        }

        // A bool against a number casts the NUMBER to bool, which is a different answer from
        // comparing the two words.
        assert!(probe(boolean.clone(), int.clone()).is_some());
        assert!(probe(int.clone(), boolean.clone()).is_some());
        // A string against a number, and anything boxed, still need their measured tables.
        assert!(probe(string.clone(), int.clone()).is_some());
        assert!(probe(float.clone(), string.clone()).is_some());
        assert!(probe(
            (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed),
            (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed)
        )
        .is_some());
    }

    /// Runs the production capability-and-planning gate for executable output.
    fn validate_module(module: &Module) -> Result<LoweredWasmPlan, WasmError> {
        validate_and_plan(module, Emit::Executable)
    }

    /// Builds the minimal resolved class metadata needed by WASM planning tests.
    fn minimal_class_info(class_id: u64) -> ClassInfo {
        ClassInfo {
            class_id,
            declaration_span: Span::dummy(),
            parent: None,
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            allow_dynamic_properties: false,
            constants: HashMap::new(),
            constant_types: HashMap::new(),
            constant_visibilities: HashMap::new(),
            final_constants: HashSet::new(),
            attribute_names: Vec::new(),
            attribute_args: Vec::new(),
            method_attribute_names: HashMap::new(),
            method_attribute_args: HashMap::new(),
            property_attribute_names: HashMap::new(),
            property_attribute_args: HashMap::new(),
            constant_attribute_names: HashMap::new(),
            constant_attribute_args: HashMap::new(),
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
            properties: Vec::new(),
            property_offsets: HashMap::new(),
            property_declaring_classes: HashMap::new(),
            defaults: Vec::new(),
            property_visibilities: HashMap::new(),
            property_set_visibilities: HashMap::new(),
            declared_properties: HashSet::new(),
            property_declared_slots: Vec::new(),
            final_properties: HashSet::new(),
            readonly_properties: HashSet::new(),
            reference_properties: HashSet::new(),
            owned_reference_properties: HashSet::new(),
            promoted_properties: HashSet::new(),
            property_reference_slots: Vec::new(),
            abstract_properties: HashSet::new(),
            abstract_property_hooks: HashMap::new(),
            static_properties: Vec::new(),
            static_defaults: Vec::new(),
            static_property_declaring_classes: HashMap::new(),
            static_property_visibilities: HashMap::new(),
            declared_static_properties: HashSet::new(),
            final_static_properties: HashSet::new(),
            method_decls: Vec::new(),
            methods: HashMap::new(),
            static_methods: HashMap::new(),
            late_static_method_returns: HashMap::new(),
            late_static_static_method_returns: HashMap::new(),
            callable_method_return_sigs: HashMap::new(),
            callable_array_method_return_sigs: HashMap::new(),
            method_visibilities: HashMap::new(),
            final_methods: HashSet::new(),
            method_declaring_classes: HashMap::new(),
            method_impl_classes: HashMap::new(),
            vtable_methods: Vec::new(),
            vtable_slots: HashMap::new(),
            static_method_visibilities: HashMap::new(),
            final_static_methods: HashSet::new(),
            static_method_declaring_classes: HashMap::new(),
            static_method_impl_classes: HashMap::new(),
            static_vtable_methods: Vec::new(),
            static_vtable_slots: HashMap::new(),
            interfaces: Vec::new(),
            constructor_param_to_prop: Vec::new(),
        }
    }

    /// Builds a declared no-argument void method signature.
    fn void_signature() -> FunctionSig {
        FunctionSig {
            params: Vec::new(),
            param_type_exprs: Vec::new(),
            param_attributes: Vec::new(),
            defaults: Vec::new(),
            return_type: PhpType::Void,
            declared_return: true,
            by_ref_return: false,
            ref_params: Vec::new(),
            declared_params: Vec::new(),
            variadic: None,
            deprecation: None,
        }
    }

    /// Builds a declared one-integer-to-integer method signature.
    fn int_method_signature() -> FunctionSig {
        FunctionSig {
            params: vec![("value".to_string(), PhpType::Int)],
            param_type_exprs: Vec::new(),
            param_attributes: Vec::new(),
            defaults: Vec::new(),
            return_type: PhpType::Int,
            declared_return: true,
            by_ref_return: false,
            ref_params: vec![false],
            declared_params: vec![true],
            variadic: None,
            deprecation: None,
        }
    }

    /// Builds an instance-method body with the hidden object receiver followed
    /// by the declared user parameters.
    fn method_body(class_name: &str, method_name: &str, signature: &FunctionSig) -> Function {
        let mut body = Function::new(
            format!("{class_name}::{method_name}"),
            IrType::Void,
            PhpType::Void,
        );
        body.flags.is_method = true;
        body.params.push(FunctionParam {
            name: "this".to_string(),
            ir_type: IrType::Heap(IrHeapKind::Object),
            php_type: PhpType::Object(class_name.to_string()),
            by_ref: false,
            variadic: false,
        });
        body.params
            .extend(signature.params.iter().map(|(name, php_type)| FunctionParam {
                name: name.clone(),
                ir_type: match php_type.codegen_repr() {
                    PhpType::Int | PhpType::Bool => IrType::I64,
                    other => panic!("unsupported test parameter type {other:?}"),
                },
                php_type: php_type.clone(),
                by_ref: false,
                variadic: false,
            }));
        body
    }

    /// Builds one dynamic Mixed receiver method call and returns its capability
    /// issue without requiring the surrounding test EIR to be otherwise complete.
    fn mixed_method_issue(module: &Module, method_data: DataId) -> Option<String> {
        let mut caller = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        let instruction_index = caller.instructions.len();
        {
            let mut builder = Builder::new(&mut caller);
            let entry = builder.create_named_block(
                "entry",
                vec![(IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed)],
            );
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let receiver = builder.block_param(entry, 0);
            let _ = builder.emit(
                Op::MethodCall,
                vec![receiver],
                Some(Immediate::Data(method_data)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
        }
        super::method_call_shape_issue(
            module,
            &caller,
            0,
            &caller.instructions[instruction_index],
        )
    }

    /// Rejects a dynamic call when a concrete runtime class has the requested
    /// method but a different arity, instead of misreporting it as undefined.
    #[test]
    fn mixed_method_capability_sees_concrete_arity_mismatches() {
        let method_name = "run";
        let method_key = crate::names::php_symbol_key(method_name);
        let mut module = Module::new(Target::wasm());
        let method_data = module.data.intern_string(method_name);

        let mut zero = minimal_class_info(2);
        zero.methods.insert(method_key.clone(), void_signature());
        zero.method_impl_classes
            .insert(method_key.clone(), "Zero".to_string());
        module.class_infos.insert("Zero".to_string(), zero);
        module
            .class_methods
            .push(method_body("Zero", method_name, &void_signature()));

        let mut one_signature = void_signature();
        one_signature
            .params
            .push(("value".to_string(), PhpType::Int));
        one_signature.defaults.push(None);
        one_signature.ref_params.push(false);
        one_signature.declared_params.push(true);
        let mut one = minimal_class_info(1);
        one.methods
            .insert(method_key.clone(), one_signature.clone());
        one.method_impl_classes
            .insert(method_key, "One".to_string());
        module.class_infos.insert("One".to_string(), one);
        module
            .class_methods
            .push(method_body("One", method_name, &one_signature));

        let issue = mixed_method_issue(&module, method_data)
            .expect("different concrete arity must fail the pre-emission gate");
        assert!(
            issue.contains("One: run expects 1 arguments, got 0"),
            "{issue}"
        );
    }

    /// Rejects a non-public dynamic Mixed candidate before its exact direct
    /// implementation call could bypass PHP visibility.
    #[test]
    fn mixed_method_capability_rejects_non_public_candidates() {
        let method_name = "run";
        let method_key = crate::names::php_symbol_key(method_name);
        let mut module = Module::new(Target::wasm());
        let method_data = module.data.intern_string(method_name);
        let mut class_info = minimal_class_info(1);
        class_info
            .methods
            .insert(method_key.clone(), void_signature());
        class_info
            .method_impl_classes
            .insert(method_key.clone(), "Secret".to_string());
        class_info
            .method_visibilities
            .insert(method_key, Visibility::Private);
        module.class_infos.insert("Secret".to_string(), class_info);

        let issue = mixed_method_issue(&module, method_data)
            .expect("private candidate must fail the pre-emission gate");
        assert!(issue.contains("unsupported Private visibility"), "{issue}");
    }

    /// Rejects a pointer-backed `Object|null` receiver unless the direct method
    /// call is dominated by the false edge of `IsNull(receiver)`.
    #[test]
    fn method_capability_rejects_unguarded_exact_nullable_object_receiver() {
        let class_name = "NullableReceiver";
        let method_name = "run";
        let method_key = crate::names::php_symbol_key(method_name);
        let mut module = Module::new(Target::wasm());
        let method_data = module.data.intern_string(method_name);
        let mut class_info = minimal_class_info(1);
        class_info
            .methods
            .insert(method_key.clone(), void_signature());
        class_info
            .method_impl_classes
            .insert(method_key, class_name.to_string());
        module
            .class_infos
            .insert(class_name.to_string(), class_info);
        module
            .class_methods
            .push(method_body(class_name, method_name, &void_signature()));

        let nullable = PhpType::Union(vec![
            PhpType::Object(class_name.to_string()),
            PhpType::Void,
        ]);
        let mut caller = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        let instruction_index = caller.instructions.len();
        {
            let mut builder = Builder::new(&mut caller);
            let entry = builder.create_named_block(
                "entry",
                vec![(IrType::Heap(IrHeapKind::Object), nullable)],
            );
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let receiver = builder.block_param(entry, 0);
            let _ = builder.emit(
                Op::MethodCall,
                vec![receiver],
                Some(Immediate::Data(method_data)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
        }

        let issue = super::method_call_shape_issue(
            &module,
            &caller,
            0,
            &caller.instructions[instruction_index],
        )
        .expect("unguarded exact nullable object must fail capability");
        assert!(
            issue.contains("lacks a dominating IsNull false-edge proof"),
            "{issue}"
        );
    }

    /// Excludes abstract classes from the runtime Mixed dispatch set because no
    /// valid PHP object can carry their class id.
    #[test]
    fn mixed_method_candidates_exclude_abstract_classes() {
        let method_key = crate::names::php_symbol_key("run");
        let mut module = Module::new(Target::wasm());
        let mut abstract_class = minimal_class_info(1);
        abstract_class.is_abstract = true;
        abstract_class
            .methods
            .insert(method_key.clone(), void_signature());
        module
            .class_infos
            .insert("AbstractBase".to_string(), abstract_class);
        let mut concrete_class = minimal_class_info(2);
        concrete_class
            .methods
            .insert(method_key.clone(), void_signature());
        module
            .class_infos
            .insert("Concrete".to_string(), concrete_class);

        let candidates = super::super::classes::mixed_method_candidates(&module, &method_key);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].1, "Concrete");
    }

    /// Rejects a same-width callable parameter hidden behind an integer method
    /// signature before direct, static, or dynamic lowering can raw-copy it.
    #[test]
    fn method_body_contract_rejects_same_width_callable_parameter_drift() {
        let mut body =
            Function::new("C::f".to_string(), IrType::I64, PhpType::Int);
        body.params.push(FunctionParam {
            name: "called_class".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: false,
            variadic: false,
        });
        body.params.push(FunctionParam {
            name: "value".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Callable,
            by_ref: false,
            variadic: false,
        });

        let issue = super::method_body_signature_shape_issue(
            &body,
            &int_method_signature(),
            IrType::I64,
        )
        .expect("same-width PHP metadata drift must be rejected");
        assert!(issue.contains("parameter #0"));
        assert!(issue.contains("Callable"));
    }

    /// Rejects a same-width callable return hidden behind an integer method
    /// signature before direct storage or dynamic Mixed boxing.
    #[test]
    fn method_body_contract_rejects_same_width_callable_return_drift() {
        let mut body =
            Function::new("C::f".to_string(), IrType::I64, PhpType::Callable);
        body.params.push(FunctionParam {
            name: "called_class".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: false,
            variadic: false,
        });
        body.params.push(FunctionParam {
            name: "value".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: false,
            variadic: false,
        });

        let issue = super::method_body_signature_shape_issue(
            &body,
            &int_method_signature(),
            IrType::I64,
        )
        .expect("same-width PHP return metadata drift must be rejected");
        assert!(issue.contains("return"));
        assert!(issue.contains("Callable"));
    }

    /// Verifies a typed virtual call is rejected before lowering when any
    /// concrete class in the shared dispatch subtree lacks its resolved body.
    #[test]
    fn rejects_virtual_dispatch_with_missing_descendant_body() {
        let method_key = crate::names::php_symbol_key("run");
        let mut module = Module::new(Target::wasm());
        let class_data = module.data.intern_class_name("Base");
        let method_data = module.data.intern_string("run");

        let mut base = minimal_class_info(1);
        base.methods.insert(method_key.clone(), void_signature());
        base.method_impl_classes
            .insert(method_key.clone(), "Base".to_string());
        base.vtable_slots.insert(method_key.clone(), 0);
        module.class_infos.insert("Base".to_string(), base);

        let mut child = minimal_class_info(2);
        child.parent = Some("Base".to_string());
        child.methods.insert(method_key.clone(), void_signature());
        child
            .method_impl_classes
            .insert(method_key.clone(), "Child".to_string());
        child.vtable_slots.insert(method_key, 0);
        module.class_infos.insert("Child".to_string(), child);

        let mut base_body =
            Function::new("Base::run".to_string(), IrType::Void, PhpType::Void);
        base_body.flags.is_method = true;
        base_body.params.push(FunctionParam {
            name: "this".to_string(),
            ir_type: IrType::Heap(IrHeapKind::Object),
            php_type: PhpType::Object("Base".to_string()),
            by_ref: false,
            variadic: false,
        });
        {
            let mut builder = Builder::new(&mut base_body);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            builder.terminate(Terminator::Return { value: None });
        }
        module.class_methods.push(base_body);

        let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut main);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let receiver = builder
                .emit(
                    Op::ObjectNew,
                    Vec::new(),
                    Some(Immediate::Data(class_data)),
                    IrType::Heap(IrHeapKind::Object),
                    PhpType::Object("Base".to_string()),
                    Ownership::Owned,
                )
                .expect("object allocation result");
            let _ = builder.emit(
                Op::MethodCall,
                vec![receiver],
                Some(Immediate::Data(method_data)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(main);

        let error = validate_module(&module)
            .expect_err("incomplete virtual dispatch subtree must fail capability");
        assert!(
            error
                .to_string()
                .contains("missing method body Child::run for dynamic candidate Child"),
            "{error}"
        );
    }

    /// Builds one terminated void function suitable for collection-audit tests.
    fn void_function(name: &str) -> Function {
        let mut function = Function::new(name.to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            builder.terminate(Terminator::Return { value: None });
        }
        function
    }

    /// Builds a malformed branch from a Mixed cell into tagged-scalar storage.
    fn invalid_mixed_transfer_module() -> Module {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("invalid_mixed_transfer".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block(
                "entry",
                vec![(IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed)],
            );
            let target = builder.create_named_block(
                "target",
                vec![(IrType::TaggedScalar, PhpType::TaggedScalar)],
            );
            builder.set_entry(entry);
            let mixed = builder.block_param(entry, 0);
            builder.position_at_end(entry);
            builder.terminate(Terminator::Br {
                target,
                args: vec![mixed],
            });
            builder.position_at_end(target);
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);
        module
    }

    /// Builds a module whose `ConstStr` references no real literal.
    fn invalid_const_str_module() -> Module {
        let mut module = Module::new(Target::wasm());
        let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut main);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::ConstStr,
                Vec::new(),
                Some(Immediate::Data(DataId::from_raw(99))),
                IrType::Str,
                PhpType::Str,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(main);
        module
    }

    /// Builds a closure whose recorded capture count exceeds its parameters.
    fn invalid_capture_count_module() -> Module {
        let mut module = Module::new(Target::wasm());
        let mut closure = void_function("__eir_closure_invalid_capture_0");
        closure.flags.is_closure = true;
        closure.flags.closure_capture_count = 1;
        module.add_closure(closure);
        module
    }

    /// Builds class metadata with a destructor implementation that no class declares.
    fn stale_destructor_metadata_module() -> Module {
        let mut module = Module::new(Target::wasm());
        let mut class = minimal_class_info(1);
        class.method_impl_classes.insert(
            crate::names::php_symbol_key("__destruct"),
            "MissingDestructorOwner".to_string(),
        );
        module.class_infos.insert("Victim".to_string(), class);
        module
    }

    /// Builds an FCC target whose by-reference parameter cannot be wrapped.
    fn invalid_fcc_wrapper_module() -> Module {
        let mut module = Module::new(Target::wasm());
        let target_name = module.data.intern_string("bad_fcc");
        let mut target = Function::new("bad_fcc".to_string(), IrType::Void, PhpType::Void);
        target.params.push(FunctionParam {
            name: "value".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: true,
            variadic: false,
        });
        target.add_local(
            Some("value".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        {
            let mut builder = Builder::new(&mut target);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(target);

        let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut main);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::FirstClassCallableNew,
                Vec::new(),
                Some(Immediate::Data(target_name)),
                IrType::I64,
                PhpType::Callable,
                Ownership::Owned,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(main);
        module
    }

    /// Builds a dynamic-looking closure call whose body has an unsupported
    /// visible Mixed parameter; `ClosureNew` must reject it before planning.
    fn invalid_masked_closure_wrapper_param_module() -> Module {
        let mut module = Module::new(Target::wasm());
        let closure_name = "__eir_closure_bad_visible_0";
        let closure_name_id = module.data.intern_string(closure_name);
        let mut closure =
            Function::new(closure_name.to_string(), IrType::I64, PhpType::Int);
        closure.flags.is_closure = true;
        closure.params.push(FunctionParam {
            name: "value".to_string(),
            // A Mixed parameter is SUPPORTED now — the argument buffer already holds cells —
            // so the fixture uses a storage that still has no unboxing: a tagged scalar.
            ir_type: IrType::TaggedScalar,
            php_type: PhpType::TaggedScalar,
            by_ref: false,
            variadic: false,
        });
        {
            let mut builder = Builder::new(&mut closure);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let result = builder.emit_const_i64(1);
            builder.terminate(Terminator::Return {
                value: Some(result),
            });
        }
        module.add_closure(closure);

        let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        let callable_slot = main.add_local(
            Some("callable".to_string()),
            IrType::I64,
            PhpType::Callable,
            LocalKind::PhpLocal,
        );
        {
            let mut builder = Builder::new(&mut main);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let descriptor = builder
                .emit(
                    Op::ClosureNew,
                    Vec::new(),
                    Some(Immediate::Data(closure_name_id)),
                    IrType::I64,
                    PhpType::Callable,
                    Ownership::Owned,
                )
                .expect("closure descriptor");
            builder.emit_store_local(callable_slot, descriptor);
            let masked =
                builder.emit_load_local(callable_slot, IrType::I64, PhpType::Callable);
            let argument = builder.emit_const_i64(1);
            let _ = builder.emit(
                Op::ClosureCall,
                vec![masked, argument],
                None,
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(main);
        module
    }

    /// Builds a closure whose tagged-scalar return cannot be boxed by the
    /// generated callable wrapper.
    fn invalid_closure_wrapper_return_module() -> Module {
        let mut module = Module::new(Target::wasm());
        let closure_name = "__eir_closure_bad_return_0";
        let closure_name_id = module.data.intern_string(closure_name);
        let mut closure = Function::new(
            closure_name.to_string(),
            IrType::TaggedScalar,
            PhpType::TaggedScalar,
        );
        closure.flags.is_closure = true;
        module.add_closure(closure);

        let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut main);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::ClosureNew,
                Vec::new(),
                Some(Immediate::Data(closure_name_id)),
                IrType::I64,
                PhpType::Callable,
                Ownership::Owned,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(main);
        module
    }

    /// Builds closure calls whose Mixed and null-sentinel arguments are outside
    /// the exact boxing surface used by `lower_closure_call`.
    fn invalid_closure_call_argument_boxing_module() -> Module {
        let mut module = Module::new(Target::wasm());
        let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut main);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let callable = builder
                .emit(
                    Op::ConstI64,
                    Vec::new(),
                    Some(Immediate::I64(0)),
                    IrType::I64,
                    PhpType::Callable,
                    Ownership::NonHeap,
                )
                .expect("dynamic callable placeholder");
            let integer = builder.emit_const_i64(1);
            let mixed = builder
                .emit(
                    Op::MixedBox,
                    vec![integer],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .expect("mixed argument");
            let null = builder.emit_const_null();
            for argument in [mixed, null] {
                let _ = builder.emit(
                    Op::ClosureCall,
                    vec![callable, argument],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(main);
        module
    }

    /// Builds a branch whose argument count differs from the target parameters.
    fn invalid_branch_arity_module() -> Module {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("invalid_branch_arity".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            let target =
                builder.create_named_block("target", vec![(IrType::I64, PhpType::Int)]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            builder.terminate(Terminator::Br {
                target,
                args: Vec::new(),
            });
            builder.position_at_end(target);
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);
        module
    }

    /// Builds one valid module combining class/destructor, closure, and FCC planning.
    fn combined_plan_module() -> Module {
        let mut module = Module::new(Target::wasm());
        let destruct_key = crate::names::php_symbol_key("__destruct");
        let mut class = minimal_class_info(1);
        class
            .methods
            .insert(destruct_key.clone(), void_signature());
        class
            .method_impl_classes
            .insert(destruct_key, "PlanClass".to_string());
        module.class_infos.insert("PlanClass".to_string(), class);

        let mut destructor = Function::new(
            "PlanClass::__destruct".to_string(),
            IrType::Void,
            PhpType::Void,
        );
        destructor.flags.is_method = true;
        destructor.params.push(FunctionParam {
            name: "this".to_string(),
            ir_type: IrType::Heap(IrHeapKind::Object),
            php_type: PhpType::Object("PlanClass".to_string()),
            by_ref: false,
            variadic: false,
        });
        destructor.add_local(
            Some("this".to_string()),
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object("PlanClass".to_string()),
            LocalKind::PhpLocal,
        );
        {
            let mut builder = Builder::new(&mut destructor);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            builder.terminate(Terminator::Return { value: None });
        }
        module.class_methods.push(destructor);

        let mut closure = void_function("__eir_closure_plan_combo_0");
        closure.flags.is_closure = true;
        module.add_closure(closure);

        let fcc_name = module.data.intern_string("plan_fcc");
        module.add_function(void_function("plan_fcc"));

        let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut main);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::FirstClassCallableNew,
                Vec::new(),
                Some(Immediate::Data(fcc_name)),
                IrType::I64,
                PhpType::Callable,
                Ownership::Owned,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(main);
        module
    }

    /// Adds a minimal closure whose only capture parameter is an int passed by
    /// reference, returning the interned closure-name data id.
    fn add_by_ref_int_closure(module: &mut Module, name: &str) -> crate::ir::DataId {
        let name_id = module.data.intern_string(name);
        let mut closure = Function::new(name.to_string(), IrType::Void, PhpType::Void);
        closure.flags.is_closure = true;
        closure.flags.closure_capture_count = 1;
        closure.params.push(FunctionParam {
            name: "captured".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: true,
            variadic: false,
        });
        closure.add_local(
            Some("captured".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        {
            let mut builder = Builder::new(&mut closure);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_closure(closure);
        name_id
    }

    /// Emits one closure descriptor creation from `capture` in the positioned
    /// builder and returns the callable value.
    fn emit_by_ref_closure_new(
        builder: &mut Builder<'_>,
        capture: crate::ir::ValueId,
        name_id: crate::ir::DataId,
    ) -> crate::ir::ValueId {
        builder
            .emit(
                Op::ClosureNew,
                vec![capture],
                Some(Immediate::Data(name_id)),
                IrType::I64,
                PhpType::Callable,
                Ownership::Owned,
            )
            .expect("closure descriptor")
    }

    /// Verifies capability validation rejects a direct call whose lowered
    /// operand count cannot satisfy the resolved target's exact WAT signature.
    #[test]
    fn direct_call_shape_rejects_arity_mismatch_before_lowering() {
        let mut module = Module::new(Target::wasm());
        let name = module.data.intern_function_name("Target");
        let mut target = Function::new("target".to_string(), IrType::Void, PhpType::Void);
        target.params.push(FunctionParam {
            name: "value".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: false,
            variadic: false,
        });
        module.add_function(target);
        let owner = void_function("owner");
        let call = crate::ir::Instruction::new(
            Op::Call,
            Vec::new(),
            Some(Immediate::Data(name)),
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::Call.default_effects(),
            None,
        );

        let issue = super::direct_call_shape_issue(
            &module,
            &owner,
            &call,
            &super::RefCellProvenance::default(),
        )
        .expect("arity mismatch");

        assert!(issue.contains("expects 1 lowered operands, got 0"), "{issue}");
    }

    /// Verifies a concrete heap pointer cannot cross a direct-call boundary
    /// when its PHP payload layout differs from the resolved parameter.
    #[test]
    fn direct_call_shape_rejects_array_payload_type_mismatch() {
        let mut module = Module::new(Target::wasm());
        let name = module.data.intern_function_name("consume_strings");
        let mut target =
            Function::new("consume_strings".to_string(), IrType::Void, PhpType::Void);
        target.params.push(FunctionParam {
            name: "values".to_string(),
            ir_type: IrType::Heap(IrHeapKind::Array),
            php_type: PhpType::Array(Box::new(PhpType::Str)),
            by_ref: false,
            variadic: false,
        });
        module.add_function(target);
        let mut owner = Function::new("owner".to_string(), IrType::Void, PhpType::Void);
        let argument;
        {
            let mut builder = Builder::new(&mut owner);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            argument = builder
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::I64(0)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Owned,
                )
                .expect("array argument");
            builder.terminate(Terminator::Return { value: None });
        }
        let call = crate::ir::Instruction::new(
            Op::Call,
            vec![argument],
            Some(Immediate::Data(name)),
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::Call.default_effects(),
            None,
        );

        let issue = super::direct_call_shape_issue(
            &module,
            &owner,
            &call,
            &super::collect_ref_cell_provenance(&owner),
        )
        .expect("array payload mismatch");

        assert!(issue.contains("unsupported wasm value transfer"), "{issue}");
    }

    /// Verifies a direct call cannot route a Mixed cell through the generic
    /// integer cast path when the callee expects a callable descriptor.
    #[test]
    fn direct_call_shape_rejects_generic_mixed_to_callable_unboxing() {
        let mut module = Module::new(Target::wasm());
        let name = module.data.intern_function_name("invoke");
        let mut target = Function::new("invoke".to_string(), IrType::Void, PhpType::Void);
        target.params.push(FunctionParam {
            name: "callback".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Callable,
            by_ref: false,
            variadic: false,
        });
        module.add_function(target);
        let mut owner = Function::new("owner".to_string(), IrType::Void, PhpType::Void);
        let argument;
        {
            let mut builder = Builder::new(&mut owner);
            let entry = builder.create_named_block(
                "entry",
                vec![(IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed)],
            );
            builder.set_entry(entry);
            argument = builder.block_param(entry, 0);
            builder.position_at_end(entry);
            builder.terminate(Terminator::Return { value: None });
        }
        let call = crate::ir::Instruction::new(
            Op::Call,
            vec![argument],
            Some(Immediate::Data(name)),
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::Call.default_effects(),
            None,
        );

        let issue = super::direct_call_shape_issue(
            &module,
            &owner,
            &call,
            &super::RefCellProvenance::default(),
        )
        .expect("generic Mixed-to-callable transfer must be rejected");

        assert!(issue.contains("unboxing a Mixed cell"), "{issue}");
        assert!(issue.contains("Callable"), "{issue}");
    }

    /// Verifies a malformed `LoadRefCell` cannot reach direct-call emission
    /// unless the slot was registered as an owned or borrowed ref binding.
    #[test]
    fn direct_call_shape_rejects_unregistered_ref_cell_operand() {
        let mut module = Module::new(Target::wasm());
        let name = module.data.intern_function_name("mutate");
        let mut target = Function::new("mutate".to_string(), IrType::Void, PhpType::Void);
        target.params.push(FunctionParam {
            name: "value".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: true,
            variadic: false,
        });
        module.add_function(target);
        let mut owner = Function::new("owner".to_string(), IrType::Void, PhpType::Void);
        let slot = owner.add_local(
            Some("value".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let argument;
        {
            let mut builder = Builder::new(&mut owner);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            argument = builder
                .emit(
                    Op::LoadRefCell,
                    Vec::new(),
                    Some(Immediate::LocalSlot(slot)),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .expect("malformed ref-cell load");
            builder.terminate(Terminator::Return { value: None });
        }
        let call = crate::ir::Instruction::new(
            Op::Call,
            vec![argument],
            Some(Immediate::Data(name)),
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::Call.default_effects(),
            None,
        );

        let issue = super::direct_call_shape_issue(
            &module,
            &owner,
            &call,
            &super::collect_ref_cell_provenance(&owner),
        )
        .expect("unregistered ref cell");

        assert!(issue.contains("unregistered ref-cell local"), "{issue}");
    }

    /// Verifies a by-reference `LoadLocal` cannot name a slot absent from the
    /// function's local table, even when its result metadata matches the callee.
    #[test]
    fn direct_call_shape_rejects_missing_fresh_local_slot() {
        let mut module = Module::new(Target::wasm());
        let name = module.data.intern_function_name("mutate_missing");
        let mut target =
            Function::new("mutate_missing".to_string(), IrType::Void, PhpType::Void);
        target.params.push(FunctionParam {
            name: "value".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: true,
            variadic: false,
        });
        module.add_function(target);
        let mut owner = Function::new("owner".to_string(), IrType::Void, PhpType::Void);
        let missing = crate::ir::LocalSlotId::from_raw(99);
        let argument;
        {
            let mut builder = Builder::new(&mut owner);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            argument = builder
                .emit(
                    Op::LoadLocal,
                    Vec::new(),
                    Some(Immediate::LocalSlot(missing)),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .expect("malformed local load");
            builder.terminate(Terminator::Return { value: None });
        }
        let call = crate::ir::Instruction::new(
            Op::Call,
            vec![argument],
            Some(Immediate::Data(name)),
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::Call.default_effects(),
            None,
        );

        let issue = super::direct_call_shape_issue(
            &module,
            &owner,
            &call,
            &super::RefCellProvenance::default(),
        )
        .expect("missing local must fail");

        assert!(issue.contains("local#99 is missing"), "{issue}");
    }

    /// Verifies both fresh and already-bound by-reference sources reject local
    /// metadata that differs from their loaded value and callee parameter.
    #[test]
    fn by_ref_sources_require_exact_slot_value_parameter_metadata() {
        let mut owner = Function::new("owner".to_string(), IrType::Void, PhpType::Void);
        let slot = owner.add_local(
            Some("value".to_string()),
            IrType::I64,
            PhpType::Callable,
            LocalKind::PhpLocal,
        );
        let fresh;
        let bound;
        {
            let mut builder = Builder::new(&mut owner);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            fresh = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
            bound = builder
                .emit(
                    Op::LoadRefCell,
                    Vec::new(),
                    Some(Immediate::LocalSlot(slot)),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .expect("ref-cell load");
            builder.terminate(Terminator::Return { value: None });
        }
        let parameter = FunctionParam {
            name: "value".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: true,
            variadic: false,
        };

        for (value_id, source) in [
            (fresh, super::ByRefSource::FreshLocal(slot)),
            (
                bound,
                super::ByRefSource::AlreadyRefBound(slot.as_raw()),
            ),
        ] {
            let value = owner.value(value_id).expect("loaded value");
            let issue = super::by_ref_source_shape_issue(&owner, value, &parameter, source)
                .expect("metadata drift must fail");
            assert!(issue.contains("must match exactly"), "{issue}");
            assert!(issue.contains("Callable"), "{issue}");
        }
    }

    /// Verifies a by-reference parameter's borrowed cell can be forwarded to
    /// another by-reference callee without inventing a local owner.
    #[test]
    fn direct_call_shape_accepts_registered_borrowed_ref_cell_operand() {
        let mut module = Module::new(Target::wasm());
        let name = module.data.intern_function_name("forwarded_mutation");
        let mut target =
            Function::new("forwarded_mutation".to_string(), IrType::Void, PhpType::Void);
        target.params.push(FunctionParam {
            name: "value".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: true,
            variadic: false,
        });
        module.add_function(target);
        let mut owner = Function::new("owner".to_string(), IrType::Void, PhpType::Void);
        owner.params.push(FunctionParam {
            name: "value".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: true,
            variadic: false,
        });
        let slot = owner.add_local(
            Some("value".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let argument;
        {
            let mut builder = Builder::new(&mut owner);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            argument = builder
                .emit(
                    Op::LoadRefCell,
                    Vec::new(),
                    Some(Immediate::LocalSlot(slot)),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .expect("borrowed ref-cell load");
            builder.terminate(Terminator::Return { value: None });
        }
        let call = crate::ir::Instruction::new(
            Op::Call,
            vec![argument],
            Some(Immediate::Data(name)),
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::Call.default_effects(),
            None,
        );

        let issue = super::direct_call_shape_issue(
            &module,
            &owner,
            &call,
            &super::collect_ref_cell_provenance(&owner),
        );

        assert!(issue.is_none(), "{issue:?}");
    }

    /// Verifies descriptor invocation rejects an indexed array whose elements
    /// are not Mixed cells, preventing the wrapper from interpreting 8-byte
    /// integer slots as 16-byte cell slots.
    #[test]
    fn descriptor_invoke_shape_rejects_non_mixed_argument_array() {
        let module = Module::new(Target::wasm());
        let mut owner = Function::new("owner".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut owner);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let callable = builder
                .emit(
                    Op::ConstI64,
                    Vec::new(),
                    Some(Immediate::I64(0)),
                    IrType::I64,
                    PhpType::Callable,
                    Ownership::NonHeap,
                )
                .expect("callable value");
            let arguments = builder
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::I64(0)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Owned,
                )
                .expect("argument array");
            let _ = builder.emit(
                Op::CallableDescriptorInvoke,
                vec![callable, arguments],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        let invoke = owner
            .instructions
            .iter()
            .find(|instruction| instruction.op == Op::CallableDescriptorInvoke)
            .expect("descriptor invoke");

        let issue = super::callable_descriptor_invoke_shape_issue(&module, &owner, invoke)
            .expect("array<int> must be rejected");

        assert!(issue.contains("array<mixed>"), "{issue}");
    }

    /// Verifies validation returns the exact lowered plan for an accepted module.
    #[test]
    fn accepted_module_returns_an_assemblable_plan() {
        let mut module = Module::new(Target::wasm());
        let mut main = void_function("main");
        main.flags.is_main = true;
        module.add_function(main);

        let wat = validate_module(&module)
            .expect("the capability gate should return the exact plan")
            .into_wat();

        assert!(wat.contains("(export \"_start\")"), "{wat}");
        let bytes =
            ::wat::parse_str(&wat).unwrap_or_else(|error| panic!("WAT did not assemble: {error}"));
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM3)
            .validate_all(&bytes)
            .unwrap_or_else(|error| panic!("WASM did not validate: {error}"));
    }

    /// Verifies one accepted plan combines classes, destructors, closures, and FCC.
    #[test]
    fn accepted_combined_module_plans_every_dispatch_surface() {
        let wat = validate_module(&combined_plan_module())
            .expect("the combined capability surface should produce one exact plan")
            .into_wat();
        let expected_symbols = [
            super::super::symbols::method_symbol("PlanClass::__destruct"),
            super::super::symbols::closure_body_symbol("__eir_closure_plan_combo_0"),
            super::super::symbols::closure_wrapper_symbol("__eir_closure_plan_combo_0"),
            super::super::symbols::fcc_wrapper_symbol("plan_fcc"),
        ];
        for symbol in expected_symbols {
            assert!(wat.contains(&format!("${symbol}")), "missing ${symbol}: {wat}");
        }
        let bytes =
            ::wat::parse_str(&wat).unwrap_or_else(|error| panic!("WAT did not assemble: {error}"));
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM3)
            .validate_all(&bytes)
            .unwrap_or_else(|error| panic!("WASM did not validate: {error}"));
    }

    /// Verifies an admitted opcode with a malformed shape fails inside validation.
    #[test]
    fn exact_lowering_failure_prevents_capability_acceptance() {
        let mut module = Module::new(Target::wasm());
        let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut main);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::ArrayNew,
                Vec::new(),
                None,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
                Ownership::Owned,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(main);

        let error =
            validate_module(&module).expect_err("malformed lowering must prevent plan acceptance");
        let message = error.to_string();
        assert!(message.contains("array_new without a capacity"), "{message}");
        assert!(
            !message.contains("WASM capability audit found"),
            "the exact lowerer error should be surfaced by the validation boundary: {message}"
        );
    }

    /// Verifies representative late lowerer defects all remain inside validation.
    #[test]
    fn exact_plan_rejects_every_representative_late_unsupported_shape() {
        let cases = [
            (invalid_const_str_module(), "unknown string literal"),
            (
                invalid_capture_count_module(),
                "capture_count 1 > params 0",
            ),
            (
                stale_destructor_metadata_module(),
                "class MissingDestructorOwner resolved as __destruct impl does not declare it",
            ),
        ];

        for (module, expected) in cases {
            let error = validate_module(&module)
                .expect_err("the malformed module must not produce an accepted plan");
            let message = error.to_string();
            assert!(message.contains(expected), "missing {expected:?}: {message}");
            assert!(
                !message.contains("WASM capability audit found"),
                "the exact plan boundary should surface {expected:?}: {message}"
            );
        }
    }

    /// Verifies malformed value and block transfers fail in the static audit.
    #[test]
    fn rejects_invalid_transfers_before_lowering() {
        let cases = [
            (
                invalid_mixed_transfer_module(),
                "unboxing a Mixed cell to TaggedScalar",
            ),
            (
                invalid_branch_arity_module(),
                "expects 1 arguments, got 0",
            ),
        ];

        for (module, expected) in cases {
            let error =
                validate_module(&module).expect_err("malformed transfer must fail capability");
            let message = error.to_string();
            assert!(message.contains(expected), "missing {expected:?}: {message}");
            assert!(
                message.contains("WASM capability audit found"),
                "transfer defect must not leak to exact planning: {message}"
            );
        }
    }

    /// Verifies an unsupported FCC wrapper is rejected by the static shape
    /// audit rather than leaking to the exact WAT planning boundary.
    #[test]
    fn rejects_invalid_fcc_wrapper_before_lowering() {
        let error = validate_module(&invalid_fcc_wrapper_module())
            .expect_err("by-reference FCC parameter must fail capability");
        let message = error.to_string();

        assert!(message.contains("WASM capability audit found 1 issue(s)"), "{message}");
        assert!(
            message.contains("callback parameter value is by-reference or variadic"),
            "{message}"
        );
    }

    /// Verifies every `ClosureNew` audits visible wrapper parameters even when a
    /// later local load masks the descriptor from static call resolution.
    #[test]
    fn rejects_masked_closure_wrapper_param_before_lowering() {
        let error = validate_module(&invalid_masked_closure_wrapper_param_module())
            .expect_err("unsupported visible closure parameter must fail capability");
        let message = error.to_string();

        assert!(
            message.contains("callback parameter value has unsupported storage"),
            "{message}"
        );
        assert!(
            message.contains("WASM capability audit found"),
            "wrapper defect must not leak to exact planning: {message}"
        );
    }

    /// Verifies every `ClosureNew` audits its result-boxing contract before the
    /// exact wrapper builder can return a late unsupported error.
    #[test]
    fn rejects_closure_wrapper_return_before_lowering() {
        let error = validate_module(&invalid_closure_wrapper_return_module())
            .expect_err("unsupported closure return must fail capability");
        let message = error.to_string();

        assert!(
            message.contains("callable return TaggedScalar/TaggedScalar cannot be boxed"),
            "{message}"
        );
        assert!(
            message.contains("WASM capability audit found"),
            "wrapper defect must not leak to exact planning: {message}"
        );
    }

    /// Verifies closure-call argument admission mirrors the specialized boxer,
    /// rejecting already-Mixed cells and I64 null sentinels at capability time.
    #[test]
    fn rejects_closure_call_arguments_outside_specialized_boxer() {
        let error = validate_module(&invalid_closure_call_argument_boxing_module())
            .expect_err("unsupported closure arguments must fail capability");
        let message = error.to_string();

        assert!(
            message.contains("WASM capability audit found 2 issue(s)"),
            "{message}"
        );
        assert!(
            message.contains("cannot be boxed from Heap(Mixed)/Mixed"),
            "{message}"
        );
        assert!(message.contains("cannot be boxed from I64/Void"), "{message}");
    }

    /// Verifies aggregate static diagnostics short-circuit exact planning.
    #[test]
    fn aggregate_audit_failure_precedes_exact_planning() {
        let mut module = Module::new(Target::wasm());
        let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        main.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut main);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::StrEq,
                Vec::new(),
                None,
                IrType::I64,
                PhpType::Bool,
                Ownership::NonHeap,
            );
            let _ = builder.emit(
                Op::ArrayNew,
                Vec::new(),
                None,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
                Ownership::Owned,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(main);

        let error = validate_module(&module)
            .expect_err("the aggregate audit must reject before exact planning");
        let message = error.to_string();
        assert!(message.contains("WASM capability audit found 1 issue(s)"), "{message}");
        assert!(message.contains("unsupported op str_eq"), "{message}");
        assert!(
            !message.contains("array_new without a capacity"),
            "planning must not mask the stable aggregate diagnostic: {message}"
        );
    }

    /// Verifies all non-emitted EIR function collections are named in one error.
    #[test]
    fn reports_every_non_emitted_function_collection() {
        let mut module = Module::new(Target::wasm());
        module.fiber_wrappers.push(void_function("fiber"));
        module.callback_wrappers.push(void_function("callback"));
        module
            .extern_callback_trampolines
            .push(void_function("extern_callback"));
        module
            .runtime_callable_invokers
            .push(void_function("runtime_invoker"));

        let error = validate_module(&module).expect_err("omitted collections must fail");
        let message = error.to_string();
        assert!(message.contains("fiber_wrappers::fiber"), "{message}");
        assert!(message.contains("callback_wrappers::callback"), "{message}");
        assert!(
            message.contains("extern_callback_trampolines::extern_callback"),
            "{message}"
        );
        assert!(
            message.contains("runtime_callable_invokers::runtime_invoker"),
            "{message}"
        );
        assert!(message.contains("4 issue(s)"), "{message}");
    }

    /// Verifies unsupported opcodes and typed runtime functions are aggregated.
    #[test]
    fn aggregates_instruction_and_runtime_gaps() {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::StrEq,
                Vec::new(),
                None,
                IrType::I64,
                PhpType::Bool,
                Ownership::NonHeap,
            );
            let _ = builder.emit(
                Op::RuntimeCall,
                Vec::new(),
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::Count,
                ))),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error = validate_module(&module).expect_err("unsupported instructions must fail");
        let message = error.to_string();
        assert!(message.contains("unsupported op str_eq"), "{message}");
        assert!(
            message.contains("unsupported runtime function count"),
            "{message}"
        );
        assert!(message.contains("2 issue(s)"), "{message}");
    }

    /// Verifies runtime functions with known PHP-visible divergences fail together
    /// in the pre-emission audit instead of reaching their partial lowerings.
    #[test]
    fn rejects_semantically_partial_runtime_functions_before_lowering() {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            for target in [
                RuntimeFnId::ArrayFilter,
                RuntimeFnId::Uasort,
                RuntimeFnId::Uksort,
                RuntimeFnId::ArrayWalk,
                RuntimeFnId::GetClass,
            ] {
                let _ = builder.emit(
                    Op::RuntimeCall,
                    Vec::new(),
                    Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))),
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error = validate_module(&module)
            .expect_err("known PHP-visible runtime divergences must fail before lowering");
        let message = error.to_string();
        for name in ["array_filter", "uasort", "uksort", "array_walk"] {
            assert!(
                message.contains(&format!("unsupported runtime function {name}")),
                "{message}"
            );
        }
        assert!(
            message.contains("unsupported runtime function get_class shape")
                && message.contains("expected one object operand"),
            "{message}"
        );
        assert!(message.contains("5 issue(s)"), "{message}");
    }

    /// Verifies admitted higher-order runtimes reject unproved callback,
    /// result, and initial-carry shapes during the aggregate capability audit.
    #[test]
    fn rejects_invalid_admitted_runtime_shapes_before_lowering() {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let callback = builder
                .emit(
                    Op::ConstI64,
                    Vec::new(),
                    Some(Immediate::I64(0)),
                    IrType::I64,
                    PhpType::Callable,
                    Ownership::NonHeap,
                )
                .expect("callback placeholder");
            let array = builder
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(0)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Owned,
                )
                .expect("array value");
            let _ = builder.emit(
                Op::RuntimeCall,
                vec![callback, array],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::ArrayMap,
                ))),
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Mixed)),
                Ownership::Owned,
            );
            let _ = builder.emit(
                Op::RuntimeCall,
                vec![array, callback],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::Usort,
                ))),
                IrType::F64,
                PhpType::Float,
                Ownership::NonHeap,
            );
            let _ = builder.emit(
                Op::RuntimeCall,
                vec![array, callback, callback],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::ArrayReduce,
                ))),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error = validate_module(&module).expect_err("invalid runtime shapes must fail");
        let message = error.to_string();
        assert!(
            message.contains("array_map shape")
                && message.contains("must resolve statically to a direct closure"),
            "{message}"
        );
        assert!(
            message.contains("usort shape") && message.contains("result must be bool/I64"),
            "{message}"
        );
        assert!(
            message.contains("array_reduce shape")
                && message.contains("initial carry must be an int/I64"),
            "{message}"
        );
    }

    /// Verifies the audited P0 instruction forms and main return contract are
    /// rejected by the capability gate rather than by WAT lowering or validation.
    #[test]
    fn rejects_invalid_p0_instruction_shapes_before_lowering() {
        let mut module = Module::new(Target::wasm());
        let method_data = module.data.intern_string("get");
        let mut function = Function::new("main".to_string(), IrType::I64, PhpType::Int);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let lhs = builder.emit_const_i64(1);
            let rhs = builder.emit_const_i64(2);
            let _ = builder.emit(
                Op::ICheckedAdd,
                vec![lhs, rhs],
                None,
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            );
            let _ = builder.emit(
                Op::Cast,
                vec![lhs],
                Some(Immediate::CastTarget(IrType::Str)),
                IrType::Str,
                PhpType::Str,
                Ownership::Owned,
            );
            let array = builder
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(1)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Owned,
                )
                .expect("array value");
            let _ = builder.emit(
                Op::ArrayGet,
                vec![array, lhs],
                None,
                IrType::F64,
                PhpType::Float,
                Ownership::NonHeap,
            );
            let object = builder
                .emit(
                    Op::ConstNull,
                    Vec::new(),
                    None,
                    IrType::Heap(IrHeapKind::Object),
                    PhpType::Object("P0Shape".to_string()),
                    Ownership::NonHeap,
                )
                .expect("object-shaped placeholder");
            let _ = builder.emit(
                Op::RuntimeCall,
                vec![object],
                Some(Immediate::RuntimeCall(
                    RuntimeCallTarget::ProfiledFunction {
                        target: RuntimeFnId::GetClass,
                        strict_php: true,
                    },
                )),
                IrType::F64,
                PhpType::Float,
                Ownership::NonHeap,
            );
            let mixed = builder
                .emit(
                    Op::MixedBox,
                    vec![lhs],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .expect("mixed receiver");
            let _ = builder.emit(
                Op::NullsafeMethodCall,
                vec![mixed],
                Some(Immediate::Data(method_data)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: Some(lhs) });
        }
        module.add_function(function);

        let error = validate_module(&module).expect_err("P0 shapes must fail at the gate");
        let message = error.to_string();
        for expected in [
            "main must declare a void return",
            "unsupported ichecked_add shape",
            "unsupported cast shape",
            "unsupported array_get shape",
            "unsupported runtime function get_class shape",
            "unsupported nullsafe_method_call shape",
            "main return value would be discarded",
        ] {
            assert!(message.contains(expected), "missing {expected:?}: {message}");
        }
    }

    /// Null-capable array reads are admitted only with the exact Tagged/Mixed
    /// result shapes, including the silent opcode used by null coalescing.
    #[test]
    fn accepts_nullable_array_get_shapes_including_silent_reads() {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("reads".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let index = builder.emit_const_i64(0);
            for (op, element, ir_type, php_type, ownership) in [
                (
                    Op::ArrayGet,
                    PhpType::Int,
                    IrType::TaggedScalar,
                    PhpType::TaggedScalar,
                    Ownership::NonHeap,
                ),
                (
                    Op::ArrayGet,
                    PhpType::Bool,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                ),
                (
                    Op::ArrayGetSilent,
                    PhpType::Str,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                ),
            ] {
                let array = builder
                    .emit(
                        Op::ArrayNew,
                        Vec::new(),
                        Some(Immediate::Capacity(1)),
                        IrType::Heap(IrHeapKind::Array),
                        PhpType::Array(Box::new(element)),
                        Ownership::Owned,
                    )
                    .expect("array value");
                let _ = builder.emit(
                    op,
                    vec![array, index],
                    None,
                    ir_type,
                    php_type,
                    ownership,
                );
            }
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let wat = validate_module(&module)
            .expect("nullable array-get shapes must pass the gate")
            .into_wat();
        assert_eq!(
            wat.matches("call $__rt_warn_undefined_array_key_int")
                .count(),
            2,
            "only the two normal reads should call the warning helper: {wat}"
        );
    }

    /// A warning-producing read cannot be admitted into an import-free reactor,
    /// because that module deliberately has no WASI stderr runtime.
    #[test]
    fn rejects_warning_array_get_without_command_runtime() {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("reactor_read".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let array = builder
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(1)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Owned,
                )
                .expect("array value");
            let index = builder.emit_const_i64(2);
            let _ = builder.emit(
                Op::ArrayGet,
                vec![array, index],
                None,
                IrType::TaggedScalar,
                PhpType::TaggedScalar,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error =
            validate_module(&module).expect_err("reactor warning read must fail capability");
        assert!(
            error
                .to_string()
                .contains("warning-producing indexed read requires a main-bearing command module"),
            "{error}"
        );
    }

    /// A silent nullable read remains valid in an import-free reactor and does
    /// not cause a warning helper or WASI import to appear in the rendered WAT.
    #[test]
    fn accepts_silent_array_get_without_command_runtime() {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("reactor_silent_read".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let array = builder
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(1)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Str)),
                    Ownership::Owned,
                )
                .expect("array value");
            let index = builder.emit_const_i64(2);
            let _ = builder.emit(
                Op::ArrayGetSilent,
                vec![array, index],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let wat = validate_module(&module)
            .expect("silent reactor read must remain supported")
            .into_wat();
        assert!(!wat.contains("__rt_warn_undefined_array_key_int"), "{wat}");
        assert!(!wat.contains("wasi_snapshot_preview1"), "{wat}");
    }

    /// Legacy non-null result shapes are rejected for both warning and silent
    /// reads so no sentinel, truthy bool, or empty-string alias can escape.
    #[test]
    fn rejects_non_nullable_array_get_result_shapes() {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("legacy_reads".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let index = builder.emit_const_i64(0);
            for (op, element, ir_type, php_type, ownership) in [
                (
                    Op::ArrayGet,
                    PhpType::Int,
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                ),
                (
                    Op::ArrayGet,
                    PhpType::Bool,
                    IrType::I64,
                    PhpType::Bool,
                    Ownership::NonHeap,
                ),
                (
                    Op::ArrayGetSilent,
                    PhpType::Str,
                    IrType::Str,
                    PhpType::Str,
                    Ownership::MaybeOwned,
                ),
            ] {
                let array = builder
                    .emit(
                        Op::ArrayNew,
                        Vec::new(),
                        Some(Immediate::Capacity(1)),
                        IrType::Heap(IrHeapKind::Array),
                        PhpType::Array(Box::new(element)),
                        Ownership::Owned,
                    )
                    .expect("array value");
                let _ = builder.emit(
                    op,
                    vec![array, index],
                    None,
                    ir_type,
                    php_type,
                    ownership,
                );
            }
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error =
            validate_module(&module).expect_err("legacy non-null result shapes must fail");
        let message = error.to_string();
        assert_eq!(message.matches("unsupported array_get shape").count(), 2, "{message}");
        assert_eq!(
            message
                .matches("unsupported array_get_silent shape")
                .count(),
            1,
            "{message}"
        );
    }

    /// Fresh Mixed cells returned for bool/string reads must remain tracked as
    /// releasable results, while tagged integer reads stay non-heap values.
    #[test]
    fn rejects_array_get_result_ownership_mismatches() {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("ownership_reads".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let index = builder.emit_const_i64(0);
            for (element, ir_type, php_type, ownership) in [
                (
                    PhpType::Int,
                    IrType::TaggedScalar,
                    PhpType::TaggedScalar,
                    Ownership::Borrowed,
                ),
                (
                    PhpType::Bool,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::NonHeap,
                ),
                (
                    PhpType::Str,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Persistent,
                ),
            ] {
                let array = builder
                    .emit(
                        Op::ArrayNew,
                        Vec::new(),
                        Some(Immediate::Capacity(1)),
                        IrType::Heap(IrHeapKind::Array),
                        PhpType::Array(Box::new(element)),
                        Ownership::Owned,
                    )
                    .expect("array value");
                let _ = builder.emit(
                    Op::ArrayGet,
                    vec![array, index],
                    None,
                    ir_type,
                    php_type,
                    ownership,
                );
            }
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error =
            validate_module(&module).expect_err("array-read ownership drift must fail");
        let message = error.to_string();
        assert_eq!(
            message.matches("result has incompatible ownership").count(),
            3,
            "{message}"
        );
    }

    /// Nullable associative reads accept tagged integers, boxed scalar results,
    /// and precise `container|null` pointers. Warning emission remains reserved
    /// for the non-silent opcode.
    #[test]
    fn accepts_nullable_hash_get_shapes_including_silent_reads() {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("hash_reads".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let key = builder.emit_const_i64(0);
            for (op, element, ir_type, php_type, ownership) in [
                (
                    Op::HashGet,
                    PhpType::Int,
                    IrType::TaggedScalar,
                    PhpType::TaggedScalar,
                    Ownership::NonHeap,
                ),
                (
                    Op::HashGet,
                    PhpType::Bool,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                ),
                (
                    Op::HashGetSilent,
                    PhpType::Float,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                ),
                (
                    Op::HashGet,
                    PhpType::Str,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::MaybeOwned,
                ),
                (
                    Op::HashGetSilent,
                    PhpType::Array(Box::new(PhpType::Int)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Union(vec![
                        PhpType::Array(Box::new(PhpType::Int)),
                        PhpType::Void,
                    ]),
                    Ownership::MaybeOwned,
                ),
            ] {
                let hash = builder
                    .emit(
                        Op::HashNew,
                        Vec::new(),
                        Some(Immediate::Capacity(1)),
                        IrType::Heap(IrHeapKind::Hash),
                        PhpType::AssocArray {
                            key: Box::new(PhpType::Int),
                            value: Box::new(element),
                        },
                        Ownership::Owned,
                    )
                    .expect("hash value");
                let _ = builder.emit(
                    op,
                    vec![hash, key],
                    None,
                    ir_type,
                    php_type,
                    ownership,
                );
            }
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let wat = validate_module(&module)
            .expect("nullable hash-get shapes must pass the gate")
            .into_wat();
        assert_eq!(
            wat.matches("call $__rt_warn_undefined_array_key_int")
                .count(),
            3,
            "only normal reads should call the integer warning arm: {wat}"
        );
        assert_eq!(
            wat.matches("call $__rt_warn_undefined_array_key_str")
                .count(),
            3,
            "only normal reads should call the string warning arm: {wat}"
        );
    }

    /// A nullable container pointer cannot feed a typed read unless the
    /// consumer is dominated by the non-null edge of `IsNull(source)`.
    #[test]
    fn rejects_unguarded_nullable_container_consumer() {
        let mut module = Module::new(Target::wasm());
        let inner = PhpType::Array(Box::new(PhpType::Int));
        let outer = PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(inner.clone()),
        };
        let mut function =
            Function::new("unguarded_chain".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let hash = builder
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(1)),
                    IrType::Heap(IrHeapKind::Hash),
                    outer,
                    Ownership::Owned,
                )
                .expect("outer hash");
            let key = builder.emit_const_i64(0);
            let nullable_array = builder
                .emit(
                    Op::HashGetSilent,
                    vec![hash, key],
                    None,
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Union(vec![inner, PhpType::Void]),
                    Ownership::MaybeOwned,
                )
                .expect("nullable array");
            let index = builder.emit_const_i64(0);
            let _ = builder.emit(
                Op::ArrayGetSilent,
                vec![nullable_array, index],
                None,
                IrType::TaggedScalar,
                PhpType::TaggedScalar,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error =
            validate_module(&module).expect_err("unguarded nullable read must fail closed");
        assert!(
            error
                .to_string()
                .contains("source must be an indexed array or a proven non-null nullable array"),
            "{error}"
        );
    }

    /// WASM may clear a consumed hidden owned temp, but must continue rejecting
    /// `UnsetLocal` for PHP-visible local slots.
    #[test]
    fn unset_local_is_limited_to_consumed_owned_temps() {
        let mut accepted = Module::new(Target::wasm());
        let mut accepted_function =
            Function::new("clear_temp".to_string(), IrType::Void, PhpType::Void);
        accepted_function.flags.is_main = true;
        let owned_temp = accepted_function.add_local(
            Some("__temp".to_string()),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::OwnedTemp,
        );
        {
            let mut builder = Builder::new(&mut accepted_function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::UnsetLocal,
                Vec::new(),
                Some(Immediate::LocalSlot(owned_temp)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        accepted.add_function(accepted_function);
        validate_module(&accepted).expect("OwnedTemp clear must lower");

        let mut rejected = Module::new(Target::wasm());
        let mut rejected_function =
            Function::new("clear_php_local".to_string(), IrType::Void, PhpType::Void);
        rejected_function.flags.is_main = true;
        let php_local = rejected_function.add_local(
            Some("visible".to_string()),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::PhpLocal,
        );
        {
            let mut builder = Builder::new(&mut rejected_function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::UnsetLocal,
                Vec::new(),
                Some(Immediate::LocalSlot(php_local)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        rejected.add_function(rejected_function);
        let error =
            validate_module(&rejected).expect_err("PHP-visible local clear must stay rejected");
        assert!(
            error
                .to_string()
                .contains("only OwnedTemp slots may be cleared"),
            "{error}"
        );
    }

    /// A warning-producing associative read cannot be admitted into an
    /// import-free reactor because its diagnostic writes through WASI.
    #[test]
    fn rejects_warning_hash_get_without_command_runtime() {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("reactor_hash_read".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let hash = builder
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(1)),
                    IrType::Heap(IrHeapKind::Hash),
                    PhpType::AssocArray {
                        key: Box::new(PhpType::Int),
                        value: Box::new(PhpType::Int),
                    },
                    Ownership::Owned,
                )
                .expect("hash value");
            let key = builder.emit_const_i64(2);
            let _ = builder.emit(
                Op::HashGet,
                vec![hash, key],
                None,
                IrType::TaggedScalar,
                PhpType::TaggedScalar,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error =
            validate_module(&module).expect_err("reactor warning read must fail capability");
        assert!(
            error.to_string().contains(
                "warning-producing associative read requires a main-bearing command module"
            ),
            "{error}"
        );
    }

    /// A silent associative read remains valid in an import-free reactor and
    /// introduces neither warning helpers nor WASI imports.
    #[test]
    fn accepts_silent_hash_get_without_command_runtime() {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("reactor_silent_hash_read".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let hash = builder
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(1)),
                    IrType::Heap(IrHeapKind::Hash),
                    PhpType::AssocArray {
                        key: Box::new(PhpType::Int),
                        value: Box::new(PhpType::Int),
                    },
                    Ownership::Owned,
                )
                .expect("hash value");
            let key = builder.emit_const_i64(2);
            let _ = builder.emit(
                Op::HashGetSilent,
                vec![hash, key],
                None,
                IrType::TaggedScalar,
                PhpType::TaggedScalar,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let wat = validate_module(&module)
            .expect("silent reactor hash read must remain supported")
            .into_wat();
        assert!(!wat.contains("__rt_warn_undefined_array_key"), "{wat}");
        assert!(!wat.contains("wasi_snapshot_preview1"), "{wat}");
    }

    /// Dynamic Mixed associative keys are rejected for reads, writes, and
    /// `unset` until illegal PHP offset tags have an exact fatal path.
    #[test]
    fn rejects_dynamic_mixed_hash_keys_on_every_operation() {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("mixed_hash_keys".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let hash = builder
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(1)),
                    IrType::Heap(IrHeapKind::Hash),
                    PhpType::AssocArray {
                        key: Box::new(PhpType::Mixed),
                        value: Box::new(PhpType::Int),
                    },
                    Ownership::Owned,
                )
                .expect("hash value");
            let integer = builder.emit_const_i64(1);
            let mixed_key = builder
                .emit(
                    Op::MixedBox,
                    vec![integer],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .expect("dynamic Mixed key");
            for op in [Op::HashGet, Op::HashGetSilent] {
                let _ = builder.emit(
                    op,
                    vec![hash, mixed_key],
                    None,
                    IrType::TaggedScalar,
                    PhpType::TaggedScalar,
                    Ownership::NonHeap,
                );
            }
            let _ = builder.emit(
                Op::HashSet,
                vec![hash, mixed_key, integer],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let _ = builder.emit(
                Op::HashUnset,
                vec![hash, mixed_key],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error = validate_module(&module).expect_err("Mixed keys must fail closed");
        let message = error.to_string();
        assert_eq!(
            message
                .matches("dynamic Mixed associative keys require exact per-tag PHP diagnostics")
                .count(),
            4,
            "{message}"
        );
    }

    /// Distinguishes source-level PHP casts from implicit coercions for exact
    /// nullable bool/string container reads while keeping float-to-int closed.
    #[test]
    fn explicit_cast_capability_preserves_the_php_coercion_boundary() {
        let array_get_cast = |element: PhpType, immediate: Immediate| {
            let mut function =
                Function::new("cast_boundary".to_string(), IrType::Void, PhpType::Void);
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let array = builder
                    .emit(
                        Op::ArrayNew,
                        Vec::new(),
                        Some(Immediate::Capacity(0)),
                        IrType::Heap(IrHeapKind::Array),
                        PhpType::Array(Box::new(element)),
                        Ownership::Owned,
                    )
                    .expect("array source");
                let index = builder.emit_const_i64(0);
                let mixed = builder
                    .emit(
                        Op::ArrayGetSilent,
                        vec![array, index],
                        None,
                        IrType::Heap(IrHeapKind::Mixed),
                        PhpType::Mixed,
                        Ownership::Owned,
                    )
                    .expect("nullable element");
                let _ = builder.emit(
                    Op::Cast,
                    vec![mixed],
                    Some(immediate),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                );
                builder.terminate(Terminator::Return { value: None });
            }
            function
        };
        let cast_issue = |function: &Function| {
            let cast = function
                .instructions
                .iter()
                .find(|instruction| instruction.op == Op::Cast)
                .expect("cast instruction");
            super::cast_shape_issue(function, cast)
        };

        for element in [PhpType::Bool, PhpType::Str] {
            let explicit = array_get_cast(
                element.clone(),
                Immediate::ExplicitCastTarget(IrType::I64),
            );
            assert_eq!(cast_issue(&explicit), None);

            let implicit = array_get_cast(element, Immediate::CastTarget(IrType::I64));
            assert!(cast_issue(&implicit).is_some());
        }

        // The explicit `(int)` cast is admitted (its 8.5 diagnostic is implemented);
        // the implicit coercion keeps its own, different diagnostics and stays out.
        for (immediate, admitted) in [
            (Immediate::CastTarget(IrType::I64), false),
            (Immediate::ExplicitCastTarget(IrType::I64), true),
        ] {
            let mut function =
                Function::new("float_cast_boundary".to_string(), IrType::Void, PhpType::Void);
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let float = builder.emit_const_f64(1.5);
                let _ = builder.emit(
                    Op::Cast,
                    vec![float],
                    Some(immediate),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                );
                builder.terminate(Terminator::Return { value: None });
            }
            assert_eq!(cast_issue(&function).is_none(), admitted);
        }
    }

    /// Verifies the `IToStr` capability admits only integer-backed PHP int/bool
    /// sources and the exact conservative EIR ownership contract.
    #[test]
    fn int_like_to_string_shape_is_exact() {
        let shape_issue =
            |source_ir: IrType, source_php: PhpType, result_ownership: Ownership| {
                let mut function = Function::new(
                    "int_like_to_string".to_string(),
                    IrType::Void,
                    PhpType::Void,
                );
                {
                    let mut builder = Builder::new(&mut function);
                    let entry = builder.create_named_block("entry", Vec::new());
                    builder.set_entry(entry);
                    builder.position_at_end(entry);
                    let source = builder
                        .emit(
                            Op::ConstI64,
                            Vec::new(),
                            Some(Immediate::I64(1)),
                            source_ir,
                            source_php,
                            Ownership::NonHeap,
                        )
                        .expect("source value");
                    let _ = builder.emit(
                        Op::IToStr,
                        vec![source],
                        None,
                        IrType::Str,
                        PhpType::Str,
                        result_ownership,
                    );
                    builder.terminate(Terminator::Return { value: None });
                }
                let conversion = function
                    .instructions
                    .iter()
                    .find(|instruction| instruction.op == Op::IToStr)
                    .expect("IToStr instruction");
                super::int_like_to_string_shape_issue(&function, conversion)
            };

        assert_eq!(
            shape_issue(IrType::I64, PhpType::Int, Ownership::MaybeOwned),
            None
        );
        assert_eq!(
            shape_issue(IrType::I64, PhpType::Bool, Ownership::MaybeOwned),
            None
        );
        assert!(
            shape_issue(
                IrType::TaggedScalar,
                PhpType::TaggedScalar,
                Ownership::MaybeOwned
            )
            .is_some()
        );
        assert!(
            shape_issue(IrType::I64, PhpType::Float, Ownership::MaybeOwned).is_some()
        );
        assert!(shape_issue(IrType::I64, PhpType::Int, Ownership::Borrowed).is_some());
    }

    /// Verifies strict equality admits only exact scalar, string, and object
    /// families with their valid storage ownership.
    #[test]
    fn strict_compare_shape_is_exact_and_fail_closed() {
        type Shape = (IrType, PhpType, Ownership);

        let shape_issue = |
            op: Op,
            operand_shapes: Vec<Shape>,
            immediate: Option<Immediate>,
            result: Shape,
        | {
            let mut function =
                Function::new("strict_compare".to_string(), IrType::Void, PhpType::Void);
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let operands = operand_shapes
                    .into_iter()
                    .map(|(ir_type, php_type, ownership)| {
                        builder
                            .emit(
                                Op::ConstI64,
                                Vec::new(),
                                Some(Immediate::I64(1)),
                                ir_type,
                                php_type,
                                ownership,
                            )
                            .expect("strict source")
                    })
                    .collect();
                let _ = builder.emit(
                    op,
                    operands,
                    immediate,
                    result.0,
                    result.1,
                    result.2,
                );
                builder.terminate(Terminator::Return { value: None });
            }
            let comparison = function
                .instructions
                .iter()
                .find(|instruction| instruction.op == op)
                .expect("strict comparison");
            super::strict_compare_shape_issue(&function, comparison)
        };

        for op in [Op::StrictEq, Op::StrictNotEq] {
            for operands in [
                vec![
                    (IrType::I64, PhpType::Int, Ownership::NonHeap),
                    (IrType::I64, PhpType::Int, Ownership::NonHeap),
                ],
                vec![
                    (IrType::I64, PhpType::Bool, Ownership::NonHeap),
                    (IrType::I64, PhpType::False, Ownership::NonHeap),
                ],
                vec![
                    (IrType::I64, PhpType::Void, Ownership::NonHeap),
                    (IrType::I64, PhpType::Void, Ownership::NonHeap),
                ],
                vec![
                    (IrType::F64, PhpType::Float, Ownership::NonHeap),
                    (IrType::F64, PhpType::Float, Ownership::NonHeap),
                ],
                vec![
                    (IrType::Str, PhpType::Str, Ownership::Persistent),
                    (IrType::Str, PhpType::Str, Ownership::MaybeOwned),
                ],
                vec![
                    (
                        IrType::Heap(IrHeapKind::Object),
                        PhpType::Object("ParentValue".to_string()),
                        Ownership::Owned,
                    ),
                    (
                        IrType::Heap(IrHeapKind::Object),
                        PhpType::Object("ChildValue".to_string()),
                        Ownership::Borrowed,
                    ),
                ],
                vec![
                    (IrType::I64, PhpType::Int, Ownership::NonHeap),
                    (IrType::Str, PhpType::Str, Ownership::Persistent),
                ],
                vec![
                    (
                        IrType::Heap(IrHeapKind::Object),
                        PhpType::Object("Value".to_string()),
                        Ownership::Borrowed,
                    ),
                    (IrType::I64, PhpType::Void, Ownership::NonHeap),
                ],
            ] {
                assert_eq!(
                    shape_issue(
                        op,
                        operands,
                        None,
                        (IrType::I64, PhpType::Bool, Ownership::NonHeap)
                    ),
                    None
                );
            }

            // A runtime-tagged Mixed cell against a CONCRETE value is admitted: the cell's tag
            // decides the type and the concrete side is never an array, so this never needs
            // PHP's deep array identity. Two of them are still refused for exactly that reason.
            assert_eq!(
                shape_issue(
                    op,
                    vec![
                        (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned),
                        (IrType::I64, PhpType::Int, Ownership::NonHeap),
                    ],
                    None,
                    (IrType::I64, PhpType::Bool, Ownership::NonHeap)
                ),
                None
            );
            assert!(
                shape_issue(
                    op,
                    vec![
                        (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned),
                        (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned),
                    ],
                    None,
                    (IrType::I64, PhpType::Bool, Ownership::NonHeap)
                )
                .is_some(),
                "two Mixed cells could both be arrays"
            );

            for invalid in [
                (
                    IrType::TaggedScalar,
                    PhpType::TaggedScalar,
                    Ownership::NonHeap,
                ),
                (
                    IrType::Heap(IrHeapKind::Union),
                    PhpType::Union(vec![PhpType::Int, PhpType::Str]),
                    Ownership::Owned,
                ),
                (
                    IrType::I64,
                    PhpType::Resource(None),
                    Ownership::NonHeap,
                ),
                (
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Borrowed,
                ),
                (
                    IrType::Heap(IrHeapKind::Hash),
                    PhpType::AssocArray {
                        key: Box::new(PhpType::Str),
                        value: Box::new(PhpType::Int),
                    },
                    Ownership::Borrowed,
                ),
                (
                    IrType::Heap(IrHeapKind::Iterable),
                    PhpType::Iterable,
                    Ownership::Borrowed,
                ),
                (IrType::I64, PhpType::Callable, Ownership::Owned),
                (
                    IrType::I64,
                    PhpType::Pointer(None),
                    Ownership::NonHeap,
                ),
                (
                    IrType::I64,
                    PhpType::Packed("Packet".to_string()),
                    Ownership::NonHeap,
                ),
                (IrType::I64, PhpType::Int, Ownership::Owned),
            ] {
                assert!(
                    shape_issue(
                        op,
                        vec![
                            invalid,
                            (IrType::I64, PhpType::Int, Ownership::NonHeap)
                        ],
                        None,
                        (IrType::I64, PhpType::Bool, Ownership::NonHeap)
                    )
                    .is_some()
                );
            }

            assert!(
                shape_issue(
                    op,
                    vec![(IrType::I64, PhpType::Int, Ownership::NonHeap)],
                    None,
                    (IrType::I64, PhpType::Bool, Ownership::NonHeap)
                )
                .is_some()
            );
            assert!(
                shape_issue(
                    op,
                    vec![
                        (IrType::I64, PhpType::Int, Ownership::NonHeap),
                        (IrType::I64, PhpType::Int, Ownership::NonHeap)
                    ],
                    Some(Immediate::I64(1)),
                    (IrType::I64, PhpType::Bool, Ownership::NonHeap)
                )
                .is_some()
            );
            assert!(
                shape_issue(
                    op,
                    vec![
                        (IrType::I64, PhpType::Int, Ownership::NonHeap),
                        (IrType::I64, PhpType::Int, Ownership::NonHeap)
                    ],
                    None,
                    (IrType::F64, PhpType::Float, Ownership::NonHeap)
                )
                .is_some()
            );
            assert!(
                shape_issue(
                    op,
                    vec![
                        (IrType::I64, PhpType::Int, Ownership::NonHeap),
                        (IrType::I64, PhpType::Int, Ownership::NonHeap)
                    ],
                    None,
                    (IrType::I64, PhpType::Bool, Ownership::Owned)
                )
                .is_some()
            );
        }
    }

    /// Float-to-int EIR forms stay outside the public WASM surface until their
    /// context- and profile-specific PHP diagnostics are preserved.
    #[test]
    fn rejects_diagnostic_sensitive_float_to_int_operations() {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("float_to_int".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let float = builder.emit_const_f64(1.5);
            let _ = builder.emit(
                Op::FToI,
                vec![float],
                None,
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            );
            let _ = builder.emit(
                Op::Cast,
                vec![float],
                Some(Immediate::CastTarget(IrType::I64)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            );
            let mixed = builder
                .emit(
                    Op::MixedBox,
                    vec![float],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .expect("Mixed float");
            let _ = builder.emit(
                Op::IsTruthy,
                vec![float],
                None,
                IrType::I64,
                PhpType::Bool,
                Ownership::NonHeap,
            );
            let _ = builder.emit(
                Op::IsTruthy,
                vec![mixed],
                None,
                IrType::I64,
                PhpType::Bool,
                Ownership::NonHeap,
            );
            let _ = builder.emit(
                Op::Cast,
                vec![mixed],
                Some(Immediate::CastTarget(IrType::I64)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error = validate_module(&module).expect_err("float-to-int forms must fail closed");
        let message = error.to_string();
        assert!(
            message.contains(
                "implicit float-to-int coercion requires exact profile-specific warning and deprecation diagnostics"
            ),
            "{message}"
        );
        assert!(
            message.contains(
                "float-to-int casts require exact profile-specific out-of-range diagnostics"
            ),
            "{message}"
        );
        assert!(
            message.contains(
                "Mixed-to-scalar casts require exact per-tag PHP values and diagnostics"
            ),
            "{message}"
        );
        assert_eq!(
            message
                .matches(
                    "float or Mixed truthiness requires exact profile-specific NAN diagnostics"
                )
                .count(),
            2,
            "{message}"
        );
    }

    /// Float associative keys are rejected for reads, writes, and `unset`
    /// because PHP exposes versioned precision-loss and range diagnostics.
    #[test]
    fn rejects_float_hash_keys_on_every_operation() {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("float_hash_keys".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let hash = builder
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(1)),
                    IrType::Heap(IrHeapKind::Hash),
                    PhpType::AssocArray {
                        key: Box::new(PhpType::Float),
                        value: Box::new(PhpType::Int),
                    },
                    Ownership::Owned,
                )
                .expect("hash value");
            let float_key = builder.emit_const_f64(1.5);
            let integer = builder.emit_const_i64(1);
            for op in [Op::HashGet, Op::HashGetSilent] {
                let _ = builder.emit(
                    op,
                    vec![hash, float_key],
                    None,
                    IrType::TaggedScalar,
                    PhpType::TaggedScalar,
                    Ownership::NonHeap,
                );
            }
            let _ = builder.emit(
                Op::HashSet,
                vec![hash, float_key, integer],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let _ = builder.emit(
                Op::HashUnset,
                vec![hash, float_key],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error = validate_module(&module).expect_err("float keys must fail closed");
        let message = error.to_string();
        assert_eq!(
            message
                .matches(
                    "float associative keys require exact profile-specific implicit-conversion diagnostics"
                )
                .count(),
            4,
            "{message}"
        );
    }

    /// Dynamic hash values cannot be coerced into concrete element storage
    /// until each runtime tag preserves PHP conversion diagnostics.
    #[test]
    fn rejects_dynamic_values_for_concrete_hash_storage() {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("mixed_hash_values".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let hash = builder
                .emit(
                    Op::HashNew,
                    Vec::new(),
                    Some(Immediate::Capacity(1)),
                    IrType::Heap(IrHeapKind::Hash),
                    PhpType::AssocArray {
                        key: Box::new(PhpType::Int),
                        value: Box::new(PhpType::Int),
                    },
                    Ownership::Owned,
                )
                .expect("hash value");
            let key = builder.emit_const_i64(1);
            let integer = builder.emit_const_i64(2);
            let mixed = builder
                .emit(
                    Op::MixedBox,
                    vec![integer],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .expect("dynamic Mixed value");
            let _ = builder.emit(
                Op::HashSet,
                vec![hash, key, mixed],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            let _ = builder.emit(
                Op::HashAppend,
                vec![hash, mixed],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error =
            validate_module(&module).expect_err("dynamic concrete hash stores must fail closed");
        let message = error.to_string();
        assert_eq!(
            message
                .matches(
                    "hash write value Heap(Mixed)/Mixed must exactly match concrete storage Int"
                )
                .count(),
            2,
            "{message}"
        );
    }

    /// Legacy scalar/string result shapes and untracked boxed ownership are
    /// rejected before WAT lowering can erase the missing-key null. Concrete
    /// containers must retain exact pointer storage plus `container|null`
    /// metadata; boxing them would break typed chained consumers.
    #[test]
    fn rejects_non_nullable_hash_get_result_shapes() {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("legacy_hash_reads".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let key = builder.emit_const_i64(0);
            for (element, ir_type, php_type, ownership) in [
                (
                    PhpType::Int,
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                ),
                (
                    PhpType::Float,
                    IrType::F64,
                    PhpType::Float,
                    Ownership::NonHeap,
                ),
                (
                    PhpType::Str,
                    IrType::Str,
                    PhpType::Str,
                    Ownership::MaybeOwned,
                ),
                (
                    PhpType::Bool,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::NonHeap,
                ),
                (
                    PhpType::Array(Box::new(PhpType::Int)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::MaybeOwned,
                ),
                (
                    PhpType::Array(Box::new(PhpType::Int)),
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                ),
            ] {
                let hash = builder
                    .emit(
                        Op::HashNew,
                        Vec::new(),
                        Some(Immediate::Capacity(1)),
                        IrType::Heap(IrHeapKind::Hash),
                        PhpType::AssocArray {
                            key: Box::new(PhpType::Int),
                            value: Box::new(element),
                        },
                        Ownership::Owned,
                    )
                    .expect("hash value");
                let _ = builder.emit(
                    Op::HashGetSilent,
                    vec![hash, key],
                    None,
                    ir_type,
                    php_type,
                    ownership,
                );
            }
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error =
            validate_module(&module).expect_err("legacy hash-read shapes must fail");
        let message = error.to_string();
        assert_eq!(
            message
                .matches("unsupported hash_get_silent shape")
                .count(),
            6,
            "{message}"
        );
        assert!(
            message.contains("result has incompatible ownership"),
            "{message}"
        );
    }

    /// Indexed-to-hash promotion is admitted with one release-tracked indexed
    /// source and an owned associative result whose integer-keyed value type is
    /// preserved. Capability accepts the shape; hash/CLI tests cover production
    /// lowering and runtime assembly/execution.
    #[test]
    fn accepts_shape_complete_array_to_hash() {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("promote".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let array = builder
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(0)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Owned,
                )
                .expect("array value");
            let _ = builder.emit(
                Op::ArrayToHash,
                vec![array],
                None,
                IrType::Heap(IrHeapKind::Hash),
                PhpType::AssocArray {
                    key: Box::new(PhpType::Int),
                    value: Box::new(PhpType::Int),
                },
                Ownership::Owned,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        validate_module(&module).expect("shape-complete ArrayToHash must pass planning");
    }

    /// ArrayToHash rejects malformed arity, source type/ownership, result
    /// storage/type compatibility, and result ownership at the static capability
    /// gate instead of falling through to WAT lowering.
    #[test]
    fn rejects_invalid_array_to_hash_shapes_before_lowering() {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("bad_promotions".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let integer = builder.emit_const_i64(1);
            let array = builder
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(0)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Owned,
                )
                .expect("array value");
            let borrowed_array = builder
                .emit(
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(0)),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    Ownership::Borrowed,
                )
                .expect("borrowed array value");
            let assoc_int = PhpType::AssocArray {
                key: Box::new(PhpType::Int),
                value: Box::new(PhpType::Int),
            };
            let _ = builder.emit(
                Op::ArrayToHash,
                Vec::new(),
                None,
                IrType::Heap(IrHeapKind::Hash),
                assoc_int.clone(),
                Ownership::Owned,
            );
            let _ = builder.emit(
                Op::ArrayToHash,
                vec![integer],
                None,
                IrType::Heap(IrHeapKind::Hash),
                assoc_int.clone(),
                Ownership::Owned,
            );
            let _ = builder.emit(
                Op::ArrayToHash,
                vec![array],
                None,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
                Ownership::Owned,
            );
            let _ = builder.emit(
                Op::ArrayToHash,
                vec![array],
                None,
                IrType::Heap(IrHeapKind::Hash),
                PhpType::AssocArray {
                    key: Box::new(PhpType::Int),
                    value: Box::new(PhpType::Str),
                },
                Ownership::Owned,
            );
            let _ = builder.emit(
                Op::ArrayToHash,
                vec![array],
                None,
                IrType::Heap(IrHeapKind::Hash),
                assoc_int.clone(),
                Ownership::Borrowed,
            );
            let _ = builder.emit(
                Op::ArrayToHash,
                vec![borrowed_array],
                None,
                IrType::Heap(IrHeapKind::Hash),
                assoc_int,
                Ownership::Owned,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error =
            validate_module(&module).expect_err("invalid ArrayToHash shapes must fail");
        let message = error.to_string();
        assert_eq!(
            message.matches("unsupported array_to_hash shape").count(),
            6,
            "{message}"
        );
        for expected in [
            "expected one indexed-array operand",
            "source must be an indexed array",
            "result must be AssocArray<Int|Mixed, T>",
            "must preserve Int or widen to Mixed",
            "result must own a releasable hash reference",
            "consumed source must own a releasable reference",
        ] {
            assert!(message.contains(expected), "missing {expected:?}: {message}");
        }
    }

    /// Verifies `exit`/`die` in a nested function is rejected until WASM can
    /// unwind and clean every caller-owned frame before `proc_exit`.
    #[test]
    fn rejects_exit_outside_main_before_lowering() {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("nested".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::LanguageConstructCall,
                Vec::new(),
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Unreachable);
        }
        module.add_function(function);

        let error = validate_module(&module).expect_err("nested exit must fail before lowering");
        let message = error.to_string();
        assert!(
            message.contains("exit/die outside main cannot unwind caller-owned WASM frames"),
            "{message}"
        );
    }

    /// Verifies unsupported block-parameter storage cannot bypass the audit.
    #[test]
    fn audits_block_parameter_types() {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("buffer_param".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block(
                "entry",
                vec![(
                    IrType::Heap(crate::ir::IrHeapKind::Buffer),
                    PhpType::Buffer(Box::new(PhpType::Int)),
                )],
            );
            builder.set_entry(entry);
            builder.position_at_end(entry);
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error = validate_module(&module).expect_err("buffer block params must fail");
        let message = error.to_string();
        assert!(message.contains("block#0 param#0"), "{message}");
        assert!(message.contains("unsupported storage type Heap(Buffer)"), "{message}");
    }

    /// Fresh locals and unambiguously owned promoted cells are both admitted as
    /// escaping by-ref closure captures by the pre-emission capability gate.
    #[test]
    fn accepts_fresh_and_owned_by_ref_closure_captures() {
        let mut module = Module::new(Target::wasm());
        let name_id = add_by_ref_int_closure(&mut module, "__cap_owned_ref");

        let mut fresh = Function::new("fresh_creator".to_string(), IrType::Void, PhpType::Void);
        let fresh_slot = fresh.add_local(
            Some("x".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        {
            let mut builder = Builder::new(&mut fresh);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let one = builder.emit_const_i64(1);
            builder.emit_store_local(fresh_slot, one);
            let capture = builder.emit_load_local(fresh_slot, IrType::I64, PhpType::Int);
            let _ = emit_by_ref_closure_new(&mut builder, capture, name_id);
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(fresh);

        let mut owned = Function::new("owned_creator".to_string(), IrType::Void, PhpType::Void);
        let owned_slot = owned.add_local(
            Some("x".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let owner_slot = owned.add_local(
            Some("__ref_owner_x".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::HiddenTemp,
        );
        {
            let mut builder = Builder::new(&mut owned);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let one = builder.emit_const_i64(1);
            builder.emit_store_local(owned_slot, one);
            let _ = builder.emit(
                Op::PromoteLocalRefCell,
                Vec::new(),
                Some(Immediate::LocalSlotPair {
                    first: owned_slot,
                    second: owner_slot,
                }),
                IrType::Void,
                PhpType::Int,
                Ownership::NonHeap,
            );
            let capture = builder
                .emit(
                    Op::LoadRefCell,
                    Vec::new(),
                    Some(Immediate::LocalSlot(owned_slot)),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .expect("owned capture");
            let _ = emit_by_ref_closure_new(&mut builder, capture, name_id);
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(owned);

        validate_module(&module).expect("owned by-ref capture shapes must be admitted");
    }

    /// By-ref parameters and foreach element addresses are borrowed bindings;
    /// both must be rejected before WAT emission when a closure would outlive them.
    #[test]
    fn rejects_borrowed_by_ref_closure_captures_before_emission() {
        let mut module = Module::new(Target::wasm());
        let name_id = add_by_ref_int_closure(&mut module, "__cap_borrowed_ref");

        let mut parameter =
            Function::new("parameter_creator".to_string(), IrType::Void, PhpType::Void);
        parameter.params.push(FunctionParam {
            name: "x".to_string(),
            ir_type: IrType::I64,
            php_type: PhpType::Int,
            by_ref: true,
            variadic: false,
        });
        let parameter_slot = parameter.add_local(
            Some("x".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        {
            let mut builder = Builder::new(&mut parameter);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let capture = builder
                .emit(
                    Op::LoadRefCell,
                    Vec::new(),
                    Some(Immediate::LocalSlot(parameter_slot)),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .expect("borrowed parameter capture");
            let _ = emit_by_ref_closure_new(&mut builder, capture, name_id);
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(parameter);

        let mut interior =
            Function::new("interior_creator".to_string(), IrType::Void, PhpType::Void);
        let interior_slot = interior.add_local(
            Some("item".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        {
            let mut builder = Builder::new(&mut interior);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::IterCurrentValueRef,
                Vec::new(),
                Some(Immediate::LocalSlot(interior_slot)),
                IrType::Void,
                PhpType::Int,
                Ownership::NonHeap,
            );
            let capture = builder
                .emit(
                    Op::LoadRefCell,
                    Vec::new(),
                    Some(Immediate::LocalSlot(interior_slot)),
                    IrType::I64,
                    PhpType::Int,
                    Ownership::NonHeap,
                )
                .expect("borrowed interior capture");
            let _ = emit_by_ref_closure_new(&mut builder, capture, name_id);
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(interior);

        let error = validate_module(&module).expect_err("borrowed bindings must not escape");
        let message = error.to_string();
        assert!(message.contains("parameter_creator"), "{message}");
        assert!(message.contains("interior_creator"), "{message}");
        assert_eq!(
            message
                .matches("cannot escape non-owned ref-bound local#0")
                .count(),
            2,
            "{message}"
        );
    }

    /// Verifies a reachable EIR `Unreachable` is rejected before WAT lowering.
    #[test]
    fn rejects_reachable_unreachable_terminator_without_proof() {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("reachable_trap".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            builder.terminate(Terminator::Unreachable);
        }
        module.add_function(function);

        let error =
            validate_module(&module).expect_err("reachable raw trap must fail capability");
        assert!(
            error
                .to_string()
                .contains("reachable EIR unreachable terminator lacks a no-return proof"),
            "{error}"
        );
    }

    /// Admits the exact result-free offset-on-null warning in a command module.
    #[test]
    fn accepts_exact_array_offset_on_null_warning_boundary() {
        let mut module = Module::new(Target::wasm());
        let message = module
            .data
            .intern_string(crate::codegen_support::runtime::array_offset_on_null_warning());
        let mut function =
            Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::Warn,
                Vec::new(),
                Some(Immediate::Data(message)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let wat = validate_module(&module)
            .expect("exact command warning boundary must lower")
            .into_wat();
        assert!(
            wat.contains("call $__rt_warn_array_offset_on_null"),
            "{wat}"
        );
    }

    /// Rejects unrelated static warning messages at the capability boundary.
    #[test]
    fn rejects_unrelated_static_warning_boundary() {
        let mut module = Module::new(Target::wasm());
        let message = module.data.intern_string("Warning: unrelated\n");
        let mut function =
            Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::Warn,
                Vec::new(),
                Some(Immediate::Data(message)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error =
            validate_module(&module).expect_err("general static warning must remain unsupported");
        assert!(
            error
                .to_string()
                .contains("unsupported static warning"),
            "{error}"
        );
    }

    /// Rejects the offset-on-null warning in an import-free reactor module.
    #[test]
    fn rejects_array_offset_on_null_warning_without_command_runtime() {
        let mut module = Module::new(Target::wasm());
        let message = module
            .data
            .intern_string(crate::codegen_support::runtime::array_offset_on_null_warning());
        let mut function =
            Function::new("reactor".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::Warn,
                Vec::new(),
                Some(Immediate::Data(message)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let error =
            validate_module(&module).expect_err("reactor warning must fail capability");
        assert!(
            error
                .to_string()
                .contains("require the public WASI command runtime"),
            "{error}"
        );
    }

    /// Admits only the exact method-on-null static `Error` in a command module
    /// and proves its reachable EIR `Unreachable` through the final helper call.
    #[test]
    fn accepts_exact_method_on_null_error_boundary() {
        let mut module = Module::new(Target::wasm());
        let message = module
            .data
            .intern_string("Call to a member function value() on null");
        let mut function =
            Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::ThrowError,
                Vec::new(),
                Some(Immediate::Data(message)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Unreachable);
        }
        module.add_function(function);

        let wat = validate_module(&module)
            .expect("exact command fatal boundary must lower")
            .into_wat();
        assert!(
            wat.contains("call $__rt_fail_method_call_non_object"),
            "{wat}"
        );
        assert!(wat.contains("post-noreturn:method-null-error"), "{wat}");
    }

    /// Rejects unrelated static `Error` messages instead of silently turning
    /// general catchable PHP errors into process exits.
    #[test]
    fn rejects_unrelated_static_throw_error_boundary() {
        let mut module = Module::new(Target::wasm());
        let message = module.data.intern_string("unrelated static error");
        let mut function =
            Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::ThrowError,
                Vec::new(),
                Some(Immediate::Data(message)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Unreachable);
        }
        module.add_function(function);

        let error =
            validate_module(&module).expect_err("general static Error must remain unsupported");
        let message = error.to_string();
        assert!(message.contains("unsupported static Error message"), "{message}");
        assert!(
            message.contains("reachable EIR unreachable terminator lacks a no-return proof"),
            "{message}"
        );
    }

    /// Rejects the method-on-null fatal opcode in import-free reactor fixtures
    /// because the PHP diagnostic depends on WASI stderr and `proc_exit`.
    #[test]
    fn rejects_method_on_null_error_without_command_runtime() {
        let mut module = Module::new(Target::wasm());
        let message = module
            .data
            .intern_string("Call to a member function value() on null");
        let mut function =
            Function::new("reactor".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::ThrowError,
                Vec::new(),
                Some(Immediate::Data(message)),
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Unreachable);
        }
        module.add_function(function);

        let error =
            validate_module(&module).expect_err("reactor fatal must fail capability");
        assert!(
            error
                .to_string()
                .contains("require the public WASI command runtime"),
            "{error}"
        );
    }

    /// Verifies an unreachable terminator in a disconnected EIR block retains a
    /// machine-checkable CFG proof and remains admissible.
    #[test]
    fn accepts_unreachable_terminator_in_disconnected_block() {
        let mut module = Module::new(Target::wasm());
        let mut function =
            Function::new("dead_trap".to_string(), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            let dead = builder.create_named_block("dead", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            builder.terminate(Terminator::Return { value: None });
            builder.position_at_end(dead);
            builder.terminate(Terminator::Unreachable);
        }
        module.add_function(function);

        validate_module(&module).expect("disconnected raw trap has a CFG proof");
    }
}
