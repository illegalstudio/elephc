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
    BlockId, CmpPredicate, Effects, Function, Immediate, InstId, Instruction, IrHeapKind, IrType,
    LocalKind, Module, Op, Ownership, RuntimeCallTarget, RuntimeFnId, Terminator,
    UnaryStringRuntime, ValueDef, ValueId,
};
use crate::parser::ast::Visibility;
use crate::types::PhpType;
use std::collections::HashSet;

/// Compile-time provenance of ref-bound local slots within one EIR function.
///
/// `owned` means the slot points at a promotable kind-8 heap cell. `borrowed`
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

/// Reports a by-value parameter whose frame slot is stored with a different representation.
///
/// A WASM function's parameters ARE its slot locals (see `function.rs`), declared with the
/// parameter's type. Native boxes such a slot into a fresh Mixed cell in the prologue; WASM has no
/// such conversion, so a closure capture assigned another type in the body (its slot widens to
/// Mixed) would emit an invalid module.
fn widened_parameter_slot_issue(function: &Function) -> Option<String> {
    if function.flags.is_main {
        return None;
    }
    // A generator's wrapper DOES convert: it boxes a scalar, string or container argument into
    // the Mixed word its widened slot is (`generators::widened_argument_cell`). Any other
    // widening has no conversion and is refused like a closure's.
    let generator = function.flags.is_generator;
    function
        .params
        .iter()
        .zip(&function.locals)
        .find(|(param, local)| {
            if param.by_ref || param.ir_type == local.ir_type {
                return false;
            }
            if !generator || local.ir_type != IrType::Heap(crate::ir::IrHeapKind::Mixed) {
                return true;
            }
            let components: Vec<(String, super::wat::ValType)> =
                super::values::WasmRepr::val_types(param.ir_type)
                    .into_iter()
                    .map(|val_type| (String::new(), val_type))
                    .collect();
            super::generators::widened_argument_cell(param, &components).is_none()
        })
        .map(|(param, local)| {
            format!(
                "parameter ${} arrives as {:?} but its slot is stored as {:?} (a capture widened by \
                 the body needs a prologue conversion WASM does not emit)",
                param.name, param.ir_type, local.ir_type
            )
        })
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
    if let Some(issue) = widened_parameter_slot_issue(function) {
        issues.push(format!("{collection}::{}: {issue}", function.name));
    }
    if function.flags.is_generator {
        if let Some(param) = function.params.iter().find(|param| param.by_ref) {
            // The generator frame holds values, not a borrowed cell: its wrapper has no pointer
            // local for the caller's reference, so the body would read the wrong storage.
            issues.push(format!(
                "{collection}::{}: generator with by-reference parameter ${} (the generator frame cannot \
                 hold a borrowed cell)",
                function.name, param.name
            ));
        }
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
            // main. `isset` and `empty` have no such constraint — they only read their
            // operand — so they are exempt.
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
                && construct_name.as_deref() == Some("empty")
            {
                if let Some(issue) = empty_construct_shape_issue(function, instruction) {
                    issues.push(format!(
                        "{collection}::{} block#{} instruction#{}: {issue}",
                        function.name,
                        block.id.as_raw(),
                        inst_id.as_raw()
                    ));
                }
            }
            if instruction.op == Op::LanguageConstructCall
                && !function.flags.is_main
                && !matches!(construct_name.as_deref(), Some("isset") | Some("empty"))
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
        Op::StoreGlobal => store_global_shape_issue(module, function, inst),
        Op::StoreRefCell => store_ref_cell_shape_issue(function, inst),
        Op::Move | Op::Borrow | Op::Acquire => forward_transfer_shape_issue(function, inst),
        Op::UnsetLocal => unset_owned_temp_shape_issue(function, inst),
        Op::Cast => cast_shape_issue(module, function, inst),
        Op::IToStr => int_like_to_string_shape_issue(function, inst),
        Op::StrictEq | Op::StrictNotEq => strict_compare_shape_issue(function, inst),
        Op::IsTruthy => truthiness_shape_issue(module, function, inst),
        Op::TypePredicate => type_predicate_shape_issue(module, function, inst),
        Op::StrIncDec => str_inc_dec_shape_issue(function, inst),
        Op::ArraySet => array_store_shape_issue(function, inst, 2, false),
        Op::ArrayPush => array_store_shape_issue(function, inst, 1, true),
        Op::ArrayToMixed => array_to_mixed_shape_issue(function, inst),
        Op::LooseEq | Op::LooseNotEq => loose_eq_shape_issue(module, function, inst),
        Op::PhpRelCmp => php_rel_cmp_shape_issue(module, function, inst),
        Op::Spaceship => spaceship_shape_issue(function, inst),
        Op::ObjectCloneShallow => object_clone_shape_issue(module, function, inst),
        Op::ZeroLocalSlot => zero_local_slot_shape_issue(function, inst),
        Op::PushCallOperandOwner | Op::PopCallOperandOwner => {
            call_operand_owner_shape_issue(function, inst)
        }
        Op::LoadStaticLocal
        | Op::StoreStaticLocal
        | Op::InitStaticLocal
        | Op::StaticLocalUninitialized => {
            static_local_shape_issue(function, inst)
        }
        Op::PropInitialized => prop_initialized_shape_issue(module, function, inst),
        Op::ReleaseUnlessAliases => release_unless_aliases_shape_issue(function, inst),
        Op::MixedClone => mixed_clone_shape_issue(function, inst),
        Op::MixedArrayAppend => mixed_array_append_shape_issue(module, function, inst),
        Op::PropGetForWrite => prop_get_for_write_shape_issue(module, function, inst),
        Op::ReturnBoundaryMixedToInt => {
            return_boundary_int_shape_issue(module, function, inst)
        }
        Op::IterStart => iter_start_shape_issue(module, function, inst),
        Op::IncludeOnceMark | Op::IncludeOnceGuard => include_once_shape_issue(module, inst),
        Op::FunctionVariantMark => function_variant_mark_shape_issue(module, inst),
        // The group's own dispatcher carries the dispatch; the marker introduces no code.
        Op::FunctionVariantDispatch => None,
        Op::IterCurrentValueRef => iter_current_value_ref_shape_issue(function, inst),
        Op::ArrayGet
        | Op::ArrayGetSilent
        | Op::ArrayGetMixedKey
        | Op::ArrayGetMixedKeySilent => {
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
        Op::StaticPropInitialized => static_prop_initialized_shape_issue(module, function, inst),
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
        Op::PropUnset => property_unset_shape_issue(module, function, inst),
        Op::DynamicPropGet | Op::DynamicPropSet => {
            dynamic_property_shape_issue(module, function, inst)
        }
        Op::GeneratorYield => generator_yield_shape_issue(module, function, inst),
        Op::GeneratorYieldFrom => generator_yield_from_shape_issue(module, function, inst),
        Op::Warn => array_offset_on_null_warning_shape_issue(module, inst),
        Op::ThrowError => method_call_on_null_error_shape_issue(module, inst),
        Op::ThrowErrorValue => throw_error_value_shape_issue(module, function, inst),
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

/// Returns whether a reachable EIR `Unreachable` immediately follows an
/// admitted PHP fatal boundary.
///
/// Two proofs are accepted. `ThrowError` is the static fatal, and it is only a
/// proof for the one message shape the backend renders. A raised exception —
/// `ThrowException` or `ThrowErrorValue` — proves it structurally instead:
/// `inst::lower_throw` ends the block with a WASM `throw`, which transfers
/// control unconditionally, so nothing after it can execute.
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
    match instruction.op {
        Op::ThrowError => method_call_on_null_error_shape_issue(module, instruction).is_none(),
        Op::ThrowException | Op::ThrowErrorValue => !instruction.operands.is_empty(),
        _ => false,
    }
}

/// Validates `ThrowErrorValue`, whose operand is an error MESSAGE rather than an object.
///
/// Native builds an `Error` around the message; wasm cannot, because a program that never names
/// `Error` carries no `ClassInfo` for it — no class id, no property layout. What the wasm lowering
/// reproduces instead is the observable of an UNCAUGHT one (PHP's stderr line and exit 255), so
/// the form is only supported where nothing could have caught it.
///
/// The handler test is per-FUNCTION, not per-block: an EIR handler pushed in an enclosing block
/// is live across the throw site, and a cheaper block-local test would call a catchable throw
/// uncatchable. A function with any `TryPushHandler` is refused whole.
fn throw_error_value_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let Some(&message) = inst.operands.first() else {
        return Some("throw_error_value expects a message operand".to_string());
    };
    let Some(value) = function.value(message) else {
        return Some("throw_error_value message is missing from the value table".to_string());
    };
    // An object operand is a re-raise of something already built, which the tag carries directly.
    if value.ir_type != IrType::Str {
        return None;
    }
    if !module
        .functions
        .iter()
        .any(|candidate| candidate.flags.is_main)
    {
        return Some(
            "throw_error_value of a message requires the public WASI command runtime".to_string(),
        );
    }
    if function
        .instructions
        .iter()
        .any(|candidate| candidate.op == Op::TryPushHandler)
    {
        return Some(
            "throw_error_value of a message inside a function with a catch handler on \
             wasm32-wasi (the Error object it would bind is not built on this target)"
                .to_string(),
        );
    }
    None
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
    // `Class::method(...)` names a STATIC method, whose body lives with the class methods. Its
    // wrapper forwards the called-class id the body takes as a hidden first parameter, so the
    // only extra requirement is that the class is one this module compiles.
    let matches: Vec<&Function> = module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .filter(|function| {
            !function.flags.is_main
                && crate::names::php_symbol_key(function.name.trim_start_matches('\\')) == key
        })
        .collect();
    if name.contains("::") && super::closures::fcc_hidden_class_id(module, &name).is_none() {
        return Some(format!(
            "first-class callable target {name:?} names a class this module does not compile"
        ));
    }
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
            | transfer::TransferKind::NullPointer
            | transfer::TransferKind::TaggedNull
            | transfer::TransferKind::TaggedScalarFromConcrete { .. }
            // Narrowing a pair back out to a concrete scalar keeps its payload and drops its
            // tag, which is exactly what the native backend does — its tag lives in a separate
            // register the concrete parameter never reads.
            | transfer::TransferKind::TaggedScalarToConcrete { .. }
            // An int-typed value into a bool slot, normalized to php's truthiness. This is the
            // transfer `isset(...)` needs: the construct answers `int` and the slot around it is
            // `bool`.
            | transfer::TransferKind::BoolFromInt,
        ) => None,
        Err(error) => Some(error.to_string()),
    }
}

/// Names the function and parameter behind an implicit `Str` cast that feeds a builtin argument.
///
/// This is the OTHER implicit `Str` coercion, and it is a different operation from the one
/// `cast_feeds_string_context` admits. Measured on php-src 8.5.6 for `strtoupper($mixed)`: a
/// string, int, float or bool converts exactly as `(string)` does; `null` converts to `""` but
/// raises `Deprecated: F(): Passing null to parameter #N ($p) of type string is deprecated`; and
/// an array, object or resource does NOT convert at all — it is a `TypeError` naming the type
/// that arrived, where `(string)` of the same array would have produced `"Array"` with a warning.
///
/// Returns `(php_function_name, parameter_name, one_based_position)` when every one of those
/// answers is knowable at compile time, which needs all of:
///
/// - the cast's result reaching exactly ONE consumer, a typed runtime call, at one position;
/// - that runtime id belonging to exactly one PHP name (`count`/`sizeof` share one, and php-src
///   reports the name as written, so an alias is refused rather than guessed);
/// - the parameter being declared plain `string` — a `?string` takes null with no deprecation.
pub(super) fn mixed_string_argument_coercion(
    function: &Function,
    inst: &Instruction,
) -> Option<(&'static str, String, usize)> {
    mixed_argument_coercion(function, inst, PhpType::Str)
}

/// Names the function and parameter behind a BOXED operand at a builtin's declared `int`.
///
/// The cast-based sibling below covers the case where the frontend materialised a conversion;
/// this one covers the commoner shape, where it did not. `substr($s, $mixed)` reaches the call
/// with the Mixed operand intact — there is no `Op::Cast` anywhere — so the coercion has to be
/// emitted where the argument is pushed rather than where a cast would have been.
///
/// Returns `(php_function_name, parameter_name, one_based_position)` under the same conditions
/// the cast form requires: one PHP name owning the runtime target, and a parameter declared
/// plain `int` — a `?int` would take null with no deprecation.
pub(super) fn runtime_call_int_operand_coercion(
    function: &Function,
    call: &Instruction,
    index: usize,
) -> Option<(&'static str, String, usize)> {
    let operand = call.operands.get(index)?;
    let value = function.value(*operand)?;
    if value.ir_type != IrType::Heap(IrHeapKind::Mixed)
        || value.php_type.codegen_repr() != PhpType::Mixed
    {
        return None;
    }
    let Some(Immediate::RuntimeCall(target)) = &call.immediate else {
        return None;
    };
    let (name, parameter, declared) =
        crate::builtins::registry::runtime_call_sole_parameter(*target, index)?;
    (declared == PhpType::Int).then_some((name, parameter, index + 1))
}

/// The same for a declared `int` parameter — `substr($s, $mixed)` most visibly.
///
/// Measured on php-src 8.5.6, the conversion is the one a declared `int` RETURN performs, with
/// two differences: `null` does not raise there but converts to 0 after a `Deprecated` naming
/// the parameter, and the failure names `Argument #N ($p)`. Everything numeric in between is
/// identical, including both precision deprecations, which is why the runtime shares a core
/// rather than carrying a second copy.
pub(super) fn mixed_int_argument_coercion(
    function: &Function,
    inst: &Instruction,
) -> Option<(&'static str, String, usize)> {
    mixed_argument_coercion(function, inst, PhpType::Int)
}

/// Names the function and parameter behind an implicit cast that feeds a builtin argument.
fn mixed_argument_coercion(
    function: &Function,
    inst: &Instruction,
    declared_type: PhpType,
) -> Option<(&'static str, String, usize)> {
    let result = inst.result?;
    // A slot a `push_call_operand_owner` names only keeps the argument alive for an unwind; a
    // store into it is ownership bookkeeping, not a second context.
    let operand_owner_slots: HashSet<u32> = function
        .instructions
        .iter()
        .filter(|candidate| candidate.op == Op::PushCallOperandOwner)
        .filter_map(|candidate| match candidate.immediate {
            Some(Immediate::LocalSlot(slot)) => Some(slot.as_raw()),
            _ => None,
        })
        .collect();
    let mut consumer: Option<(&Instruction, usize)> = None;
    // Main's EIR reaches the call through an `acquire` into that owner slot and a `borrow` of
    // the acquired value, so both forward the argument rather than ending the walk.
    let mut pending = vec![result];
    let mut seen: HashSet<ValueId> = HashSet::new();
    while let Some(value) = pending.pop() {
        if !seen.insert(value) {
            continue;
        }
        for candidate in &function.instructions {
            if !candidate.operands.contains(&value) {
                continue;
            }
            // Ownership bookkeeping is not a use; it says nothing about the context.
            if matches!(candidate.op, Op::Release | Op::Move) {
                continue;
            }
            if matches!(candidate.op, Op::Acquire | Op::Borrow) {
                pending.extend(candidate.result);
                continue;
            }
            if candidate.op == Op::StoreLocal
                && matches!(
                    candidate.immediate,
                    Some(Immediate::LocalSlot(slot)) if operand_owner_slots.contains(&slot.as_raw())
                )
            {
                continue;
            }
            for (index, operand) in candidate.operands.iter().enumerate() {
                if *operand != value {
                    continue;
                }
                // A second use means a second context, which may not share this one's contract.
                if consumer.is_some() {
                    return None;
                }
                consumer = Some((candidate, index));
            }
        }
    }
    let (call, index) = consumer?;
    if call.op != Op::RuntimeCall {
        return None;
    }
    let Some(Immediate::RuntimeCall(target)) = &call.immediate else {
        return None;
    };
    let (name, parameter, declared) =
        crate::builtins::registry::runtime_call_sole_parameter(*target, index)?;
    if declared != declared_type {
        return None;
    }
    Some((name, parameter, index + 1))
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
    let mut pending = vec![result];
    let mut seen_values: HashSet<ValueId> = HashSet::new();
    let mut seen_slots: HashSet<u32> = HashSet::new();
    let mut consumed = false;
    while let Some(value) = pending.pop() {
        if !seen_values.insert(value) {
            continue;
        }
        for candidate in &function.instructions {
            if !candidate.operands.contains(&value) {
                continue;
            }
            // Ownership bookkeeping is not a USE: the EIR releases the cast's temporary right
            // after the concat consumes it, and that release says nothing about the context.
            // An ACQUIRE does forward the value, though, so its result is followed rather than
            // dismissed — `echo $x ?? "d"` stabilizes the merged value through one.
            if matches!(candidate.op, Op::Release | Op::Move | Op::Borrow) {
                continue;
            }
            if candidate.op == Op::Acquire {
                if let Some(forwarded) = candidate.result {
                    pending.push(forwarded);
                }
                continue;
            }
            // A `??` merge parks its value in a hidden slot and reads it back in the merge
            // block, so the echo is two hops away from the cast. Following the slot keeps that
            // reachable — and following EVERY load of it is what keeps the answer sound: one
            // load reaching a non-string context still refuses the whole cast.
            if candidate.op == Op::StoreLocal {
                let Some(Immediate::LocalSlot(slot)) = candidate.immediate.as_ref() else {
                    return false;
                };
                if seen_slots.insert(slot.as_raw()) {
                    for load in &function.instructions {
                        if load.op != Op::LoadLocal {
                            continue;
                        }
                        if load.immediate.as_ref() != Some(&Immediate::LocalSlot(*slot)) {
                            continue;
                        }
                        if let Some(loaded) = load.result {
                            pending.push(loaded);
                        }
                    }
                }
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
    }
    consumed
}

/// Returns whether a `Heap(Mixed)` value is PROVABLY a boxed container.
///
/// Some builtins answer `mixed` in the EIR for a reason that is about their CALLBACK, not
/// their result: `array_map`'s result type is deliberately Mixed because a string callback
/// picks its element ABI at runtime (see `src/builtins/array/array_map.rs`). The value is
/// still always an array. Where that is provable from the defining instruction a consumer can
/// unbox it with no runtime check — and, more importantly, with no `TypeError` to raise, which
/// is what makes admitting it exact rather than approximate.
///
/// Only builtins whose runtime helper ALWAYS answers a fresh `array<mixed>` qualify, and only
/// those this target actually lowers — `array_filter` would belong here too but is not yet
/// admitted as a builtin, and claiming it would be claiming something unreachable.
pub(super) fn value_is_container_by_construction(function: &Function, value: ValueId) -> bool {
    let Some(defining) = function
        .instructions
        .iter()
        .find(|instruction| instruction.result == Some(value))
    else {
        return false;
    };
    if defining.op != Op::RuntimeCall {
        return false;
    }
    // The lowerer emits either target form depending on whether the call carries a profile.
    let target = match &defining.immediate {
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))) => *target,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction { target, .. })) => *target,
        _ => return false,
    };
    matches!(target, RuntimeFnId::ArrayMap)
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
/// Validates `Op::StoreGlobal`, which a top-level `const NAME = …` and an assignment through
/// `global $x` both lower to.
///
/// `argc`/`argv` are answered by WASI helpers rather than storage, so they have no slot to write
/// and stay refused — PHP would let a program assign them, and silently dropping that write
/// would be worse than not compiling it.
fn store_global_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let Some(Immediate::GlobalName(name)) = inst.immediate else {
        return Some("global store requires a GlobalName immediate".to_string());
    };
    let Some(name) = module.data.global_names.get(name.as_raw() as usize) else {
        return Some("global store references an unknown name".to_string());
    };
    if name == "argc" || name == "argv" {
        return Some(format!("global ${name} is answered by WASI, not stored"));
    }
    let Some(value) = inst
        .operands
        .first()
        .and_then(|operand| function.value(*operand))
    else {
        return Some(format!("global ${name} store has no value operand"));
    };
    match value.php_type.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Float | PhpType::Str => None,
        other => Some(format!(
            "global ${name} of type {other:?} has no slot shape on this target"
        )),
    }
}

/// Validates an `Op::LoadGlobal`: a `GlobalName` immediate naming a known global and a result to
/// materialize, returning the reason this target cannot lower it or `None`.
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
        // Every other global gets a 16-byte slot, exactly like a static property. Only the
        // shapes a slot can hold are admitted: a heap global would have to own its payload for
        // the whole program and the slot has nowhere to record that it does.
        _ => {
            return match inst.result_php_type.codegen_repr() {
                PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Float | PhpType::Str => {
                    None
                }
                other => Some(format!(
                    "global ${name} of type {other:?} has no slot shape on this target"
                )),
            }
        }
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
    // A Mixed value narrowing into a concrete payload is the shape `foreach ($a as &$x) { $x +=
    // 5; }` produces: `$x + 5` types Mixed because the add can overflow into a float, and the
    // cell it writes through is the array's own `int`. `coerce_mixed_ref_cell_store` already
    // emits exactly this through `__rt_mixed_cast_*`, so the gate was the only thing refusing a
    // store the emitter could do — and the native backend answers the same shape correctly.
    //
    // What it INHERITS is the EIR's widening gap, not a new one: on a real overflow the value is
    // a float and narrowing it into an `int` cell is wrong, which is what the native backend
    // does there too. Refusing the whole shape to avoid that costs every ordinary
    // `foreach`-by-reference accumulate, which no program writes expecting an overflow.
    let narrows_mixed_to_concrete = source_php == PhpType::Mixed
        && matches!(target_php, PhpType::Int | PhpType::Bool | PhpType::Float)
        && source.ir_type == IrType::Heap(IrHeapKind::Mixed);
    if !narrows_mixed_to_concrete
        && (source_php != target_php
            || transfer::validate_storage_pair(source.ir_type, &source.php_type).is_err())
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
/// The class whose `__toString` a string conversion of `class_name` must call, when that call
/// is decidable here.
///
/// PHP converts an object at a string boundary by CALLING `__toString`, and raises
/// `Error: Object of class X could not be converted to string` only when there is none. The EIR
/// hands this cast a STATICALLY known class — `Heap(Object)/Object("Talks")`, not a Mixed — so
/// no dynamic dispatch is involved: the conversion is a direct call to an ordinary method body
/// already in the module.
///
/// A method that a subclass could still override is refused: the receiver's static class is not
/// then the class PHP would dispatch on, and answering the base implementation would be wrong
/// for an instance of the subclass. That is the same `vtable_slots` / `final_methods` test
/// `methods::lower_method_call` makes before binding a call directly.
/// Whether a string conversion of `class_name` is PHP's `Error`, decidably and for every
/// instance the receiver could hold.
///
/// PHP raises `Object of class X could not be converted to string` when there is no
/// `__toString`. Answering it needs the same certainty the direct call needs, in reverse: no
/// class the receiver could BE may declare one, or a subclass would convert where this says it
/// cannot. The check is therefore rooted at the whole known class table rather than at the one
/// static class.
pub(super) fn object_never_stringifies(module: &Module, class_name: &str) -> bool {
    let key = crate::names::php_symbol_key("__toString");
    let Some(info) = module.class_infos.get(class_name) else {
        return false;
    };
    if info.methods.contains_key(&key) {
        return false;
    }
    !module.class_infos.iter().any(|(candidate, candidate_info)| {
        candidate_info.methods.contains_key(&key)
            && class_descends_from(module, candidate, class_name)
    })
}

/// Returns the symbol of the `__toString` body an object of `class_name` runs, or `None` when the
/// class has no zero-argument string-returning `__toString` this target can call directly.
pub(super) fn object_to_string_impl(module: &Module, class_name: &str) -> Option<String> {
    let key = crate::names::php_symbol_key("__toString");
    let info = module.class_infos.get(class_name)?;
    let signature = info.methods.get(&key)?;
    if !signature.params.is_empty() || signature.return_type.codegen_repr() != PhpType::Str {
        return None;
    }
    let implementation = info
        .method_impl_classes
        .get(&key)
        .cloned()
        .unwrap_or_else(|| class_name.to_string());
    // A non-final method owns a vtable slot whether or not anything overrides it, so the slot
    // alone does not make the call dynamic. What matters is whether every class the receiver
    // could BE — this class and its descendants, nothing above it — resolves to the same body:
    // PHP dispatches on the RUNTIME class, and answering the base implementation would be wrong
    // for a subclass that overrides it. A sibling branch of the hierarchy cannot reach this
    // receiver, so it does not count against the call.
    let overridden_below = module.class_infos.iter().any(|(candidate, candidate_info)| {
        candidate != class_name
            && class_descends_from(module, candidate, class_name)
            && candidate_info
                .method_impl_classes
                .get(&key)
                .is_some_and(|candidate_impl| *candidate_impl != implementation)
    });
    if overridden_below {
        return None;
    }
    // The body must exist with the exact shape the direct call assumes: a lone receiver in,
    // a string out.
    let body = find_method_function(module, &implementation, &key)?;
    (body.params.len() == 1
        && body.params[0].ir_type == IrType::Heap(IrHeapKind::Object)
        && body.return_type == IrType::Str)
        .then_some(implementation)
}

/// The boxed value a cast is standing in for, when the cast exists only to feed a comparison.
///
/// `$n <= 1` with `$n` untyped lowers to `cast Mixed -> I64` then `icmp`, and that pair is NOT
/// what PHP does: PHP compares without converting. Measured on php-src 8.5.6 — `"abc" <= 1` is
/// FALSE, because a non-numeric string makes PHP render the LONG as a string and compare bytes,
/// while the cast answers 0 and reports true. The native backend still takes the cast and gets
/// both this and `[1] <= 1` wrong; this target answers PHP by comparing the box directly.
///
/// Answers `(cell, other, mixed_on_left)`: the box, the value PHP compares it against, and which
/// side of the `ICmp` the box sits on. Only when EVERY use of the cast's result is that one
/// comparison — otherwise the converted integer is genuinely wanted somewhere and the cast stands.
///
/// The comparison reads the BOX, so it has to run while the box is alive, and the EIR releases an
/// owned temporary — a property read, a call result — immediately after the cast:
///
/// ```text
///   v1 = prop_get v0 data[0]      ; owned
///   v3 = cast v1 I64
///   release v1                    ; the box is gone HERE
///   v4 = icmp v3 v2 Sgt
/// ```
///
/// So the comparison is emitted AT THE CAST rather than at the `ICmp`. There the box is always
/// live, whatever owns it, and the `ICmp` is left comparing the three-way answer against zero.
/// The price is that the value compared against must already be materialized at the cast, which
/// is what the position walk below establishes; an earlier attempt to keep the comparison at the
/// `ICmp` and hold the box alive across the release instead was reverted.
/// The same stand-in when BOTH sides of the comparison are boxed.
///
/// `$a[$i] > $max` with two untyped operands is the commonest shape left in the corpus, and it
/// is what `__rt_mixed_cmp_mixed` exists for. The pair is answered ONCE, by the cast on the
/// left: it yields php-src's -1/0/1, and the cast on the right yields 0, so the `ICmp` the EIR
/// already emitted becomes `pred(cmp, 0)` — the correct predicate with no change to its own
/// lowering. Answering at both casts would compare the pair twice and discard one answer.
///
/// Returns the two CELLS in source order plus whether `inst` is the left one.
pub(super) fn cast_pair_stands_in_for_mixed_comparison(
    function: &Function,
    inst: &Instruction,
) -> Option<(ValueId, ValueId, bool)> {
    if !matches!(inst.immediate, Some(Immediate::CastTarget(IrType::I64))) {
        return None;
    }
    if inst.result_type != IrType::I64 || inst.result_php_type.codegen_repr() != PhpType::Int {
        return None;
    }
    let [source] = inst.operands.as_slice() else {
        return None;
    };
    let value = function.value(*source)?;
    if value.ir_type != IrType::Heap(IrHeapKind::Mixed)
        || value.php_type.codegen_repr() != PhpType::Mixed
    {
        return None;
    }
    let result = inst.result?;
    let mut comparison = None;
    for candidate in &function.instructions {
        if !candidate.operands.contains(&result) {
            continue;
        }
        if candidate.op != Op::ICmp || comparison.is_some() {
            return None;
        }
        comparison = Some(candidate);
    }
    // A terminator use is invisible to the scan above and would read the three-way answer as
    // though it were the integer conversion — the same trap the single-sided stand-in avoids.
    if function
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .any(|terminator| terminator_reads(terminator, result))
    {
        return None;
    }
    let comparison = comparison?;
    let [left, right] = comparison.operands.as_slice() else {
        return None;
    };
    let is_left = *left == result;
    let other = if is_left { *right } else { *left };
    if other == result {
        return None;
    }
    // The other side must be the SAME kind of stand-in: a cast of a boxed value feeding this
    // very comparison. Its own source is the second cell.
    let other_cast = function
        .instructions
        .iter()
        .find(|candidate| candidate.result == Some(other))?;
    if other_cast.op != Op::Cast
        || !matches!(other_cast.immediate, Some(Immediate::CastTarget(IrType::I64)))
    {
        return None;
    }
    let [other_source] = other_cast.operands.as_slice() else {
        return None;
    };
    let other_value = function.value(*other_source)?;
    if other_value.ir_type != IrType::Heap(IrHeapKind::Mixed)
        || other_value.php_type.codegen_repr() != PhpType::Mixed
    {
        return None;
    }
    let (left_cell, right_cell) = if is_left {
        (*source, *other_source)
    } else {
        (*other_source, *source)
    };
    Some((left_cell, right_cell, is_left))
}

/// Recognizes the `(int)` cast the EIR inserts to compare a `mixed` value with an int, rather
/// than one the program wrote.
///
/// Answers the cast's source, the other operand and whether the comparison is negated, so the
/// audit can judge the comparison as a whole; an explicit `(int)` is a real conversion and
/// answers `None`.
pub(super) fn cast_stands_in_for_mixed_comparison(
    function: &Function,
    inst: &Instruction,
) -> Option<(ValueId, ValueId, bool)> {
    if !matches!(inst.immediate, Some(Immediate::CastTarget(IrType::I64))) {
        return None; // an EXPLICIT `(int)` really is a conversion
    }
    if inst.result_type != IrType::I64 || inst.result_php_type.codegen_repr() != PhpType::Int {
        return None;
    }
    let [source] = inst.operands.as_slice() else {
        return None;
    };
    let value = function.value(*source)?;
    if value.ir_type != IrType::Heap(IrHeapKind::Mixed)
        || value.php_type.codegen_repr() != PhpType::Mixed
    {
        return None;
    }
    let result = inst.result?;
    let mut comparison = None;
    for candidate in &function.instructions {
        if !candidate.operands.contains(&result) {
            continue;
        }
        if candidate.op != Op::ICmp || comparison.is_some() {
            return None; // the integer is wanted elsewhere, or by more than one comparison
        }
        comparison = Some(candidate);
    }
    // A TERMINATOR is not in the instruction table, so a use there — a `CondBr` condition, a
    // `Switch` scrutinee, block arguments on any edge — is invisible to the scan above. This
    // result stops being the integer conversion and becomes a -1/0/1 answer, so any such reader
    // would silently get the wrong value.
    if function
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .any(|terminator| terminator_reads(terminator, result))
    {
        return None;
    }
    let comparison = comparison?;
    let [left, right] = comparison.operands.as_slice() else {
        return None;
    };
    let mixed_on_left = *left == result;
    let other = if mixed_on_left { *right } else { *left };
    if other == result {
        return None; // a box compared with itself has no second operand to load
    }
    // `__rt_mixed_cmp_i64` is `zend_compare(mixed, long)`. Against a PHP BOOL, PHP converts both
    // sides to bool first — a different rule — so only a genuine integer is admitted here.
    let other_value = function.value(other)?;
    if other_value.ir_type != IrType::I64 || other_value.php_type.codegen_repr() != PhpType::Int {
        return None;
    }
    // Both sides boxed needs php-src's full cross-type table, which this target does not carry.
    // The other side is an `I64/Int` by now, but it may be the CONVERSION of a box — and then it
    // is a -1/0/1 answer rather than the integer, so comparing against it means nothing.
    if value_is_a_boxed_mixed_cast(function, other) {
        return None;
    }
    // The cast and the comparison share a block, so "already materialized" is a question about
    // one instruction order rather than about dominance.
    let (cast_block, cast_slot) = instruction_position(function, result)?;
    let (cmp_block, cmp_slot) = comparison
        .result
        .and_then(|value| instruction_position(function, value))?;
    if cast_block != cmp_block || cmp_slot <= cast_slot {
        return None;
    }
    // A value defined in ANOTHER block dominates this one already — the comparison uses it here.
    // Only a definition later in THIS block is unavailable at the cast.
    if let Some((other_block, other_slot)) = instruction_position(function, other) {
        if other_block == cast_block && other_slot >= cast_slot {
            return None;
        }
    }
    Some((*source, other, mixed_on_left))
}

/// Returns the boxed source when a `cast Mixed -> I64` exists only to feed integer ARITHMETIC.
///
/// PHP does not cast there either, but it does not compare either — it coerces, under a THIRD
/// contract distinct from both the declared-return and the declared-parameter ones. Measured on
/// php-src 8.5.6 with `$mixed % 3`: `null` is silently 0 where a parameter deprecates and a
/// return raises; a non-numeric string is `Unsupported operand types: string % int` rather than
/// `must be of type int`; and `INF` warns that it is not representable and yields 0 rather than
/// raising at all.
///
/// Only `%` is admitted so far, and only with the box on the LEFT and a concrete integer on the
/// right — the shape `$n % 2` takes. Two boxed operands need php-src's full cross-type table,
/// which is a different problem.
pub(super) fn cast_feeds_integer_arithmetic(
    function: &Function,
    inst: &Instruction,
) -> Option<ValueId> {
    if !matches!(inst.immediate, Some(Immediate::CastTarget(IrType::I64))) {
        return None; // an EXPLICIT `(int)` really is a conversion
    }
    if inst.result_type != IrType::I64 || inst.result_php_type.codegen_repr() != PhpType::Int {
        return None;
    }
    let [source] = inst.operands.as_slice() else {
        return None;
    };
    let value = function.value(*source)?;
    if value.ir_type != IrType::Heap(IrHeapKind::Mixed)
        || value.php_type.codegen_repr() != PhpType::Mixed
    {
        return None;
    }
    let result = inst.result?;
    // The integer must be wanted by exactly one int-coercing operator and nothing else: any
    // would be handed a value produced under a contract it did not ask for.
    let mut arithmetic = None;
    for candidate in &function.instructions {
        if !candidate.operands.contains(&result) {
            continue;
        }
        // All six convert their operand under the same php contract: TypeError for a
        // non-numeric operand, the leading-numeric warning otherwise. Only the SYMBOL
        // in the diagnostic differs, which the lowering picks from the consumer.
        if !matches!(
            candidate.op,
            Op::ISMod | Op::IShl | Op::IShrA | Op::IBitAnd | Op::IBitOr | Op::IBitXor
        ) || arithmetic.is_some()
        {
            return None;
        }
        arithmetic = Some(candidate);
    }
    // A terminator use is invisible to that scan, and would read the same value.
    if function
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .any(|terminator| terminator_reads(terminator, result))
    {
        return None;
    }
    let arithmetic = arithmetic?;
    let [left, right] = arithmetic.operands.as_slice() else {
        return None;
    };
    if *left != result {
        return None; // the box has to be the LEFT operand
    }
    let right_value = function.value(*right)?;
    if right_value.ir_type != IrType::I64
        || right_value.php_type.codegen_repr() != PhpType::Int
        || value_is_a_boxed_mixed_cast(function, *right)
    {
        return None; // the right side must be a genuine integer, not another conversion
    }
    Some(*source)
}

/// Whether a terminator reads `value`, as a condition, a scrutinee, or a branch argument.
fn terminator_reads(terminator: &Terminator, value: ValueId) -> bool {
    match terminator {
        Terminator::Br { args, .. } => args.contains(&value),
        Terminator::CondBr {
            cond,
            then_args,
            else_args,
            ..
        } => *cond == value || then_args.contains(&value) || else_args.contains(&value),
        // Each CASE edge carries its own arguments, not just the default one — a use there is as
        // real as any other, and reading the three-way answer through it would be as wrong.
        Terminator::Switch {
            scrutinee,
            cases,
            default_args,
            ..
        } => {
            *scrutinee == value
                || default_args.contains(&value)
                || cases.iter().any(|case| case.args.contains(&value))
        }
        Terminator::Return { value: returned } => *returned == Some(value),
        Terminator::Throw { value: thrown } => *thrown == value,
        Terminator::GeneratorSuspend {
            key,
            value: yielded,
            resume_args,
            ..
        } => {
            *key == Some(value) || *yielded == Some(value) || resume_args.contains(&value)
        }
        Terminator::Fatal { .. } | Terminator::Unreachable => false,
    }
}

/// Whether `value` is the result of a cast whose source is a boxed Mixed.
///
/// Forwarders are chased: one `Move` between the cast and the use hides the same not-an-integer
/// this rejects directly. A value with no defining instruction — a block parameter — cannot be a
/// stand-in result, because a cast whose result reaches a terminator is refused outright, so no
/// three-way answer ever crosses an edge.
fn value_is_a_boxed_mixed_cast(function: &Function, mut value: ValueId) -> bool {
    for _ in 0..=function.values.len() {
        let Some(defining) = function
            .instructions
            .iter()
            .find(|candidate| candidate.result == Some(value))
        else {
            return false;
        };
        match defining.op {
            Op::Cast => {
                return defining.operands.first().is_some_and(|source| {
                    // A cast of a WIDENING ARTEFACT is not "the conversion of a box": `$i * $i`
                    // on two ints is typed Mixed only because an overflow would promote it to a
                    // float, and narrowing it back is exact for every value that did not. The
                    // same reasoning already admits that cast on its own; refusing it as another
                    // side's operand contradicted it, and left `$i * $i <= $n` and
                    // `$n % ($i + 2)` refused for having a perfectly good integer on the far
                    // side.
                    if value_is_widened_integer_arithmetic(
                        function,
                        *source,
                        &mut HashSet::new(),
                    ) {
                        return false;
                    }
                    function.value(*source).is_some_and(|operand| {
                        operand.ir_type == IrType::Heap(IrHeapKind::Mixed)
                            && operand.php_type.codegen_repr() == PhpType::Mixed
                    })
                })
            }
            Op::Move | Op::Borrow | Op::Acquire => {
                let Some(next) = defining.operands.first() else {
                    return false;
                };
                value = *next;
            }
            _ => return false,
        }
    }
    false
}

/// Locates the instruction defining `result` as a `(block index, position within the block)` pair.
fn instruction_position(function: &Function, result: ValueId) -> Option<(usize, usize)> {
    let defining = function
        .instructions
        .iter()
        .position(|candidate| candidate.result == Some(result))?;
    function.blocks.iter().enumerate().find_map(|(block, entry)| {
        entry
            .instructions
            .iter()
            .position(|id| id.as_raw() as usize == defining)
            .map(|slot| (block, slot))
    })
}

/// The operands of an `ICmp` that is really a PHP comparison against a boxed value.
///
/// Answers `(cell, other, mixed_on_left)`. Only ONE side may be boxed: comparing two boxes needs
/// php-src's full cross-type table, which this target does not carry yet.
pub(super) fn icmp_compares_a_boxed_value(
    function: &Function,
    inst: &Instruction,
) -> Option<(ValueId, ValueId, bool)> {
    let [left, right] = inst.operands.as_slice() else {
        return None;
    };
    let boxed = |value: ValueId| -> Option<(ValueId, ValueId, bool)> {
        let defining = function
            .instructions
            .iter()
            .find(|candidate| candidate.result == Some(value))?;
        (defining.op == Op::Cast)
            .then(|| cast_stands_in_for_mixed_comparison(function, defining))
            .flatten()
    };
    // When BOTH sides are stand-ins the pair was already answered at the left cast and the
    // right one is the literal zero this comparison tests against, so the ordinary two-operand
    // lowering is exactly right. Firing here instead would compare that zero with itself —
    // `0 <= 0` is always true, which turned `while ($i * $i <= $n)` into an endless first pass.
    let pair = |value: ValueId| -> bool {
        function
            .instructions
            .iter()
            .find(|candidate| candidate.result == Some(value))
            .filter(|defining| defining.op == Op::Cast)
            .and_then(|defining| cast_pair_stands_in_for_mixed_comparison(function, defining))
            .is_some()
    };
    if pair(*left) || pair(*right) {
        return None;
    }
    match (boxed(*left), boxed(*right)) {
        (Some(found), None) | (None, Some(found)) => Some(found),
        _ => None,
    }
}

/// The declared return type this cast is PHP's IMPLICIT coercion for, if it is one at all.
///
/// That coercion is a DIFFERENT operation from a `(T)` cast, which is why it needs its own
/// predicate rather than riding on the explicit one: `(int)null` is 0 and `(int)"abc"` is 0,
/// while RETURNING either from a function declared `int` raises a `TypeError`; and `(string)`
/// of an array answers "Array" with a warning where a declared `string` return refuses it.
/// Measured on php-src 8.5.6 for all four scalar targets, with the `int` rules — the only ones
/// with an accepting path that DIVERGES rather than merely narrowing — validated against a
/// 1200-value random sweep. `runtime::emit_return_coercion_runtime` carries them.
///
/// The diagnostic names the function, so only a name PHP would print is admitted. The EIR
/// already spells a method `C::m`, which is PHP's text exactly, but a closure's name there is
/// `{closure:<absolute path>:<line>}` — not derivable from the EIR — so closures and every
/// other compiler-generated body stay refused.
pub(super) fn declared_return_coercion_target(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<IrType> {
    // Under `declare(strict_types=1)` PHP performs NO coercion at all: `return 5.7` from a
    // function declared `int` is an immediate `TypeError`, and so is `return "42"` and even
    // `return true` — whose message names the VALUE (`true returned`), not the type. Those are
    // different rules from the ones measured here, so a strict file stays refused rather than
    // silently answering the weak-mode result.
    if module.strict_types {
        return None;
    }
    let Some(Immediate::CastTarget(target)) = inst.immediate else {
        return None; // an EXPLICIT cast keeps its own, silent rules
    };
    if inst.result_type != target || function.return_type != target {
        return None;
    }
    // A nullable or union return never reaches here: the EIR types `?int` as `TaggedScalar` and
    // `int|string` as `Heap(Mixed)`, so neither matches a scalar target.
    let declared = function.return_php_type.codegen_repr();
    let matches_target = match target {
        IrType::I64 => matches!(declared, PhpType::Int | PhpType::Bool),
        IrType::F64 => declared == PhpType::Float,
        // `string` joined once `__rt_object_to_string` existed: the runtime dispatch this
        // comment once said was missing now answers a Stringable object through its own
        // `__toString`, and `__rt_mixed_return_string` raises php's RETURN TypeError —
        // naming the class — for one without it (measured on php 8.5.6).
        IrType::Str => declared == PhpType::Str,
        _ => false,
    };
    // A declared `bool` return leaves the EIR typing the cast's RESULT `int` — the two share
    // `I64` storage, and the bool-ness lives on the function's return type, which is what a
    // caller reads. So the result type is required to match the target's storage, not the
    // declared PHP type.
    let result_matches = match declared {
        PhpType::Bool => matches!(inst.result_php_type.codegen_repr(), PhpType::Bool | PhpType::Int),
        other => inst.result_php_type.codegen_repr() == other,
    };
    if !matches_target || !result_matches {
        return None;
    }
    let flags = &function.flags;
    // `is_synthetic` is NOT excluded: prelude bodies are compiler-written ordinary
    // methods with faithful declared types, and they are exactly where a boxed value
    // meets a declared return (DateTime::__elephc_*, PDO). The remaining flags carry
    // special ABIs or entry conventions the return rules do not describe.
    if flags.is_main
        || flags.is_closure
        || flags.is_generator
        || flags.is_fiber_wrapper
        || flags.is_callback_wrapper
        || flags.is_runtime_callable_invoker
    {
        return None;
    }
    let result = inst.result?;
    // Only a value that is literally RETURNED gets the return rules; the same cast feeding
    // anything else would be some other coercion with some other message.
    function
        .blocks
        .iter()
        .any(|block| {
            matches!(
                block.terminator,
                Some(Terminator::Return { value: Some(value) }) if value == result
            )
        })
        .then_some(target)
}

/// Validates an `Op::Cast`: one source operand and a source/target pair this target converts,
/// returning the reason it cannot or `None`.
fn cast_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
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
    // The IMPLICIT coercion at a declared `int` return has its own exact runtime, but its
    // diagnostics go through WASI, so it needs a main-bearing command module the way every
    // other diagnostic-producing rule here does.
    let return_coercion = declared_return_coercion_target(module, function, inst).is_some()
        && module.functions.iter().any(|candidate| candidate.flags.is_main);
    // A cast that only feeds a comparison is not emitted as a conversion at all: the comparison
    // reads the box. Admitting it here is what lets PHP's own comparison replace it. The
    // predicate already names the `ICmp` it feeds, so there is nothing further to look for; the
    // runtime traps on an object, a resource or a callable, and that diagnostic goes through WASI.
    let comparison_stand_in = cast_stands_in_for_mixed_comparison(function, inst).is_some()
        && module.functions.iter().any(|candidate| candidate.flags.is_main);
    // The arithmetic coercion diagnoses through WASI, so it is a command-module rule too.
    let arithmetic_coercion = cast_feeds_integer_arithmetic(function, inst).is_some()
        && module.functions.iter().any(|candidate| candidate.flags.is_main);
    // The coercion at a builtin's declared `string` parameter raises through WASI, so it needs a
    // command module the way every other diagnostic-producing rule here does.
    let string_argument_coercion = mixed_string_argument_coercion(function, inst).is_some()
        && module.functions.iter().any(|candidate| candidate.flags.is_main);
    let int_argument_coercion = mixed_int_argument_coercion(function, inst).is_some()
        && module.functions.iter().any(|candidate| candidate.flags.is_main);
    // Two boxed operands of one comparison: `__rt_mixed_cmp_mixed` answers the pair, so neither
    // cast is a conversion. It can reach php's own failure for an object, so it is a
    // command-module rule like the rest.
    let comparison_pair = cast_pair_stands_in_for_mixed_comparison(function, inst).is_some()
        && module.functions.iter().any(|candidate| candidate.flags.is_main);
    let admitted_mixed_scalar = (explicit
        || widened_by_checked_arithmetic
        || return_coercion
        || comparison_stand_in
        || arithmetic_coercion
        || comparison_pair
        || (int_argument_coercion && target == IrType::I64))
        && matches!(target, IrType::I64 | IrType::F64)
        || ((explicit
            || cast_feeds_string_context(function, inst)
            || string_argument_coercion
            // The declared `: string` return, now that the boundary helper carries the
            // full measured matrix (object through __toString, null the RETURN TypeError).
            || return_coercion)
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
    // `(string)$obj` and `echo $obj` reach PHP's `__toString`. With the class known statically
    // that is an ordinary direct call, so the conversion is exact whenever the call is decidable.
    if source.ir_type == IrType::Heap(IrHeapKind::Object) && target == IrType::Str {
        if let PhpType::Object(class_name) = &source_php {
            let has_main = module.functions.iter().any(|f| f.flags.is_main);
            // A class that provably has none raises PHP's `Error` instead — the same certainty,
            // in reverse. That fatal writes through WASI, so it needs a command module.
            if result_php == PhpType::Str
                && (object_to_string_impl(module, class_name).is_some()
                    || (has_main && object_never_stringifies(module, class_name)))
            {
                return None;
            }
        }
    }
    // A `(bool)` cast is php's TRUTHINESS of whatever the source holds, and the lowering
    // routes every one of them through the same emitter `Op::IsTruthy` uses — so the
    // admissible sources are exactly the ones that emitter handles, and no per-storage
    // conversion rule applies. Object is in the list because php calls every object truthy.
    if target == IrType::I64 && result_php == PhpType::Bool {
        let handled = matches!(
            source.ir_type,
            IrType::I64
                | IrType::F64
                | IrType::Str
                | IrType::TaggedScalar
                | IrType::Void
                | IrType::Heap(
                    IrHeapKind::Array
                        | IrHeapKind::Hash
                        | IrHeapKind::Iterable
                        | IrHeapKind::Mixed
                        | IrHeapKind::Union
                        | IrHeapKind::Object
                )
        );
        return (!handled).then(|| {
            format!(
                "(bool) of a {:?}/{source_php:?} source has no truthiness rule on wasm32-wasi",
                source.ir_type
            )
        });
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
        // `(int) $string` parses the LEADING numeric prefix and answers 0 for anything else,
        // silently — the same `__rt_str_to_int` a boxed string already casts through, so the
        // two spellings agree. `(bool) $string` shares the storage and NOTHING else: it is
        // php's truthiness, where the only falsy strings are `""` and `"0"`, and the lowering
        // routes it through the same predicate `Op::IsTruthy` uses.
        (IrType::Str, IrType::I64) => {
            source_php == PhpType::Str && matches!(result_php, PhpType::Int | PhpType::Bool)
        }
        (IrType::I64, IrType::F64) => {
            source_php == PhpType::Int && result_php == PhpType::Float
        }
        // `(float) $string` parses the LEADING numeric prefix and answers 0.0 for anything
        // else, silently — the same parser `(int) $string` routes float-form prefixes through.
        //
        // The EXPLICIT cast is always admitted. The IMPLICIT one — `"10" / 3`, where arithmetic
        // coerces the operand — is admitted only for a LITERAL whose bytes are a well-formed
        // number, because that is the case with no diagnostic to reproduce: php warns first for
        // a non-numeric string and deprecates a leading-numeric one, and neither warning is
        // emitted here. A literal's bytes are known now, so the question is settled now.
        (IrType::Str, IrType::F64) => {
            source_php == PhpType::Str
                && result_php == PhpType::Float
                && (explicit || numeric_string_literal_operand(module, function, inst))
        }
        // `(string) $float` renders through the same `__rt_ftoa` a float in a string context
        // uses, so the two spellings cannot disagree.
        (IrType::F64, IrType::Str) => {
            source_php == PhpType::Float && result_php == PhpType::Str
        }
        // `(string) $int` and `(string) $bool` render through the same `__rt_itoa` that `Op::IToStr`
        // already uses for `"$n"` and `echo $n`, so the spellings agree by construction. PHP renders
        // an integer as decimal, `true` as "1" and `false` as the empty string.
        (IrType::I64, IrType::Str) => {
            matches!(source_php, PhpType::Int | PhpType::Bool) && result_php == PhpType::Str
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

/// Admits float and boxed truthiness once the module can carry its one diagnostic.
///
/// The per-tag ANSWERS were always exact — `__rt_mixed_cast_bool` gets all seventeen right,
/// `"0.0"` being TRUE and `-0.0` FALSE included. What was missing is the warning a NaN raises on
/// the way to `true`, and that goes through WASI: a reactor module has no stderr to write it to,
/// so it keeps the refusal rather than answering silently where php-src speaks.
/// Validates `Op::StrIncDec`: the operand is a BOXED cell or a concrete string, and the
/// result is always a boxed cell — `"9"++` is int(10), so no concrete slot can hold both
/// outcomes. Anything else reaching the helper would be read as a cell POINTER.
fn str_inc_dec_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let [source] = inst.operands.as_slice() else {
        return Some(format!(
            "str_inc_dec takes one operand, got {}",
            inst.operands.len()
        ));
    };
    let value = function.value(*source)?;
    let operand_ok = matches!(
        (value.ir_type, value.php_type.codegen_repr()),
        (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed) | (IrType::Str, PhpType::Str)
    );
    if !operand_ok {
        return Some(format!(
            "str_inc_dec operand is {:?}/{:?}, expected a boxed cell or a string",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if inst.result_type != IrType::Heap(IrHeapKind::Mixed)
        || inst.result_php_type.codegen_repr() != PhpType::Mixed
    {
        return Some(format!(
            "str_inc_dec result is {:?}/{:?}, expected a boxed cell",
            inst.result_type,
            inst.result_php_type.codegen_repr()
        ));
    }
    if !matches!(inst.immediate, Some(Immediate::I64(1 | -1))) {
        return Some("str_inc_dec delta must be +1 or -1".to_string());
    }
    None
}

/// Validates a truthiness test, returning the reason this target cannot lower it or `None`.
///
/// A float or `mixed` operand may hold NaN, whose php diagnostic needs the command runtime, so
/// such a test is only admitted where that runtime exists.
fn truthiness_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let [operand] = inst.operands.as_slice() else {
        return None;
    };
    let value = function.value(*operand)?;
    let needs_nan_diagnostic = value.ir_type == IrType::F64
        || value.ir_type == IrType::Heap(IrHeapKind::Mixed)
        || matches!(value.php_type.codegen_repr(), PhpType::Float | PhpType::Mixed);
    if needs_nan_diagnostic
        && !module.functions.iter().any(|candidate| candidate.flags.is_main)
    {
        return Some(
            "float or Mixed truthiness needs a command module for its NaN diagnostic".to_string(),
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
    // A TAGGED scalar is the int-or-null an `array<int>` read answers. PHP renders its null arm
    // as the empty string — `$a[9] . "|"` is just `"|"` — so the pair is exact, and refusing it
    // turned away `echo $values[0] . "|" . $values[1]` over an ordinary list of ints.
    let tagged_int_or_null = source.ir_type == IrType::TaggedScalar
        && source.php_type.codegen_repr() == PhpType::TaggedScalar;
    if !tagged_int_or_null
        && (source.ir_type != IrType::I64
            || !matches!(source.php_type.codegen_repr(), PhpType::Int | PhpType::Bool))
    {
        return Some(format!(
            "IToStr requires I64/Int, I64/Bool or a tagged int|null, got {:?}/{:?}",
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
fn loose_eq_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
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
        // A BOXED operand goes through php-src's `zend_compare`, which the two comparison
        // helpers implement. Against a concrete side only a genuine INT is admitted: a PHP bool
        // makes BOTH sides booleans, a different rule. These can reach php's own failure for an
        // object, so they are command-module rules like every other diagnosing one.
        ((IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed), (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed))
        | ((IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed), (IrType::I64, PhpType::Int))
        | ((IrType::I64, PhpType::Int), (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed)) => {
            module.functions.iter().any(|candidate| candidate.flags.is_main)
        }
        // The int|null TAGGED pair against a genuine int: an int payload compares the
        // longs, and null equals only 0 (php's bool rule). Both are exact and warn-free.
        ((IrType::TaggedScalar, PhpType::TaggedScalar), (IrType::I64, PhpType::Int))
        | ((IrType::I64, PhpType::Int), (IrType::TaggedScalar, PhpType::TaggedScalar)) => true,
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

/// Validates `Op::ReturnBoundaryMixedToInt`: PHP's verification at a DECLARED `int` return.
///
/// This is not a `(int)` cast and does not share its rules: an in-range float truncates after a
/// deprecation, a numeric string coerces, and a float outside `[-2^63, 2^63)`, a non-numeric
/// string, null, or any container raises a catchable `TypeError` naming the runtime type. The
/// whole of that is `__rt_mixed_return_int`, which this target already carries.
///
/// Admitted operand storage is the boxed cell the helper reads, or an integer word that is
/// already the answer. A raw `F64` can only appear once a fold has retyped the operand under
/// the op, which this target's pipeline does not do, so it stays refused rather than boxed on
/// a path no test could reach.
///
/// The `TypeError` text goes through WASI, so this is a main-bearing command rule.
fn return_boundary_int_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let [source] = inst.operands.as_slice() else {
        return Some(format!(
            "return_boundary_mixed_to_int takes one operand, got {}",
            inst.operands.len()
        ));
    };
    let Some(value) = function.value(*source) else {
        return Some(
            "return_boundary_mixed_to_int operand is missing from the value table".to_string(),
        );
    };
    let admitted = match value.ir_type {
        IrType::Heap(IrHeapKind::Mixed) => value.php_type.codegen_repr() == PhpType::Mixed,
        IrType::I64 => matches!(
            value.php_type.codegen_repr(),
            PhpType::Int | PhpType::Bool | PhpType::False
        ),
        _ => false,
    };
    if !admitted {
        return Some(format!(
            "return_boundary_mixed_to_int on {:?}/{:?}: this lowering reads a boxed cell or an \
             integer word",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if inst.result_type != IrType::I64 {
        return Some(format!(
            "return_boundary_mixed_to_int answers an integer word, not {:?}",
            inst.result_type
        ));
    }
    if !matches!(inst.immediate, Some(Immediate::Data(_))) {
        return Some(
            "return_boundary_mixed_to_int carries the TypeError message prefix as data"
                .to_string(),
        );
    }
    if !module.functions.iter().any(|candidate| candidate.flags.is_main) {
        return Some(
            "return_boundary_mixed_to_int raises php's TypeError, whose diagnostic needs a \
             main-bearing command module"
                .to_string(),
        );
    }
    None
}

/// Validates `Op::ReleaseUnlessAliases`: a release the callee's answer can cancel.
///
/// A nullsafe method call hands its receiver to the callee and then releases it — unless the
/// callee returned that very pointer, in which case ownership moved into the result and a
/// release here would free a live value. The operands are therefore the argument and the
/// result, and the comparison is on the POINTERS.
///
/// Admitted when the released operand is a heap pointer, which is the only storage that can
/// alias a returned one; a scalar has nothing to release and nothing to compare, so the
/// instruction is a no-op for it rather than a refusal.
fn release_unless_aliases_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let [value, result] = inst.operands.as_slice() else {
        return Some(format!(
            "release_unless_aliases takes the released value and the call result, got {}",
            inst.operands.len()
        ));
    };
    for operand in [value, result] {
        let Some(found) = function.value(*operand) else {
            return Some(
                "release_unless_aliases operand is missing from the value table".to_string(),
            );
        };
        if !matches!(
            found.ir_type,
            IrType::Heap(_) | IrType::I64 | IrType::F64 | IrType::Str | IrType::Void
        ) {
            return Some(format!(
                "release_unless_aliases on {:?}: this lowering compares pointers",
                found.ir_type
            ));
        }
    }
    None
}

/// Validates `Op::ZeroLocalSlot`: the slot whose storage is being written back to zero.
///
/// EIR emits this where a binding is ABANDONED rather than overwritten — `$q = 1` after
/// `$q` held a string — because the frame epilogue still walks that slot and a stale pointer
/// left in it would be freed a second time. Zero is what both prologues leave a cleanup-tracked
/// slot in, so the epilogue walks past it.
///
/// The only requirement is that the slot exists: every WASM storage shape is a fixed set of
/// locals, and writing the zero of each local's own type is always expressible.
fn zero_local_slot_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    if !inst.operands.is_empty() {
        return Some(format!(
            "zero_local_slot takes no operands, got {}",
            inst.operands.len()
        ));
    }
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return Some("zero_local_slot names its slot as a LocalSlot immediate".to_string());
    };
    if function.locals.get(slot.as_raw() as usize).is_none() {
        return Some(format!("zero_local_slot: unknown slot {slot:?}"));
    }
    None
}

/// Whether `value` is a boxed array the checker guarantees: php's declared `array`.
///
/// A plain `mixed` could hold a string, an object implementing `ArrayAccess`, or anything else
/// a write through `[]` means something different for. A declared `array` cannot: the
/// parameter, property and return boundaries already raised php's `TypeError` for anything
/// else, so the cell holds a list or a hash.
fn value_is_a_declared_array_cell(function: &Function, value: ValueId) -> bool {
    function.value(value).is_some_and(|value| {
        value.ir_type == IrType::Heap(IrHeapKind::Mixed) && value.php_type.is_php_array()
    })
}

/// Whether `value` is one the container-write lowering can box into a fresh cell of its own.
///
/// Scalars, strings and `int|null` pairs have an exact tag and payload, and boxing them hands
/// the write the cell's only reference. A boxed value is COPIED into a new cell — its string
/// persisted, its container shared copy-on-write — so the write never takes over the source's
/// reference, whatever the EIR does with it. A raw container is left out: boxing it takes over
/// the operand's reference, which is only right where nothing else releases it.
pub(super) fn value_boxes_into_a_container_write(function: &Function, value: ValueId) -> bool {
    function.value(value).is_some_and(|value| {
        matches!(
            (value.ir_type, value.php_type.codegen_repr()),
            (IrType::I64, PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Void)
                | (IrType::F64, PhpType::Float)
                | (IrType::Str, PhpType::Str)
                | (IrType::TaggedScalar, PhpType::TaggedScalar)
                | (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed)
        )
    })
}

/// Validates `Op::MixedArrayAppend`: `$a[] = $v` through a declared `array`.
///
/// `__rt_mixed_cell_append` republishes the container inside the cell, so the receiver must be
/// that cell, and the module must carry the command runtime its php fatal is reported through.
fn mixed_array_append_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let [receiver, value] = inst.operands.as_slice() else {
        return Some(format!(
            "mixed_array_append takes a receiver and a value, got {} operands",
            inst.operands.len()
        ));
    };
    if !module.functions.iter().any(|candidate| candidate.flags.is_main) {
        return Some("mixed_array_append reports its fatals through the command runtime".into());
    }
    if !value_is_a_declared_array_cell(function, *receiver) {
        return Some(format!(
            "mixed_array_append on {:?}: only a declared `array` is known to hold one",
            function.value(*receiver).map(|value| value.php_type.clone())
        ));
    }
    if !value_boxes_into_a_container_write(function, *value) {
        return Some(format!(
            "mixed_array_append of {:?} (a container or boxed element is not lowered)",
            function.value(*value).map(|value| (value.ir_type, value.php_type.codegen_repr()))
        ));
    }
    None
}

/// Returns true when an untyped `Op::RuntimeCall` is `$a[$k] = $v` through a declared `array`.
///
/// Three operands and no result, like `offsetSet` — but the receiver is a boxed array cell
/// rather than an object, which is what tells the two apart. The key is an int or a string,
/// whose normalized form `__rt_mixed_cell_set` takes; a float, bool or null key carries php's
/// own coercion diagnostics and is refused.
pub(super) fn mixed_array_write_is_supported(
    module: &Module,
    function: &Function,
    call: &Instruction,
) -> bool {
    let [receiver, key, value] = call.operands.as_slice() else {
        return false;
    };
    call.is_void()
        && module.functions.iter().any(|candidate| candidate.flags.is_main)
        && value_is_a_declared_array_cell(function, *receiver)
        && function.value(*key).is_some_and(|key| {
            matches!(
                (key.ir_type, key.php_type.codegen_repr()),
                (IrType::I64, PhpType::Int) | (IrType::Str, PhpType::Str)
            )
        })
        && value_boxes_into_a_container_write(function, *value)
}

/// Validates `Op::PropGetForWrite` on a declared `array` property, the receiver of a write.
///
/// The slot holds the property's cell. The lowering replaces it with a private copy before the
/// write — a cell another owner shares must not see the write — so it needs a concrete class
/// layout, a boxed slot that is not a reference, and proof the slot was written: php raises
/// `Error` for a typed property read before initialization, and an unwritten slot holds no cell.
fn prop_get_for_write_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let [receiver_id] = inst.operands.as_slice() else {
        return Some("prop_get_for_write takes one receiver".to_string());
    };
    let Some(receiver) = function.value(*receiver_id) else {
        return Some("prop_get_for_write receiver is missing from the value table".to_string());
    };
    let PhpType::Object(class_name) = receiver.php_type.codegen_repr() else {
        return Some(format!(
            "prop_get_for_write on {:?}: a concrete class layout is required",
            receiver.php_type.codegen_repr()
        ));
    };
    if receiver.ir_type != IrType::Heap(IrHeapKind::Object) {
        return Some("prop_get_for_write receiver must be an object pointer".to_string());
    }
    let class_name = class_name.trim_start_matches('\\').to_string();
    let Some(class_info) = module.class_infos.get(&class_name) else {
        return Some(format!("prop_get_for_write on unknown class {class_name}"));
    };
    let Some(property) = data_string(module, inst) else {
        return Some("prop_get_for_write names its property as data".to_string());
    };
    let Some(property_index) = class_info
        .properties
        .iter()
        .position(|(name, _)| name == property)
    else {
        return Some(format!("prop_get_for_write of undeclared ${property} on {class_name}"));
    };
    let property_type = class_info.properties[property_index].1.codegen_repr();
    if property_type != PhpType::Mixed || class_info.reference_properties.contains(property) {
        return Some(format!(
            "prop_get_for_write of ${property}: only a boxed, non-reference slot is lowered, got {property_type:?}"
        ));
    }
    if typed_property_may_be_uninitialized(
        module,
        function,
        inst,
        &class_name,
        class_info,
        property,
        property_index,
    ) {
        return Some(format!(
            "typed property ${property} may be uninitialized and requires an exact PHP fatal check"
        ));
    }
    if inst.result_type != IrType::Heap(IrHeapKind::Mixed) {
        return Some(format!(
            "prop_get_for_write of a boxed slot answers its cell, not {:?}",
            inst.result_type
        ));
    }
    None
}

/// Validates `Op::MixedClone`: a fresh, independently owned cell copied from a boxed value.
///
/// EIR emits it where a by-value copy of a `mixed` must not share its CELL with the source — a
/// `mixed` parameter's copy-on-write shadow, a by-value assignment of a boxed value — so that
/// replacing the copy's payload leaves the source alone. The lowering reads the source's
/// `(tag, lo, hi)` and builds a new cell from it, which persists a string copy and increfs a
/// refcounted child; only a cell source (Mixed or a boxed union) has those three words.
fn mixed_clone_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let [source] = inst.operands.as_slice() else {
        return Some(format!("mixed_clone takes one operand, got {}", inst.operands.len()));
    };
    let source_ir = function.value(*source).map(|value| value.ir_type);
    if !matches!(
        source_ir,
        Some(IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union))
    ) {
        return Some(format!(
            "mixed_clone of a non-cell source {source_ir:?} (boxing a concrete value is mixed_box)"
        ));
    }
    let result_ir = inst
        .result
        .and_then(|result| function.value(result))
        .map(|value| value.ir_type);
    if result_ir != Some(IrType::Heap(IrHeapKind::Mixed)) {
        return Some(format!("mixed_clone must produce a Mixed cell, got {result_ir:?}"));
    }
    None
}

/// Validates `Op::PushCallOperandOwner` / `Op::PopCallOperandOwner`: the frame slot they scope.
///
/// The pair brackets a call whose operand an owner SLOT holds, so that a throw inside the call
/// retires the slot from php's unwind record. The normal path retires it with its own
/// `release_local_slot`, which this target already lowers; the pair adds nothing there.
/// This target does not release a frame's owned values while unwinding (a known gap shared by
/// every function), so a slot published here leaks on a throw exactly like any other local.
fn call_operand_owner_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let name = inst.op.name();
    if !inst.operands.is_empty() || inst.result.is_some() {
        return Some(format!("{name} takes no operands and produces no value"));
    }
    let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
        return Some(format!("{name} names its owner as a LocalSlot immediate"));
    };
    if function.locals.get(slot.as_raw() as usize).is_none() {
        return Some(format!("{name}: unknown slot {slot:?}"));
    }
    None
}

/// Validates `Op::TypePredicate` (`is_int()`, `is_countable()`, ...) against the two answers
/// `lower_type_predicate` can give: one the static type decides, or one a Mixed cell's tag decides.
///
/// `is_countable()` and `is_iterable()` over a boxed value also accept a Countable or Traversable
/// OBJECT, which the tag cannot tell apart, so they are refused here by name instead of failing
/// later in lowering with an anonymous backend error.
fn type_predicate_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let Some(Immediate::TypePredicate(predicate)) = inst.immediate else {
        return Some("type predicate without a PhpTypePredicate immediate".to_string());
    };
    let source = inst
        .operands
        .first()
        .and_then(|operand| function.value(*operand))
        .map(|value| value.php_type.codegen_repr())?;
    if source == PhpType::Mixed {
        if super::inst::boxed_predicate_tags(predicate).is_none() {
            return Some(format!(
                "type predicate {predicate:?} over a Mixed value needs a class check the cell tag cannot make"
            ));
        }
    } else if super::inst::static_type_predicate(module, &source, predicate).is_none() {
        return Some(format!("type predicate {predicate:?} over {source:?}"));
    }
    None
}

/// Validates the unwind-guard runtime calls: `exception.guard_owned`, `exception.unguard_owned`
/// and the two guard refreshes after a container may have moved.
///
/// Natively they maintain the chain a throw walks to release values a statement still owns.
/// The normal path never releases through them (`unguard_owned` drops the guard, not the value),
/// so lowering them to nothing is exact there; the guard token is only ever another guard's
/// anchor or an unguard's operand. As with `call_operand_owner_shape_issue`, a value guarded
/// here leaks on a throw, like every value a frame owns when an exception leaves it.
fn exception_guard_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeCallTarget,
) -> Option<String> {
    let ir = |index: usize| {
        call.operands
            .get(index)
            .and_then(|operand| function.value(*operand))
            .map(|value| value.ir_type)
    };
    let result_ir = call
        .result
        .and_then(|result| function.value(result))
        .map(|value| value.ir_type);
    let (name, operands, token_operand, result) = match target {
        RuntimeCallTarget::ExceptionGuardOwned => ("exception.guard_owned", 2, 1, Some(IrType::I64)),
        RuntimeCallTarget::ExceptionUnguardOwned => ("exception.unguard_owned", 1, 0, None),
        RuntimeCallTarget::ExceptionUpdateArrayGuard => ("exception.update_array_guard", 2, 1, None),
        _ => ("exception.update_hash_guard", 2, 1, None),
    };
    if call.operands.len() != operands
        || ir(token_operand) != Some(IrType::I64)
        || result_ir != result
    {
        return Some(format!(
            "{name} takes {operands} operand(s) with an i64 guard token and answers {result:?}"
        ));
    }
    None
}

/// Returns whether a declared typed property can be READ before it is ever assigned.
///
/// PHP answers such a read with `Error: Typed property C::$p must not be accessed before
/// initialization`. The object allocator zeroes the slot, and zero is a legitimate value for an
/// int or a bool, so there is no sentinel to test — implementing that check needs an
/// initialization bitmap this target does not carry. Four situations prove the write happened:
///
/// - a DEFAULT, which the allocator stores;
/// - CONSTRUCTOR PROMOTION, which assigns from the signature before the body runs;
/// - an ENUM CASE, whose `name`/`value` the materializer writes and which
///   `scoped_constant_get` is the only way to obtain;
/// - an ABSTRACT declaration every concrete descendant initializes — no instance has the
///   unwritten slot, so refusing the abstract declaration's own read answers a question no
///   object can ask;
/// - a CONSTRUCTOR that writes it unconditionally, which covers every read from outside;
///   inside the constructor the store must additionally be shown to come first, which is a
///   question about this one read rather than about the class.
fn typed_property_may_be_uninitialized(
    module: &Module,
    function: &Function,
    inst: &Instruction,
    class_name: &str,
    class_info: &crate::types::ClassInfo,
    property: &str,
    property_index: usize,
) -> bool {
    class_info
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
        && !module.enum_infos.contains_key(class_name)
        && !every_concrete_descendant_initializes(module, class_name, property)
        && !(function.name != format!("{class_name}::__construct")
            && constructor_initializes_property(module, class_name, property))
        && !(function.name == format!("{class_name}::__construct")
            && constructor_initializes_property(module, class_name, property)
            && constructor_store_precedes(module, function, inst, property))
}

/// Whether an instance of `class_name` can hold `property` UNINITIALIZED when an outside
/// observer walks it, which is what `json_encode` and `get_object_vars` do.
///
/// The class-level twin of `typed_property_may_be_uninitialized`: same conjunction, minus the
/// two clauses that are about one read inside one function. A typed property with no default
/// that no constructor writes is skipped by php's walks, and this backend's zeroed slot is
/// indistinguishable from a legitimate `0`, so the walkers refuse such a class by name instead
/// of exposing the slot.
pub(super) fn class_property_may_be_uninitialized(
    module: &Module,
    class_name: &str,
    property: &str,
) -> bool {
    let Some(class_info) = module.class_infos.get(class_name) else {
        return false;
    };
    let Some(property_index) = class_info
        .properties
        .iter()
        .position(|(name, _)| name == property)
    else {
        return false;
    };
    class_info
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
        && !module.enum_infos.contains_key(class_name)
        && !every_concrete_descendant_initializes(module, class_name, property)
        && !constructor_initializes_property(module, class_name, property)
}

/// Why a receiver's PHP type names no single declared class layout.
///
/// The two are different mistakes and each boundary has more context to name them with, so the
/// RULE lives in `receiver_object_class` and the WORDING stays with the caller.
enum ReceiverClassIssue {
    /// A union of objects naming more than one class: no single layout to read.
    SeveralClasses,
    /// Not an object receiver at all.
    NotAnObject,
}

/// Answers the class whose declared layout a property boundary reads, opening a NULLABLE object.
///
/// `?C` reaches this backend as `Union([Object(C), Void])` stored as one boxed Mixed cell, and
/// three boundaries ask the same question of it: `->`, which warns on the null arm, `?->`, which
/// answers null silently, and the `isset` probe, which answers false. They differ only in what
/// they do about null — never in which layout they read — so the opening is done once here
/// rather than copied, which is how the probe came to refuse a receiver `->` was already
/// admitting.
fn receiver_object_class(php_type: &PhpType) -> Result<String, ReceiverClassIssue> {
    match php_type {
        PhpType::Object(class_name) => Ok(class_name.clone()),
        PhpType::Union(variants)
            if variants
                .iter()
                .all(|variant| matches!(variant, PhpType::Object(_) | PhpType::Void)) =>
        {
            let mut classes = variants.iter().filter_map(|variant| match variant {
                PhpType::Object(class_name) => Some(class_name.clone()),
                _ => None,
            });
            match (classes.next(), classes.next()) {
                (Some(class_name), None) => Ok(class_name),
                _ => Err(ReceiverClassIssue::SeveralClasses),
            }
        }
        _ => Err(ReceiverClassIssue::NotAnObject),
    }
}

/// Whether this method call is one the generator frame answers directly.
///
/// `Generator`'s methods have no bodies to lower — they read the frame, which is this backend's
/// own structure. Only `getReturn()` is answered in the first slice: the frame stores what the
/// body returned because that is the whole point of a generator's `return`, so the read is one
/// load. Every other method needs a frame word this slice does not maintain, and falls through
/// to the ordinary missing-body refusal rather than being listed here.
fn generator_method_is_lowered(
    function: &Function,
    inst: &Instruction,
    method_name: &str,
) -> bool {
    if !receiver_is_generator(function, inst) {
        return false;
    }
    matches!(
        method_name.to_ascii_lowercase().as_str(),
        "getreturn" | "current" | "key" | "valid" | "next" | "send" | "throw" | "rewind"
    )
}

/// Refuses a `Generator` method the frame cannot answer yet, by NAME rather than by absence.
///
/// Without this the diagnostic would be "missing method body Generator::send", which describes
/// the symbol table rather than the reason — there is no body to find for any of them.
fn generator_method_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
    method_name: &str,
) -> Option<String> {
    let _ = module;
    if !receiver_is_generator(function, inst) {
        return None;
    }
    // Every `Generator` method the frame answers is lowered now. The gate stays because the op
    // is admitted and an admitted op is shape-checked: a name this backend does not know must
    // refuse by name rather than fall through to the method-table lookup, which would report a
    // missing BODY for a method that never had one.
    if generator_method_is_lowered(function, inst, method_name) {
        return None;
    }
    Some(format!(
        "Generator::{method_name} is not one of the methods this frame answers"
    ))
}

/// Validates the ARGUMENT of `$gen->throw($e)`, which the resume raises as an object.
///
/// It has to BE an object, with its class id where the landing pad reads one: a boxed cell would
/// be thrown as though its tag word were a class id, and no `catch` clause would match.
fn generator_throw_argument_issue(function: &Function, inst: &Instruction) -> Option<String> {
    if inst.operands.len() != 2 {
        return Some(format!(
            "Generator::throw takes one exception, got {} arguments",
            inst.operands.len().saturating_sub(1)
        ));
    }
    let value = inst.operands.get(1).and_then(|id| function.value(*id))?;
    match (value.ir_type, value.php_type.codegen_repr()) {
        (IrType::Heap(IrHeapKind::Object), PhpType::Object(_)) => None,
        (ir_type, php_type) => Some(format!(
            "Generator::throw needs an exception object, got {ir_type:?}/{php_type:?}"
        )),
    }
}

/// Validates the ARGUMENT of `$gen->send($v)`, which the frame parks as a boxed cell.
fn generator_send_argument_issue(function: &Function, inst: &Instruction) -> Option<String> {
    if inst.operands.len() != 2 {
        return Some(format!(
            "Generator::send takes one value, got {} arguments",
            inst.operands.len().saturating_sub(1)
        ));
    }
    let value = inst.operands.get(1).and_then(|id| function.value(*id))?;
    if value.ir_type == IrType::Heap(IrHeapKind::Mixed) {
        return None;
    }
    super::transfer::classify_transfer(
        value.ir_type,
        value.php_type.codegen_repr(),
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    )
    .err()
    .map(|_| {
        format!(
            "a sent {:?}/{:?} has no boxing, and the frame parks a boxed cell",
            value.ir_type,
            value.php_type.codegen_repr()
        )
    })
}

/// Whether a method call's receiver is a `Generator`.
fn receiver_is_generator(function: &Function, inst: &Instruction) -> bool {
    inst.operands
        .first()
        .and_then(|id| function.value(*id))
        .map(|value| value.php_type.codegen_repr())
        .is_some_and(|php_type| {
            matches!(php_type, PhpType::Object(name) if name.trim_start_matches('\\') == "Generator")
        })
}

/// Validates `Op::GeneratorYield`: `yield $v`, which suspends the body.
///
/// The suspension itself needs no proof — the frame carries every local and the dispatch
/// loop re-enters at the segment past the yield. What this checks is the boundary of the
/// FIRST generator slice, so every shape outside it is refused by name rather than
/// miscompiled:
///
/// * a KEY (`yield $k => $v`) needs the frame's `auto_key_counter` and `last_key`, and
///   `iter_current_key` to read them;
/// * `yield from` needs the frame's `delegated_iter` and an inner-iterator loop;
/// * a yield inside a `try` would have to spill the armed handler as well, and this
///   target's handler lives in a local the frame has no word for;
/// * a by-REFERENCE local in the body is a ref-cell pointer, which a frame word cannot
///   carry across a suspension without the cell outliving the call.
fn generator_yield_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let _ = module;
    if !function.flags.is_generator {
        return Some("generator_yield outside a function marked as a generator".to_string());
    }
    // php runs a `finally` when a SUSPENDED generator is destroyed (`unset($g)` mid-iteration).
    // A frame here is released without resuming it, so the block would silently never run.
    if function.flags.yield_in_try_finally {
        return Some(
            "a yield inside a try that has a finally block: php runs the finally when a suspended \
             generator is destroyed, which this target does not do"
                .to_string(),
        );
    }
    // A KEY arrives as a second OPERAND, which is what distinguishes `yield $k => $v` from
    // `yield $v`. Both are admitted; anything else is not a yield this backend knows.
    if !matches!(inst.operands.len(), 1 | 2) {
        return Some(format!(
            "generator_yield takes a value and an optional key, got {} operands",
            inst.operands.len()
        ));
    }
    if function
        .locals
        .iter()
        .any(|local| local.kind == crate::ir::LocalKind::RefCell)
    {
        return Some(
            "a by-reference local in a generator body outlives the suspension, which a frame word \
             cannot carry"
                .to_string(),
        );
    }
    // Both the value and the key are boxed into the frame, so both have to be boxable.
    let value_index = inst.operands.len() - 1;
    let Some(value) = inst.operands.get(value_index).and_then(|id| function.value(*id)) else {
        return Some("generator_yield value is missing from the value table".to_string());
    };
    if inst.operands.len() == 2 {
        let Some(key) = inst.operands.first().and_then(|id| function.value(*id)) else {
            return Some("generator_yield key is missing from the value table".to_string());
        };
        if super::transfer::classify_transfer(
            key.ir_type,
            key.php_type.codegen_repr(),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        )
        .is_err()
        {
            return Some(format!(
                "a yielded key of {:?}/{:?} has no boxing, and the frame carries a boxed cell",
                key.ir_type,
                key.php_type.codegen_repr()
            ));
        }
    }
    if value.ir_type != IrType::Heap(IrHeapKind::Mixed)
        && super::transfer::classify_transfer(
            value.ir_type,
            value.php_type.codegen_repr(),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        )
        .is_err()
    {
        return Some(format!(
            "a yielded {:?}/{:?} has no boxing, and the frame carries a boxed cell",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `Op::GeneratorYieldFrom`: `yield from <source>`, a suspension that repeats.
///
/// The source is the whole question. Delegating to another GENERATOR needs nothing the frame does
/// not already have: `delegated_iter` holds it, and advancing it is one resume of a body this
/// backend compiled the same way. Delegating to an ARRAY or any other `Traversable` needs a CURSOR
/// as well — where the source has got to — and the frame's header has no word for one, so it is
/// refused by name rather than delegated to and read from the top each time.
///
/// The two constraints a plain yield has apply here for the same reasons, since a delegation
/// suspends: a `try` would have to carry its armed handler across, and a by-reference local would
/// have to outlive the call.
fn generator_yield_from_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let _ = module;
    if !function.flags.is_generator {
        return Some("generator_yield_from outside a function marked as a generator".to_string());
    }
    if inst.operands.len() != 1 {
        return Some(format!(
            "generator_yield_from takes the delegated source, got {} operands",
            inst.operands.len()
        ));
    }
    if function
        .locals
        .iter()
        .any(|local| local.kind == crate::ir::LocalKind::RefCell)
    {
        return Some(
            "a by-reference local in a generator body outlives the suspension, which a frame word              cannot carry"
                .to_string(),
        );
    }
    let Some(source) = inst.operands.first().and_then(|id| function.value(*id)) else {
        return Some("generator_yield_from source is missing from the value table".to_string());
    };
    match (source.ir_type, source.php_type.codegen_repr()) {
        (IrType::Heap(IrHeapKind::Object), PhpType::Object(name))
            if name.trim_start_matches('\\') == "Generator" =>
        {
            None
        }
        (ir_type, php_type) => Some(format!(
            "`yield from` a {ir_type:?}/{php_type:?} needs a cursor for how far the source has              got, and the generator frame has no word for one; delegating to a Generator needs              none, because the source keeps its own"
        )),
    }
}

/// Validates `Op::DynamicPropGet` / `Op::DynamicPropSet`: `$obj->$name` with a COMPUTED name.
///
/// The name is a run-time string, so nothing here can say which property it means — and that is
/// the whole constraint. It is routed to the object's dynamic-property hash, which is only the
/// right answer when no declared slot could have answered instead: the class must carry a
/// dyn-prop tail AND declare no property slots. With a slot present the name might name it, and
/// deciding which needs a chain of string compares this backend does not emit; sending it to the
/// hash regardless would read a declared property as absent, silently. The native backend draws
/// the same line.
///
/// `stdClass` is the shape this is really for — every property of it is dynamic, which is what
/// makes it what hydrators and decoders build.
/// The literal a dynamic property name operand is defined by, with its data id.
pub(super) fn literal_property_name_data(
    function: &Function,
    inst: &Instruction,
) -> Option<crate::ir::DataId> {
    let name = *inst.operands.get(1)?;
    let defining = function
        .instructions
        .iter()
        .find(|candidate| candidate.result == Some(name))?;
    match (defining.op, defining.immediate.as_ref()) {
        (Op::ConstStr, Some(Immediate::Data(data))) => Some(*data),
        _ => None,
    }
}

/// The literal text of a dynamic property name operand, when a `ConstStr` defines it.
fn literal_property_name<'m>(
    module: &'m Module,
    function: &Function,
    inst: &Instruction,
) -> Option<&'m str> {
    let data = literal_property_name_data(function, inst)?;
    module.data.strings.get(data.as_raw() as usize).map(String::as_str)
}

/// Validates a computed-name property read or write (`$o->$name`), returning the reason this
/// target cannot lower it or `None`.
///
/// The receiver must be one concrete class with dynamic-property storage and no declared
/// property the name could also reach, and the name must be a string value.
fn dynamic_property_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let expected = if inst.op == Op::DynamicPropSet { 3 } else { 2 };
    if inst.operands.len() != expected {
        return Some(format!(
            "{} takes {expected} operands, got {}",
            inst.op.name(),
            inst.operands.len()
        ));
    }
    let Some(receiver) = inst.operands.first().and_then(|id| function.value(*id)) else {
        return Some("dynamic property receiver is missing from the value table".to_string());
    };
    // `$mixed->k ?? $default` reaches here with the name as a literal operand: it is the
    // ordinary `$mixed->prop` read, whose class-id ladder needs only the name.
    if inst.op == Op::DynamicPropGet
        && matches!(receiver.php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
    {
        if let Some(property) = literal_property_name(module, function, inst) {
            return mixed_property_read_named_issue(module, inst, &receiver.php_type, property);
        }
    }
    let PhpType::Object(class_name) = receiver.php_type.codegen_repr() else {
        return Some(format!(
            "a computed property name on {:?}, which is not a concrete object",
            receiver.php_type.codegen_repr()
        ));
    };
    let class_name = class_name.trim_start_matches('\\').to_string();
    let Some(class_info) = module.class_infos.get(&class_name) else {
        return Some(format!("dynamic property access on unknown class {class_name}"));
    };
    if !class_info.allow_dynamic_properties
        && !class_name.eq_ignore_ascii_case("stdClass")
    {
        return Some(format!(
            "{class_name} carries no dynamic-property storage for a computed name"
        ));
    }
    if !class_info.properties.is_empty() {
        return Some(format!(
            "a computed property name on {class_name}, which declares {} propert{} the name could \
             also reach",
            class_info.properties.len(),
            if class_info.properties.len() == 1 { "y" } else { "ies" }
        ));
    }
    // The NAME has to be a string value: the hash key is its `(pointer, length)`, and a boxed or
    // tagged name has neither until something converts it.
    let Some(name) = inst.operands.get(1).and_then(|id| function.value(*id)) else {
        return Some("dynamic property name is missing from the value table".to_string());
    };
    if name.ir_type != IrType::Str || name.php_type.codegen_repr() != PhpType::Str {
        return Some(format!(
            "a computed property name arrives as {:?}/{:?}, not a string",
            name.ir_type,
            name.php_type.codegen_repr()
        ));
    }
    if inst.op == Op::DynamicPropSet {
        if inst.result.is_some() {
            return Some("dynamic_prop_set answers nothing".to_string());
        }
        let Some(value) = inst.operands.get(2).and_then(|id| function.value(*id)) else {
            return Some("dynamic property value is missing from the value table".to_string());
        };
        // The hash stores one boxed Mixed cell per entry, so the value has to be boxable.
        if super::transfer::classify_transfer(
            value.ir_type,
            value.php_type.codegen_repr(),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        )
        .is_err()
        {
            return Some(format!(
                "a dynamic property takes a boxed value, and {:?}/{:?} has no boxing",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
        return None;
    }
    // A READ answers the hash's cell, or the narrowing of it the EIR asked for.
    let Some(result) = inst.result else {
        return Some("dynamic_prop_get answers a value".to_string());
    };
    let Some(result) = function.value(result) else {
        return Some("dynamic_prop_get result is missing from the value table".to_string());
    };
    if result.ir_type != IrType::Heap(IrHeapKind::Mixed)
        && super::transfer::classify_transfer(
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            result.ir_type,
            result.php_type.codegen_repr(),
        )
        .is_err()
    {
        return Some(format!(
            "a dynamic property read answers {:?}/{:?}, which no unboxing reaches",
            result.ir_type,
            result.php_type.codegen_repr()
        ));
    }
    // The miss warns, naming the class from the receiver's own id word.
    if !module.functions.iter().any(|function| function.flags.is_main) {
        return Some(
            "a dynamic property read warns on a miss, and only a main-bearing module carries the \
             warning runtime"
                .to_string(),
        );
    }
    None
}

/// Validates `Op::PropUnset`: `unset($obj->$p)` on a declared property.
///
/// php REMOVES the property, and a fixed slot cannot express that — so the slot is MARKED, in the
/// same word and with the same value the native backend uses. What the mark is worth depends on
/// there being no live value it can be confused with, which is why the admitted set is exactly the
/// shapes whose second word is a tag, a length or a container tag.
///
/// An UNTYPED declared property is refused, as it is on native and for the same reason: php's
/// removed-then-read answers `null` with a warning there rather than raising, and a slot whose
/// declared type has no null has nowhere to put that answer. A property reached through `__unset()`
/// or a dynamic-property hash is a different lowering and not this one.
fn property_unset_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let [receiver] = inst.operands.as_slice() else {
        return Some(format!(
            "prop_unset expects one receiver, got {} operands",
            inst.operands.len()
        ));
    };
    let Some(receiver) = function.value(*receiver) else {
        return Some("prop_unset receiver is missing from the value table".to_string());
    };
    let Some(property) = data_string(module, inst) else {
        return Some("prop_unset names its property as data".to_string());
    };
    let PhpType::Object(class_name) = receiver.php_type.codegen_repr() else {
        return Some(format!(
            "prop_unset on {:?}: this writes a declared class layout",
            receiver.php_type.codegen_repr()
        ));
    };
    let class_name = class_name.trim_start_matches('\\').to_string();
    let Some(class_info) = module.class_infos.get(&class_name) else {
        return Some(format!("prop_unset on unknown class {class_name}"));
    };
    let Some(index) = class_info
        .properties
        .iter()
        .position(|(name, _)| name == property)
    else {
        return Some(format!(
            "prop_unset of undeclared ${property} on {class_name}, which needs the dynamic-property \
             hash or __unset()"
        ));
    };
    if !class_info
        .property_declared_slots
        .get(index)
        .copied()
        .unwrap_or(false)
    {
        return Some(format!(
            "unset() of the untyped declared property {class_name}::${property}, whose \
             removed-then-null read has no slot representation"
        ));
    }
    if class_info.reference_properties.contains(property) {
        return Some(format!(
            "unset() of the by-reference property {class_name}::${property}"
        ));
    }
    // The marker lives in the slot's SECOND word, so the shape has to be one whose second word
    // carries a tag, a length or a container tag — never a value that could equal the marker.
    let slot = class_info.properties[index].1.codegen_repr();
    if !matches!(
        slot,
        PhpType::Int
            | PhpType::Bool
            | PhpType::False
            | PhpType::Float
            | PhpType::Str
            | PhpType::TaggedScalar
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Iterable
    ) {
        return Some(format!(
            "unset() of {class_name}::${property} typed {slot:?}, which has no marker word"
        ));
    }
    if inst.result.is_some() {
        return Some("prop_unset answers nothing".to_string());
    }
    // The read that follows raises php's `Error`, which needs the class constructible and the
    // module's exception tag declared — the same two things `Enum::from()`'s raise needs.
    if !super::objects::runtime_error_is_constructible(module, "Error") {
        return Some(
            "unset() of a property needs a constructible Error for the read that follows"
                .to_string(),
        );
    }
    None
}

/// Validates `Op::PropInitialized`: the probe `isset($obj->typed)` lowers to.
///
/// The answer is a CONSTANT here, and that is the whole reason this is admissible: a property
/// whose write is proved — by a default, by promotion, by an enum materializer, by every
/// concrete descendant, or by the constructor — is initialized on every path, so the probe is
/// `true`. A property whose write is NOT proved is exactly the one this target refuses to read
/// at all, for want of the initialization bitmap the real check needs, so probing it is refused
/// for the same reason rather than guessed at.
fn prop_initialized_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let Some(receiver_id) = inst.operands.first().copied() else {
        return Some("prop_initialized takes a receiver".to_string());
    };
    let Some(receiver) = function.value(receiver_id) else {
        return Some("prop_initialized receiver is missing from the value table".to_string());
    };
    let Some(property) = data_string(module, inst) else {
        return Some("prop_initialized names its property as data".to_string());
    };
    let class_name = match receiver.php_type.codegen_repr() {
        PhpType::Object(class_name) => class_name,
        // A NULLABLE receiver is the shape both `?->` past its first hop and `isset($maybeNull->p)`
        // arrive with: `Node|null`, stored as one boxed cell. The question asked HERE is only the
        // LAYOUT, which is the union's single class, read through the same function `->` reads it
        // through. NULL is not a layout question — php answers the probe false for it, and the
        // lowering answers that from the cell's own tag, so nothing about it is decided here.
        _ => match receiver_object_class(&receiver.php_type) {
            Ok(class_name) if receiver.ir_type == IrType::Heap(IrHeapKind::Mixed) => class_name,
            _ => {
                return Some(format!(
                    "prop_initialized on {:?}: this probe reads a declared class layout",
                    receiver.php_type.codegen_repr()
                ))
            }
        },
    };
    let class_name = class_name.trim_start_matches('\\').to_string();
    let Some(class_info) = module.class_infos.get(&class_name) else {
        return Some(format!("prop_initialized on unknown class {class_name}"));
    };
    let Some(property_index) = class_info
        .properties
        .iter()
        .position(|(name, _)| name == property)
    else {
        return Some(format!(
            "prop_initialized of undeclared ${property} on {class_name}"
        ));
    };
    if typed_property_may_be_uninitialized(
        module,
        function,
        inst,
        &class_name,
        class_info,
        property,
        property_index,
    ) {
        return Some(format!(
            "typed property ${property} may be uninitialized and requires an exact PHP fatal check"
        ));
    }
    if inst.result_type != IrType::I64 || inst.result_php_type.codegen_repr() != PhpType::Bool {
        return Some(format!(
            "prop_initialized answers a bool, not {:?}/{:?}",
            inst.result_type,
            inst.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Returns whether a cast's operand is a string LITERAL php reads as a whole number.
///
/// php's implicit string-to-number coercion is silent only for a wholly numeric string:
/// `"10" / 3` is 3.3333…, while `"10abc" / 3` deprecates ("A non-numeric value") and `"x" / 3`
/// is a `TypeError`. Neither diagnostic is emitted by this conversion, so the implicit form is
/// admitted only where the operand's bytes are known and are wholly numeric — which a
/// `const_str` makes decidable at compile time.
///
/// Leading and trailing whitespace is php's own allowance: `" 10 "` is numeric to php-src.
fn numeric_string_literal_operand(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> bool {
    let Some(source_id) = inst.operands.first().copied() else {
        return false;
    };
    let Some(value) = function.value(source_id) else {
        return false;
    };
    let ValueDef::Instruction { inst: def, .. } = value.def else {
        return false;
    };
    let Some(def) = function.instruction(def) else {
        return false;
    };
    if def.op != Op::ConstStr {
        return false;
    }
    let Some(Immediate::Data(data)) = def.immediate else {
        return false;
    };
    let Some(text) = module.data.strings.get(data.as_raw() as usize) else {
        return false;
    };
    let text = text.trim();
    !text.is_empty() && text.parse::<f64>().is_ok()
}

/// Validates `Op::PhpRelCmp`: a relational operator whose ordering needs a runtime tag.
///
/// EIR emits this opcode only for `<`, `<=`, `>` and `>=` where at least one operand is boxed,
/// so the machine cannot decide the order from the storage alone. The pair reaching here is the
/// same pair `loose_eq_shape_issue` admits, minus the concrete/concrete combinations the
/// frontend never routes through this opcode: two boxed cells, or one boxed cell against a
/// genuine int. A PHP bool on the concrete side is a different rule — it makes BOTH sides
/// booleans — and stays refused.
///
/// It is a main-bearing command rule for the same reason the loose comparison is: php's own
/// failure for an object operand goes through WASI.
fn php_rel_cmp_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let [left, right] = inst.operands.as_slice() else {
        return Some(format!(
            "php_rel_cmp takes two operands, got {}",
            inst.operands.len()
        ));
    };
    if php_rel_cmp_predicate(inst).is_none() {
        return Some(format!(
            "php_rel_cmp carries {:?}, not one of the four relational predicates",
            inst.immediate
        ));
    }
    let mut kinds = Vec::new();
    for operand in [left, right] {
        let Some(value) = function.value(*operand) else {
            return Some("php_rel_cmp operand is missing from the value table".to_string());
        };
        kinds.push((value.ir_type, value.php_type.codegen_repr()));
    }
    // Everything the helpers compare is a Mixed cell or a plain long. A `TaggedScalar` — the
    // inline `{payload, tag}` pair a possibly-missing `array<int>` element reads as — is one
    // boxing away from the first, and its tag word is already the cell's own, so it is admitted
    // wherever a cell is and boxed at the call site.
    let boxed = (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
    let tagged = (IrType::TaggedScalar, PhpType::TaggedScalar);
    let plain_int = (IrType::I64, PhpType::Int);
    let cell_like = |kind: &(IrType, PhpType)| *kind == boxed || *kind == tagged;
    let left_kind = &kinds[0];
    let right_kind = &kinds[1];
    let admitted = (cell_like(left_kind) && cell_like(right_kind))
        || (cell_like(left_kind) && *right_kind == plain_int)
        || (*left_kind == plain_int && cell_like(right_kind));
    if !admitted {
        return Some(format!(
            "php_rel_cmp of {:?}/{:?} against {:?}/{:?} is not lowered",
            kinds[0].0, kinds[0].1, kinds[1].0, kinds[1].1
        ));
    }
    if inst.result_type != IrType::I64 || inst.result_php_type.codegen_repr() != PhpType::Bool {
        return Some(format!(
            "php_rel_cmp answers a bool, not {:?}/{:?}",
            inst.result_type,
            inst.result_php_type.codegen_repr()
        ));
    }
    if !module.functions.iter().any(|candidate| candidate.flags.is_main) {
        return Some(
            "php_rel_cmp can reach php's own object-comparison failure, whose diagnostic needs a \
             main-bearing command module"
                .to_string(),
        );
    }
    None
}

/// Returns the runtime predicate word for a `php_rel_cmp`, or `None` for any other form.
///
/// 0 = `<`, 1 = `<=`, 2 = `>`, 3 = `>=`, matching `__rt_threeway_predicate`. `==`, `!=` and
/// `<=>` have their own opcodes and never carry this one's immediate.
pub(super) fn php_rel_cmp_predicate(inst: &Instruction) -> Option<i32> {
    match inst.immediate {
        Some(Immediate::CmpPredicate(CmpPredicate::Slt | CmpPredicate::Olt)) => Some(0),
        Some(Immediate::CmpPredicate(CmpPredicate::Sle | CmpPredicate::Ole)) => Some(1),
        Some(Immediate::CmpPredicate(CmpPredicate::Sgt | CmpPredicate::Ogt)) => Some(2),
        Some(Immediate::CmpPredicate(CmpPredicate::Sge | CmpPredicate::Oge)) => Some(3),
        _ => None,
    }
}

/// Returns the predicate that answers the same question with the operands swapped.
///
/// `cmp(int, cell)` is `-cmp(cell, int)`, so putting the cell first and mirroring the predicate
/// is the same answer without a negation step.
pub(super) fn mirrored_rel_predicate(predicate: i32) -> i32 {
    match predicate {
        0 => 2,
        1 => 3,
        2 => 0,
        _ => 1,
    }
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
        // An already-boxed cell, push or set alike. This was PUSH-only for as long as an owned
        // `load_local` handed out a BORROW while the EIR released it, which freed the cell the
        // array had just taken a share of and made `$a[0]` read back null. `lower_load_local`
        // now increfs exactly when the EIR declares `own=owned`, so the write is balanced.
        (PhpType::Mixed, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed) => true,
        // A raw scalar boxed into a Mixed cell at the WRITE site — push or set alike, since
        // `__rt_array_set_mixed` stores what `__rt_array_push_mixed` appends. Each of these has
        // an exact tag and payload; a heap container has neither, so it stays refused.
        (
            PhpType::Mixed,
            IrType::I64,
            PhpType::Int | PhpType::Bool | PhpType::False | PhpType::Void,
        ) => true,
        (PhpType::Mixed, IrType::F64, PhpType::Float) => true,
        (PhpType::Mixed, IrType::Str, PhpType::Str) => true,
        // A `TaggedScalar` is the two-word `{payload, tag}` form of `int|null` — what reading a
        // possibly-missing element out of an `array<int>` produces. Its tag word already uses the
        // cell's own tag scheme, so boxing it is a move rather than a decision, and the result is
        // the same tag-0 or tag-8 cell a raw int or a raw null would have produced.
        (PhpType::Mixed, IrType::TaggedScalar, PhpType::TaggedScalar) => true,
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

/// Validates `Op::ObjectCloneShallow`, php's `clone`.
///
/// The copy's layout is read from the class descriptor at run time, so the one thing the audit
/// has to establish is that a descriptor EXISTS for the class named here — a clone of a class
/// the module does not declare would allocate the right number of bytes and then retain
/// nothing, handing back a copy whose children the release walk frees once too often.
fn object_clone_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    let [source] = inst.operands.as_slice() else {
        return Some(format!(
            "clone takes one receiver, got {} operands",
            inst.operands.len()
        ));
    };
    if inst.result.is_none() {
        return Some("clone answers the copy and needs a result".to_string());
    }
    let Some(Immediate::Data(data)) = inst.immediate else {
        return Some("clone carries its class as an interned name".to_string());
    };
    let Some(class_name) = module.data.class_names.get(data.as_raw() as usize) else {
        return Some("clone's class name is missing from the data pool".to_string());
    };
    let Some(class_info) = module.class_infos.get(class_name) else {
        return Some(format!("clone of {class_name}, which this module does not declare"));
    };
    // A property this target cannot lay out is one the copy cannot carry either, and the
    // descriptor is what both the allocation and the release walk read.
    if class_info.properties.len() > u16::MAX as usize {
        return Some(format!(
            "clone of {class_name}, whose {} properties overflow the descriptor's count field",
            class_info.properties.len()
        ));
    }
    let Some(receiver) = function.value(*source) else {
        return Some("clone receiver is missing from the value table".to_string());
    };
    if receiver.ir_type != IrType::Heap(IrHeapKind::Object) {
        return Some(format!(
            "clone receiver is {:?}, not a concrete object pointer",
            receiver.ir_type
        ));
    }
    None
}

/// Validates `Op::Spaceship`, php's `<=>`.
///
/// Two operands, an integer result, and each side in a storage one of the three ordering
/// helpers takes: a plain long, a plain double, or a boxed cell. A `TaggedScalar` is refused
/// rather than boxed here — the lowering would have to decide what its null tag orders as, and
/// php's answer for that depends on the OTHER side's type.
fn spaceship_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let [left, right] = inst.operands.as_slice() else {
        return Some(format!(
            "spaceship takes two operands, got {}",
            inst.operands.len()
        ));
    };
    let Some(result) = inst.result else {
        return Some("spaceship answers -1, 0 or 1 and needs a result".to_string());
    };
    if function.value(result).map(|value| value.ir_type) != Some(IrType::I64) {
        return Some("spaceship answers an integer word".to_string());
    }
    for (side, value) in [("left", left), ("right", right)] {
        let Some(operand) = function.value(*value) else {
            return Some(format!("spaceship {side} operand is missing from the value table"));
        };
        let php = operand.php_type.codegen_repr();
        let ordered = match operand.ir_type {
            IrType::I64 => matches!(php, PhpType::Int | PhpType::Bool | PhpType::False),
            IrType::F64 => php == PhpType::Float,
            IrType::Heap(IrHeapKind::Mixed) => true,
            _ => false,
        };
        if !ordered {
            return Some(format!(
                "spaceship {side} operand is {:?}/{:?}, which no ordering helper takes",
                operand.ir_type, php
            ));
        }
    }
    None
}

/// Validates the three function static-local ops.
///
/// A static local is named by a `LocalSlot` immediate into the function's OWN local table, and
/// the planner keys its cell by `(function, variable)`. So the audit checks exactly what the
/// planner needed and the lowering will read: the immediate is a local slot, that slot exists,
/// it carries the source variable's name, and its storage type is one of the widths the shared
/// static-slot load and store can express.
///
/// Anything else is refused rather than guessed, because the storage is a fixed address: a slot
/// the lowering cannot find would silently read whatever else lives there.
fn static_local_shape_issue(function: &Function, inst: &Instruction) -> Option<String> {
    let Some(Immediate::LocalSlot(slot_id)) = inst.immediate else {
        return Some(format!(
            "{} names its variable with a local-slot immediate, got {:?}",
            inst.op.name(),
            inst.immediate
        ));
    };
    let Some(local) = function.locals.get(slot_id.as_raw() as usize) else {
        return Some(format!(
            "{} names local slot {} which the function does not have",
            inst.op.name(),
            slot_id.as_raw()
        ));
    };
    if local.name.is_none() {
        return Some(format!(
            "{} names an unnamed local slot, so its cell has no key",
            inst.op.name()
        ));
    }
    let php_type = local.php_type.codegen_repr();
    if !matches!(
        php_type,
        PhpType::Int
            | PhpType::Bool
            | PhpType::False
            | PhpType::Float
            | PhpType::Str
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Mixed
            | PhpType::Union(_)
    ) {
        return Some(format!(
            "static local ${} is stored as {:?}, which this target's static slot cannot hold",
            local.name.as_deref().unwrap_or("?"),
            php_type
        ));
    }
    // The two writes differ in ownership, and both take the value as operand 0; a read, and
    // the has-it-run test that guards the initializer, take none. Getting this wrong would
    // release the wrong reference, so it is checked here rather than assumed in the lowering.
    let expected = usize::from(!matches!(
        inst.op,
        Op::LoadStaticLocal | Op::StaticLocalUninitialized
    ));
    if inst.op == Op::StaticLocalUninitialized
        && inst
            .result
            .and_then(|result| function.value(result))
            .map(|value| value.ir_type)
            != Some(IrType::I64)
    {
        return Some("static_local_uninitialized answers an I64 flag".to_string());
    }
    if inst.operands.len() != expected {
        return Some(format!(
            "{} takes {expected} operand(s), got {}",
            inst.op.name(),
            inst.operands.len()
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
    let scope = super::statics::enclosing_class(&function.name);
    // `static::$p` names one slot per class the call could have been made on, and picks between
    // them at run time from the called-class id — so what the audit checks is that they exist and
    // that they agree on a width, not that there is exactly one.
    let php_type = match super::statics::resolve_label(module, &slots, label, scope) {
        Some(slot) => slot.php_type.clone(),
        None => {
            let Some((_, property)) = label.split_once("::") else {
                return Some(format!("malformed static property label {label}"));
            };
            let Some(scope) = scope else {
                return Some(format!("static property {label} has no lowered slot"));
            };
            let arms = super::statics::late_static_slots(
                module,
                &slots,
                property.trim_start_matches('$'),
                scope,
            );
            let Some((_, first)) = arms.first() else {
                return Some(format!("static property {label} has no lowered slot"));
            };
            if arms
                .iter()
                .any(|(_, slot)| slot.php_type.codegen_repr() != first.php_type.codegen_repr())
            {
                return Some(format!(
                    "static property {label} has a different declared type per class, \
                     which a late-bound access cannot read at one width"
                ));
            }
            if !function
                .locals
                .iter()
                .any(|local| super::objects::is_called_class_id_local(local.name.as_deref()))
            {
                return Some(format!(
                    "static property {label} outside a function carrying the called-class id"
                ));
            }
            first.php_type.clone()
        }
    };
    let slot = StaticSlotShape { php_type };
    if !matches!(
        slot.php_type.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::False
            | PhpType::Float
            | PhpType::Str
            // A container slot holds one pointer, which is the same 16-byte slot read at a
            // different width — and the startup initializer is what puts a non-empty default
            // there, since static data can express bytes but not a heap block.
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            // A NULLABLE scalar is the `int|null` pair written flat: payload in the first
            // word, Mixed tag in the second. It carries no heap pointer, so it is the same
            // 16 bytes read at a third width.
            | PhpType::TaggedScalar
            // A nullable container, or a `mixed`, holds one CELL pointer — the shape the
            // static-local work already reads and writes. Its null is the null pointer, which
            // is what an unwritten slot carries too.
            | PhpType::Mixed
            | PhpType::Union(_)
    ) {
        return Some(format!(
            "static property {label} typed {:?} has no WASM slot shape",
            slot.php_type.codegen_repr()
        ));
    }
    // A boxed slot's default has to be laid out as BYTES, and only the null pointer is. A
    // `public static mixed $m = 5;` would need a heap cell built before main runs, and the
    // zero the layout would otherwise write reads back as null — the silent answer this
    // refuses rather than gives.
    if matches!(slot.php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        if let Some(issue) = boxed_static_default_issue(module, label, function) {
            return Some(issue);
        }
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
        // Otherwise the question is not "are the two types equal" but "does a conversion
        // between them exist", and the transfer table is where that is already decided:
        // `C::$maybe = 5;` on a `?int` stores a long into a tagged pair, and
        // `C::$list = [1, 2];` on a `?array` boxes a container into a cell. Comparing the
        // reprs refused both, although the lowering has the conversion.
        let slot_php = slot.php_type.codegen_repr();
        let convertible = value.php_type.codegen_repr() == slot_php
            || super::transfer::storage_shapes_agree(
                value.ir_type,
                value.php_type.codegen_repr(),
                IrType::from_php(&slot_php),
                slot_php.clone(),
            )
            .unwrap_or(false)
            || super::transfer::classify_transfer(
                value.ir_type,
                value.php_type.codegen_repr(),
                IrType::from_php(&slot_php),
                slot_php.clone(),
            )
            .is_ok();
        if !narrows && !convertible {
            return Some(format!(
                "static property {label} takes {:?}, got {:?}",
                slot_php,
                value.php_type.codegen_repr()
            ));
        }
    }
    None
}

/// Validates `Op::StaticPropInitialized`: the probe `isset(C::$p)` lowers to.
///
/// The slot questions are `static_property_shape_issue`'s and are asked through it, so the probe
/// and the read it precedes cannot disagree about which slots exist or how wide they are.
///
/// What this adds is the same question `prop_initialized` asks of an instance property, and the
/// answer has to come from the DECLARATION because a static slot carries no sentinel: the layout
/// writes bytes for every slot, so an uninitialized `public static int $n;` is indistinguishable
/// from one holding `0`. php fatals on reading that property and answers `isset()` false for it;
/// this target can do neither, so the probe is refused there rather than answered. A default, or an
/// untyped declaration — which php gives an implicit `null` — proves the write and makes the
/// constant exact.
///
/// The native backend tests a marker word against `UNINITIALIZED_TYPED_PROPERTY_SENTINEL` instead.
/// Carrying that sentinel here would change the width of every static slot shape, which is a
/// layout change rather than a lowering, so the declaration is asked instead.
fn static_prop_initialized_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
    if !inst.operands.is_empty() {
        return Some(format!(
            "static_prop_initialized takes no operands, got {}",
            inst.operands.len()
        ));
    }
    if let Some(issue) = static_property_shape_issue(module, function, inst) {
        return Some(issue);
    }
    if inst.result_type != IrType::I64 || inst.result_php_type.codegen_repr() != PhpType::Bool {
        return Some(format!(
            "static_prop_initialized answers a bool, not {:?}/{:?}",
            inst.result_type,
            inst.result_php_type.codegen_repr()
        ));
    }
    let Some(Immediate::Data(data)) = inst.immediate else {
        return Some("static_prop_initialized without an interned label".to_string());
    };
    let Some(label) = module.data.strings.get(data.as_raw() as usize) else {
        return Some("static_prop_initialized label is missing from the data pool".to_string());
    };
    let Some((class_name, property)) = label.split_once("::") else {
        return Some(format!("malformed static property label {label}"));
    };
    let property = property.trim_start_matches('$');
    let Some((class_info, index)) = declared_static_property(module, class_name, property) else {
        return Some(format!("static property {label} is not declared on {class_name}"));
    };
    // An UNTYPED static gets php's implicit `null`, which is a written value; a TYPED one with no
    // default is the uninitialized case there is no way to see here.
    if class_info.declared_static_properties.contains(property)
        && class_info
            .static_defaults
            .get(index)
            .map(|default| default.is_none())
            .unwrap_or(true)
    {
        return Some(format!(
            "typed static property {label} may be uninitialized, and a static slot carries no \
             sentinel to tell that from its zero"
        ));
    }
    None
}

/// Resolves `Class::$property` to the `ClassInfo` that DECLARES it and the property's index there.
///
/// A child sharing a parent's static shares its storage, so the declaration — and therefore its
/// default — lives on the parent. `static_property_declaring_classes` is the same map the slot
/// placement keys by, which is what keeps the two from disagreeing.
fn declared_static_property<'a>(
    module: &'a Module,
    class_name: &str,
    property: &str,
) -> Option<(&'a crate::types::ClassInfo, usize)> {
    let class_name = class_name.trim_start_matches('\\');
    let info = module.class_infos.get(class_name)?;
    let declaring = info
        .static_property_declaring_classes
        .get(property)
        .map(String::as_str)
        .unwrap_or(class_name);
    let info = module.class_infos.get(declaring)?;
    let index = info
        .static_properties
        .iter()
        .position(|(name, _)| name == property)?;
    Some((info, index))
}

/// Reports the default a BOXED static property carries when it is one no byte layout can
/// express.
///
/// Only the null pointer is expressible, and it is also what an unwritten slot holds, so a
/// property with no default at all is fine. Anything else — an int, a string, an array — needs
/// a cell built on the heap before `main`, and writing zero instead would read back as null.
/// Whether `main`'s prologue can build this declared-`array` static default: a list of scalar
/// or string literals, or an empty hash.
fn static_array_default_is_buildable(php_type: &PhpType, expr: &crate::parser::ast::Expr) -> bool {
    use crate::codegen::LiteralArrayElement as Element;
    match literal_default_value("static property", php_type, &expr.kind, "StaticInit") {
        Ok(LiteralDefaultValue::BoxedArray { elements, .. }) => elements.iter().all(|element| {
            matches!(
                element,
                Element::Int(_) | Element::Bool(_) | Element::Float(_) | Element::Null | Element::Str(_)
            )
        }),
        Ok(LiteralDefaultValue::BoxedAssocArray { entries, .. }) => entries.is_empty(),
        _ => false,
    }
}

/// Validates the default of a static property reached as `label` (`Class::$name`), returning why
/// its boxed slot cannot be seeded or `None`.
///
/// A `static::` receiver reaches whichever class the call was made on and is checked per
/// ladder arm, so it answers `None` here.
fn boxed_static_default_issue(
    module: &Module,
    label: &str,
    function: &Function,
) -> Option<String> {
    let (receiver, property) = label.split_once("::")?;
    let property = property.trim_start_matches('$');
    let receiver = receiver.trim_start_matches('\\');
    let scope = super::statics::enclosing_class(&function.name);
    let start = match receiver {
        // A late-bound receiver reaches whichever class the call was made on; the ladder's
        // arms are each checked as their own slot, so there is nothing to resolve here.
        "static" => return None,
        "self" | "parent" => scope?.to_string(),
        named => named.to_string(),
    };
    let mut owner = Some(start);
    while let Some(name) = owner {
        let info = module.class_infos.get(&name)?;
        if let Some(index) = info
            .static_properties
            .iter()
            .position(|(candidate, _)| candidate == property)
        {
            // A declared `array`'s list of literals, or its empty hash, is built in `main`'s
            // prologue (`function::emit_container_static_defaults`).
            if info.static_properties[index].1.is_php_array() {
                if let Some(Some(expr)) = info.static_defaults.get(index) {
                    if static_array_default_is_buildable(&info.static_properties[index].1, expr) {
                        return None;
                    }
                }
            }
            return match info.static_defaults.get(index) {
                // No default, or one the layout could not render, both leave the zero slot —
                // which for this shape IS null, so both are right.
                None | Some(None) => None,
                Some(Some(expr)) => match &expr.kind {
                    crate::parser::ast::ExprKind::Null => None,
                    other => Some(format!(
                        "static property {label} is boxed and its default {other:?} needs a \
                         heap cell before main"
                    )),
                },
            };
        }
        owner = info.parent.clone();
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
fn iter_start_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<String> {
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
        // A BOXED source names no storage until the cell is read, so the walk dispatches on the
        // runtime tag: indexed, hash, or — measured on php-src 8.5.6 — a warning naming the type
        // and zero iterations for anything else. Both the key and the value come back boxed,
        // which is what the EIR already types them, so no element contract is needed here. The
        // warning goes through WASI, so this is a command-module rule like every other
        // diagnostic-producing one.
        (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed)
            if module.functions.iter().any(|candidate| candidate.flags.is_main) =>
        {
            None
        }
        // An `iterable` PARAMETER is the same runtime question asked of a container POINTER:
        // the heap header's kind byte separates an indexed array from a hash, and the advance
        // and read helpers are the boxed source's own — they box whatever the container's
        // `value_type` holds, so no element contract applies here either.
        (IrType::Heap(IrHeapKind::Iterable), PhpType::Iterable)
            if module.functions.iter().any(|candidate| candidate.flags.is_main) =>
        {
            None
        }
        // A GENERATOR is not a container, and that is why it needs its own arm: there is no
        // element type to contract over and no length to compare a cursor against. Advancing it
        // runs its body, and the value is whatever the yield left in the frame — so what has to
        // hold instead is that the body itself lowers, which `generator_yield_shape_issue`
        // decides for every yield in it.
        (IrType::Heap(IrHeapKind::Object), PhpType::Object(name))
            if name.trim_start_matches('\\') == "Generator" =>
        {
            None
        }
        (ir_type, php_type) => Some(format!(
            "foreach requires a concrete indexed or associative array, got {ir_type:?}/{php_type:?}"
        )),
    };
    if source_shape.is_some() {
        return source_shape;
    }
    iterator_alias_mutation_issue(module, function, inst, *inst.operands.first()?)
}

/// Rejects mutations that can invalidate a live iterator snapshot.
///
/// Only a mutation the LOOP can reach counts. Walking every instruction after the `IterStart`
/// instead — which the flat instruction table makes easy and wrong — refuses code that touches
/// the container once the loop is over: `foreach ($h as ...) {}` followed by `$h["c"] = 3;` is
/// ordinary PHP, and the iterator is dead by then. The live range is the loop itself: the blocks
/// reachable from the header that also reach it back, plus the tail of the block that starts the
/// iterator, where the source pointer is already captured but iteration has not begun.
fn iterator_alias_mutation_issue(
    module: &Module,
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
    // A REFERENCE to the container makes every mutation through the alias invisible to the slot
    // comparisons below: `$r = &$h;` gives `$r` its own slot, and `foreach ($h as $v) { $r[] = 99; }`
    // printed `5 | 0` where php-src prints `5 3 9 1 | 8`. The promotion and the aliasing both name
    // the slot, and either one anywhere in the function means the container can change under the
    // loop, so neither is scoped to the live range.
    if let Some(source_slot) = source_slot {
        if function.instructions.iter().any(|candidate| {
            matches!(
                candidate.op,
                Op::PromoteLocalRefCell | Op::AliasLocalRefCell
            ) && instruction_names_slot(candidate, source_slot)
        }) {
            return Some(
                "a reference to the iterated container can mutate it without PHP snapshot/COW \
                 semantics"
                    .to_string(),
            );
        }
    }
    let live = iterator_live_instructions(function, start, start_id);
    for candidate in live
        .iter()
        .filter_map(|id| function.instruction(*id))
    {
        // A CALL that receives the container BY REFERENCE mutates it in a body this scan never
        // sees. Measured: `function bump(array &$a) { $a[] = 99; }` called from the loop grew the
        // array the loop was reading and exhausted memory, where php-src walks its snapshot and
        // stops at four. A BY-VALUE call cannot do that, and refusing those too turned away
        // `foreach ($h as $v) { echo count_of($h); }` — ordinary PHP — so the callee's own
        // signature decides, and only a callee this module cannot resolve is refused wholesale.
        if matches!(
            candidate.op,
            Op::Call
                | Op::MethodCall
                | Op::NullsafeMethodCall
                | Op::StaticMethodCall
                | Op::EvalStaticMethodCall
                | Op::IteratorMethodCall
                // A closure can declare `&$a` as readily as a named function can. That shape is
                // refused earlier today — `closure_new` rejects a by-reference callback parameter
                // — so this arm costs no coverage and closes the route before it opens.
                | Op::ClosureCall
                | Op::CallableDescriptorInvoke
        ) {
            let passed: Vec<usize> = candidate
                .operands
                .iter()
                .enumerate()
                .filter(|(_, operand)| {
                    **operand == source
                        || (source_slot.is_some()
                            && value_local_origin(function, **operand) == source_slot)
                })
                .map(|(index, _)| index)
                .collect();
            if !passed.is_empty() && call_can_take_argument_by_reference(module, candidate, &passed)
            {
                return Some(format!(
                    "{} receives the iterated container by reference and may mutate it",
                    candidate.op.name()
                ));
            }
        }
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
        let sorts_container = match candidate.immediate.as_ref() {
            Some(Immediate::RuntimeCall(
                RuntimeCallTarget::Function(target)
                | RuntimeCallTarget::ProfiledFunction { target, .. },
            )) => runtime_call_mutates_its_array(*target),
            _ => false,
        };
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

/// Whether a call could bind one of `positions` to a by-reference parameter.
///
/// A DIRECT call resolves through the same `resolve_direct_call` the lowering uses, so the
/// declared parameters are the ones that will actually be bound. Everything else — a method whose
/// receiver this pass does not track, a dynamic target — answers YES: an accepting gate cannot
/// treat "I could not find out" as "no".
///
/// Note the callee name lives in `module.data.function_names`, a different pool from
/// `module.data.strings`; reading the wrong one resolves every call to the empty string and
/// refuses them all, which is how `foreach ($h as $v) { echo look($h); }` briefly stopped
/// compiling for a by-VALUE callee that cannot mutate anything.
fn call_can_take_argument_by_reference(
    module: &Module,
    call: &Instruction,
    positions: &[usize],
) -> bool {
    if call.op != Op::Call {
        return true;
    }
    let Ok(target) = crate::codegen_wasm::calls::resolve_direct_call(module, call) else {
        return true;
    };
    positions.iter().any(|position| {
        // The operand list and the parameter list are aligned for a plain call; a variadic tail
        // takes the last declared parameter's contract.
        let declared = target
            .function
            .params
            .get(*position)
            .or_else(|| target.function.params.last());
        declared.is_none_or(|param| param.by_ref)
    })
}

/// Whether an instruction's slot immediate mentions `slot`, in either the single or paired form.
///
/// `promote_local_ref_cell slots[0,1]` and `alias_local_ref_cell slots[2,0]` both carry a PAIR, so
/// a check that only reads `Immediate::LocalSlot` sees neither.
fn instruction_names_slot(inst: &Instruction, slot: u32) -> bool {
    match inst.immediate.as_ref() {
        Some(Immediate::LocalSlot(named)) => named.as_raw() == slot,
        Some(Immediate::LocalSlotPair { first, second }) => {
            first.as_raw() == slot || second.as_raw() == slot
        }
        _ => false,
    }
}

/// Whether a runtime function rewrites the array handed to it, rather than answering a new one.
///
/// Naming only `usort` here was measured wrong: `foreach ($a as $v) { echo $v; sort($a); }` over
/// `[5,3,9,1]` printed `5 3 5 9` where php-src prints `5 3 9 1`, because PHP iterates a SNAPSHOT
/// and this target re-reads the array it just reordered. Every in-place mutator in the registry
/// is listed, including the ones this backend has no lowering for yet — a name that arrives later
/// must not arrive through a hole.
fn runtime_call_mutates_its_array(target: RuntimeFnId) -> bool {
    matches!(
        target,
        RuntimeFnId::Sort
            | RuntimeFnId::Rsort
            | RuntimeFnId::Asort
            | RuntimeFnId::Arsort
            | RuntimeFnId::Ksort
            | RuntimeFnId::Krsort
            | RuntimeFnId::Usort
            | RuntimeFnId::Uasort
            | RuntimeFnId::Uksort
            | RuntimeFnId::Natsort
            | RuntimeFnId::Natcasesort
            | RuntimeFnId::Shuffle
            | RuntimeFnId::ArrayMultisort
            | RuntimeFnId::ArrayPop
            | RuntimeFnId::ArrayPush
            | RuntimeFnId::ArrayShift
            | RuntimeFnId::ArrayUnshift
            | RuntimeFnId::ArraySplice
            | RuntimeFnId::ArrayWalk
            | RuntimeFnId::ArrayWalkRecursive
    )
}

/// The instructions that can run while the iterator started by `start` is still live.
///
/// Answers, in block order: the tail of `start`'s own block, then every instruction of every
/// block in the loop. A block is in the loop when a header — a block whose `IterNext` advances
/// THIS iterator — both reaches it and is reachable from it.
///
/// This is an ACCEPTING gate, so every uncertainty has to widen the answer rather than narrow it:
/// a body this walk cannot map returns the whole function, which is what the rule scanned before
/// it was scoped at all. Silently returning nothing would turn the audit into a no-op for exactly
/// the shapes it understands least.
fn iterator_live_instructions(
    function: &Function,
    start: &Instruction,
    start_id: InstId,
) -> Vec<InstId> {
    let everything = || -> Vec<InstId> {
        (0..function.instructions.len())
            .map(|index| InstId::from_raw(index as u32))
            .collect()
    };
    let iterator = start.result;
    let block_of = |id: InstId| -> Option<usize> {
        function
            .blocks
            .iter()
            .position(|block| block.instructions.contains(&id))
    };
    let Some(start_block) = block_of(start_id) else {
        return everything();
    };
    let mut live: Vec<InstId> = function.blocks[start_block]
        .instructions
        .iter()
        .copied()
        .skip_while(|id| *id != start_id)
        .skip(1)
        .collect();
    // EVERY block that advances this iterator is a header, not just the first one found. A single
    // `foreach` has one, but rotating or peeling a loop leaves a priming `IterNext` outside the
    // cycle; taking the first in block order could then pick that one, whose region is a
    // singleton, and the body would go unscanned — the guard would pass exactly the mutation it
    // exists to refuse.
    let headers: Vec<usize> = function
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| {
            block.instructions.iter().any(|id| {
                function.instruction(*id).is_some_and(|inst| {
                    inst.op == Op::IterNext && inst.operands.first().copied() == iterator
                })
            })
        })
        .map(|(index, _)| index)
        .collect();
    if headers.is_empty() {
        // An iterator nothing advances: no loop to bound, so the whole function stands in.
        return everything();
    }
    // The live range ends where the SAME `IterStart` runs again, so no walk may pass through its
    // block. Without that barrier a `foreach` nested in a `while` counts everything after the
    // inner loop as still inside it — the outer back edge reaches the inner header — and refuses
    // `while (…) { foreach ($a as $v) {…} $a[] = 9; }`, where the append runs with no live
    // iterator. A shape that starts and advances in ONE block has no such boundary to draw.
    let mut inside: HashSet<usize> = HashSet::new();
    for header in &headers {
        let barrier = (*header != start_block).then_some(start_block);
        let forward = blocks_reachable_from(function, *header, false, barrier);
        let backward = blocks_reachable_from(function, *header, true, barrier);
        inside.extend(forward.intersection(&backward).copied());
    }
    // The cycle alone is not the whole live range: a block can sit BETWEEN the start and the
    // advance without lying on the cycle at all, and the iterator is fully alive there. Two
    // independent reviewers reached this from opposite directions — an `IterStart` in the loop's
    // own header leaves the body outside the cycle, and a split critical edge inserts a block into
    // the entry wedge. Both are the same gap: blocks the start reaches, that reach an advance,
    // without either walk re-entering the start.
    let after_start = blocks_reachable_from(function, start_block, false, None);
    let mut wedge: HashSet<usize> = HashSet::new();
    for header in &headers {
        let reaches_header = blocks_reachable_from(function, *header, true, Some(start_block));
        wedge.extend(
            after_start
                .intersection(&reaches_header)
                .copied()
                .filter(|block| *block != start_block),
        );
    }
    inside.extend(wedge);
    for block in 0..function.blocks.len() {
        if block != start_block && inside.contains(&block) {
            live.extend(function.blocks[block].instructions.iter().copied());
        }
    }
    live
}

/// Blocks reachable from `origin` over control-flow edges, or reaching it when `reverse` is set.
///
/// A `barrier` block is never entered and its edges are never followed, which is how the caller
/// stops a walk at the point the iterator is restarted.
fn blocks_reachable_from(
    function: &Function,
    origin: usize,
    reverse: bool,
    barrier: Option<usize>,
) -> HashSet<usize> {
    let successors = |block: usize| -> Vec<usize> {
        let Some(terminator) = function.blocks[block].terminator.as_ref() else {
            return Vec::new();
        };
        let targets = match terminator {
            Terminator::Br { target, .. } => vec![*target],
            Terminator::CondBr {
                then_target,
                else_target,
                ..
            } => vec![*then_target, *else_target],
            Terminator::Switch { cases, default, .. } => {
                let mut out: Vec<_> = cases.iter().map(|case| case.target).collect();
                out.push(*default);
                out
            }
            Terminator::GeneratorSuspend { resume, .. } => vec![*resume],
            Terminator::Return { .. }
            | Terminator::Throw { .. }
            | Terminator::Fatal { .. }
            | Terminator::Unreachable => Vec::new(),
        };
        targets
            .into_iter()
            .map(|target| target.as_raw() as usize)
            .filter(|target| *target < function.blocks.len())
            .collect()
    };
    let edges: Vec<Vec<usize>> = if reverse {
        let mut reversed = vec![Vec::new(); function.blocks.len()];
        for block in 0..function.blocks.len() {
            for next in successors(block) {
                reversed[next].push(block);
            }
        }
        reversed
    } else {
        (0..function.blocks.len()).map(successors).collect()
    };
    let mut seen = HashSet::new();
    let mut stack = vec![origin];
    while let Some(block) = stack.pop() {
        if Some(block) == barrier && block != origin {
            continue; // the walk stops here: past it the iterator is a different one
        }
        if !seen.insert(block) {
            continue;
        }
        stack.extend(edges[block].iter().copied());
    }
    seen
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
            // A ref-cell load names its slot exactly as a plain load does. Reading only
            // `LoadLocal` left a referenced container with no slot at all, so every slot-keyed
            // check downstream silently passed: `$r = &$h; foreach ($h as $v) { $r[] = 99; }`
            // printed `5 | 0` where php-src prints `5 3 9 1 | 8`.
            Op::LoadLocal | Op::LoadRefCell => {
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
    // A read PROVEN in bounds produces no warning, so it does not need the diagnostic runtime a
    // main-bearing module carries — the same proof that lets it keep the element's own width.
    if inst.op == Op::ArrayGet
        && !array_read_is_provably_in_bounds(function, inst)
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
    // A BOXED key is admitted through php's key-coercion helper, which can deprecate
    // and warn — a command-module rule — and always answers a BOXED cell, so the read's
    // result must be one for the storage to line up.
    // A concrete STRING key rides the same coercion (boxed at the call site): a literal
    // "01" is a string KEY php never converts, exactly like its boxed counterpart.
    let boxed_key = (index_value.ir_type == IrType::Heap(IrHeapKind::Mixed)
        && index_value.php_type.codegen_repr() == PhpType::Mixed)
        || (index_value.ir_type == IrType::Str
            && index_value.php_type.codegen_repr() == PhpType::Str);
    if boxed_key {
        if !module.functions.iter().any(|candidate| candidate.flags.is_main) {
            return Some("boxed array key diagnostics need the command entry point".to_string());
        }
        // An `array<int>` read comes back as the allocation-free int|null TAGGED pair;
        // everything else stays a boxed cell. Both are exact storages the lowering builds.
        let tagged = inst.result_type == IrType::TaggedScalar
            && inst.result_php_type.codegen_repr() == PhpType::TaggedScalar;
        let cell = inst.result_type == IrType::Heap(IrHeapKind::Mixed)
            && inst.result_php_type.codegen_repr() == PhpType::Mixed;
        if !tagged && !cell {
            return Some(format!(
                "boxed-key read result is {:?}/{:?}, expected a boxed cell or int|null pair",
                inst.result_type,
                inst.result_php_type.codegen_repr()
            ));
        }
        return None;
    }
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
        // The tagged pair is the general shape, because a miss has to be answerable as null and
        // a raw i64 has no null to answer with. A read PROVEN in bounds has no miss to answer,
        // so it may keep the element's own width — which is what the array-spread walk the EIR
        // generates for `[...$a]` needs, since it types its read `int` rather than `?int`.
        PhpType::Int => {
            (inst.result_type == IrType::TaggedScalar && result_php == PhpType::TaggedScalar)
                || (inst.result_type == IrType::I64
                    && result_php == PhpType::Int
                    && array_read_is_in_bounds(function, inst))
        }
        // A FLOAT element has no tagged fallback, because every bit pattern is a valid float
        // and the getter answers NaN for a miss — a value a real element could equal. So only
        // the PROVEN read is admitted, which is the one the array-spread walk emits.
        PhpType::Float => {
            inst.result_type == IrType::F64
                && result_php == PhpType::Float
                && array_read_is_in_bounds(function, inst)
        }
        // An element that is ALREADY a Mixed cell meets the missing-index null in the same
        // representation: the miss boxes null, the hit hands back the stored cell.
        PhpType::Bool | PhpType::Str | PhpType::Mixed => {
            inst.result_type == IrType::Heap(IrHeapKind::Mixed) && result_php == PhpType::Mixed
        }
        // A nested ARRAY or an OBJECT element is one pointer in an 8-byte slot, and pointer 0
        // carries the missing-index null — the same sentinel the native backend uses, and the
        // one every getter now reads as "through a missed element" instead of dereferencing
        // address 0. The result must keep the element's own class or element type: the read
        // hands back the stored pointer unchanged, so anything else would be a relabelling.
        PhpType::Array(_) => {
            inst.result_type == IrType::Heap(IrHeapKind::Array) && result_php == element_type
        }
        PhpType::Object(_) => {
            inst.result_type == IrType::Heap(IrHeapKind::Object) && result_php == element_type
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
        // A scalar element owns nothing, whichever width it is read at.
        PhpType::Int | PhpType::Float => inst.result_ownership == Ownership::NonHeap,
        PhpType::Bool | PhpType::Str | PhpType::Mixed => matches!(
            inst.result_ownership,
            Ownership::Owned | Ownership::MaybeOwned
        ),
        // Only `Owned`, which is the one the lowering answers: it takes a reference of its own
        // after the borrowed read. `MaybeOwned` was admitted here too, but the lowering increfs
        // for `Owned` alone — and an admitted shape the emitter does not handle is exactly the
        // asymmetry that frees a child its parent still points at. No input has been found that
        // stamps this op `MaybeOwned` (every measured pointer-element read is `own=owned`), so
        // narrowing costs nothing observable and closes the gap rather than resting on that.
        PhpType::Array(_) | PhpType::Object(_) => inst.result_ownership == Ownership::Owned,
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
/// Refuses a by-reference parameter whose callee REPLACES the storage representation.
///
/// A ref cell carries a pointer, not a type. When the callee widens the container it was
/// handed — `$a[] = $i` on an `array<int>` promotes to `array<mixed>` via `Op::ArrayToMixed`,
/// then `store_ref_cell`s the wider array back — the caller receives the new pointer but keeps
/// reading it with the element type it passed in. Measured against php-src 8.5.6: with
/// `function m(array &$a) { for ($i = 0; $i < 5; $i++) { $a[] = $i; } } $v = [0]; m($v);` the
/// caller reads `106920 0 106960 0 106824 0` for php-src's `0 0 1 2 3 4` — raw heap addresses,
/// because 24-byte Mixed cells are being read as a dense i64 buffer. `count()` is right (the
/// length field IS shared), so nothing announces the mismatch.
///
/// This is not a WASM defect: the NATIVE backend prints the same raw pointers from the same
/// EIR, so the disagreement is upstream — the callee's post-condition never reaches the call
/// site's type facts. Until that is repaired in the checker, WASM refuses the call rather than
/// answering garbage. `$a[] = 7` (no widening) and by-ref `int`/`string` are unaffected.
fn by_ref_parameter_representation_issue(
    callee: &Function,
    index: usize,
    parameter: &crate::ir::FunctionParam,
) -> Option<String> {
    let expected = parameter.php_type.codegen_repr();
    for block in &callee.blocks {
        for inst_id in &block.instructions {
            let Some(inst) = callee.instruction(*inst_id) else {
                continue;
            };
            if inst.op != Op::StoreRefCell {
                continue;
            }
            let Some(Immediate::LocalSlot(slot)) = inst.immediate.as_ref() else {
                continue;
            };
            if slot.as_raw() as usize != index {
                continue;
            }
            let Some(stored) = inst.operands.first().and_then(|value| callee.value(*value)) else {
                continue;
            };
            if stored.ir_type == parameter.ir_type
                && stored.php_type.codegen_repr() == expected
            {
                continue;
            }
            return Some(format!(
                "callee {:?} stores {:?}/{:?} back through the cell, replacing the caller's {:?}/{:?} representation",
                callee.name,
                stored.ir_type,
                stored.php_type.codegen_repr(),
                parameter.ir_type,
                expected
            ));
        }
    }
    None
}

/// Validates a direct call instruction against the callee's signature, returning the reason this
/// target cannot lower it or `None`.
///
/// Checks arity, the packed-variadic contract, by-reference parameters (using the ref-cell
/// provenance of `owner`) and the storage each argument and the result must have.
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
            if let Some(issue) =
                by_ref_parameter_representation_issue(target.function, index, parameter)
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
        } else {
            // A SUBCLASS argument is the parameter's own storage: both sides are one object
            // pointer, and the callee was already audited against its declared class and every
            // descendant of it. Classifying against the parameter's class is what makes that a
            // plain copy instead of a refusal on the class NAME alone.
            let argument_php = if argument_is_a_descendant_of_the_parameter(
                module,
                &value.php_type.codegen_repr(),
                &parameter.php_type.codegen_repr(),
            ) {
                parameter.php_type.codegen_repr()
            } else {
                value.php_type.codegen_repr()
            };
            if let Some(issue) = value_transfer_shape_issue(
                value.ir_type,
                argument_php,
                parameter.ir_type,
                parameter.php_type.codegen_repr(),
            ) {
                return Some(format!("argument #{index}: {issue}"));
            }
        }
    }
    if let Some(result) = inst.result {
        let Some(value) = owner.value(result) else {
            return Some("call result is missing from the value table".to_string());
        };
        // A BY-REFERENCE return hands back an ADDRESS, and the call site labels that address
        // with what it points AT — `function &all()` returning the address of an `array`
        // property answers `Heap(Array)` here while the body answers the raw `I64`. Both are
        // the same machine address, so the relabel is admitted exactly where it can only be an
        // address: the result must feed a reference binding and nothing else.
        if target.function.flags.by_ref_return
            && by_ref_return_result_only_binds_a_reference(owner, result)
        {
            return None;
        }
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

/// Whether a by-reference call's result is used only to bind a reference.
///
/// That is what makes it safe to read the result's type as the POINTEE rather than as the
/// returned value: `bind_ref_cell_ptr` treats it as an address, and every read afterwards goes
/// through `load_ref_cell`, which reads the pointee at the slot's own width. A result used any
/// other way would be the address itself masquerading as the value.
fn by_ref_return_result_only_binds_a_reference(function: &Function, result: ValueId) -> bool {
    let mut bound = false;
    for candidate in &function.instructions {
        if !candidate.operands.contains(&result) {
            continue;
        }
        if candidate.op != Op::BindRefCellPtr {
            return false;
        }
        bound = true;
    }
    bound
}

/// The only thing the static-property audit needs from a slot: the width it is read at.
///
/// A late-bound access names several slots and picks between them at run time, so there is no one
/// address to carry — and the audit never wanted one.
struct StaticSlotShape {
    php_type: PhpType,
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
    // STORAGE has to agree, which is what `codegen_repr` names: a `mixed` slot passed to an
    // `array &$list` parameter is the same boxed cell under two PHP types.
    let exact_metadata = slot.ir_type == value.ir_type
        && value.ir_type == parameter.ir_type
        && slot.php_type.codegen_repr() == value.php_type.codegen_repr()
        && value.php_type.codegen_repr() == parameter.php_type.codegen_repr();
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

/// Validates `$mixed->prop`: a read whose receiver names no class until run time.
///
/// What the ladder needs is a closed set of classes to compare a class id against, so the module
/// has to DECLARE the property somewhere — a program where nothing does could only ever answer
/// php's undefined-property warning, and refusing says that rather than emitting a ladder with no
/// arms. Every candidate's declared type must also box, because the instruction's result is one
/// Mixed cell whatever storage answered it.
///
/// A class that would answer from its DYNAMIC-property tail is refused by name: that read is a
/// hash lookup rather than a slot, and a ladder that left the class out would report the property
/// undefined on an object that has it.
fn mixed_property_read_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
    receiver_php: &PhpType,
) -> Option<String> {
    let _ = function;
    let Some(property) = data_string(module, inst) else {
        return Some("property read has no property-name Data immediate".to_string());
    };
    mixed_property_read_named_issue(module, inst, receiver_php, property)
}

/// `mixed_property_read_issue` once the property's name is known, however it was spelled.
fn mixed_property_read_named_issue(
    module: &Module,
    inst: &Instruction,
    receiver_php: &PhpType,
    property: &str,
) -> Option<String> {
    if inst.result_type != IrType::Heap(IrHeapKind::Mixed)
        || inst.result_php_type.codegen_repr() != PhpType::Mixed
    {
        return Some(format!(
            "a mixed receiver's property read answers a boxed cell, got {:?}/{:?}",
            inst.result_type,
            inst.result_php_type.codegen_repr()
        ));
    }
    let candidates = super::classes::mixed_property_candidates(module, property, receiver_php);
    if candidates.is_empty() {
        return Some(format!(
            "no class in the module declares ${property}, so a mixed receiver's read of it has no \
             candidate to dispatch to"
        ));
    }
    candidates.iter().find_map(|(_, class_name, source)| {
        let super::classes::MixedPropertySource::Slot { prop_type, .. } = source else {
            return None;
        };
        super::transfer::classify_transfer(
            IrType::from_php(prop_type),
            prop_type.clone(),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        )
        .err()
        .map(|_| {
            format!(
                "{class_name}::${property} is declared {prop_type:?}, which has no boxing into the \
                 Mixed cell the read answers"
            )
        })
    })
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
    // `Generator` has no method BODIES — its methods are the runtime's, and the frame they read
    // is this backend's own. `getReturn()` is the one the first generator slice answers: the
    // frame already stores what the body returned, because that is what a generator's `return`
    // is for. Every one of them is answered from the frame now, so what the gate checks is the
    // ARGUMENTS: `send` takes a value the frame parks as a boxed cell, `throw` an exception
    // object the resume raises. A name none of them knows falls through to the missing-body
    // diagnostic below, which is the truth for a method `Generator` does not have.
    if let Some(issue) = generator_method_shape_issue(module, function, inst, method_name) {
        return Some(issue);
    }
    if generator_method_is_lowered(function, inst, method_name) {
        if method_name.eq_ignore_ascii_case("send") {
            return generator_send_argument_issue(function, inst);
        }
        if method_name.eq_ignore_ascii_case("throw") {
            return generator_throw_argument_issue(function, inst);
        }
        return None;
    }
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
                // An INTERFACE receiver is not an unknown class, it is a class-less one. It
                // names no storage and has no body, so what decides the call is the closed
                // set of concrete implementors — enumerable here, and dispatched through the
                // same class-id if-ladder an ordinary virtual call uses.
                if module.interface_infos.contains_key(&class_name) {
                    return interface_method_call_shape_issue(
                        module,
                        function,
                        inst,
                        arguments,
                        &class_name,
                        method_name,
                        &method_key,
                    );
                }
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
                if let Some(issue) = method_body_argument_shape_issue(module, function, inst, body) {
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

            let candidates = super::classes::mixed_method_candidates(
                module,
                &method_key,
                &receiver_declared_php,
                arguments.len(),
            );
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
                if let Some(issue) = method_body_argument_shape_issue(module, function, inst, body) {
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
        if null_default_is_overwritten_by_the_constructor(
            module,
            class_name,
            property,
            &default.kind,
        ) {
            continue;
        }
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
                // A tagged slot's default is two words the construction writes flat, and the
                // null among them is the one that must NOT be left as the zeroed slot: tag 0
                // is the integer tag.
                | LiteralDefaultValue::TaggedNull
                | LiteralDefaultValue::TaggedInt(_)
        ) && !matches!(
            &literal,
            LiteralDefaultValue::Array { elements, .. }
                | LiteralDefaultValue::BoxedArray { elements, .. }
                if elements.is_empty()
        ) {
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

/// Validates an interface-typed call to an open-coded `Throwable` accessor.
///
/// Two obligations the class-typed check does not have. Every implementor must store the
/// accessor's property in the SAME representation, because one dispatch stub declares one
/// result signature — an implementor that stored `$code` as a Mixed cell could not share a
/// stub that returns an `i64`. And the destination is checked against that storage rather
/// than against the interface's declared return type, which for `getPrevious()` is the wider
/// `?Throwable`.
fn interface_throwable_intrinsic_shape_issue(
    module: &Module,
    interface_name: &str,
    method_name: &str,
    intrinsic: super::objects::ThrowableIntrinsic,
    candidates: &[(String, String)],
    arguments: &[ValueId],
    inst: &Instruction,
) -> Option<String> {
    if !arguments.is_empty() {
        return Some(format!(
            "{interface_name}::{method_name} takes no arguments, got {}",
            arguments.len()
        ));
    }
    let mut storage: Option<(IrType, PhpType)> = None;
    for (candidate, _) in candidates {
        let Some(candidate_info) = module.class_infos.get(candidate) else {
            return Some(format!("missing implementor class {candidate}"));
        };
        let found = match super::objects::throwable_intrinsic_storage(candidate_info, intrinsic) {
            Ok(found) => found,
            Err(error) => return Some(format!("{candidate}::{method_name}: {error}")),
        };
        match &storage {
            None => storage = Some(found),
            Some(agreed) if *agreed == found => {}
            Some(agreed) => {
                return Some(format!(
                    "{interface_name} implementors disagree on {method_name} storage: \
                     {candidate} stores {found:?}, another stores {agreed:?}"
                ))
            }
        }
    }
    let Some((source_ir, source_php)) = storage else {
        return Some(format!(
            "{interface_name}::{method_name} has no implementor to read"
        ));
    };
    if inst.result.is_none() {
        // A discarded accessor result is dropped by arity; nothing reaches a destination.
        return None;
    }
    value_transfer_shape_issue(
        source_ir,
        source_php,
        inst.result_type,
        inst.result_php_type.codegen_repr(),
    )
    .map(|issue| format!("{interface_name}::{method_name} result: {issue}"))
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
            module,
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
    // A NULLABLE object receiver is opened the same way for `->` as for `?->`: the union carries
    // exactly one class, and the null arm is the only thing that differs.
    let class_name = match receiver_object_class(&receiver_php) {
        Ok(class_name) => class_name,
        Err(ReceiverClassIssue::SeveralClasses) => {
            return Some(
                "a nullable property receiver union must name exactly one concrete class"
                    .to_string(),
            );
        }
        Err(ReceiverClassIssue::NotAnObject) if receiver_php.codegen_repr() == PhpType::Mixed => {
            return mixed_property_read_issue(module, function, inst, &receiver_php);
        }
        Err(ReceiverClassIssue::NotAnObject) => {
            return Some(format!(
                "property receiver must resolve to a concrete object, got {:?}/{receiver_php:?}",
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
        // An undeclared property on a class with `__get` is not storage at all: PHP calls the
        // magic accessor. The lowering dispatches to it, so this is admitted before the
        // storage question is asked.
        if magic_get_dispatch_is_supported(module, function, inst) {
            return None;
        }
        if !class_info.allow_dynamic_properties
            && !class_name
                .trim_start_matches('\\')
                .eq_ignore_ascii_case("stdClass")
        {
            return Some(format!(
                "class {class_name} does not provide dynamic property storage for ${property}"
            ));
        }
        // A read whose write is PROVED cannot miss, so it needs nothing of the miss path.
        let cannot_miss = dynamic_property_initialized_before_read(
            function,
            inst,
            receiver_id,
            inst.immediate.as_ref(),
        );
        // And one that CAN miss is admitted now that the miss says so: php's
        // `Warning: Undefined property: C::$p`, which `__rt_warn_undefined_property` renders from
        // the receiver's own class id. Before that helper existed this was refused outright, which
        // turned away every first read of a dynamic property — including the `$o->$name` form,
        // whose name no analysis can prove written.
        if !cannot_miss && !module.functions.iter().any(|function| function.flags.is_main) {
            return Some(format!(
                "dynamic property ${property} may miss, and only a main-bearing module carries the \
                 undefined-property warning"
            ));
        }
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
        // The EIR narrows the read when it has seen the write, and the lowering opens the cell for
        // that — so a concrete result is admitted exactly when an unboxing reaches it.
        if inst.result.is_some()
            && super::transfer::classify_transfer(
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                inst.result_type,
                inst.result_php_type.codegen_repr(),
            )
            .is_ok()
        {
            return None;
        }
        return Some(format!(
            "dynamic property ${property} reads answer {:?}/{:?}, which no unboxing reaches",
            inst.result_type,
            inst.result_php_type.codegen_repr()
        ));
    };
    if typed_property_may_be_uninitialized(
        module,
        function,
        inst,
        &class_name,
        class_info,
        property,
        property_index,
    ) {
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
    // A NULLABLE receiver makes the read itself nullable: `?Node` reading a declared `int`
    // answers `int|null`, which this target stores as a tagged pair rather than as the slot's
    // own width. The slot is still read at its declared type; only the RESULT widens.
    if matches!(receiver.php_type, PhpType::Union(_)) {
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

/// Whether the class's constructor writes `property` before anything outside it can read.
///
/// A typed property with no default answers `Error: Typed property C::$p must not be accessed
/// before initialization` until something writes it, and this backend has no sentinel to test
/// for that — the allocator zeroes the slot, and zero is a legitimate `int` or `bool`. A write
/// in the constructor's ENTRY block removes the question rather than answering it: the store
/// dominates every exit from `new`, so the property is always initialized by the time any other
/// method or caller can reach it.
fn constructor_initializes_property(module: &Module, class_name: &str, property: &str) -> bool {
    let key = crate::names::php_symbol_key("__construct");
    let Some(info) = module.class_infos.get(class_name) else {
        return false;
    };
    let implementation = info
        .method_impl_classes
        .get(&key)
        .cloned()
        .unwrap_or_else(|| class_name.to_string());
    let Some(constructor) = find_method_function(module, &implementation, &key) else {
        return false;
    };
    let Some(entry) = constructor
        .blocks
        .iter()
        .find(|block| block.id == constructor.entry)
    else {
        return false;
    };
    // The store has to come before ANYTHING in the entry block that could observe the slot.
    // Naming the property is one way — `$this->p = $this->p ?? 1;` reads it first — but not the
    // only one: `$this` ESCAPING is enough. Measured on
    //
    //     public function __construct(int $v) { $this->init(); $this->value = $v; }
    //     private function init(): void { var_dump($this->value); }
    //
    // php-src prints NULL and this backend printed int(0), because `init` reads the zeroed slot
    // and no `PropGet` on `value` appears in the constructor at all. So any call, any dynamic
    // property access, and any reference binding before the store ends the proof. Reads in later
    // blocks are dominated by the store and need no check.
    //
    // The receiver has to be `$this` too. Matching the property by NAME alone counts
    // `$this->child->p = 1;` as a write to `$this->p` — a different object's slot that happens
    // to share a name — and the proof would rest on a store that never touches this class.
    //
    // Touching ANOTHER property is not automatically neutral: on a class with `__get` or `__set`
    // it runs user code free to read this one. But treating it as fatal unconditionally would
    // refuse `__construct() { $this->a = 1; $this->b = 2; }` for `$b`, so the magic methods the
    // class actually declares decide. A store of a DIFFERENT property must not end the walk
    // claiming success either — `$this->self = $this;` is a `PropSet` on `$this`, and reading it
    // as proof that some other slot is initialized is exactly the escape it performs.
    let declares = |magic: &str| -> bool {
        info.method_impl_classes
            .contains_key(&crate::names::php_symbol_key(magic))
    };
    let reads_dispatch = declares("__get");
    let writes_dispatch = declares("__set");
    for candidate in entry
        .instructions
        .iter()
        .filter_map(|inst_id| constructor.instruction(*inst_id))
    {
        let on_this = candidate
            .operands
            .first()
            .is_some_and(|receiver| value_local_origin(constructor, *receiver) == Some(0));
        let names_it = data_string(module, candidate) == Some(property);
        // A `PropSet` whose receiver is NOT `$this` is not a sibling slot at all — it is a store
        // into someone else's object, and when the VALUE is `$this` it is the same escape as
        // `self::$last = $this`, just through a different opcode: `$box->held = $this;`.
        let stores_this = candidate
            .operands
            .iter()
            .skip(1)
            .any(|operand| value_local_origin(constructor, *operand) == Some(0));
        match candidate.op {
            Op::PropSet if on_this && names_it => return true,
            Op::PropGet | Op::NullsafePropGet if names_it => return false,
            Op::PropGet | Op::NullsafePropGet if reads_dispatch => return false,
            Op::PropSet if !on_this && stores_this => return false,
            Op::PropSet if on_this && writes_dispatch => return false,
            Op::PropSet => continue, // a declared sibling slot: no dispatch, no escape
            _ if instruction_can_observe_this(constructor, candidate) => return false,
            _ => continue,
        }
    }
    false
}

/// Whether an instruction could let something else read the object being constructed.
///
/// Anything that hands `$this` to code this predicate cannot see — a method call, a free call, a
/// dynamic property access, a reference binding — is a point past which "no reader can precede
/// the store" stops being provable. `ObjectCloneShallow` belongs here for a different reason: it
/// copies EVERY slot, so it observes the one in question whatever its name.
///
/// A CLOSURE is the subtle one: `usort($a, function ($x, $y) { var_dump($this->p); ... })` in a
/// constructor binds `$this` implicitly, with no operand naming it anywhere, and php-src prints
/// NULL at each comparison. So closure creation counts whatever its operands say.
///
/// Parking `$this` somewhere reachable — `$arr[] = $this;`, `$GLOBALS['x'] = $this;`,
/// `self::$last = $this;` — escapes it too, but ONLY when `$this` is what is being stored.
/// Listing those opcodes outright would refuse `__construct() { $a = [1,2]; $this->list = $a; }`,
/// which builds an array before the first property write and is entirely ordinary; so the operand
/// decides, not the opcode.
///
/// A typed `RuntimeCall` is decided the same way, and this is what the object being constructed
/// makes sound: it is FRESH from `new`, so nothing outside the constructor holds a reference to
/// it yet. User code can therefore only reach it through this call's own arguments — a callback
/// that captured it would be an `Op::ClosureNew` above, and an array or a global holding it would
/// be one of the stores above, both of which end the walk before this is asked. So a builtin
/// whose operands do not name `$this` cannot observe it, however much user code it runs.
/// `ArrayIterator::__construct` is the measured case: it calls `array_keys($array)` before its
/// first property write, and treating that as an escape refused every `$this->position` read in
/// the whole SPL iterator family.
fn instruction_can_observe_this(function: &Function, inst: &Instruction) -> bool {
    if matches!(
        inst.op,
        Op::MethodCall
            | Op::NullsafeMethodCall
            | Op::StaticMethodCall
            | Op::EvalStaticMethodCall
            | Op::IteratorMethodCall
            | Op::MethodLookup
            | Op::Call
            | Op::ClosureNew
            | Op::ClosureBind
            | Op::ClosureCall
            | Op::ClosureCapture
            | Op::DynamicPropGet
            | Op::DynamicPropSet
            | Op::LoadPropRefCell
            | Op::BindRefCellPtr
            | Op::ObjectCloneShallow
    ) {
        return true;
    }
    // An UNTYPED runtime call carries no immediate naming what it does, so it keeps the
    // conservative answer; only a typed one is decided by its operands.
    if inst.op == Op::RuntimeCall
        && !matches!(inst.immediate, Some(Immediate::RuntimeCall(_)))
    {
        return true;
    }
    // Every ARGUMENT counts for a call — a builtin's first operand is an ordinary argument, not
    // a container the way a store's is — so this one does not skip operand 0.
    if inst.op == Op::RuntimeCall {
        return inst
            .operands
            .iter()
            .any(|operand| value_local_origin(function, *operand) == Some(0));
    }
    matches!(
        inst.op,
        Op::ArraySet
            | Op::ArrayPush
            | Op::HashSet
            | Op::HashAppend
            | Op::StoreGlobal
            | Op::StoreStaticProperty
    ) && inst
        .operands
        .iter()
        .skip(1)
        .any(|operand| value_local_origin(function, *operand) == Some(0))
}

/// Validates an `include_once`/`require_once` site: it must name the label its flag is keyed by.
///
/// The mark and the guard share one flag, so they share one check. `IncludeOnceMark` used to be
/// admitted as a NO-OP on the strength of the guard being refused — an invariant this pair now
/// replaces, since both carry real storage.
fn include_once_shape_issue(module: &Module, inst: &Instruction) -> Option<String> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return Some("include-once site without an interned label".to_string());
    };
    if module.data.strings.get(data.as_raw() as usize).is_none() {
        return Some(format!("include-once label data {data:?} is missing"));
    }
    None
}

/// Validates a `FunctionVariantMark`: its label must name a group this module dispatches.
///
/// A mark whose group has no dispatcher would set a slot nothing reads, so a call to the public
/// name would fatal as undefined even though the include ran. Refusing here is what keeps the
/// mark and the dispatcher from disagreeing.
fn function_variant_mark_shape_issue(module: &Module, inst: &Instruction) -> Option<String> {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return Some("function variant mark without an interned label".to_string());
    };
    let Some(label) = module.data.strings.get(data.as_raw() as usize) else {
        return Some(format!("function variant label data {data:?} is missing"));
    };
    let Some(parsed) = crate::ir::function_variants::parse_variant_label(label) else {
        return Some(format!("malformed function variant label {label:?}"));
    };
    if parsed.variants.len() != 1 {
        return Some(format!(
            "function variant mark for {:?} names {} variants",
            parsed.name,
            parsed.variants.len()
        ));
    }
    if super::includes::dispatch_group_for(module, &parsed.name).is_none() {
        return Some(format!(
            "function variant mark for {:?} has no dispatch group in this module",
            parsed.name
        ));
    }
    None
}

/// Whether `class_name` is abstract and every concrete class below it initializes `property`.
///
/// "Initializes" means the same two things it means anywhere else here: a literal default on the
/// slot, or a constructor that writes it before anything can read. A hierarchy with no concrete
/// descendant at all answers false — there is nothing to instantiate, but there is also nothing
/// to prove, and claiming otherwise would rest on an empty set.
fn every_concrete_descendant_initializes(
    module: &Module,
    class_name: &str,
    property: &str,
) -> bool {
    let Some(declaring) = module.class_infos.get(class_name) else {
        return false;
    };
    if !declaring.is_abstract {
        return false;
    }
    let descends_from = |candidate: &crate::types::ClassInfo| -> bool {
        let mut parent = candidate.parent.clone();
        for _ in 0..module.class_infos.len() {
            let Some(name) = parent.clone() else { return false };
            if crate::names::php_symbol_key(&name) == crate::names::php_symbol_key(class_name) {
                return true;
            }
            parent = module
                .class_infos
                .get(&name)
                .and_then(|info| info.parent.clone());
        }
        false
    };
    let mut concrete = 0usize;
    for (name, info) in &module.class_infos {
        if info.is_abstract || !descends_from(info) {
            continue;
        }
        concrete += 1;
        let Some(index) = info.properties.iter().position(|(slot, _)| slot == property) else {
            return false;
        };
        let has_default = info
            .defaults
            .get(index)
            .map(|default| default.is_some())
            .unwrap_or(false);
        if !has_default
            && !info.promoted_properties.contains(property)
            && !constructor_initializes_property(module, name, property)
        {
            return false;
        }
    }
    concrete > 0
}

/// Whether the constructor's own store of `property` dominates this read.
///
/// `constructor_initializes_property` has already shown the store is the FIRST property event in
/// the entry block. So a read in a LATER block is dominated by it — the entry block dominates the
/// whole body — and a read in the entry block is dominated exactly when it sits after the store.
fn constructor_store_precedes(
    module: &Module,
    constructor: &Function,
    read: &Instruction,
    property: &str,
) -> bool {
    let Some(entry) = constructor
        .blocks
        .iter()
        .find(|block| block.id == constructor.entry)
    else {
        return false;
    };
    let position = |predicate: &dyn Fn(&Instruction) -> bool| -> Option<usize> {
        entry.instructions.iter().position(|inst_id| {
            constructor.instruction(*inst_id).is_some_and(predicate)
        })
    };
    let Some(store) = position(&|candidate: &Instruction| {
        candidate.op == Op::PropSet && data_string(module, candidate) == Some(property)
    }) else {
        return false;
    };
    match position(&|candidate: &Instruction| std::ptr::eq(candidate, read)) {
        Some(here) => here > store,
        // Not in the entry block at all: the entry dominates every other block.
        None => true,
    }
}

/// Whether a property's NULL default is unobservable because the constructor always overwrites it.
///
/// PHP gives an untyped property an implicit `= null`, and the checker then narrows the slot from
/// what the constructor stores — so `public $value;` written `$this->value = $v;` with an int
/// arrives here as an `Int` slot carrying a null default, which has no representation. There is
/// nothing to represent: every path out of `new` runs the constructor's store, so no reader can
/// ever see the null. Skipping the default is what makes `class Node { public $value; public
/// $next; }` — a linked-list node, ordinary PHP — constructible on this target.
pub(super) fn null_default_is_overwritten_by_the_constructor(
    module: &Module,
    class_name: &str,
    property: &str,
    default: &crate::parser::ast::ExprKind,
) -> bool {
    matches!(default, crate::parser::ast::ExprKind::Null)
        && constructor_initializes_property(module, class_name, property)
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
    let class_name = match &receiver.php_type {
        PhpType::Object(class_name) => class_name.clone(),
        // A `C|null` receiver is the write-side twin of the nullable READ: exactly one concrete
        // class, opened from its boxed cell, with php's `Error` on the null arm.
        PhpType::Union(variants)
            if variants
                .iter()
                .all(|variant| matches!(variant, PhpType::Object(_) | PhpType::Void)) =>
        {
            let classes = variants
                .iter()
                .filter_map(|variant| match variant {
                    PhpType::Object(class_name) => Some(class_name.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [class_name] = classes.as_slice() else {
                return Some(
                    "a nullable property receiver union must name exactly one concrete class"
                        .to_string(),
                );
            };
            // The null arm raises php's `Error`, and this target reproduces the observable of an
            // UNCAUGHT one — so the shape is only supported where nothing could have caught it.
            // Same per-FUNCTION test `throw_error_value` uses, and for the same reason: a handler
            // pushed in an enclosing block is live here even though this block never pushes one.
            if function
                .instructions
                .iter()
                .any(|candidate| candidate.op == Op::TryPushHandler)
            {
                return Some(
                    "a nullable property receiver inside a function with a catch handler on                      wasm32-wasi (the Error its null arm raises is not built on this target)"
                        .to_string(),
                );
            }
            class_name.clone()
        }
        _ => {
            return Some(format!(
                "property receiver must be a concrete object, got {:?}/{:?}",
                receiver.ir_type,
                receiver.php_type.codegen_repr()
            ));
        }
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
        module,
        source.ir_type,
        &source.php_type,
        &property_type,
    )
    .map(|issue| format!("property ${property}: {issue}"))
}

/// Whether an indexed read can be shown, here, to always hit.
///
/// The one shape this proves is the counted walk the EIR generates for an array spread, and it is
/// proved from the EIR rather than from the block names, so a hand-written `for ($i = 0; $i <
/// count($a); $i++) { $a[$i]; }` earns the same result:
///
/// ```text
///   L = array_len A          ... in some block
///   br H(0)
/// H(i):
///   c = icmp i L Slt
///   cond_br c, B, _
/// B:
///   v = array_get A i        <- this read
/// ```
///
/// Four things have to hold, and each of them is load-bearing:
///
/// - `B` is entered ONLY through that `cond_br`'s true edge, so `i < L` holds on entry;
/// - every value flowing into `i` is a non-negative constant or `i + non-negative`, so `i >= 0`;
/// - `L` is `array_len` of the SAME SSA value the read indexes;
/// - nothing in the function writes through that SSA value, so `L` still describes it here.
///
/// The last one is what makes the aliasing question go away without an alias analysis: the array
/// is not written through this name at all, and PHP's copy-on-write means a write through any
/// other name separates the storage first.
/// Whether this read is in bounds by EITHER proof this target knows.
///
/// Two shapes reach here and neither subsumes the other: a `foreach`-style loop tests
/// `i < len` per iteration, and a spread tests `len >= k` once and then reads constants. Both
/// end with the same write test, so what differs is only how the bound was established.
fn array_read_is_in_bounds(function: &Function, inst: &Instruction) -> bool {
    array_read_is_provably_in_bounds(function, inst)
        || array_read_is_in_bounds_by_arity_guard(function, inst)
}

/// Whether an array read sits in a block whose only entry is the true arm of an `index < length`
/// test on the same array, so the index is provably inside it and the read cannot warn.
fn array_read_is_provably_in_bounds(function: &Function, inst: &Instruction) -> bool {
    let [array, index] = inst.operands.as_slice() else {
        return false;
    };
    let Some(block) = function
        .blocks
        .iter()
        .find(|block| {
            block
                .instructions
                .iter()
                .filter_map(|id| function.instruction(*id))
                .any(|candidate| std::ptr::eq(candidate, inst))
        })
    else {
        return false;
    };

    // Exactly one edge into this block, and it is the TRUE arm of a bound test.
    let entries: Vec<_> = function
        .blocks
        .iter()
        .filter(|predecessor| {
            predecessor
                .terminator
                .as_ref()
                .is_some_and(|terminator| terminator_targets(terminator).contains(&block.id))
        })
        .collect();
    let [entry] = entries.as_slice() else {
        return false;
    };
    let Some(Terminator::CondBr {
        cond,
        then_target,
        else_target,
        ..
    }) = entry.terminator.as_ref()
    else {
        return false;
    };
    if *then_target != block.id || *else_target == block.id {
        return false;
    }

    // The guard is `index < length-of-this-array`.
    let Some(compare) = value_defining_instruction(function, *cond) else {
        return false;
    };
    if compare.op != Op::ICmp
        || compare.immediate != Some(Immediate::CmpPredicate(CmpPredicate::Slt))
        || compare.operands.first() != Some(index)
    {
        return false;
    }
    let Some(&length) = compare.operands.get(1) else {
        return false;
    };
    let Some(length_of) = value_defining_instruction(function, length) else {
        return false;
    };
    if length_of.op != Op::ArrayLen || length_of.operands.first() != Some(array) {
        return false;
    }

    // The index is a block parameter, and everything flowing into it is non-negative.
    let Some(position) = entry.params.iter().position(|param| param == index) else {
        return false;
    };
    let non_negative = function.blocks.iter().all(|predecessor| {
        predecessor
            .terminator
            .as_ref()
            .map(|terminator| {
                terminator_args_into(terminator, entry.id)
                    .iter()
                    .all(|args| {
                        args.get(position).is_none_or(|incoming| {
                            index_argument_is_non_negative(function, *incoming, *index)
                        })
                    })
            })
            .unwrap_or(true)
    });
    if !non_negative {
        return false;
    }

    // The recorded length still describes the array at the read. Every write through this SSA
    // name has to sit in the `array_len`'s own block, ahead of it — same block means straight-line
    // order, so the length was taken after the last of them, and re-entering that block re-takes
    // the length after re-running them. A write anywhere else could land between the two.
    writes_through_array_precede_the_length(function, *array, length_of)
}

/// Whether this read is in bounds because a SPREAD checked the array's length up front.
///
/// `f(...$args)` does not emit an `i < len` test per element. It tests the whole call's arity
/// once and then reads fixed indices:
///
/// ```text
///   l = array_len A
///   c = icmp l k Sge          ; "at least k elements", k = the callee's parameter count
///   cond_br c, B, fatal
/// B:
///   v = array_get A 0         ; and A 1, A 2, … up to k-1
/// ```
///
/// So the proof is the same idea spelled the other way round: the block is entered only through
/// that guard's true arm, the index is a CONSTANT below `k`, and the length was taken of the
/// same array this read indexes. The guard's false arm is the spread's own `fatal`, which is
/// what makes the true arm's `len >= k` unconditional rather than merely likely.
///
/// The array is named by a different SSA value at the length and at the read — two loads of one
/// local slot — so the identity is checked through the slot rather than by value. A write
/// through either name between them would break it, and the same write test the `i < len` proof
/// uses answers that.
fn array_read_is_in_bounds_by_arity_guard(function: &Function, inst: &Instruction) -> bool {
    let [array, index] = inst.operands.as_slice() else {
        return false;
    };
    // The index is a constant, and the guard promises at least `k` elements.
    let Some(index_of) = value_defining_instruction(function, *index) else {
        return false;
    };
    let Some(Immediate::I64(position)) = index_of.immediate else {
        return false;
    };
    if index_of.op != Op::ConstI64 || position < 0 {
        return false;
    }

    let Some(block) = block_containing(function, inst) else {
        return false;
    };
    let entries: Vec<_> = function
        .blocks
        .iter()
        .filter(|predecessor| {
            predecessor
                .terminator
                .as_ref()
                .is_some_and(|terminator| terminator_targets(terminator).contains(&block.id))
        })
        .collect();
    let [entry] = entries.as_slice() else {
        return false;
    };
    let Some(Terminator::CondBr {
        cond,
        then_target,
        else_target,
        ..
    }) = entry.terminator.as_ref()
    else {
        return false;
    };
    if *then_target != block.id || *else_target == block.id {
        return false;
    }

    // `array_len(X) >= k`, with the index below `k`.
    let Some(compare) = value_defining_instruction(function, *cond) else {
        return false;
    };
    if compare.op != Op::ICmp
        || compare.immediate != Some(Immediate::CmpPredicate(CmpPredicate::Sge))
    {
        return false;
    }
    let Some(&measured) = compare.operands.first() else {
        return false;
    };
    let Some(&required) = compare.operands.get(1) else {
        return false;
    };
    let Some(required_of) = value_defining_instruction(function, required) else {
        return false;
    };
    let Some(Immediate::I64(count)) = required_of.immediate else {
        return false;
    };
    if required_of.op != Op::ConstI64 || position >= count {
        return false;
    }
    let Some(length_of) = value_defining_instruction(function, measured) else {
        return false;
    };
    if length_of.op != Op::ArrayLen {
        return false;
    }

    // The length and the read name the same array: the same SSA value, or two loads of one slot.
    let Some(&measured_array) = length_of.operands.first() else {
        return false;
    };
    if measured_array != *array
        && local_slot_loaded_by(function, measured_array) != local_slot_loaded_by(function, *array)
    {
        return false;
    }
    if local_slot_loaded_by(function, *array).is_none() && measured_array != *array {
        return false;
    }

    writes_through_array_precede_the_length(function, *array, length_of)
        && writes_through_array_precede_the_length(function, measured_array, length_of)
}

/// The local slot a value was loaded from, when it is a plain load of one.
fn local_slot_loaded_by(function: &Function, value: ValueId) -> Option<crate::ir::LocalSlotId> {
    let inst = value_defining_instruction(function, value)?;
    if inst.op != Op::LoadLocal {
        return None;
    }
    match inst.immediate {
        Some(Immediate::LocalSlot(slot)) => Some(slot),
        _ => None,
    }
}

/// The block whose instruction list contains `inst`.
fn block_containing<'a>(
    function: &'a Function,
    inst: &Instruction,
) -> Option<&'a crate::ir::BasicBlock> {
    function.blocks.iter().find(|block| {
        block
            .instructions
            .iter()
            .filter_map(|id| function.instruction(*id))
            .any(|candidate| std::ptr::eq(candidate, inst))
    })
}

/// Whether every write through `array` happens before `length_of` took its length.
fn writes_through_array_precede_the_length(
    function: &Function,
    array: ValueId,
    length_of: &Instruction,
) -> bool {
    let Some(length_block) = function.blocks.iter().find(|block| {
        block
            .instructions
            .iter()
            .filter_map(|id| function.instruction(*id))
            .any(|candidate| std::ptr::eq(candidate, length_of))
    }) else {
        return false;
    };
    let ordered: Vec<&Instruction> = length_block
        .instructions
        .iter()
        .filter_map(|id| function.instruction(*id))
        .collect();
    let Some(length_at) = ordered
        .iter()
        .position(|candidate| std::ptr::eq(*candidate, length_of))
    else {
        return false;
    };
    function.instructions.iter().all(|candidate| {
        !candidate.effects.contains(Effects::WRITES_HEAP)
            || !candidate.operands.contains(&array)
            || ordered[..length_at]
                .iter()
                .any(|earlier| std::ptr::eq(*earlier, candidate))
    })
}

/// The blocks one terminator can transfer control to.
fn terminator_targets(terminator: &Terminator) -> Vec<BlockId> {
    match terminator {
        Terminator::Br { target, .. } => vec![*target],
        Terminator::CondBr {
            then_target,
            else_target,
            ..
        } => vec![*then_target, *else_target],
        Terminator::Switch {
            cases, default, ..
        } => cases
            .iter()
            .map(|case| case.target)
            .chain(std::iter::once(*default))
            .collect(),
        _ => Vec::new(),
    }
}

/// The argument lists one terminator passes to a given block, one per edge into it.
fn terminator_args_into<'a>(terminator: &'a Terminator, target: BlockId) -> Vec<&'a Vec<ValueId>> {
    match terminator {
        Terminator::Br { target: t, args } if *t == target => vec![args],
        Terminator::CondBr {
            then_target,
            then_args,
            else_target,
            else_args,
            ..
        } => {
            let mut edges = Vec::new();
            if *then_target == target {
                edges.push(then_args);
            }
            if *else_target == target {
                edges.push(else_args);
            }
            edges
        }
        Terminator::Switch {
            cases,
            default,
            default_args,
            ..
        } => {
            let mut edges: Vec<&Vec<ValueId>> = cases
                .iter()
                .filter(|case| case.target == target)
                .map(|case| &case.args)
                .collect();
            if *default == target {
                edges.push(default_args);
            }
            edges
        }
        _ => Vec::new(),
    }
}

/// Whether one value flowing into a loop counter keeps it non-negative.
///
/// A non-negative literal starts it, and `counter + non-negative literal` advances it. The
/// induction is over that one parameter only — an increment computed from anything else is not
/// proved here, and the caller then keeps the tagged shape.
fn index_argument_is_non_negative(
    function: &Function,
    incoming: ValueId,
    counter: ValueId,
) -> bool {
    if incoming == counter {
        return true;
    }
    let Some(instruction) = value_defining_instruction(function, incoming) else {
        return false;
    };
    match instruction.op {
        Op::ConstI64 => matches!(instruction.immediate, Some(Immediate::I64(value)) if value >= 0),
        Op::IAdd | Op::ICheckedAdd => {
            let [left, right] = instruction.operands.as_slice() else {
                return false;
            };
            (*left == counter && const_i64_at_least_zero(function, *right))
                || (*right == counter && const_i64_at_least_zero(function, *left))
        }
        _ => false,
    }
}

/// Whether a value is a literal integer of zero or more.
fn const_i64_at_least_zero(function: &Function, value: ValueId) -> bool {
    value_defining_instruction(function, value).is_some_and(|instruction| {
        instruction.op == Op::ConstI64
            && matches!(instruction.immediate, Some(Immediate::I64(literal)) if literal >= 0)
    })
}

/// The instruction that produced one SSA value, if an instruction did.
fn value_defining_instruction<'a>(function: &'a Function, value: ValueId) -> Option<&'a Instruction> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    function.instruction(inst)
}

/// Validates one value against the DECLARED property slot it will be written into.
///
/// A Mixed/Union/Iterable slot holds a Mixed cell, so the write is the ordinary boxing transfer.
/// A concrete slot is stored raw, so it demands an exact storage match — a narrower rule than the
/// transfer contract, and deliberately so: there is no conversion step on that path.
/// Shared by `PropSet` and by the inherited Throwable constructor that construction open-codes,
/// so both are audited by the rule the emitter actually implements.
///
/// The one relaxation is the same one arguments carry: a DESCENDANT or an IMPLEMENTOR written
/// into a slot declared as an ancestor class or an interface is a pointer either way, and every
/// read of that slot dispatches on the runtime class where it happens.
fn property_write_shape_issue(
    module: &Module,
    source_ir: IrType,
    source_php: &PhpType,
    property_type: &PhpType,
) -> Option<String> {
    if source_ir == IrType::Heap(IrHeapKind::Object)
        && argument_is_a_descendant_of_the_parameter(
            module,
            &source_php.codegen_repr(),
            property_type,
        )
    {
        return None;
    }
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
    // A TAGGED slot takes the `(payload, tag)` pair, and an assignment arrives as one value:
    // `$this->v = 5;` hands over a bare long and `$this->v = null;` a Void. The transfer table
    // states the conversion, the store performs it, and comparing representations here refused
    // both — the same omission the static-property store and the call boundary had.
    if *property_type == PhpType::TaggedScalar
        && source_php.codegen_repr() != PhpType::TaggedScalar
        && transfer::classify_transfer(
            source_ir,
            source_php.codegen_repr(),
            IrType::TaggedScalar,
            PhpType::TaggedScalar,
        )
        .is_ok()
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
    // A concrete-element array stored into an `array<mixed>` slot is a WIDENING, not a mismatch.
    // `private array $items = []; $this->items = [1, 2, 3];` is as ordinary as PHP gets, and a
    // declared `array` property IS `array<mixed>` — so the pair that refused was the common one.
    // The conversion already exists and is the call boundary's: `__rt_array_widen_to_mixed`
    // copies the elements into boxed cells, because a `mixed` array is a different LAYOUT rather
    // than the same array seen loosely.
    if matches!(property_type, PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed)
        && matches!(&source_repr, PhpType::Array(_))
        && transfer::classify_transfer(
            source_ir,
            source_repr.clone(),
            IrType::Heap(IrHeapKind::Array),
            property_type.clone(),
        )
        .is_ok()
    {
        return None;
    }
    // An object slot holds a POINTER, whatever the class, so a genuine subtype stores into it
    // unchanged: `public Iterator $it;` assigned a `RecursiveIterator` implementor is ordinary
    // PHP. What is checked is that the source really is one — by extension or interface — not
    // that the two names match, which turned away every interface-typed property.
    let object_subtype = match (&source_repr, property_type) {
        (PhpType::Object(source_class), PhpType::Object(slot_class)) => {
            source_class == slot_class
                || class_extends(module, source_class, slot_class)
                || class_implements_interface(module, source_class, slot_class)
                // The SOURCE may itself be an interface — `RecursiveIterator` assigned into an
                // `Iterator` slot is interface-to-interface, and neither class walk sees it
                // because an interface has no entry in `class_infos`.
                || (module.interface_infos.contains_key(source_class)
                    && interface_extends(module, source_class, slot_class))
        }
        _ => false,
    };
    if object_subtype && transfer::validate_storage_pair(source_ir, source_php).is_ok() {
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

/// Validates a method call whose receiver is typed by an INTERFACE.
///
/// The interface supplies the call's contract — the arity and storage the call site was
/// compiled against — and every concrete implementor supplies a body that has to honour it.
/// Both are checked: the arguments against the interface signature, then each implementor's
/// body against its own declared signature, the call's arguments, and the result. That is
/// the same obligation an ordinary virtual call carries, for the same reason: PHP picks the
/// body from the RUNTIME class, so a set that disagrees anywhere cannot share one stub.
fn interface_method_call_shape_issue(
    module: &Module,
    function: &Function,
    inst: &Instruction,
    arguments: &[ValueId],
    interface_name: &str,
    method_name: &str,
    method_key: &str,
) -> Option<String> {
    let Some(interface_info) = module.interface_infos.get(interface_name) else {
        return Some(format!("unknown receiver interface {interface_name}"));
    };
    let Some(signature) = interface_info.methods.get(method_key) else {
        return Some(format!(
            "unknown interface method {interface_name}::{method_name}"
        ));
    };
    if let Some(issue) = method_signature_shape_issue(function, arguments, signature, method_name) {
        return Some(issue);
    }
    let candidates = match interface_dispatch_candidates(module, interface_name, method_key) {
        Ok(candidates) => candidates,
        Err(issue) => return Some(issue),
    };
    // A `Throwable` accessor has a signature but no EIR body for any implementor. The call is
    // open-coded against the receiver's slot in the dispatch stub rather than forwarded, so it
    // is audited against that slot — and against every implementor agreeing on it, since one
    // stub carries one result signature.
    if let Some(intrinsic) =
        super::objects::interface_throwable_intrinsic(module, method_key, &candidates)
    {
        return interface_throwable_intrinsic_shape_issue(
            module,
            interface_name,
            method_name,
            intrinsic,
            &candidates,
            arguments,
            inst,
        );
    }
    for (candidate, implementation) in candidates {
        let Some(candidate_info) = module.class_infos.get(&candidate) else {
            return Some(format!("missing implementor class {candidate}"));
        };
        let Some(candidate_signature) = candidate_info.methods.get(method_key) else {
            return Some(format!(
                "missing implementor signature {candidate}::{method_name}"
            ));
        };
        let Some(body) = find_method_function(module, &implementation, method_key) else {
            return Some(format!(
                "missing method body {implementation}::{method_name} for {interface_name} implementor {candidate}"
            ));
        };
        if let Some(issue) = method_body_signature_shape_issue(
            body,
            candidate_signature,
            IrType::Heap(IrHeapKind::Object),
        ) {
            return Some(issue);
        }
        if let Some(issue) = method_body_argument_shape_issue(module, function, inst, body) {
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

/// Audits `Enum::cases()`, `Enum::tryFrom()` and `Enum::from()`, which PHP synthesizes and no
/// body backs.
///
/// Returns `None` when the call is not one of them, so the caller falls through to the ordinary
/// body-backed path; `Some(None)` when it is one and is acceptable; `Some(Some(issue))` when it
/// is one and is not.
///
/// `from()` was once left to fail as a missing body, because it has to raise php-src's
/// `ValueError` naming both the enum and the offending value and answering it without that raise
/// would turn a fatal into a wrong value. It is admitted now that the raise exists: the message
/// is composed at run time and carried on the object, which is why it claims the same uncaught
/// diagnostic a user `throw` does rather than a laid-out constant of its own.
fn enum_static_intrinsic_shape_issue(
    module: &Module,
    owner: &Function,
    inst: &Instruction,
    enum_name: &str,
    method_key: &str,
) -> Option<Option<String>> {
    let Some(enum_info) = module.enum_infos.get(enum_name) else {
        return None;
    };
    match method_key {
        "cases" => {
            if !inst.operands.is_empty() {
                return Some(Some(format!(
                    "{enum_name}::cases() takes no arguments, got {}",
                    inst.operands.len()
                )));
            }
            // The emitter builds a pointer-slot array of the case singletons, so the result has
            // to be exactly that: an owned `array<Enum>` of object pointers.
            let result_php = inst.result_php_type.codegen_repr();
            let expects_array = matches!(
                &result_php,
                PhpType::Array(element)
                    if matches!(element.codegen_repr(), PhpType::Object(class) if class == enum_name)
            );
            if inst.result.is_none()
                || inst.result_type != IrType::Heap(IrHeapKind::Array)
                || !expects_array
            {
                return Some(Some(format!(
                    "{enum_name}::cases() must produce an owned array<{enum_name}>, got {:?}/{result_php:?}",
                    inst.result_type
                )));
            }
            if inst.result_ownership != Ownership::Owned {
                return Some(Some(format!(
                    "{enum_name}::cases() result must be owned, got {:?}",
                    inst.result_ownership
                )));
            }
            Some(None)
        }
        // `from()` differs from `tryFrom()` in its RESULT and in its miss: the singleton pointer
        // rather than a boxed cell, and a raise rather than null. Everything else — a backed
        // enum, one argument of the backing type — is the same question.
        "from" => {
            let Some(backing) = enum_info.backing_type.as_ref().map(PhpType::codegen_repr) else {
                return Some(Some(format!("{enum_name}::from() needs a backed enum")));
            };
            let [argument] = inst.operands.as_slice() else {
                return Some(Some(format!(
                    "{enum_name}::from() takes one argument, got {}",
                    inst.operands.len()
                )));
            };
            let Some(value) = owner.value(*argument) else {
                return Some(Some("from argument is missing from the value table".to_string()));
            };
            let argument_matches = matches!(
                (&backing, value.ir_type, value.php_type.codegen_repr()),
                (PhpType::Int, IrType::I64, PhpType::Int) | (PhpType::Str, IrType::Str, PhpType::Str)
            );
            if !argument_matches {
                return Some(Some(format!(
                    "{enum_name}::from() expects its {backing:?} backing value, got {:?}/{:?}",
                    value.ir_type,
                    value.php_type.codegen_repr()
                )));
            }
            // The raise builds a `ValueError` the same way every runtime error here does, so the
            // class has to be one this target can construct — and the message it carries is
            // computed, so it must be a Throwable with a string `$message` slot.
            if !super::objects::runtime_error_is_constructible(module, "ValueError") {
                return Some(Some(format!(
                    "{enum_name}::from() raises a ValueError this module cannot construct"
                )));
            }
            let result_php = inst.result_php_type.codegen_repr();
            let answers_the_case = matches!(
                &result_php,
                PhpType::Object(class) if class == enum_name
            );
            if inst.result.is_none()
                || inst.result_type != IrType::Heap(IrHeapKind::Object)
                || !answers_the_case
            {
                return Some(Some(format!(
                    "{enum_name}::from() must produce an owned {enum_name}, got {:?}/{result_php:?}",
                    inst.result_type
                )));
            }
            if inst.result_ownership != Ownership::Owned {
                return Some(Some(format!(
                    "{enum_name}::from() result must be owned, got {:?}",
                    inst.result_ownership
                )));
            }
            Some(None)
        }
        "tryfrom" => {
            // Only a BACKED enum has a value to look up; a pure one has no `tryFrom` at all.
            let Some(backing) = enum_info.backing_type.as_ref().map(PhpType::codegen_repr) else {
                return Some(Some(format!(
                    "{enum_name}::tryFrom() needs a backed enum"
                )));
            };
            let [argument] = inst.operands.as_slice() else {
                return Some(Some(format!(
                    "{enum_name}::tryFrom() takes one argument, got {}",
                    inst.operands.len()
                )));
            };
            let Some(value) = owner.value(*argument) else {
                return Some(Some("tryFrom argument is missing from the value table".to_string()));
            };
            let argument_matches = match (&backing, value.ir_type, value.php_type.codegen_repr()) {
                (PhpType::Int, IrType::I64, PhpType::Int) => true,
                (PhpType::Str, IrType::Str, PhpType::Str) => true,
                _ => false,
            };
            if !argument_matches {
                return Some(Some(format!(
                    "{enum_name}::tryFrom() expects its {backing:?} backing value, got {:?}/{:?}",
                    value.ir_type,
                    value.php_type.codegen_repr()
                )));
            }
            // A miss is null, so the result has to be able to hold one: a Mixed cell.
            let result_php = inst.result_php_type.codegen_repr();
            if inst.result.is_none()
                || inst.result_type != IrType::Heap(IrHeapKind::Mixed)
                || result_php != PhpType::Mixed
            {
                return Some(Some(format!(
                    "{enum_name}::tryFrom() must produce a boxed Mixed result, got {:?}/{result_php:?}",
                    inst.result_type
                )));
            }
            if inst.result_ownership != Ownership::Owned {
                return Some(Some(format!(
                    "{enum_name}::tryFrom() result must be owned, got {:?}",
                    inst.result_ownership
                )));
            }
            // Every case needs a backing value the ladder can compare against.
            if let Some(case) = enum_info.cases.iter().find(|case| case.value.is_none()) {
                return Some(Some(format!(
                    "{enum_name}::{} has no backing value to match",
                    case.name
                )));
            }
            Some(None)
        }
        _ => None,
    }
}

/// Enumerates every concrete class that can arrive at an INTERFACE-typed receiver.
///
/// An interface names no storage and has no body: what arrives is one pointer to an object
/// whose header carries its real class id, and PHP resolves the call on THAT class. So the
/// receiver's static type is not the callee — the closed set of concrete implementors is,
/// and the same class-id if-ladder that serves an ordinary virtual call serves this one.
///
/// Each pair is `(runtime class, class whose body implements the method)`; the two differ
/// when an implementor inherits the implementation from a parent. Ordered by class id so the
/// audit and `emit_method_dispatch_stubs` walk the arms in the same order.
pub(super) fn interface_dispatch_candidates(
    module: &Module,
    interface_name: &str,
    method_key: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut candidates = module
        .class_infos
        .iter()
        .filter(|(class_name, class_info)| {
            !class_info.is_abstract
                && class_info.methods.contains_key(method_key)
                && class_implements_interface(module, class_name, interface_name)
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
    candidates.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    if candidates.is_empty() {
        return Err(format!(
            "interface method {interface_name}::{method_key} has no concrete implementor"
        ));
    }
    Ok(candidates
        .into_iter()
        .map(|(_, class_name, implementation)| (class_name, implementation))
        .collect())
}

/// Returns whether `class_name` implements `interface_name`, by inheritance or extension.
///
/// Both directions have to be walked: PHP gives a class its parents' interfaces, and an
/// interface its own parents' methods, so `class C extends B` where `B implements J` and
/// `interface J extends I` makes a `C` a legitimate `I`. Reading only the class's own
/// `interfaces` list would miss every implementor that inherits the obligation.
pub(super) fn class_implements_interface(
    module: &Module,
    class_name: &str,
    interface_name: &str,
) -> bool {
    let mut current = Some(class_name.to_string());
    let mut visited = HashSet::new();
    while let Some(name) = current {
        if !visited.insert(name.clone()) {
            return false;
        }
        let Some(class_info) = module.class_infos.get(&name) else {
            return false;
        };
        if class_info
            .interfaces
            .iter()
            .any(|declared| interface_extends(module, declared, interface_name))
        {
            return true;
        }
        current = class_info.parent.clone();
    }
    false
}

/// Returns whether `class_name` transitively extends `ancestor`.
pub(super) fn class_extends(module: &Module, class_name: &str, ancestor: &str) -> bool {
    let mut current = module.class_infos.get(class_name).and_then(|c| c.parent.clone());
    let mut visited = HashSet::new();
    while let Some(name) = current {
        if name == ancestor {
            return true;
        }
        if !visited.insert(name.clone()) {
            return false;
        }
        current = module.class_infos.get(&name).and_then(|c| c.parent.clone());
    }
    false
}

/// Returns whether `interface_name` is, or transitively extends, `ancestor`.
fn interface_extends(module: &Module, interface_name: &str, ancestor: &str) -> bool {
    let mut stack = vec![interface_name.to_string()];
    let mut visited = HashSet::new();
    while let Some(name) = stack.pop() {
        if name == ancestor {
            return true;
        }
        if !visited.insert(name.clone()) {
            continue;
        }
        if let Some(interface_info) = module.interface_infos.get(&name) {
            stack.extend(interface_info.parents.iter().cloned());
        }
    }
    false
}

/// Returns whether `class_name` belongs to the class subtree rooted at `ancestor`.
pub(super) fn class_descends_from(module: &Module, class_name: &str, ancestor: &str) -> bool {
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
/// Whether this module could ever hold an instance of `class_name`.
///
/// A class that DECLARES `__construct` but whose resolved implementation has no body here cannot
/// be instantiated: `object_new_shape_issue` refuses `new C` for exactly that reason, so a module
/// that compiles contains no such object, and no dynamic dispatch can select it. A class that
/// declares no constructor at all is constructible with no body needed, so it stays a candidate.
///
/// This is what the SPL prelude needs. `__ElephcAppendIteratorArrayIterator` extends
/// `ArrayIterator` and declares `append`, but the module carries NONE of its bodies — so
/// `$this->append(...)` inside `ArrayIterator::offsetSet` collected it as a dispatch candidate
/// and refused for a body that will never exist, taking `ArrayIterator` itself down with it. The
/// same audit that would refuse constructing it is what licenses dropping it.
pub(super) fn class_is_constructible(module: &Module, class_name: &str) -> bool {
    let key = crate::names::php_symbol_key("__construct");
    let Some(class_info) = module.class_infos.get(class_name) else {
        return false;
    };
    if !class_info.methods.contains_key(&key) {
        return true; // no declared constructor: `new C` needs no body
    }
    // A THROWABLE does not need one: the runtime raises `ValueError` and its siblings directly,
    // without ever reaching `new`, and the `Throwable` accessors are open-coded against bodyless
    // classes on purpose. Dropping them left `catch (ValueError $e) { $e->getMessage(); }` with
    // no dispatch candidate at all.
    if super::objects::is_throwable_class(module, class_name) {
        return true;
    }
    let implementation = class_info
        .method_impl_classes
        .get(&key)
        .cloned()
        .unwrap_or_else(|| class_name.to_string());
    find_method_function(module, &implementation, &key).is_some()
}

/// Lists the `(class, symbol)` pairs a dynamically dispatched call to `method_key` on a
/// `receiver_class` value can reach: the introducer's implementation and every override.
///
/// Fails with a message naming the problem when the method is missing or a candidate cannot
/// be called through the dispatch ladder.
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
                && class_is_constructible(module, class_name)
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
    // `cases()` and `tryFrom()` are SYNTHESIZED by PHP for every enum: they have a signature but
    // no body on either backend, so they are audited against what the emitter open-codes rather
    // than against a function that will never exist — the same treatment the Throwable accessors
    // get.
    if module.enum_infos.contains_key(receiver_class.as_str()) {
        if let Some(result) =
            enum_static_intrinsic_shape_issue(module, owner, inst, &receiver_class, &method_key)
        {
            return result;
        }
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
        // A concrete scalar bound to a `mixed` parameter is a legal boxing transfer, which the
        // call lowering performs — the same relaxation the instance-call path applies.
        if argument_boxes_into_a_mixed_parameter(value, parameter)
            || argument_boxes_a_container_into_a_mixed_parameter(value, parameter)
            || argument_widens_into_an_array_parameter(value, parameter)
            || argument_tags_into_a_nullable_parameter(value, parameter)
            || argument_narrows_out_of_a_tagged_value(value, parameter)
        {
            continue;
        }
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

/// Accepts a SUBCLASS argument where the parameter declares an ancestor class.
///
/// `Probe::check(Widget $item)` handed a `Button` compared unequal on the class NAME alone —
/// both sides are `IrType::Heap(IrHeapKind::Object)`, so the transfer itself is a pointer copy
/// and always representationally exact. What the equality was standing in for is whether the
/// callee's own instructions are valid for the object that actually arrives, and they already
/// are: every method call is audited against its declared receiver class AND every descendant
/// of it, precisely because PHP dispatches on the RUNTIME class. A `Button` is one of the
/// classes `Widget` was already proven safe for, so refusing it proves nothing extra.
///
/// An IMPLEMENTOR passed where the parameter declares an interface is the same question and
/// gets the same answer. `AppendIterator::append(Iterator $it)` handed an `ArrayIterator` was
/// refused because an interface names no storage of its own — but neither does a parent class,
/// and the argument here is a pointer either way. What the callee does with an interface-typed
/// parameter is dispatch on the runtime class, which is audited where it happens: a method call
/// the interface stub cannot serve is refused there, by name, rather than by forbidding every
/// argument that could reach it.
fn argument_is_a_descendant_of_the_parameter(
    module: &Module,
    argument: &PhpType,
    parameter: &PhpType,
) -> bool {
    let (PhpType::Object(argument), PhpType::Object(parameter)) = (argument, parameter) else {
        return false;
    };
    class_descends_from(module, argument, parameter)
        || class_implements_interface(module, argument, parameter)
}

/// Validates argument storage against the concrete method body's WASM signature.
fn method_body_argument_shape_issue(
    module: &Module,
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
        if argument_boxes_into_a_mixed_parameter(value, parameter)
            || argument_boxes_a_container_into_a_mixed_parameter(value, parameter)
            || argument_widens_into_an_array_parameter(value, parameter)
            || argument_tags_into_a_nullable_parameter(value, parameter)
            || argument_narrows_out_of_a_tagged_value(value, parameter)
        {
            continue;
        }
        if value.ir_type != parameter.ir_type
            || (value.php_type.codegen_repr() != parameter.php_type.codegen_repr()
                && !argument_is_a_descendant_of_the_parameter(
                    module,
                    &value.php_type.codegen_repr(),
                    &parameter.php_type.codegen_repr(),
                ))
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

/// Whether the call site's result is a SUBCLASS of what the body declared.
///
/// This is the `static` return type: the body says `Builder` because that is where the method
/// lives, and the call site says `QueryBuilder` because that is what its receiver is. Only that
/// direction is admitted — a body declaring the subclass and a site expecting the base would be
/// a widening the pointer does not justify.
fn result_narrows_declared_class(result: &PhpType, declared: &PhpType) -> bool {
    let (PhpType::Object(result_class), PhpType::Object(declared_class)) =
        (result.codegen_repr(), declared.codegen_repr())
    else {
        return false;
    };
    result_class != declared_class
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
                && (inst.result_php_type.codegen_repr() == declared_php
                    // A `static` return type is the RECEIVER's class, which the call site knows
                    // more precisely than the body that declared it: `Builder::andWhere(): static`
                    // returns `$this`, so a `QueryBuilder` receiver makes the call answer a
                    // `QueryBuilder`. The pointer is the same one either way, so the narrowing
                    // relabels rather than converts — and it is only sound in that direction.
                    || result_narrows_declared_class(&inst.result_php_type, &declared_php))
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
        // Every lowering here reads the PHP arguments as separate operands. The packed form
        // hands them over as ONE indexed array of boxed values instead, and nothing unpacks it.
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
            target,
            arguments: crate::ir::RuntimeArgumentLayout::IndexedArray,
            ..
        })) => {
            issues.push(format!(
                "{context}: unsupported runtime function {} shape: packed indexed-array arguments",
                target.as_eir()
            ));
        }
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
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionChain)) => {
            let objects = call.operands.len() == 2
                && call.is_void()
                && call.operands.iter().all(|operand| {
                    function
                        .value(*operand)
                        .is_some_and(|value| value.ir_type == IrType::Heap(IrHeapKind::Object))
                });
            if !objects {
                issues.push(format!(
                    "{context}: exception chain takes two exception objects and answers nothing"
                ));
            } else if let Err(issue) = super::objects::throwable_previous_offset(module) {
                issues.push(format!("{context}: exception chain: {issue}"));
            }
        }
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ArrayFetchForWrite)) => {
            issues.push(format!(
                "{context}: unsupported runtime target array.fetch_for_write"
            ));
        }
        Some(Immediate::RuntimeCall(
            target @ (RuntimeCallTarget::ExceptionGuardOwned
            | RuntimeCallTarget::ExceptionUnguardOwned
            | RuntimeCallTarget::ExceptionUpdateArrayGuard
            | RuntimeCallTarget::ExceptionUpdateHashGuard),
        )) => {
            if let Some(issue) = exception_guard_shape_issue(function, call, target) {
                issues.push(format!("{context}: {issue}"));
            }
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
        // The generic `$mixed[$key]` read: no immediate, a boxed receiver, a key, and PHP's own
        // warning flag. `__rt_mixed_array_get` dispatches the rest on the receiver's runtime tag,
        // so the only compile-time questions are the shapes of the three operands and whether the
        // module carries the command runtime the warnings live in.
        None if mixed_array_read_is_supported(module, function, call) => {}
        // `$obj[$key]` on an `ArrayAccess` implementor: dispatched to `offsetGet`.
        None if array_access_read_is_supported(function, call) => {}
        // `$obj[$key] = $value` on the same: dispatched to `offsetSet`.
        None if array_access_write_is_supported(function, call) => {}
        // `$a[$k] = $v` through a declared `array`, which is a boxed cell.
        None if mixed_array_write_is_supported(module, function, call) => {}
        // The one-operand narrowing of a boxed value into a declared class slot.
        None if mixed_object_narrowing_is_supported(function, call) => {}
        // The widening of a concrete scalar into a `?int` slot.
        None if tagged_scalar_widening_is_supported(function, call) => {}
        other => {
            let operands = call
                .operands
                .iter()
                .map(|operand| {
                    function
                        .value(*operand)
                        .map_or_else(|| "?".to_string(), |value| format!("{:?}", value.ir_type))
                })
                .collect::<Vec<_>>()
                .join(", ");
            issues.push(format!(
                "{context}: missing typed runtime target, carries {} over ({operands})",
                runtime_call_immediate_kind(other.as_ref())
            ));
        }
    }
}

/// Returns true when an untyped `Op::RuntimeCall` is the generic `$mixed[$key]` read this
/// backend serves.
///
/// The native backend discriminates these calls by operand count and type rather than by an
/// immediate, and this is the three-operand form with a result: a boxed receiver, a key, and
/// PHP's warning flag. The receiver's tag is settled at RUNTIME, so the only questions here are
/// representational.
///
/// The key must be one `materialize_hash_key` can normalize — an int, a float, a string, or a
/// boxed cell — because that normalization is what makes `$a["1"]` find the element `$a[1]`
/// holds. And the read must be in a main-bearing module: its miss and its bad-receiver answers
/// go through the warning helpers, which are part of the command runtime.
fn mixed_array_read_is_supported(module: &Module, function: &Function, call: &Instruction) -> bool {
    if call.operands.len() != 3 || call.is_void() {
        return false;
    }
    if !module.functions.iter().any(|f| f.flags.is_main) {
        return false;
    }
    if call.result_type != IrType::Heap(IrHeapKind::Mixed) {
        return false;
    }
    let operand_ir = |index: usize| {
        call.operands
            .get(index)
            .and_then(|value| function.value(*value))
            .map(|value| value.ir_type)
    };
    if operand_ir(0) != Some(IrType::Heap(IrHeapKind::Mixed)) {
        return false;
    }
    if !matches!(
        operand_ir(1),
        Some(IrType::I64 | IrType::F64 | IrType::Str | IrType::Heap(IrHeapKind::Mixed))
    ) {
        return false;
    }
    operand_ir(2) == Some(IrType::I64)
}

/// Returns true when a property read must dispatch to the class's `__get`.
///
/// PHP calls `__get($name)` when a property is not declared on the receiver, so this is a method
/// call wearing a property read's shape — the same situation `$obj[$key]` is in for `offsetGet`.
/// Admitted only for an exactly-known receiver class whose `__get` this module can call, and
/// only when the read produces a boxed result, which is what a `mixed` return needs.
pub(super) fn magic_get_dispatch_is_supported(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> bool {
    let Some(receiver) = inst.operands.first() else {
        return false;
    };
    let Some(receiver_value) = function.value(*receiver) else {
        return false;
    };
    if receiver_value.ir_type != IrType::Heap(IrHeapKind::Object) {
        return false;
    }
    let PhpType::Object(class_name) = receiver_value.php_type.codegen_repr() else {
        return false;
    };
    let Some(class_info) = module.class_infos.get(&class_name) else {
        return false;
    };
    // A DECLARED property is ordinary storage; `__get` only runs for one that is not.
    let Some(property) = data_string(module, inst) else {
        return false;
    };
    if class_info.properties.iter().any(|(name, _)| name == property) {
        return false;
    }
    if !class_info.methods.contains_key("__get") {
        return false;
    }
    // The checker types the READ as whatever `__get` returns — `Str` here, not Mixed — so the
    // two must agree exactly, or the call's result would be stored through the wrong shape.
    let implementation = class_info
        .method_impl_classes
        .get("__get")
        .cloned()
        .unwrap_or(class_name);
    module
        .class_methods
        .iter()
        .find(|body| body.name == format!("{implementation}::__get"))
        .is_some_and(|body| {
            // The name is passed as a literal unless the parameter is declared `mixed`, in which
            // case the lowering boxes it. Any other parameter shape has no conversion here.
            let name_shape = body.params.get(1).map(|parameter| parameter.ir_type);
            body.return_type == inst.result_type
                && matches!(
                    name_shape,
                    Some(IrType::Str | IrType::Heap(IrHeapKind::Mixed))
                )
        })
}

/// Returns true when an untyped `Op::RuntimeCall` is `$obj[$key] = $value` on an `ArrayAccess`
/// implementor, which dispatches to `offsetSet`.
///
/// Told apart from the READ by the RESULT: a subscript write lowers void, while a read always
/// produces one. That is how the native backend decides it too — operand count alone cannot,
/// since a read carries a trailing warn-on-missing flag that makes both forms three-operand.
pub(super) fn array_access_write_is_supported(function: &Function, call: &Instruction) -> bool {
    if call.operands.len() != 3 || !call.is_void() {
        return false;
    }
    let operand_ir = |index: usize| {
        call.operands
            .get(index)
            .and_then(|value| function.value(*value))
            .map(|value| value.ir_type)
    };
    let boxable = |index: usize| {
        matches!(
            operand_ir(index),
            Some(IrType::I64 | IrType::F64 | IrType::Str | IrType::Heap(IrHeapKind::Mixed))
        )
    };
    operand_ir(0) == Some(IrType::Heap(IrHeapKind::Object)) && boxable(1) && boxable(2)
}

/// Returns true when a concrete CONTAINER argument reaches a `mixed` parameter.
///
/// php's declared `array` is a boxed value on this compiler, so `f(array $a)` called with a
/// literal list hands an `array<string>` to a Mixed parameter. The transfer layer boxes it into
/// a tag-4 (list), tag-5 (hash) or tag-6 (object) cell that holds one more reference to the same
/// payload — copy-on-write keeps the caller's array apart from a callee write — and the call
/// site releases that cell after the call, exactly as it does a boxed scalar.
pub(super) fn argument_boxes_a_container_into_a_mixed_parameter(
    value: &crate::ir::Value,
    parameter: &crate::ir::FunctionParam,
) -> bool {
    parameter.ir_type == IrType::Heap(IrHeapKind::Mixed)
        && parameter.php_type.codegen_repr() == PhpType::Mixed
        && matches!(
            value.ir_type,
            IrType::Heap(IrHeapKind::Array | IrHeapKind::Hash | IrHeapKind::Object)
        )
        && matches!(
            transfer::classify_transfer(
                value.ir_type,
                value.php_type.clone(),
                parameter.ir_type,
                parameter.php_type.clone(),
            ),
            Ok(transfer::TransferKind::BoxMixed)
        )
}

/// Returns true when a CONCRETE scalar argument reaches a `mixed` parameter, which the call
/// lowering boxes at the call site.
///
/// `$map[$k]` on an `ArrayAccess` implementor reaches `offsetExists(mixed $offset)` with a bare
/// string, and a `mixed`-typed parameter is the single commonest shape a concrete argument meets
/// — 25 of the examples that still refuse carry it. Each of these has an EXACT tag and payload,
/// which is what `box_value_into_mixed_cell` needs; a heap container has neither, so it is left
/// out of this relaxation rather than guessed at.
pub(super) fn argument_boxes_into_a_mixed_parameter(
    value: &crate::ir::Value,
    parameter: &crate::ir::FunctionParam,
) -> bool {
    parameter.ir_type == IrType::Heap(IrHeapKind::Mixed)
        && parameter.php_type.codegen_repr() == PhpType::Mixed
        && matches!(value.ir_type, IrType::I64 | IrType::F64 | IrType::Str)
        && matches!(
            value.php_type.codegen_repr(),
            PhpType::Int
                | PhpType::Bool
                | PhpType::False
                | PhpType::Float
                | PhpType::Str
                | PhpType::Void
        )
}

/// Returns true when a TAGGED argument reaches a concrete scalar parameter it narrows into.
///
/// The mirror of `argument_tags_into_a_nullable_parameter`, and what `f(...$args)` needs from
/// the other side: the spread reads each element as `int|null` — an element read might miss —
/// and the callee declares `int`, one word. The pair's payload is the value and its tag is the
/// word the callee has no place for.
///
/// Dropping the tag is the NATIVE contract rather than a shortcut: native keeps a tagged
/// value's payload in the integer result register and its tag in a separate one, so a concrete
/// parameter reads the payload and never looks at the tag. A genuinely null element at a
/// non-nullable parameter is a `TypeError` the checker raises before either backend sees it.
pub(super) fn argument_narrows_out_of_a_tagged_value(
    value: &crate::ir::Value,
    parameter: &crate::ir::FunctionParam,
) -> bool {
    if value.ir_type != IrType::TaggedScalar
        || value.php_type.codegen_repr() != PhpType::TaggedScalar
    {
        return false;
    }
    if parameter.php_type.codegen_repr() == PhpType::TaggedScalar {
        return false; // already the destination shape; nothing to convert
    }
    transfer::classify_transfer(
        value.ir_type,
        value.php_type.codegen_repr(),
        parameter.ir_type,
        parameter.php_type.codegen_repr(),
    )
    .is_ok()
}

/// Returns true when an argument reaches a NULLABLE scalar parameter it has to be tagged for.
///
/// `function f(?int $n)` called as `f(5)` or `f(null)` hands over a bare long or a Void, and
/// the call lowering makes the `(payload, tag)` pair out of either — `TaggedScalarFromConcrete`
/// and `TaggedNull` have always been in the transfer table. What was missing was the call
/// boundary asking, exactly as it was for an `array<T>` reaching an `array<mixed>`.
///
/// The TABLE is the authority on which sources have a conversion, so a source it cannot shape
/// — a string, whose `(pointer, length)` does not fit one payload word — stays refused here
/// rather than guessed at.
pub(super) fn argument_tags_into_a_nullable_parameter(
    value: &crate::ir::Value,
    parameter: &crate::ir::FunctionParam,
) -> bool {
    if parameter.ir_type != IrType::TaggedScalar
        || parameter.php_type.codegen_repr() != PhpType::TaggedScalar
    {
        return false;
    }
    let source = value.php_type.codegen_repr();
    if source == PhpType::TaggedScalar {
        return false; // already the destination shape; nothing to convert
    }
    transfer::classify_transfer(
        value.ir_type,
        source,
        parameter.ir_type,
        parameter.php_type.codegen_repr(),
    )
    .is_ok()
}

/// Returns true when an `array<T>` argument reaches an `array<mixed>` parameter, which the call
/// lowering CONVERTS at the call site.
///
/// Element-wise, not a pointer copy: the two layouts differ, so admitting this without emitting
/// the conversion would hand the callee raw string pointers to read as Mixed cells. The transfer
/// layer has always known how (`TransferKind::WidenArrayToMixed`); what was missing was the call
/// boundary asking it to. `array_widen_shape` is the authority on which element types have a
/// conversion, so an element it cannot shape stays refused here rather than guessed at.
pub(super) fn argument_widens_into_an_array_parameter(
    value: &crate::ir::Value,
    parameter: &crate::ir::FunctionParam,
) -> bool {
    if value.ir_type != IrType::Heap(IrHeapKind::Array)
        || parameter.ir_type != IrType::Heap(IrHeapKind::Array)
        || parameter.php_type.codegen_repr() != PhpType::Array(Box::new(PhpType::Mixed))
    {
        return false;
    }
    let source = value.php_type.codegen_repr();
    if source == parameter.php_type.codegen_repr() {
        return false; // already the destination shape; nothing to convert
    }
    let PhpType::Array(element) = &source else {
        return false;
    };
    transfer::array_widen_shape(element).is_some()
}

/// Returns the LITERAL constant name a `define()` call names, when it has one.
///
/// Only a literal is admitted: the duplicate flag is a per-name global, and a computed name
/// would have no global to read. php-src allows `define($computed, …)`, so this is a coverage
/// limit rather than a semantic one.
pub(super) fn define_constant_name<'a>(
    module: &'a Module,
    function: &Function,
    call: &Instruction,
) -> Option<&'a str> {
    let name_value = *call.operands.first()?;
    let defining = function
        .instructions
        .iter()
        .find(|inst| inst.result == Some(name_value))?;
    if defining.op != Op::ConstStr {
        return None;
    }
    let Some(Immediate::Data(data_id)) = defining.immediate else {
        return None;
    };
    module
        .data
        .strings
        .get(data_id.as_raw() as usize)
        .map(String::as_str)
}

/// Returns true when an untyped `Op::RuntimeCall` is `$obj[$key]` on an `ArrayAccess` receiver.
///
/// Same three operands and result as the boxed `$mixed[$key]` read above; the RECEIVER is what
/// separates them. Admitted only for a receiver whose exact class is known and whose
/// `offsetGet` this module can call directly — an interface-typed or Mixed receiver would need
/// the dispatch the method lowering does from a name, and that name is not in the string table.
///
/// A WRITE carries the same three operands but no RESULT, and its third operand is the value
/// rather than the warn flag — see `array_access_write_is_supported`.
pub(super) fn array_access_read_is_supported(function: &Function, call: &Instruction) -> bool {
    if call.operands.len() != 3 || call.is_void() {
        return false;
    }
    let operand_ir = |index: usize| {
        call.operands
            .get(index)
            .and_then(|value| function.value(*value))
            .map(|value| value.ir_type)
    };
    operand_ir(0) == Some(IrType::Heap(IrHeapKind::Object))
        && matches!(
            operand_ir(1),
            Some(IrType::I64 | IrType::F64 | IrType::Str | IrType::Heap(IrHeapKind::Mixed))
        )
        && operand_ir(2) == Some(IrType::I64)
        && call.result_type == IrType::Heap(IrHeapKind::Mixed)
}

/// Returns true when an untyped `Op::RuntimeCall` narrows one boxed value to a declared class.
///
/// The EIR keeps a `?Money` property boxed until something demands the declared type, then asks
/// for it with an immediate-less one-operand call — which is exactly how the native backend
/// recognizes it too. The payload word of an object cell IS the object pointer, so the whole
/// operation is an unbox and an incref; nothing about it depends on WHICH class is declared,
/// only that both sides agree the result is a pointer-backed object.
pub(super) fn mixed_object_narrowing_is_supported(function: &Function, call: &Instruction) -> bool {
    if call.operands.len() != 1 || call.result.is_none() {
        return false;
    }
    if call.result_type != IrType::Heap(IrHeapKind::Object)
        || !matches!(call.result_php_type.codegen_repr(), PhpType::Object(_))
    {
        return false;
    }
    let Some(source) = call.operands.first().and_then(|value| function.value(*value)) else {
        return false;
    };
    source.ir_type == IrType::Heap(IrHeapKind::Mixed)
}

/// Returns true when an untyped `Op::RuntimeCall` widens a concrete scalar into a `?int` slot.
///
/// `function f(int $i): ?int { return 10; }` reaches the nullable slot through the same
/// immediate-less one-operand call the object narrowing uses, and this target stores a `?int` as
/// an inline `{payload, tag}` pair — so the operation is "keep the word, attach a tag".
///
/// This is a WIDENING, which is what makes it exact without any diagnostic: every `int` and
/// every `null` has a representation in the pair, so nothing can be lost and nothing can raise.
/// A narrowing is the opposite question — PHP may coerce or raise there — and stays refused
/// until it carries its own per-tag answers, which is why this does not simply defer to
/// `transfer::classify_transfer` for any pair it happens to accept.
pub(super) fn tagged_scalar_widening_is_supported(function: &Function, call: &Instruction) -> bool {
    if call.operands.len() != 1 || call.result.is_none() {
        return false;
    }
    if call.result_type != IrType::TaggedScalar
        || call.result_php_type.codegen_repr() != PhpType::TaggedScalar
    {
        return false;
    }
    let Some(source) = call.operands.first().and_then(|value| function.value(*value)) else {
        return false;
    };
    matches!(
        (source.ir_type, source.php_type.codegen_repr()),
        (IrType::I64, PhpType::Int | PhpType::Bool | PhpType::Void)
            | (IrType::F64, PhpType::Float)
            | (IrType::Void, PhpType::Void)
    )
}

/// Names the immediate an `Op::RuntimeCall` carries when it is not a typed runtime target.
///
/// Without this, every such refusal reads as one opaque `missing typed runtime target`, which
/// says nothing about what is actually missing — and it is the single most frequent refusal in
/// the example suite, so an unnamed bucket hides the largest piece of remaining work. The
/// operand types are printed alongside because an untyped call carries no other discriminator.
fn runtime_call_immediate_kind(immediate: Option<&Immediate>) -> String {
    let Some(immediate) = immediate else {
        return "no immediate at all".to_string();
    };
    match immediate {
        Immediate::RuntimeRef(id) => format!("the untyped runtime#{}", id.0),
        Immediate::BuiltinRef(id) => format!("the untyped builtin#{}", id.0),
        Immediate::FunctionRef(id) => format!("function#{}", id.as_raw()),
        Immediate::Data(id) => format!("data#{}", id.as_raw()),
        Immediate::ProfiledData { data, .. } => format!("profiled data#{}", data.as_raw()),
        other => format!("{other:?}"),
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
    if super::builtins::is_direct_builtin(target)
        || super::builtins::file_builtin_helper(target).is_some()
    {
        return super::builtins::direct_builtin_shape_issue(module, function, call, target);
    }
    match target {
        RuntimeFnId::GetClass => get_class_shape_issue(function, call),
        RuntimeFnId::GetObjectVars => get_object_vars_shape_issue(module, function, call),
        RuntimeFnId::Opendir => opendir_shape_issue(function, call),
        RuntimeFnId::Readdir => dir_handle_call_shape_issue(function, call, "readdir", true),
        RuntimeFnId::Rewinddir => dir_handle_call_shape_issue(function, call, "rewinddir", false),
        RuntimeFnId::Closedir => dir_handle_call_shape_issue(function, call, "closedir", false),
        RuntimeFnId::ArrayMap => array_map_shape_issue(module, function, call),
        RuntimeFnId::Usort => usort_shape_issue(module, function, call),
        RuntimeFnId::ArrayReduce => array_reduce_shape_issue(module, function, call),
        RuntimeFnId::Settype => settype_shape_issue(module, function, call),
        _ => Some("the runtime function has no audited WASM shape contract".to_string()),
    }
}

/// Returns the literal string the call's `index`-th operand carries, when it is one.
///
/// Same resolution as `define_constant_name`, generalized to any operand position: the
/// defining instruction must be a `ConstStr` whose data id names a real interned literal.
fn literal_string_operand<'a>(
    module: &'a Module,
    function: &Function,
    call: &Instruction,
    index: usize,
) -> Option<&'a str> {
    let name_value = *call.operands.get(index)?;
    let defining = function
        .instructions
        .iter()
        .find(|inst| inst.result == Some(name_value))?;
    if defining.op != Op::ConstStr {
        return None;
    }
    let Some(Immediate::Data(data_id)) = defining.immediate else {
        return None;
    };
    module
        .data
        .strings
        .get(data_id.as_raw() as usize)
        .map(String::as_str)
}

/// Returns the slot a value reads through a PLAIN `LoadLocal` — never a ref-cell load.
///
/// `settype` writes the variable's slot back directly, and a ref-cell-backed local must be
/// written THROUGH its cell or the by-ref aliases keep the old value; excluding `LoadRefCell`
/// here keeps that shape refused instead of silently splitting the alias.
fn plain_local_slot_origin(function: &Function, mut value: ValueId) -> Option<u32> {
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

/// Validates `empty($x)` against exactly the operand kinds its lowering answers.
///
/// The admitted arms mirror `lower_empty_construct`: null, int, bool (sentinel-aware),
/// float and Mixed (both through the warning-carrying truthiness helpers), string
/// ("" and "0" are the falsy pair), and the tagged int|null scalar. Containers and
/// objects stay refused until their arms are implemented with php's exact answers.
fn empty_construct_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [value] = call.operands.as_slice() else {
        return Some(format!(
            "empty expects one operand, got {}",
            call.operands.len()
        ));
    };
    let Some(source) = function.value(*value) else {
        return Some("empty operand is missing from the value table".to_string());
    };
    let php = source.php_type.codegen_repr();
    let supported = matches!(
        (source.ir_type, &php),
        (_, PhpType::Void)
            | (IrType::I64, PhpType::Int | PhpType::Bool | PhpType::False)
            | (IrType::F64, PhpType::Float)
            | (IrType::Str, PhpType::Str)
            | (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed | PhpType::Union(_))
            | (IrType::TaggedScalar, _)
    );
    if !supported {
        return Some(format!(
            "empty for a {:?}/{:?} operand is not yet implemented on wasm32-wasi",
            source.ir_type, php
        ));
    }
    None
}

/// Validates the exact `settype($var, "integer")` subset the lowering implements.
///
/// One narrowing is admitted — a FLOAT variable to "integer"/"int" — because it is the one
/// whose conversion this backend already performs with php's exact semantics and diagnostic
/// (`__rt_float_to_int_warn`, the same helper the `(int)` cast uses). The variable must read
/// through a plain `LoadLocal`: the write-back goes to the slot, and a ref-cell alias or a
/// property/element source has no slot to write. Every other target type or source stays
/// refused until its conversion is implemented with exact per-type PHP values.
fn settype_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
) -> Option<String> {
    let [value, _type_name] = call.operands.as_slice() else {
        return Some(format!(
            "settype expects the variable and the type name, got {} operands",
            call.operands.len()
        ));
    };
    let Some(type_name) = literal_string_operand(module, function, call, 1) else {
        return Some(
            "settype needs a LITERAL type name; a computed one cannot pick the conversion at compile time"
                .to_string(),
        );
    };
    if !matches!(type_name, "integer" | "int") {
        return Some(format!(
            "settype to {type_name:?}: only the float-to-\"integer\" narrowing is implemented"
        ));
    }
    let Some(source) = function.value(*value) else {
        return Some("settype variable is missing from the value table".to_string());
    };
    if source.ir_type != IrType::F64 || source.php_type.codegen_repr() != PhpType::Float {
        return Some(format!(
            "settype to integer reads a {:?}/{:?} variable; only a FLOAT source is implemented",
            source.ir_type,
            source.php_type.codegen_repr()
        ));
    }
    if plain_local_slot_origin(function, *value).is_none() {
        return Some(
            "settype variable must read through a plain LoadLocal; ref-cell and non-local sources have no slot to write back"
                .to_string(),
        );
    }
    if call.result.is_some() && call.result_type != IrType::I64 {
        return Some(format!(
            "settype result storage {:?} is not the expected I64 bool",
            call.result_type
        ));
    }
    None
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

/// Validates a `get_object_vars()` call against the projection table this target lays out.
///
/// The receiver must be STATICALLY one class: the row is chosen by the runtime class id, but the
/// audit has to prove every property it will meet is projectable, and a `mixed` receiver names no
/// class to prove that of.
///
/// The call must also sit OUTSIDE a class. php projects what the calling scope can see — public
/// only from outside, plus protected and the scope's own private from inside — and the table
/// carries the public properties alone. Refusing the inside-scope call keeps that from reading as
/// an object with fewer properties than it has, which is a wrong ANSWER rather than a refusal.
fn get_object_vars_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "get_object_vars takes one object operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("get_object_vars operand is missing from the value table".to_string());
    };
    let PhpType::Object(class_name) = value.php_type.codegen_repr() else {
        return Some(format!(
            "get_object_vars needs a statically object-typed operand, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    };
    if value.ir_type != IrType::Heap(IrHeapKind::Object) {
        return Some(format!(
            "get_object_vars needs a Heap(Object) operand, got {:?}",
            value.ir_type
        ));
    }
    if let Some(scope) = function.lexical_class.as_deref() {
        return Some(format!(
            "get_object_vars called inside class {scope} would have to project the protected and \
             private properties that scope can see, and this target lays out only the public ones"
        ));
    }
    if !super::objvars::class_is_projectable(module, &class_name) {
        return Some(format!(
            "a public property of {class_name} has no mixed form this projection can carry"
        ));
    }
    let result_php = call.result_php_type.codegen_repr();
    let projects_assoc = matches!(
        &result_php,
        PhpType::AssocArray { key, value }
            if matches!(key.codegen_repr(), PhpType::Str)
                && matches!(value.codegen_repr(), PhpType::Mixed)
    );
    if call.result.is_none() || call.result_type != IrType::Heap(IrHeapKind::Hash) || !projects_assoc
    {
        return Some(format!(
            "get_object_vars answers array<string, mixed>, not {:?}/{result_php:?}",
            call.result_type
        ));
    }
    None
}

/// Validates `opendir($path)`: one string path in, a boxed `resource|false` out.
fn opendir_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "opendir takes one path operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("opendir path is missing from the value table".to_string());
    };
    if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
        return Some(format!(
            "opendir takes a string path, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none() || call.result_type != IrType::Heap(IrHeapKind::Mixed) {
        return Some(format!(
            "opendir answers a boxed resource|false, not {:?}",
            call.result_type
        ));
    }
    None
}

/// Validates the three calls that take a directory handle.
///
/// `answers_value` separates `readdir`, which boxes the next name or `false`, from `rewinddir`
/// and `closedir`, which php answers null for — the EIR carries that as a plain `I64` result the
/// helper leaves at zero.
fn dir_handle_call_shape_issue(
    function: &Function,
    call: &Instruction,
    name: &str,
    answers_value: bool,
) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "{name} takes one directory handle, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some(format!("{name} handle is missing from the value table"));
    };
    // The handle is whatever `opendir` answered, which is a boxed `resource|false` cell: php
    // itself accepts the `false` and answers its own failure value rather than refusing.
    if value.ir_type != IrType::Heap(IrHeapKind::Mixed) {
        return Some(format!(
            "{name} takes the boxed handle opendir answers, got {:?}",
            value.ir_type
        ));
    }
    let expected = if answers_value {
        IrType::Heap(IrHeapKind::Mixed)
    } else {
        IrType::I64
    };
    if call.result_type != expected {
        return Some(format!(
            "{name} answers {expected:?}, not {:?}",
            call.result_type
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
    // `Void` is what an empty literal's `Never` normalizes to: there is no element to convert
    // and the map answers an empty array, so no representation is needed.
    if !matches!(element_type, PhpType::Int | PhpType::Str | PhpType::Void) {
        return Some(format!(
            "source element type {element_type:?} is not represented exactly by the map runtime"
        ));
    }
    // The EIR types this result `mixed` DELIBERATELY — a string callback picks its element ABI
    // at runtime — so the boxed form is the ordinary one and the array form is the narrowed
    // one. Both are materialized here; the lowering boxes when the slot is Mixed.
    let boxed_result = call.result_type == IrType::Heap(IrHeapKind::Mixed)
        && call.result_php_type.codegen_repr() == PhpType::Mixed;
    // A narrowed result names its element type from the callback's declared return. The map
    // builds cells, which the lowering rebuilds as int, string or float slots; any other element
    // layout would be read as the wrong width.
    if call.result.is_none()
        || (!boxed_result
            && (call.result_type != IrType::Heap(IrHeapKind::Array)
                || !matches!(
                    call.result_php_type.codegen_repr(),
                    PhpType::Array(element)
                        if matches!(
                            element.codegen_repr(),
                            PhpType::Mixed
                                | PhpType::Int
                                | PhpType::Str
                                | PhpType::Float
                                | PhpType::Void
                                | PhpType::Never
                        )
                )))
    {
        return Some(format!(
            "array_map must materialize an indexed-array or boxed result, got {:?}/{:?}",
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
        // A `mixed` parameter takes any element with NO coercion: the map's argument buffer
        // holds Mixed cells, so boxing the element is the whole conversion, and PHP performs
        // none of its own there.
        let compatible = param_type == PhpType::Mixed
            || match element_type {
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
    // A PHP ARRAY callable (`[$object, "method"]` / `["Class", "method"]`) is not a descriptor:
    // it dispatches through the class-id/name ladder in `callable_arrays`, which validates its
    // own operand shapes. What must be proven HERE is that the ladder can be built at all —
    // that this module has candidate targets and that every one of them can be given a wrapper.
    if let Some(form) = super::callable_arrays::invoke_form(owner, inst) {
        if let Some(issue) = super::callable_arrays::unsupported_target_issue(module, form) {
            return Some(issue);
        }
        return array_callable_argument_container_issue(owner, *arguments)
            .or_else(|| closure_result_shape_issue(owner, inst));
    }
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

/// Requires an array callable's positional buffer to be the indexed `array<mixed>` the
/// per-method wrappers read, exactly as the descriptor path requires of its own buffer.
fn array_callable_argument_container_issue(
    owner: &Function,
    arguments: ValueId,
) -> Option<String> {
    let Some(value) = owner.value(arguments) else {
        return Some("argument container is missing from the value table".to_string());
    };
    let is_mixed_array = value.ir_type == IrType::Heap(IrHeapKind::Array)
        && matches!(
            value.php_type.codegen_repr(),
            PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed
        );
    if is_mixed_array {
        return None;
    }
    Some(format!(
        "array-callable argument container must be array<mixed>/Heap(Array), got {:?}/{:?}",
        value.ir_type,
        value.php_type.codegen_repr()
    ))
}

/// Requires each wrapper-consumed argument to match its parameter exactly.
///
/// A VARIADIC tail ends the pairing: every argument from that position on is boxed into a Mixed
/// cell and stays boxed inside the array the wrapper collects, so there is no per-parameter
/// representation for one of them to match. The boxability of each is checked by the caller.
fn callable_argument_contract_issue(
    owner: &Function,
    target: &Function,
    visible_count: usize,
    arguments: &[ValueId],
) -> Option<String> {
    let paired = target.params[..visible_count]
        .iter()
        .position(|parameter| parameter.variadic)
        .unwrap_or(visible_count);
    for (index, (argument, parameter)) in arguments
        .iter()
        .take(paired)
        .zip(&target.params[..paired])
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
pub(super) fn closure_argument_is_boxable(ir_type: IrType, php_type: &PhpType) -> bool {
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
    // A variadic tail may receive nothing, so it is not part of the required arity.
    let required = function.params[..visible_count]
        .iter()
        .position(|parameter| parameter.variadic)
        .unwrap_or(visible_count);
    if required > supplied_args {
        return Some(format!(
            "callback declares {required} required parameters but the runtime supplies {supplied_args}"
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
    for (index, param) in function.params[..visible_count].iter().enumerate() {
        // A VARIADIC tail is what the wrapper collects out of the argument buffer, so it is
        // admitted in the one shape PHP produces: last, and already packed as an array.
        if param.variadic
            && index + 1 == visible_count
            && matches!(param.ir_type, IrType::Heap(IrHeapKind::Array))
        {
            continue;
        }
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
        // `class_exists` and its three siblings are answered from the module's own declarations:
        // the checker requires a literal name in AOT mode, and this module IS the whole program.
        RuntimeFnId::ClassExists
        | RuntimeFnId::InterfaceExists
        | RuntimeFnId::TraitExists
        | RuntimeFnId::EnumExists
        | RuntimeFnId::FunctionExists
        // Not a declaration question, but settled at compile time the same way: the extensions
        // this target serves are fixed when the module is built. See `WASM_LOADED_EXTENSIONS`.
        | RuntimeFnId::ExtensionLoaded
        | RuntimeFnId::GetLoadedExtensions
        // The program's own declarations, the SPL catalogue, an object's identity, and
        // `instanceof` asked as a call. The first two are settled when the module is built; the
        // last two go through the handle pool and `__rt_instanceof`.
        // The IANA protocol-number registry, carried rather than read: a WASI module cannot
        // open `/etc/protocols`, and what that file holds is a specification, not a machine.
        | RuntimeFnId::Getprotobyname
        | RuntimeFnId::Getprotobynumber
        // The three tuners. None of them changes how this target reads or writes — there is no
        // buffering layer to size — but what php promises is what the CALL answers, and that is
        // reproduced exactly, `stream_set_write_buffer()`'s -1 included.
        // `sprintf` written to a stream, sharing its format parser and its `__rt_fmt_*`
        // helpers — the only differences are where the bytes go and that the format is the
        // SECOND operand.
        | RuntimeFnId::Fprintf
        // One line off the stream, split by the conversions the LITERAL format names.
        | RuntimeFnId::Fscanf
        // Decimal arithmetic, written in digits here because there is no bridge to link.
        // Only these four: `bcsub`, `bcdiv`, `bcpow`, `bcsqrt`, `bccomp` and the rest have
        // no implementation on this target and stay refused rather than approximated.
        // RFC 1951, written here because there is no zlib to link. Inflate is complete;
        // deflate emits one fixed-Huffman block, which is valid DEFLATE but not
        // byte-identical to zlib's.
        // A subset of PCRE, compiled from the LITERAL pattern when the module is built.
        // The gate runs the pattern compiler, so a construct the VM cannot execute is a
        // compile error naming it rather than a wrong answer.
        // The wall clock, which WASI does provide — `clock_time_get` with id 0. Sleeping
        // does not follow: `sleep` and `usleep` need `poll_oneoff` and stay refused.
        | RuntimeFnId::Time
        | RuntimeFnId::Microtime
        | RuntimeFnId::PregMatch
        | RuntimeFnId::Gzdeflate
        | RuntimeFnId::Gzinflate
        | RuntimeFnId::Gzcompress
        | RuntimeFnId::Gzuncompress
        | RuntimeFnId::BcAdd
        | RuntimeFnId::BcMul
        | RuntimeFnId::BcDivmod
        | RuntimeFnId::BcScale
        | RuntimeFnId::StreamSetChunkSize
        | RuntimeFnId::StreamSetReadBuffer
        | RuntimeFnId::StreamSetWriteBuffer
        | RuntimeFnId::GetDeclaredClasses
        | RuntimeFnId::SplClasses
        | RuntimeFnId::SplObjectId
        | RuntimeFnId::IsA
        // The three class relations fold the same way, into an assoc hash rather than a bool:
        // which interfaces, ancestors or traits a declared name has is settled by this module.
        | RuntimeFnId::ClassImplements
        | RuntimeFnId::ClassParents
        | RuntimeFnId::ClassUses
        | RuntimeFnId::Readline
        | RuntimeFnId::Fopen
        | RuntimeFnId::Fwrite
        | RuntimeFnId::Fread
        // The two that REPLACE the caller's array, writing the relocated pointer back.
        | RuntimeFnId::ArraySplice
        | RuntimeFnId::ArrayUnshift
        // Array rebuilds: generic where they only MOVE elements, raw-i64-only where they write.
        | RuntimeFnId::ArrayPad
        | RuntimeFnId::ArrayUnique
        | RuntimeFnId::ArrayColumn
        // Values the audit proved encodable; a float or a bare `mixed` keeps refusing.
        | RuntimeFnId::JsonEncode
        // The output-buffer stack: every stdout write funnels through one interception point.
        | RuntimeFnId::ObStart
        | RuntimeFnId::ObGetClean
        | RuntimeFnId::ObEndFlush
        | RuntimeFnId::ObEndClean
        | RuntimeFnId::ObGetStatus
        // Pure computation over bytes already in memory: no WASI call is involved.
        | RuntimeFnId::Long2ip
        | RuntimeFnId::Ip2long
        | RuntimeFnId::InetNtop
        | RuntimeFnId::InetPton
        // Scalars only; the shape check refuses every other tag.
        | RuntimeFnId::VarDump
        | RuntimeFnId::Fgetc
        | RuntimeFnId::Readfile
        | RuntimeFnId::Flock
        | RuntimeFnId::Tmpfile
        | RuntimeFnId::Fclose
        | RuntimeFnId::Feof
        | RuntimeFnId::Ftell
        | RuntimeFnId::Rewind
        | RuntimeFnId::Fseek
        | RuntimeFnId::StreamGetContents
        | RuntimeFnId::StreamGetLine
        | RuntimeFnId::StreamGetMetaData
        | RuntimeFnId::StreamCopyToStream
        | RuntimeFnId::Settype
        | RuntimeFnId::Getenv
        | RuntimeFnId::PrintR
        | RuntimeFnId::GetResourceType
        | RuntimeFnId::Define
        | RuntimeFnId::FileExists
        | RuntimeFnId::Unlink
        | RuntimeFnId::FileGetContents
        | RuntimeFnId::FilePutContents
        | RuntimeFnId::GetClass
        // Answered from the same class-indexed layout table `json_encode` reads, about the same
        // properties: `get_object_vars_shape_issue` proves the receiver names one class, that the
        // call sits outside any class (so php projects the public properties alone), and that
        // every one of them has a `mixed` form.
        | RuntimeFnId::GetObjectVars
        // Directory iteration over WASI's `fd_readdir`. The handle is the same boxed resource a
        // stream handle is, so the shape checks below are the stream family's.
        | RuntimeFnId::Opendir
        | RuntimeFnId::Readdir
        | RuntimeFnId::Rewinddir
        | RuntimeFnId::Closedir
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
        | RuntimeFnId::ArrayKeyFirst
        | RuntimeFnId::ArrayKeyLast
        | RuntimeFnId::ArrayReplace
        | RuntimeFnId::ArrayDiffAssoc
        | RuntimeFnId::ArrayIntersectAssoc
        | RuntimeFnId::Ksort
        | RuntimeFnId::Krsort
        | RuntimeFnId::Asort
        | RuntimeFnId::Arsort
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
        | RuntimeFnId::Htmlspecialchars
        | RuntimeFnId::Base64Decode
        | RuntimeFnId::ParseUrl
        | RuntimeFnId::Gettype => true,
        // Added upstream after this backend's last audit; refused until each is
        // lowered and differentially tested here (fail-closed).
        RuntimeFnId::ArrayCountValues |
        RuntimeFnId::ArrayPtrSeek |
        RuntimeFnId::ArrayPtrKey |
        RuntimeFnId::ArrayPtrValue |
        RuntimeFnId::BaseConvert |
        RuntimeFnId::Bindec |
        RuntimeFnId::Decbin |
        RuntimeFnId::Dechex |
        RuntimeFnId::Decoct |
        RuntimeFnId::Hexdec |
        RuntimeFnId::Octdec |
        RuntimeFnId::ElephcObjectIsEnum |
        RuntimeFnId::ElephcObjectPropCount |
        RuntimeFnId::ElephcObjectPropName |
        RuntimeFnId::ElephcObjectPropValue |
        RuntimeFnId::ChunkSplit |
        RuntimeFnId::CountChars |
        RuntimeFnId::OpensslCipherIvLength |
        RuntimeFnId::OpensslDecrypt |
        RuntimeFnId::OpensslEncrypt |
        RuntimeFnId::OpensslGetCipherMethods |
        RuntimeFnId::StrWordCount |
        RuntimeFnId::Strncasecmp |
        RuntimeFnId::Strncmp |
        RuntimeFnId::Stripos |
        RuntimeFnId::Strripos |
        RuntimeFnId::Strtr |
        RuntimeFnId::SubstrCount |
        RuntimeFnId::IntvalBase |
        // Added upstream after this backend's last audit — the bcmath, curl and iconv
        // bridge surfaces — and refused until each is lowered and differentially tested
        // here. The match stays exhaustive so the next upstream identity is a compile
        // error rather than a silent gap.
        RuntimeFnId::BcCeil |
        RuntimeFnId::BcComp |
        RuntimeFnId::BcDiv |
        RuntimeFnId::BcFloor |
        RuntimeFnId::BcMod |
        RuntimeFnId::BcPow |
        RuntimeFnId::BcPowmod |
        RuntimeFnId::BcRound |
        RuntimeFnId::BcSqrt |
        RuntimeFnId::BcSub |
        RuntimeFnId::CurlEasyInit |
        RuntimeFnId::CurlEasySetoptLong |
        RuntimeFnId::CurlEasySetoptStr |
        RuntimeFnId::CurlEasySetoptSlist |
        RuntimeFnId::CurlEasySetCallback |
        RuntimeFnId::CurlAdapterAddr |
        RuntimeFnId::CurlEasyPerform |
        RuntimeFnId::CurlEasyBody |
        RuntimeFnId::CurlEasyErrno |
        RuntimeFnId::CurlEasyError |
        RuntimeFnId::CurlVersion |
        RuntimeFnId::CurlSetoptUnsupportedWarning |
        RuntimeFnId::CurlEasyGetinfoLong |
        RuntimeFnId::CurlEasyGetinfoDouble |
        RuntimeFnId::CurlEasyStrOp |
        RuntimeFnId::CurlEasyReset |
        RuntimeFnId::CurlEasyPause |
        RuntimeFnId::CurlEasyUpkeep |
        RuntimeFnId::CurlEasyCopy |
        RuntimeFnId::CurlStrerror |
        RuntimeFnId::CurlOptionKind |
        RuntimeFnId::CurlEasyId |
        RuntimeFnId::CurlMultiInit |
        RuntimeFnId::CurlMultiAdd |
        RuntimeFnId::CurlMultiRemove |
        RuntimeFnId::CurlMultiExec |
        RuntimeFnId::CurlMultiSelect |
        RuntimeFnId::CurlMultiInfoRead |
        RuntimeFnId::CurlMultiSetopt |
        RuntimeFnId::CurlMultiErrno |
        RuntimeFnId::CurlMultiStrerror |
        RuntimeFnId::CurlMultiSetoptUnsupportedWarning |
        RuntimeFnId::CurlShareInit |
        RuntimeFnId::CurlShareSetopt |
        RuntimeFnId::CurlShareErrno |
        RuntimeFnId::CurlShareStrerror |
        RuntimeFnId::CurlEasySetShare |
        RuntimeFnId::CurlShareInitPersistent |
        RuntimeFnId::CurlMimeNew |
        RuntimeFnId::CurlMimeAddPart |
        RuntimeFnId::CurlMimePartField |
        RuntimeFnId::CurlMimePost |
        RuntimeFnId::CurlMimeAbort |
        RuntimeFnId::Iconv |
        RuntimeFnId::IconvGetEncoding |
        RuntimeFnId::IconvMimeDecode |
        RuntimeFnId::IconvMimeDecodeHeaders |
        RuntimeFnId::IconvMimeEncode |
        RuntimeFnId::IconvSetEncoding |
        RuntimeFnId::IconvStrlen |
        RuntimeFnId::IconvStrpos |
        RuntimeFnId::IconvStrrpos |
        RuntimeFnId::IconvSubstr |
        RuntimeFnId::ArrayFilter
        | RuntimeFnId::Uasort
        | RuntimeFnId::Uksort
        | RuntimeFnId::ArrayWalk
        | RuntimeFnId::ArrayAll
        | RuntimeFnId::ArrayAny
        | RuntimeFnId::ArrayChunk
        | RuntimeFnId::ArrayCombine
        | RuntimeFnId::ArrayDiff
        | RuntimeFnId::ArrayDiffKey
        | RuntimeFnId::ArrayFillKeys
        | RuntimeFnId::ArrayFind
        | RuntimeFnId::ArrayFlip
        | RuntimeFnId::ArrayIntersect
        | RuntimeFnId::ArrayIntersectKey
        | RuntimeFnId::ArrayMergeRecursive
        | RuntimeFnId::ArrayMultisort
        | RuntimeFnId::ArrayPop
        | RuntimeFnId::ArrayPush
        | RuntimeFnId::ArrayRand
        | RuntimeFnId::ArrayReplaceRecursive
        | RuntimeFnId::ArrayShift
        | RuntimeFnId::ArrayUdiff
        | RuntimeFnId::ArrayUintersect
        | RuntimeFnId::ArrayWalkRecursive
        | RuntimeFnId::Natcasesort
        | RuntimeFnId::Natsort
        | RuntimeFnId::Shuffle
        | RuntimeFnId::CallUserFunc
        | RuntimeFnId::CallUserFuncArray
        | RuntimeFnId::ClassAlias
        | RuntimeFnId::GetDeclaredInterfaces
        | RuntimeFnId::GetDeclaredTraits
        | RuntimeFnId::GetParentClass
        | RuntimeFnId::IsSubclassOf
        | RuntimeFnId::MethodExists
        | RuntimeFnId::PregReplaceCallback
        | RuntimeFnId::PropertyExists
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
        | RuntimeFnId::Copy
        | RuntimeFnId::Dirname
        | RuntimeFnId::DiskFreeSpace
        | RuntimeFnId::DiskTotalSpace
        | RuntimeFnId::Fdatasync
        | RuntimeFnId::Fflush
        | RuntimeFnId::Fgetcsv
        | RuntimeFnId::Fgets
        | RuntimeFnId::File
        | RuntimeFnId::Fileatime
        | RuntimeFnId::Filectime
        | RuntimeFnId::Filegroup
        | RuntimeFnId::Fileinode
        | RuntimeFnId::Filemtime
        | RuntimeFnId::Fileowner
        | RuntimeFnId::Fileperms
        | RuntimeFnId::Filesize
        | RuntimeFnId::Filetype
        | RuntimeFnId::Fnmatch
        | RuntimeFnId::Fpassthru
        | RuntimeFnId::Fputcsv
        | RuntimeFnId::Fsockopen
        | RuntimeFnId::Fstat
        | RuntimeFnId::Fsync
        | RuntimeFnId::Ftruncate
        | RuntimeFnId::Getcwd
        | RuntimeFnId::Gethostbyaddr
        | RuntimeFnId::Gethostbyname
        | RuntimeFnId::Gethostname
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
        | RuntimeFnId::ObFlush
        | RuntimeFnId::ObGetContents
        | RuntimeFnId::ObGetFlush
        | RuntimeFnId::ObGetLength
        | RuntimeFnId::ObGetLevel
        | RuntimeFnId::ObImplicitFlush
        | RuntimeFnId::ObListHandlers
        | RuntimeFnId::Pathinfo
        | RuntimeFnId::Pclose
        | RuntimeFnId::Pfsockopen
        | RuntimeFnId::Popen
        | RuntimeFnId::Readlink
        | RuntimeFnId::Realpath
        | RuntimeFnId::RealpathCacheGet
        | RuntimeFnId::RealpathCacheSize
        | RuntimeFnId::Rename
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
        | RuntimeFnId::StreamFilterAppend
        | RuntimeFnId::StreamFilterPrepend
        | RuntimeFnId::StreamFilterRegister
        | RuntimeFnId::StreamFilterRemove
        | RuntimeFnId::StreamGetFilters
        | RuntimeFnId::StreamGetTransports
        | RuntimeFnId::StreamGetWrappers
        | RuntimeFnId::StreamIsLocal
        | RuntimeFnId::StreamIsatty
        | RuntimeFnId::StreamResolveIncludePath
        | RuntimeFnId::StreamSelect
        | RuntimeFnId::StreamSetBlocking
        | RuntimeFnId::StreamSetTimeout
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
        | RuntimeFnId::Touch
        | RuntimeFnId::Umask
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
        | RuntimeFnId::SplObjectHash
        | RuntimeFnId::Chop
        | RuntimeFnId::CtypeAlnum
        | RuntimeFnId::CtypeAlpha
        | RuntimeFnId::CtypeDigit
        | RuntimeFnId::CtypeSpace
        | RuntimeFnId::GraphemeStrrev
        | RuntimeFnId::Hash
        | RuntimeFnId::HashAlgos
        | RuntimeFnId::HashCopy
        | RuntimeFnId::HashEquals
        | RuntimeFnId::HashFinal
        | RuntimeFnId::HashHmac
        | RuntimeFnId::HashInit
        | RuntimeFnId::HashUpdate
        | RuntimeFnId::Htmlentities
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
        | RuntimeFnId::Defined
        | RuntimeFnId::Exec
        | RuntimeFnId::Getdate
        | RuntimeFnId::Gmdate
        | RuntimeFnId::Gmmktime
        | RuntimeFnId::Header
        | RuntimeFnId::Hrtime
        | RuntimeFnId::HttpResponseCode
        | RuntimeFnId::JsonDecode
        | RuntimeFnId::JsonLastError
        | RuntimeFnId::JsonLastErrorMsg
        | RuntimeFnId::JsonValidate
        | RuntimeFnId::Localtime
        | RuntimeFnId::Mktime
        | RuntimeFnId::Passthru
        | RuntimeFnId::PhpUname
        | RuntimeFnId::Phpversion
        | RuntimeFnId::PregMatchAll
        | RuntimeFnId::PregReplace
        | RuntimeFnId::PregSplit
        | RuntimeFnId::Putenv
        | RuntimeFnId::Serialize
        | RuntimeFnId::ShellExec
        | RuntimeFnId::Sleep
        | RuntimeFnId::Strtotime
        | RuntimeFnId::System
        | RuntimeFnId::Unserialize
        | RuntimeFnId::Usleep
        | RuntimeFnId::GetResourceId
        | RuntimeFnId::IsCallable
        | RuntimeFnId::IsFinite
        | RuntimeFnId::IsInfinite
        | RuntimeFnId::IsNan
        | RuntimeFnId::IsNumeric
        // Added on main after this backend's branch point (mbstring, opcache, `clone with`, …);
        // refused until each has a lowering and evidence of its own.
        | RuntimeFnId::CloneWith
        | RuntimeFnId::ElephcCloneOverrideReferenceGuard
        | RuntimeFnId::ElephcOpcacheRtBlacklistEntry
        | RuntimeFnId::ElephcOpcacheRtScriptField
        | RuntimeFnId::ElephcOpcacheRtScriptPath
        | RuntimeFnId::ElephcOpcacheRtReset
        | RuntimeFnId::ElephcOpcacheRtIsCached
        | RuntimeFnId::ElephcOpcacheRtDiscard
        | RuntimeFnId::ElephcOpcacheRtSoftInvalidate
        | RuntimeFnId::ElephcOpcacheRtCompile
        | RuntimeFnId::ElephcOpcacheRtInFileCache
        | RuntimeFnId::ElephcOpcacheRtStat
        | RuntimeFnId::ElephcOpcacheRtSwap
        | RuntimeFnId::MbEreg
        | RuntimeFnId::MbParseStr
        | RuntimeFnId::MbConvertVariables
        | RuntimeFnId::MbSendMail
        | RuntimeFnId::MbEregi
        | RuntimeFnId::MbEregSearchInit
        | RuntimeFnId::MbEregSearch
        | RuntimeFnId::MbEregSearchPos
        | RuntimeFnId::MbEregSearchRegs
        | RuntimeFnId::MbEregSearchGetpos
        | RuntimeFnId::MbEregSearchGetregs
        | RuntimeFnId::MbEregSearchSetpos
        | RuntimeFnId::MbSplit
        | RuntimeFnId::MbEregReplace
        | RuntimeFnId::MbEregReplaceCallback
        | RuntimeFnId::MbEregiReplace
        | RuntimeFnId::MbStrwidth
        | RuntimeFnId::MbStrtoupper
        | RuntimeFnId::MbStrtolower
        | RuntimeFnId::MbConvertCase
        | RuntimeFnId::MbUcfirst
        | RuntimeFnId::MbLcfirst
        | RuntimeFnId::MbStrimwidth
        | RuntimeFnId::MbSubstr
        | RuntimeFnId::MbStrcut
        | RuntimeFnId::MbScrub
        | RuntimeFnId::MbTrim
        | RuntimeFnId::MbLtrim
        | RuntimeFnId::MbRtrim
        | RuntimeFnId::MbStrPad
        | RuntimeFnId::MbConvertKana
        | RuntimeFnId::MbSubstrCount
        | RuntimeFnId::MbOrd
        | RuntimeFnId::MbChr
        | RuntimeFnId::MbStrpos
        | RuntimeFnId::MbStripos
        | RuntimeFnId::MbStrrpos
        | RuntimeFnId::MbStrripos
        | RuntimeFnId::MbStrstr
        | RuntimeFnId::MbStristr
        | RuntimeFnId::MbStrrchr
        | RuntimeFnId::MbStrrichr
        | RuntimeFnId::MbLanguage
        | RuntimeFnId::MbInternalEncoding
        | RuntimeFnId::MbHttpOutput
        | RuntimeFnId::MbEncodingAliases
        | RuntimeFnId::MbStrSplit
        | RuntimeFnId::MbPreferredMimeName
        | RuntimeFnId::MbCheckEncoding
        | RuntimeFnId::MbSubstituteCharacter
        | RuntimeFnId::MbListEncodings
        | RuntimeFnId::MbDetectOrder
        | RuntimeFnId::MbDetectEncoding
        | RuntimeFnId::MbConvertEncoding
        | RuntimeFnId::MbEncodeNumericentity
        | RuntimeFnId::MbDecodeNumericentity
        | RuntimeFnId::MbDecodeMimeheader
        | RuntimeFnId::MbEncodeMimeheader
        | RuntimeFnId::MbOutputHandler
        | RuntimeFnId::MbGetInfo
        | RuntimeFnId::SharedIni
        | RuntimeFnId::MbHttpInput
        | RuntimeFnId::MbRegexEncoding
        | RuntimeFnId::MbRegexSetOptions
        | RuntimeFnId::Getmypid
        => false,
    }
}

/// Returns the stable name of every unary-string runtime variant.
pub(super) fn unary_string_name(target: UnaryStringRuntime) -> &'static str {
    match target {
        UnaryStringRuntime::AddSlashes => "string.add_slashes",
        UnaryStringRuntime::Base64Encode => "string.base64_encode",
        UnaryStringRuntime::BinToHex => "string.bin_to_hex",
        UnaryStringRuntime::HexToBin => "string.hex_to_bin",
        UnaryStringRuntime::HtmlEntityDecode => "string.html_entity_decode",
        UnaryStringRuntime::NlToBr => "string.nl_to_br",
        UnaryStringRuntime::QuoteMeta => "string.quote_meta",
        UnaryStringRuntime::QuotedPrintableEncode => "string.quoted_printable_encode",
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
        | Op::LoadPropRefCell
        | Op::LoadArrayElemRefCell
        | Op::BindRefCellPtr
        | Op::LoadGlobal
        | Op::StoreGlobal
        // The `@` operator: a suppression depth the `__rt_warn_*` helpers consult.
        | Op::ErrorSuppressBegin
        | Op::ErrorSuppressEnd
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
        | Op::StrIncDec
        | Op::ArrayGetMixedKey
        | Op::ArrayGetMixedKeySilent
        | Op::StrLen
        | Op::StrPersist
        | Op::ArrayToMixed
        | Op::LooseEq
        | Op::LooseNotEq
        | Op::PhpRelCmp
        | Op::ReturnBoundaryMixedToInt
        | Op::ReleaseUnlessAliases
        | Op::ZeroLocalSlot
        | Op::PropInitialized
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
        // php's `<=>`, which is the ORDERING the relational operators are predicates over and
        // runs on the same three-way helpers.
        | Op::Spaceship
        // `clone $obj`, whose per-slot retain rule is the release walk's read backwards.
        | Op::ObjectCloneShallow
        // `isset($a[$i])` over an indexed array, the twin of the associative form above.
        | Op::ArrayIsset
        | Op::GcCollect
        // A function's `static $x`: the same 16-byte cell a static property gets, keyed
        // by the declaring function, with eight more bytes for the initializer's flag.
        | Op::LoadStaticLocal
        | Op::StoreStaticLocal
        | Op::InitStaticLocal
        | Op::StaticLocalUninitialized
        | Op::StaticPropInitialized
        | Op::PropUnset
        | Op::DynamicPropGet
        | Op::DynamicPropSet
        | Op::GeneratorYield
        | Op::GeneratorYieldFrom
        | Op::LoadStaticProperty
        | Op::StoreStaticProperty
        | Op::ScopedConstantGet
        | Op::ArrayPush
        | Op::ArrayToHash
        | Op::HashAppend
        | Op::ArrayUnion
        | Op::HashSpread
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
        | Op::ReleaseLocalSlot
        | Op::FToStr
        | Op::StrCharAt
        | Op::TypePredicate
        | Op::Nop
        | Op::MixedClone
        | Op::MixedArrayAppend
        | Op::PropGetForWrite
        | Op::PushCallOperandOwner
        | Op::PopCallOperandOwner
        | Op::ConstClassName
        | Op::DynamicObjectNew
        | Op::IncludeOnceMark
        | Op::IncludeOnceGuard
        | Op::FunctionVariantMark
        | Op::FunctionVariantDispatch
        | Op::LoadCalledClassId => true,
        Op::ConstEnumCase
        | Op::DataAddr
        | Op::LoadReflectionStaticProperty
        | Op::StoreReflectionStaticProperty
        | Op::ReflectionStaticPropertyInitialized
        | Op::IPow
        | Op::FPow
        | Op::StrEq
        | Op::StrCmp
        | Op::StrLooseEq

        | Op::IsEmpty
        | Op::FToI

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

        | Op::StrInterpolate
        | Op::WriteStrStdout
        | Op::HashLen
        | Op::ArrayElemAddr
        | Op::ArrayEnsureUnique
        | Op::HashEnsureUnique
        | Op::ArrayCloneShallow
        | Op::HashCloneShallow
        | Op::ArraySetMixedKey
        | Op::ArrayKeyExists
        | Op::OffsetExists
        | Op::OffsetUnset
        | Op::ListUnpack
        | Op::IteratorMethodCall
        | Op::SplRuntimeCall
        | Op::EvalObjectNew
        | Op::DynamicObjectNewMixed
        | Op::DynamicObjectNewWithoutConstructorMixed
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
        | Op::FinallyEnter
        | Op::FinallyExit
        | Op::FiberRuntimeCall
        | Op::GeneratorNew
        | Op::GeneratorReturn
        // Added on main after this backend's branch point (call-operand owners, ref-cell
        // adoption, descriptor arguments, …); refused until each is lowered here.
        | Op::RetainLocalRefCell
        | Op::LoadStaticPropertyRefCell
        | Op::DescriptorArgSet
        | Op::DescriptorArgKeyExists
        | Op::ThrowNamedParameterOverwrite
        | Op::DynamicPropUnset
        | Op::LoadPropRefCellChecked
        | Op::LoadArrayElemRefCellExisting
        | Op::AdoptRefCellPtr
        | Op::AcquireRefCell
        | Op::GcControl
        | Op::CoreBuiltin
        // Added upstream after this backend's last audit; refused until lowered here.
        | Op::ICheckedAddToInt
        | Op::ICheckedSubToInt
        | Op::ICheckedMulToInt
        | Op::ICheckedPow
        | Op::ArrayGetForWrite
        | Op::HashGetForWrite
        | Op::SlotDetach
        | Op::CallablePtr
        | Op::NormalizeCallable
        | Op::PdoAdapterAddr
        | Op::DynamicClassHasConstructor
        | Op::DynamicPdoStatementClassStatus
        | Op::DynamicPdoCalledClassStatus
        | Op::DynamicPdoStatementConstructorCall
        | Op::DynamicPdoStatementInitialize
        | Op::MixedArrayGetForWrite
        | Op::ICheckedNumericChainToInt
        | Op::PackedFieldMixedToInt
        // Added upstream after this backend's last audit; refused until lowered here.
        | Op::EnsureOwned => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::runtime::DIAG_SUPPRESS_GUARD;
    use super::{
        array_access_read_is_supported, get_object_vars_shape_issue, loose_eq_shape_issue,
        runtime_function_shape_issue,
        php_rel_cmp_shape_issue,
        release_unless_aliases_shape_issue, return_boundary_int_shape_issue,
        dynamic_property_shape_issue, prop_initialized_shape_issue, property_unset_shape_issue,
        static_prop_initialized_shape_issue,
        validate_module as validate_and_plan,
        zero_local_slot_shape_issue, LoweredWasmPlan, WasmError,
        call_operand_owner_shape_issue, check_runtime_call, mixed_clone_shape_issue,
        prop_get_for_write_shape_issue, value_boxes_into_a_container_write,
        value_is_a_declared_array_cell, exception_guard_shape_issue, type_predicate_shape_issue,
    };
    use crate::codegen::platform::Target;
    use crate::codegen::Emit;
    use crate::ir::{
        Builder, CmpPredicate, DataId, Function, FunctionParam, Immediate, Instruction,
        IrHeapKind, IrType, LocalKind, Module, Op, Ownership, RuntimeCallTarget, RuntimeFnId,
        Terminator, ValueId,
    };
    use crate::parser::ast::Visibility;
    use crate::span::Span;
    use crate::types::{ClassInfo, FunctionSig, PhpType};
    use std::collections::{HashMap, HashSet};

    /// `Op::LooseEq` admits only the operand pairs whose rule was MEASURED against php-src.
    ///
    /// PHP 8's `==` table is much wider than the plain word comparisons: a bool against a number
    /// casts the number to bool, and arrays compare element-wise. Answering those by guessing
    /// would be a silently wrong answer rather than a refusal, so the gate keeps them out.
    ///
    /// A Mixed cell is no longer among them: `__rt_mixed_cmp_mixed` and `__rt_mixed_cmp_i64`
    /// carry php-src's `zend_compare`, validated against `scripts/php_compare_model.py`. A cell
    /// against a PHP BOOL still is, because that rule converts both sides to booleans.
    #[test]
    fn loose_equality_admits_only_measured_pairs() {
        let probe_op = |op: Op,
                        left: (IrType, PhpType),
                        right: (IrType, PhpType),
                        command: bool| {
            let mut module = Module::new(Target::wasm());
            if command {
                let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
                main.flags.is_main = true;
                module.add_function(main);
            }
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
            loose_eq_shape_issue(&module, &function, &inst)
        };
        // `!=` is the same gate negated, so both opcodes go through the same predicate.
        let probe = |left: (IrType, PhpType), right: (IrType, PhpType)| {
            let eq = probe_op(Op::LooseEq, left.clone(), right.clone(), true);
            let ne = probe_op(Op::LooseNotEq, left, right, true);
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
        // A string against a number still needs its measured table.
        assert!(probe(string.clone(), int.clone()).is_some());
        assert!(probe(float.clone(), string.clone()).is_some());

        // A BOXED operand is now answered by php-src's own `zend_compare`, against another box
        // or against a genuine int.
        let cell = (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        assert_eq!(probe(cell.clone(), cell.clone()), None);
        assert_eq!(probe(cell.clone(), int.clone()), None);
        assert_eq!(probe(int.clone(), cell.clone()), None);
        // ...but NOT against a PHP bool, which makes both sides booleans instead.
        assert!(probe(cell.clone(), boolean.clone()).is_some());
        assert!(probe(boolean.clone(), cell.clone()).is_some());
        // The comparison can warn, so a module with no command entry point still refuses.
        assert!(probe_op(Op::LooseEq, cell.clone(), cell.clone(), false).is_some());
    }

    /// `php_rel_cmp` admits the pairs whose ordering the runtime answers.
    ///
    /// The boxed pairs mirror `loose_eq_shape_issue`'s boxed arm, for the same reason: it is
    /// `zend_compare` in both cases. The inline `int|null` pair joins them because it is one
    /// boxing away from a cell and its tag word is already the cell's own. A PHP bool on the
    /// concrete side is excluded — it makes BOTH sides booleans, which is a different rule — and
    /// so is a predicate that is not one of the four relational forms, since `==`, `!=` and
    /// `<=>` have their own opcodes.
    #[test]
    fn php_rel_cmp_admits_only_the_boxed_pairs_it_orders() {
        let probe = |left: (IrType, PhpType),
                     right: (IrType, PhpType),
                     predicate: CmpPredicate,
                     command: bool| {
            let mut module = Module::new(Target::wasm());
            if command {
                let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
                main.flags.is_main = true;
                module.add_function(main);
            }
            let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
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
                    Op::PhpRelCmp,
                    operands,
                    Some(Immediate::CmpPredicate(predicate)),
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
            php_rel_cmp_shape_issue(&module, &function, &inst)
        };

        let int = (IrType::I64, PhpType::Int);
        let boolean = (IrType::I64, PhpType::Bool);
        let float = (IrType::F64, PhpType::Float);
        let cell = (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        let tagged = (IrType::TaggedScalar, PhpType::TaggedScalar);

        // All four relational predicates, every admitted shape, in both operand orders.
        for predicate in [
            CmpPredicate::Slt,
            CmpPredicate::Sle,
            CmpPredicate::Sgt,
            CmpPredicate::Sge,
        ] {
            assert_eq!(
                probe(cell.clone(), cell.clone(), predicate, true),
                None,
                "{predicate:?} over two boxed cells"
            );
            assert_eq!(probe(cell.clone(), int.clone(), predicate, true), None);
            assert_eq!(probe(int.clone(), cell.clone(), predicate, true), None);
            assert_eq!(
                probe(tagged.clone(), int.clone(), predicate, true),
                None,
                "{predicate:?} of an inline int|null against a long"
            );
            assert_eq!(probe(int.clone(), tagged.clone(), predicate, true), None);
            assert_eq!(probe(tagged.clone(), tagged.clone(), predicate, true), None);
            assert_eq!(probe(tagged.clone(), cell.clone(), predicate, true), None);
            assert_eq!(probe(cell.clone(), tagged.clone(), predicate, true), None);
        }

        // A PHP bool makes both sides booleans, which this ordering does not implement.
        assert!(probe(cell.clone(), boolean.clone(), CmpPredicate::Slt, true).is_some());
        assert!(probe(boolean.clone(), cell.clone(), CmpPredicate::Slt, true).is_some());
        // Two concrete operands never reach this opcode, and are refused if they do.
        assert!(probe(int.clone(), float.clone(), CmpPredicate::Slt, true).is_some());
        // `==` is `Op::LooseEq`'s, not this one's.
        assert!(probe(cell.clone(), cell.clone(), CmpPredicate::Eq, true).is_some());
        // php's own object-comparison failure goes through WASI, so a reactor module refuses.
        assert!(probe(cell.clone(), cell.clone(), CmpPredicate::Slt, false).is_some());
    }

    /// The declared-`int` return boundary admits the storage its runtime rule reads.
    ///
    /// A boxed cell is what `__rt_mixed_return_int` unboxes; an integer word is already the
    /// verified answer and is forwarded. Everything else — a raw float, a string, a container —
    /// is refused, and so is a module with no command entry point, because the `TypeError` this
    /// raises is written through WASI.
    #[test]
    fn return_boundary_int_admits_the_storage_it_verifies() {
        let probe = |source: (IrType, PhpType), result: IrType, immediate: bool, command: bool| {
            let mut module = Module::new(Target::wasm());
            if command {
                let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
                main.flags.is_main = true;
                module.add_function(main);
            }
            let mut function = Function::new("probe".to_string(), IrType::I64, PhpType::Int);
            let data = module
                .data
                .intern_string("probe(): Return value must be of type int, ");
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let slot = builder.add_local(
                    Some("a".to_string()),
                    source.0,
                    source.1.clone(),
                    LocalKind::PhpLocal,
                );
                let loaded = builder.emit_load_local(slot, source.0, source.1);
                builder.emit(
                    Op::ReturnBoundaryMixedToInt,
                    vec![loaded],
                    immediate.then_some(Immediate::Data(data)),
                    result,
                    PhpType::Int,
                    Ownership::NonHeap,
                );
                builder.terminate(Terminator::Return { value: None });
            }
            let inst = function
                .instructions
                .last()
                .expect("the probe emitted the boundary")
                .clone();
            return_boundary_int_shape_issue(&module, &function, &inst)
        };

        let cell = (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        let int = (IrType::I64, PhpType::Int);
        let boolean = (IrType::I64, PhpType::Bool);
        let float = (IrType::F64, PhpType::Float);
        let string = (IrType::Str, PhpType::Str);

        assert_eq!(probe(cell.clone(), IrType::I64, true, true), None);
        assert_eq!(probe(int.clone(), IrType::I64, true, true), None);
        assert_eq!(probe(boolean.clone(), IrType::I64, true, true), None);
        // A raw float only appears once a fold has retyped the operand, which this target's
        // pipeline does not do; it stays refused rather than boxed on an unreachable path.
        assert!(probe(float, IrType::I64, true, true).is_some());
        assert!(probe(string, IrType::I64, true, true).is_some());
        // The answer is an integer word, and the message prefix is not optional.
        assert!(probe(cell.clone(), IrType::F64, true, true).is_some());
        assert!(probe(cell.clone(), IrType::I64, false, true).is_some());
        // The TypeError goes through WASI, so a reactor module refuses.
        assert!(probe(cell, IrType::I64, true, false).is_some());
    }

    /// `Op::ReleaseUnlessAliases` admits the storage whose pointers it can compare.
    ///
    /// The instruction exists because a nullsafe call can hand its own argument back, and the
    /// caller must not then release it. The comparison is on pointers, so heap and string
    /// storage are the shapes that matter; a scalar carries nothing to release and is admitted
    /// as the no-op it becomes.
    #[test]
    fn release_unless_aliases_admits_the_pointers_it_compares() {
        let probe = |value: (IrType, PhpType), result: (IrType, PhpType)| {
            let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let mut operands = Vec::new();
                for (index, (ir, php)) in [value.clone(), result.clone()].into_iter().enumerate() {
                    let slot = builder.add_local(
                        Some(format!("a{index}")),
                        ir,
                        php.clone(),
                        LocalKind::PhpLocal,
                    );
                    operands.push(builder.emit_load_local(slot, ir, php));
                }
                builder.emit(
                    Op::ReleaseUnlessAliases,
                    operands,
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
                builder.terminate(Terminator::Return { value: None });
            }
            let inst = function
                .instructions
                .last()
                .expect("the probe emitted the release")
                .clone();
            release_unless_aliases_shape_issue(&function, &inst)
        };

        let object = (
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object("C".to_string()),
        );
        let cell = (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        let string = (IrType::Str, PhpType::Str);
        let int = (IrType::I64, PhpType::Int);

        assert_eq!(probe(object.clone(), object.clone()), None);
        assert_eq!(probe(cell.clone(), cell.clone()), None);
        assert_eq!(probe(string.clone(), string.clone()), None);
        // A scalar owns nothing; the instruction is a no-op rather than a refusal.
        assert_eq!(probe(int.clone(), int.clone()), None);
        // Mixed storage on the two sides cannot alias, and releases unconditionally.
        assert_eq!(probe(object, int), None);
    }

    /// Builds `probe()` loading one local of each given storage, then emitting `op` over them.
    fn probe_over_locals(
        op: Op,
        locals: &[(IrType, PhpType)],
        result: Option<(IrType, PhpType)>,
    ) -> (Function, Instruction, Vec<ValueId>) {
        let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
        let mut loaded = Vec::new();
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            for (index, (ir, php)) in locals.iter().enumerate() {
                let slot = builder.add_local(
                    Some(format!("v{index}")),
                    *ir,
                    php.clone(),
                    LocalKind::PhpLocal,
                );
                loaded.push(builder.emit_load_local(slot, *ir, php.clone()));
            }
            let (result_ir, result_php) = result.unwrap_or((IrType::Void, PhpType::Void));
            builder.emit(op, loaded.clone(), None, result_ir, result_php, Ownership::Owned);
            builder.terminate(Terminator::Return { value: None });
        }
        let inst = function
            .instructions
            .last()
            .expect("the probe emitted its instruction")
            .clone();
        (function, inst, loaded)
    }

    /// Verifies `mixed_clone` is admitted only from a cell source into a cell.
    #[test]
    fn mixed_clone_admits_only_a_cell_source() {
        let cell = (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        let (function, inst, _) =
            probe_over_locals(Op::MixedClone, &[cell.clone()], Some(cell.clone()));
        assert_eq!(mixed_clone_shape_issue(&function, &inst), None);

        // Boxing a concrete value is `mixed_box`; it has no `(tag, lo, hi)` to copy.
        let (function, inst, _) = probe_over_locals(
            Op::MixedClone,
            &[(IrType::I64, PhpType::Int)],
            Some(cell.clone()),
        );
        assert!(mixed_clone_shape_issue(&function, &inst).is_some());

        // The copy is a cell; any other result storage would be read as the wrong width.
        let (function, inst, _) =
            probe_over_locals(Op::MixedClone, &[cell], Some((IrType::I64, PhpType::Int)));
        assert!(mixed_clone_shape_issue(&function, &inst).is_some());
    }

    /// Verifies a container write through `[]` is admitted only on a declared `array` cell,
    /// and only with a value the lowering can box into a cell of its own.
    #[test]
    fn declared_array_writes_need_a_declared_array_cell() {
        let declared = (IrType::Heap(IrHeapKind::Mixed), PhpType::php_array());
        let mixed = (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        let (function, _, values) = probe_over_locals(
            Op::MixedArrayAppend,
            &[declared.clone(), mixed.clone(), (IrType::Str, PhpType::Str)],
            None,
        );
        assert!(value_is_a_declared_array_cell(&function, values[0]));
        // A `mixed` could hold a string or an `ArrayAccess` object, where `[]` means another thing.
        assert!(!value_is_a_declared_array_cell(&function, values[1]));
        assert!(!value_is_a_declared_array_cell(&function, values[2]));

        let (function, _, values) = probe_over_locals(
            Op::MixedArrayAppend,
            &[
                (IrType::I64, PhpType::Int),
                (IrType::F64, PhpType::Float),
                (IrType::Str, PhpType::Str),
                (IrType::TaggedScalar, PhpType::TaggedScalar),
                (IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Int))),
                mixed.clone(),
            ],
            None,
        );
        // A boxed value is copied into a cell of the write's own, so it is admitted too.
        for value in [values[0], values[1], values[2], values[3], values[5]] {
            assert!(value_boxes_into_a_container_write(&function, value), "{value:?}");
        }
        // Boxing a raw container would take over the operand's reference.
        assert!(!value_boxes_into_a_container_write(&function, values[4]));

        // The write's receiver, `$this->items` fetched for write, needs a concrete class layout:
        // a boxed receiver names no slot to copy.
        let (function, inst, _) = probe_over_locals(
            Op::PropGetForWrite,
            &[mixed.clone()],
            Some(declared.clone()),
        );
        let module = Module::new(Target::wasm());
        assert!(prop_get_for_write_shape_issue(&module, &function, &inst).is_some());
    }

    /// Verifies the four unwind-guard runtime calls are admitted at exactly their EIR shapes:
    /// a value and an i64 anchor answering an i64 token, one token answering nothing, and a
    /// container with its token answering nothing.
    #[test]
    fn exception_guards_admit_only_their_token_shapes() {
        let object = (IrType::Heap(IrHeapKind::Object), PhpType::Object("P".to_string()));
        let int = (IrType::I64, PhpType::Int);
        let array = (IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Int)));
        let hash = (
            IrType::Heap(IrHeapKind::Hash),
            PhpType::AssocArray { key: Box::new(PhpType::Str), value: Box::new(PhpType::Int) },
        );
        let admitted = |locals: &[(IrType, PhpType)],
                        result: Option<(IrType, PhpType)>,
                        target: RuntimeCallTarget| {
            let (function, mut inst, _) = probe_over_locals(Op::RuntimeCall, locals, result);
            inst.immediate = Some(Immediate::RuntimeCall(target));
            exception_guard_shape_issue(&function, &inst, target).is_none()
        };

        assert!(admitted(
            &[object.clone(), int.clone()],
            Some(int.clone()),
            RuntimeCallTarget::ExceptionGuardOwned
        ));
        assert!(admitted(&[int.clone()], None, RuntimeCallTarget::ExceptionUnguardOwned));
        assert!(admitted(
            &[array.clone(), int.clone()],
            None,
            RuntimeCallTarget::ExceptionUpdateArrayGuard
        ));
        assert!(admitted(
            &[hash, int.clone()],
            None,
            RuntimeCallTarget::ExceptionUpdateHashGuard
        ));

        // A guard whose token is not produced, or whose anchor is not a token, is refused.
        assert!(!admitted(
            &[object.clone(), int.clone()],
            None,
            RuntimeCallTarget::ExceptionGuardOwned
        ));
        assert!(!admitted(
            &[object.clone(), object],
            Some(int.clone()),
            RuntimeCallTarget::ExceptionGuardOwned
        ));
        assert!(!admitted(&[array], Some(int), RuntimeCallTarget::ExceptionUnguardOwned));
    }

    /// Verifies `is_countable()`/`is_iterable()` over a Mixed value are refused by name, while a
    /// predicate the cell tag decides, or one the static type decides, is admitted.
    #[test]
    fn type_predicates_refuse_class_checks_over_a_cell() {
        use crate::ir::PhpTypePredicate as P;
        let cell = (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        let array = (IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Int)));
        let issue = |source: &(IrType, PhpType), predicate: P| {
            let (function, mut inst, _) = probe_over_locals(
                Op::TypePredicate,
                std::slice::from_ref(source),
                Some((IrType::I64, PhpType::Bool)),
            );
            inst.immediate = Some(Immediate::TypePredicate(predicate));
            type_predicate_shape_issue(&Module::new(Target::wasm()), &function, &inst)
        };
        assert_eq!(issue(&cell, P::Array), None);
        assert_eq!(issue(&array, P::Countable), None);
        assert!(issue(&cell, P::Countable).is_some_and(|text| text.contains("Countable")));
        assert!(issue(&cell, P::Iterable).is_some_and(|text| text.contains("Iterable")));
        // A class that does not itself implement the interface proves nothing: a subclass may.
        let object = (IrType::Heap(IrHeapKind::Object), PhpType::Object("N".to_string()));
        assert!(issue(&object, P::Countable).is_some());
        assert!(issue(&object, P::Iterable).is_some());
    }

    /// Verifies `RuntimeCallTarget::ExceptionChain` is admitted only where every Throwable keeps
    /// `$previous` at one offset, the one `__rt_exception_chain` walks.
    #[test]
    fn exception_chain_needs_one_previous_offset() {
        let object = (IrType::Heap(IrHeapKind::Object), PhpType::Object("Exception".to_string()));
        let (mut function, mut inst, _) =
            probe_over_locals(Op::RuntimeCall, &[object.clone(), object], None);
        inst.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionChain));
        function.instructions.pop();
        function.instructions.push(inst.clone());
        let previous = PhpType::Union(vec![
            PhpType::Object("Throwable".to_string()),
            PhpType::Void,
        ]);
        let throwable = |class_id: u64, parent: Option<&str>, previous_index: usize| {
            let mut info = minimal_class_info(class_id);
            info.parent = parent.map(str::to_string);
            info.properties = (0..=previous_index)
                .map(|index| {
                    if index == previous_index {
                        ("previous".to_string(), previous.clone())
                    } else {
                        (format!("p{index}"), PhpType::Int)
                    }
                })
                .collect();
            info
        };
        let issues = |module: &Module| {
            let mut issues = Vec::new();
            check_runtime_call(module, "functions", &function, 0, 0, &inst, &mut issues);
            issues
        };

        // No Throwable at all: there is no offset to walk.
        assert!(!issues(&Module::new(Target::wasm())).is_empty());

        let mut module = Module::new(Target::wasm());
        module.class_infos.insert("Exception".to_string(), throwable(1, None, 2));
        module.class_infos.insert("Error".to_string(), throwable(2, None, 2));
        module
            .class_infos
            .insert("NotFound".to_string(), throwable(3, Some("Exception"), 2));
        assert_eq!(issues(&module), Vec::<String>::new());

        // A Throwable laying `$previous` elsewhere cannot share the walk.
        module.class_infos.insert("Error".to_string(), throwable(2, None, 3));
        assert!(!issues(&module).is_empty());
    }

    /// Verifies the call-operand owner scope is admitted only over a slot the frame has.
    #[test]
    fn call_operand_owner_scope_names_a_frame_slot() {
        let probe = |op: Op, name_slot: bool, known_slot: bool| {
            let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let slot = builder.add_local(
                    Some("owner".to_string()),
                    IrType::Heap(IrHeapKind::Array),
                    PhpType::Array(Box::new(PhpType::Int)),
                    LocalKind::PhpLocal,
                );
                let named = if known_slot {
                    slot
                } else {
                    crate::ir::LocalSlotId::from_raw(slot.as_raw() + 7)
                };
                builder.emit(
                    op,
                    Vec::new(),
                    name_slot.then_some(Immediate::LocalSlot(named)),
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
                builder.terminate(Terminator::Return { value: None });
            }
            let inst = function
                .instructions
                .last()
                .expect("the probe emitted the scope marker")
                .clone();
            call_operand_owner_shape_issue(&function, &inst)
        };

        for op in [Op::PushCallOperandOwner, Op::PopCallOperandOwner] {
            assert_eq!(probe(op, true, true), None, "{op:?}");
            assert!(probe(op, false, true).is_some(), "{op:?} without a slot");
            assert!(probe(op, true, false).is_some(), "{op:?} over an unknown slot");
        }
    }

    /// `Op::ZeroLocalSlot` needs a slot it can name, and nothing else.
    ///
    /// Every WASM storage shape is a fixed set of typed locals, so the zero of each local's own
    /// type is always expressible — there is no representation this cannot clear. What the gate
    /// does enforce is that the instruction carries its slot and no operands, because the value
    /// it replaces was already released by a separate instruction ahead of it.
    #[test]
    fn zero_local_slot_names_a_slot_it_can_clear() {
        let probe = |php: PhpType, ir: IrType, name_slot: bool, operand: bool| {
            let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let slot = builder.add_local(
                    Some("q".to_string()),
                    ir,
                    php.clone(),
                    LocalKind::PhpLocal,
                );
                let loaded = builder.emit_load_local(slot, ir, php);
                builder.emit(
                    Op::ZeroLocalSlot,
                    if operand { vec![loaded] } else { Vec::new() },
                    name_slot.then_some(Immediate::LocalSlot(slot)),
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
                builder.terminate(Terminator::Return { value: None });
            }
            let inst = function
                .instructions
                .last()
                .expect("the probe emitted the clear")
                .clone();
            zero_local_slot_shape_issue(&function, &inst)
        };

        // Every storage shape clears, one-word and two-word alike.
        assert_eq!(probe(PhpType::Str, IrType::Str, true, false), None);
        assert_eq!(probe(PhpType::Int, IrType::I64, true, false), None);
        assert_eq!(probe(PhpType::Float, IrType::F64, true, false), None);
        assert_eq!(
            probe(PhpType::Mixed, IrType::Heap(IrHeapKind::Mixed), true, false),
            None
        );
        // The slot is the whole instruction; without it there is nothing to clear.
        assert!(probe(PhpType::Str, IrType::Str, false, false).is_some());
        // The released value is not an operand here — its release is its own instruction.
        assert!(probe(PhpType::Str, IrType::Str, true, true).is_some());
    }

    /// The directory family admits the handle shape `opendir` actually answers.
    ///
    /// `opendir` takes a string path and boxes a `resource|false`; the other three take that box
    /// back — php accepts the `false` too and answers its own failure value — and differ only in
    /// what they answer: `readdir` another box, `rewinddir` and `closedir` php's null, which the
    /// EIR carries as a plain integer word.
    #[test]
    fn directory_iteration_admits_the_handle_shape_opendir_answers() {
        let probe = |target: RuntimeFnId, operand: (IrType, PhpType), result: IrType| {
            let module = Module::new(Target::wasm());
            let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let slot = builder.add_local(
                    Some("h".to_string()),
                    operand.0,
                    operand.1.clone(),
                    LocalKind::PhpLocal,
                );
                let loaded = builder.emit_load_local(slot, operand.0, operand.1);
                builder.emit(
                    Op::RuntimeCall,
                    vec![loaded],
                    Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))),
                    result,
                    PhpType::Mixed,
                    Ownership::Owned,
                );
                builder.terminate(Terminator::Return { value: None });
            }
            let inst = function
                .instructions
                .last()
                .expect("the probe emitted the call")
                .clone();
            let _ = &module;
            runtime_function_shape_issue(&module, &function, &inst, target)
        };

        let path = (IrType::Str, PhpType::Str);
        let handle = (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        let cell = IrType::Heap(IrHeapKind::Mixed);

        assert_eq!(probe(RuntimeFnId::Opendir, path.clone(), cell), None);
        assert_eq!(probe(RuntimeFnId::Readdir, handle.clone(), cell), None);
        assert_eq!(probe(RuntimeFnId::Rewinddir, handle.clone(), IrType::I64), None);
        assert_eq!(probe(RuntimeFnId::Closedir, handle.clone(), IrType::I64), None);

        // A path that is not a string names no file the preopen can resolve.
        assert!(probe(RuntimeFnId::Opendir, handle.clone(), cell).is_some());
        // A raw fd is not the boxed handle `opendir` hands out.
        assert!(probe(RuntimeFnId::Readdir, (IrType::I64, PhpType::Int), cell).is_some());
        // `rewinddir` answers php's null, never a value.
        assert!(probe(RuntimeFnId::Rewinddir, handle, cell).is_some());
    }

    /// `get_object_vars` is admitted only where the public projection IS php's whole answer.
    ///
    /// The table carries the public properties alone, which is what php projects for a call made
    /// outside the object's class. From INSIDE one php also projects the protected and private
    /// properties that scope can see, so the same walk would answer an object with fewer
    /// properties than it has — a wrong answer, and the reason the inside-scope call is refused
    /// rather than approximated. A receiver that names no class cannot have its properties proved
    /// projectable at all, and a property with no `mixed` form has nothing to box.
    #[test]
    fn get_object_vars_admits_only_the_projection_its_table_can_answer() {
        let probe = |property: PhpType, lexical_class: Option<&str>, receiver: PhpType| {
            let mut module = Module::new(Target::wasm());
            let mut class_info = minimal_class_info(1);
            class_info.properties = vec![("open".to_string(), property)];
            class_info.property_declared_slots = vec![true];
            class_info.defaults = vec![None];
            // Promoted, so the slot is always initialized: an uninitialized typed property is
            // refused on its own account, which `json_encode`'s CLI test pins.
            class_info.promoted_properties.insert("open".to_string());
            module.class_infos.insert("Box".to_string(), class_info);

            let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
            function.lexical_class = lexical_class.map(str::to_string);
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let ir = if matches!(receiver, PhpType::Object(_)) {
                    IrType::Heap(IrHeapKind::Object)
                } else {
                    IrType::Heap(IrHeapKind::Mixed)
                };
                let slot = builder.add_local(
                    Some("o".to_string()),
                    ir,
                    receiver.clone(),
                    LocalKind::PhpLocal,
                );
                let loaded = builder.emit_load_local(slot, ir, receiver);
                builder.emit(
                    Op::RuntimeCall,
                    vec![loaded],
                    Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                        RuntimeFnId::GetObjectVars,
                    ))),
                    IrType::Heap(IrHeapKind::Hash),
                    PhpType::AssocArray {
                        key: Box::new(PhpType::Str),
                        value: Box::new(PhpType::Mixed),
                    },
                    Ownership::Owned,
                );
                builder.terminate(Terminator::Return { value: None });
            }
            let inst = function
                .instructions
                .last()
                .expect("the probe emitted the call")
                .clone();
            get_object_vars_shape_issue(&module, &function, &inst)
        };

        let box_object = PhpType::Object("Box".to_string());
        // Every scalar and container form a property slot can hold has a mixed form to box.
        for property in [
            PhpType::Int,
            PhpType::Str,
            PhpType::Float,
            PhpType::Bool,
            PhpType::Array(Box::new(PhpType::Int)),
            PhpType::Object("Box".to_string()),
            PhpType::Mixed,
        ] {
            assert_eq!(
                probe(property.clone(), None, box_object.clone()),
                None,
                "a public {property:?} property projects"
            );
        }
        // The inline `int|null` pair is two words in the slot, not a cell, so there is nothing to
        // hand the hash as a container and nothing to read as a scalar.
        assert!(probe(PhpType::TaggedScalar, None, box_object.clone()).is_some());
        // Inside the class php would project more than the table carries.
        assert!(probe(PhpType::Int, Some("Box"), box_object.clone()).is_some());
        // A receiver that names no class proves nothing about the properties it will meet.
        assert!(probe(PhpType::Int, None, PhpType::Mixed).is_some());
    }

    /// `Op::PropInitialized` is admitted only where the property's write is PROVED, and for a
    /// receiver whose declared layout this target can name.
    ///
    /// A default proves the write, so the probe folds to `true`. No default and no other proof is
    /// the case this target refuses to read at all — there is no initialization bitmap to consult
    /// — so probing it is refused for the same reason rather than guessed at.
    ///
    /// The receiver is the second axis, and the one `$a?->next?->n` needs: every hop past the
    /// first has a NULLABLE receiver, `Union([Object(N), Void])` stored as one boxed cell. Its
    /// layout is the union's single class, which is exactly what `->` already reads through
    /// `receiver_object_class` — so the probe asks the same function. A union naming two classes
    /// has no single layout, and a bare `Mixed` names none at all; both stay refused.
    ///
    /// The null arm is NOT decided here, and that is the point of the last two rows: a boxed
    /// receiver is admitted with or without a branch having tested it, because the lowering
    /// answers null from the cell's own tag rather than folding to a constant. `isset($maybeNull->n)`
    /// arrives with nothing having tested the receiver and needs the false answer; `?->` arrives
    /// under its own branch and gets the same, redundantly. One answer for both beats a dominance
    /// analysis in the gate that would let the second case keep the constant.
    #[test]
    fn prop_initialized_admits_only_a_proved_write() {
        let boxed_cell = IrType::Heap(IrHeapKind::Mixed);
        let object = IrType::Heap(IrHeapKind::Object);
        let probe = |with_default: bool,
                     receiver_ir: IrType,
                     receiver_php: PhpType,
                     guard_null: bool| {
            let mut module = Module::new(Target::wasm());
            let mut class_info = minimal_class_info(1);
            class_info.properties = vec![("leaf".to_string(), PhpType::Int)];
            class_info.property_declared_slots = vec![true];
            class_info.defaults = vec![with_default.then(|| {
                crate::parser::ast::Expr::new(
                    crate::parser::ast::ExprKind::IntLiteral(0),
                    Span::dummy(),
                )
            })];
            module.class_infos.insert("Box".to_string(), class_info.clone());
            module.class_infos.insert("Other".to_string(), class_info);

            let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
            let data = module.data.intern_string("leaf");
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let slot = builder.add_local(
                    Some("b".to_string()),
                    receiver_ir,
                    receiver_php.clone(),
                    LocalKind::PhpLocal,
                );
                let receiver = builder.emit_load_local(slot, receiver_ir, receiver_php);
                if guard_null {
                    // The chain's own shape: `is_null` on the link, the read on the FALSE arm.
                    let test = builder
                        .emit(
                            Op::IsNull,
                            vec![receiver],
                            None,
                            IrType::I64,
                            PhpType::Bool,
                            Ownership::NonHeap,
                        )
                        .expect("is_null emits");
                    let null_arm = builder.create_named_block("was.null", Vec::new());
                    let read_arm = builder.create_named_block("not.null", Vec::new());
                    builder.terminate(Terminator::CondBr {
                        cond: test,
                        then_target: null_arm,
                        then_args: Vec::new(),
                        else_target: read_arm,
                        else_args: Vec::new(),
                    });
                    builder.position_at_end(null_arm);
                    builder.terminate(Terminator::Return { value: None });
                    builder.position_at_end(read_arm);
                }
                builder.emit(
                    Op::PropInitialized,
                    vec![receiver],
                    Some(Immediate::Data(data)),
                    IrType::I64,
                    PhpType::Bool,
                    Ownership::NonHeap,
                );
                builder.terminate(Terminator::Return { value: None });
            }
            // A REFERENCE into the function's own list, not a clone: `block_containing` finds the
            // instruction by address, which is how the production caller reaches it too.
            let inst = function
                .instructions
                .last()
                .expect("the probe emitted the check");
            prop_initialized_shape_issue(&module, &function, inst)
        };

        let concrete = PhpType::Object("Box".to_string());
        let nullable = PhpType::Union(vec![PhpType::Object("Box".to_string()), PhpType::Void]);
        let two_classes = PhpType::Union(vec![
            PhpType::Object("Box".to_string()),
            PhpType::Object("Other".to_string()),
            PhpType::Void,
        ]);

        assert_eq!(
            probe(true, object, concrete.clone(), false),
            None,
            "a default proves the write"
        );
        assert!(
            probe(false, object, concrete, false)
                .is_some_and(|issue| issue.contains("may be uninitialized")),
            "without a proof the probe is refused, exactly as the read is"
        );
        // The nullable receiver every `?->` chain has past its first hop, under its own branch.
        assert_eq!(
            probe(true, boxed_cell, nullable.clone(), true),
            None,
            "a `?Box` receiver names one layout, which is all this gate is asked about"
        );
        assert!(
            probe(false, boxed_cell, nullable.clone(), true)
                .is_some_and(|issue| issue.contains("may be uninitialized")),
            "opening the union must not skip the write proof"
        );
        // The SAME receiver with nothing having tested it: `isset($maybeNull->n)`. Admitted too,
        // because the lowering reads the cell's tag instead of folding — which is the only reason
        // the constant above is allowed to stay a constant for the branch-guarded case.
        assert_eq!(
            probe(true, boxed_cell, nullable, false),
            None,
            "an untested boxed receiver is answered at run time, not refused"
        );
        // Two classes have no single layout, and a bare Mixed names none.
        assert!(
            probe(true, boxed_cell, two_classes, true).is_some(),
            "a union naming two classes has no one layout to read"
        );
        assert!(
            probe(true, boxed_cell, PhpType::Mixed, true).is_some(),
            "a bare Mixed receiver proves nothing about the properties it will meet"
        );
    }

    /// `Op::StaticPropInitialized` is admitted only where the DECLARATION proves the write.
    ///
    /// A static slot carries no uninitialized sentinel here — the layout writes bytes for every
    /// slot — so `public static int $n;` before its first write is indistinguishable from one
    /// holding `0`, and php answers `isset()` false for the first and true for the second. A
    /// default settles it, and so does an UNTYPED declaration, which php gives an implicit `null`
    /// (a written value: `isset()` is false because the VALUE is null, which the read that follows
    /// the probe decides, not the probe).
    #[test]
    fn static_prop_initialized_needs_a_declared_default() {
        let probe = |typed: bool, with_default: bool| {
            let mut module = Module::new(Target::wasm());
            let mut class_info = minimal_class_info(1);
            class_info.static_properties = vec![("n".to_string(), PhpType::Int)];
            if typed {
                class_info
                    .declared_static_properties
                    .insert("n".to_string());
            }
            class_info.static_defaults = vec![with_default.then(|| {
                crate::parser::ast::Expr::new(
                    crate::parser::ast::ExprKind::IntLiteral(0),
                    Span::dummy(),
                )
            })];
            module.class_infos.insert("Reg".to_string(), class_info);

            let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
            let label = module.data.intern_string("Reg::$n");
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                builder.emit(
                    Op::StaticPropInitialized,
                    Vec::new(),
                    Some(Immediate::Data(label)),
                    IrType::I64,
                    PhpType::Bool,
                    Ownership::NonHeap,
                );
                builder.terminate(Terminator::Return { value: None });
            }
            let inst = function
                .instructions
                .last()
                .expect("the probe emitted the check");
            static_prop_initialized_shape_issue(&module, &function, inst)
        };

        assert_eq!(probe(true, true), None, "a default proves the write");
        assert_eq!(
            probe(false, false),
            None,
            "an untyped static carries php's implicit null, which IS a written value"
        );
        assert!(
            probe(true, false).is_some_and(|issue| issue.contains("may be uninitialized")),
            "a typed static with no default is the case no slot byte can tell from zero"
        );
    }

    /// `Op::PropUnset` is admitted only for a slot whose second word can hold the marker.
    ///
    /// php REMOVES the property; a fixed slot cannot, so the slot is MARKED in its second word with
    /// the native backend's own sentinel. That only works where no live value of the slot's shape
    /// can equal it — every admitted shape puts a tag, a length or a container tag there.
    ///
    /// An UNTYPED declared property is refused for a different reason, and the same one native
    /// gives: php's removed-then-read answers `null` with a warning rather than raising, and a slot
    /// whose declared type has no null has nowhere to put that answer.
    #[test]
    fn prop_unset_needs_a_declared_typed_slot() {
        let probe = |declared: bool, by_reference: bool| {
            let mut module = Module::new(Target::wasm());
            let mut class_info = minimal_class_info(1);
            class_info.properties = vec![("n".to_string(), PhpType::Int)];
            class_info.property_declared_slots = vec![declared];
            class_info.defaults = vec![Some(crate::parser::ast::Expr::new(
                crate::parser::ast::ExprKind::IntLiteral(0),
                Span::dummy(),
            ))];
            if by_reference {
                class_info.reference_properties.insert("n".to_string());
            }
            module.class_infos.insert("Box".to_string(), class_info);
            // The read that follows the unset raises php's `Error`, so the class has to be here AND
            // be constructible: a Throwable with a string `$message` slot and layout-expressible
            // defaults, which is what the raise path builds.
            let mut error = minimal_class_info(2);
            error.interfaces = vec!["Throwable".to_string()];
            error.properties = vec![
                ("message".to_string(), PhpType::Str),
                ("code".to_string(), PhpType::Int),
            ];
            error.property_declared_slots = vec![true, true];
            error.defaults = vec![None, None];
            module.class_infos.insert("Error".to_string(), error);

            let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
            let data = module.data.intern_string("n");
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let slot = builder.add_local(
                    Some("b".to_string()),
                    IrType::Heap(IrHeapKind::Object),
                    PhpType::Object("Box".to_string()),
                    LocalKind::PhpLocal,
                );
                let receiver = builder.emit_load_local(
                    slot,
                    IrType::Heap(IrHeapKind::Object),
                    PhpType::Object("Box".to_string()),
                );
                builder.emit(
                    Op::PropUnset,
                    vec![receiver],
                    Some(Immediate::Data(data)),
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
                builder.terminate(Terminator::Return { value: None });
            }
            let inst = function
                .instructions
                .last()
                .expect("the probe emitted the unset");
            property_unset_shape_issue(&module, &function, inst)
        };

        assert_eq!(probe(true, false), None, "a declared typed slot carries the marker");
        assert!(
            probe(false, false).is_some_and(|issue| issue.contains("untyped declared property")),
            "an untyped slot has nowhere to put php's removed-then-null read"
        );
        assert!(
            probe(true, true).is_some_and(|issue| issue.contains("by-reference")),
            "a by-reference property's value lives elsewhere than the slot"
        );
    }

    /// `$obj->$name` is routed to the dynamic-property hash only when nothing else could answer.
    ///
    /// The name is a run-time string, so nothing at compile time can say which property it means.
    /// Two conditions make the hash the right destination anyway: the class must CARRY one, and it
    /// must declare no property slots at all. With a slot present the name might name that slot,
    /// and sending it to the hash regardless would read a declared property as absent — silently,
    /// which is the kind of answer this target refuses rather than gives.
    #[test]
    fn dynamic_property_needs_a_hash_and_no_declared_slots() {
        let probe = |dynamic: bool, declared: bool| {
            let mut module = Module::new(Target::wasm());
            let mut class_info = minimal_class_info(1);
            class_info.allow_dynamic_properties = dynamic;
            if declared {
                class_info.properties = vec![("n".to_string(), PhpType::Int)];
                class_info.property_declared_slots = vec![true];
                class_info.defaults = vec![None];
            }
            module.class_infos.insert("Bag".to_string(), class_info);
            // The miss warns, and only a main-bearing module carries the warning runtime — so the
            // module has to have one for the gate to reach its real questions.
            let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
            main.flags.is_main = true;
            module.add_function(main);

            let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
            {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let object = builder.add_local(
                    Some("b".to_string()),
                    IrType::Heap(IrHeapKind::Object),
                    PhpType::Object("Bag".to_string()),
                    LocalKind::PhpLocal,
                );
                let receiver = builder.emit_load_local(
                    object,
                    IrType::Heap(IrHeapKind::Object),
                    PhpType::Object("Bag".to_string()),
                );
                let name_slot = builder.add_local(
                    Some("k".to_string()),
                    IrType::Str,
                    PhpType::Str,
                    LocalKind::PhpLocal,
                );
                let name = builder.emit_load_local(name_slot, IrType::Str, PhpType::Str);
                builder.emit(
                    Op::DynamicPropGet,
                    vec![receiver, name],
                    None,
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                );
                builder.terminate(Terminator::Return { value: None });
            }
            let inst = function
                .instructions
                .last()
                .expect("the probe emitted the read");
            dynamic_property_shape_issue(&module, &function, inst)
        };

        assert_eq!(
            probe(true, false),
            None,
            "an ADP class with no declared slots has one destination for the name"
        );
        assert!(
            probe(false, false).is_some_and(|issue| issue.contains("no dynamic-property storage")),
            "a class without the attribute carries no hash to look the name up in"
        );
        assert!(
            probe(true, true).is_some_and(|issue| issue.contains("could \n             also reach")
                || issue.contains("also reach")),
            "a declared slot is a second thing the name could name"
        );
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
            constant_deprecations: HashMap::new(),
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
            is_internal: false,
            eval_property_storage: false,
            clone_override_property_storage: false,
            scope_dynamic_property_storage: false,
            constant_order: Vec::new(),
            property_hooks: HashMap::new(),
            abstract_methods: HashSet::new(),
            abstract_static_methods: HashSet::new(),
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
            is_generator: false,
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
            is_generator: false,
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

    /// Answers a concrete runtime class php-src could not enter with PHP's own
    /// `ArgumentCountError`, instead of refusing the program or misreporting it as undefined.
    ///
    /// `One::run(int $value)` shares only a method NAME with the call site. It once refused the
    /// whole program — every candidate had to fit — which let one bystander veto a dispatch it
    /// has nothing to do with. It now leaves the callable list and reappears as an arity
    /// FAILURE, which the ladder turns into an arm raising the error php-src raises, with the
    /// count and the `exactly` wording php-src uses.
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

        let method_key = crate::names::php_symbol_key(method_name);
        assert_eq!(
            mixed_method_issue(&module, method_data),
            None,
            "a bystander php-src could not enter must not refuse the dispatch"
        );
        assert_eq!(
            super::super::classes::mixed_method_candidates(
                &module,
                &method_key,
                &PhpType::Mixed,
                0
            )
            .iter()
            .map(|(_, class_name, _)| class_name.as_str())
            .collect::<Vec<_>>(),
            vec!["Zero"],
            "only the class php-src would enter stays callable"
        );
        assert_eq!(
            super::super::classes::mixed_method_arity_failures(
                &module,
                &method_key,
                &PhpType::Mixed,
                0
            ),
            vec![super::super::classes::MixedMethodArityFailure {
                class_id: 1,
                class_name: "One".to_string(),
                required: 1,
                exact: true,
            }],
            "the dropped class still gets an arm, raising php-src's ArgumentCountError"
        );
    }

    /// Keeps a class whose method takes FEWER parameters than the call passes.
    ///
    /// Measured on php-src 8.5.6: a user method accepts surplus arguments silently —
    /// `C::m(int $a)` called with two runs, and `func_num_args()` sees both. An upper bound on
    /// the arity filter therefore dropped a class php-src dispatches to, and the ladder answered
    /// `Call to undefined method` for a call that should have printed. It stays a CANDIDATE, so
    /// the shape audit downstream decides it — refusing is a coverage cost, answering wrongly is
    /// not a trade this backend may make.
    #[test]
    fn a_candidate_with_fewer_parameters_than_the_call_passes_is_not_dropped() {
        let method_name = "show";
        let method_key = crate::names::php_symbol_key(method_name);
        let mut module = Module::new(Target::wasm());

        let mut none = minimal_class_info(1);
        none.methods.insert(method_key.clone(), void_signature());
        none.method_impl_classes
            .insert(method_key.clone(), "None".to_string());
        module.class_infos.insert("None".to_string(), none);

        assert_eq!(
            super::super::classes::mixed_method_candidates(
                &module,
                &method_key,
                &PhpType::Mixed,
                1
            )
            .iter()
            .map(|(_, class_name, _)| class_name.as_str())
            .collect::<Vec<_>>(),
            vec!["None"],
            "php-src ignores the surplus argument, so the class is still reachable"
        );
        assert!(
            super::super::classes::mixed_method_arity_failures(
                &module,
                &method_key,
                &PhpType::Mixed,
                1
            )
            .is_empty(),
            "a surplus argument is not an ArgumentCountError"
        );
    }

    /// Words the expected count the way php-src does for each of its three shapes.
    ///
    /// Measured on 8.5.6: a parameter with a DEFAULT turns `exactly` into `at least`, but a
    /// VARIADIC tail does not — `m(int $a, int ...$rest)` still reports `exactly 1 expected`,
    /// even though the variadic sits in `params` carrying no default of its own.
    #[test]
    fn the_expected_argument_count_uses_php_src_wording_for_defaults_and_variadics() {
        let method_name = "m";
        let method_key = crate::names::php_symbol_key(method_name);
        let mut module = Module::new(Target::wasm());

        let mut required_only = void_signature();
        required_only.params.push(("a".to_string(), PhpType::Int));
        required_only.params.push(("b".to_string(), PhpType::Int));
        required_only.defaults.push(None);
        required_only.defaults.push(None);

        let mut with_default = void_signature();
        with_default.params.push(("a".to_string(), PhpType::Int));
        with_default.params.push(("b".to_string(), PhpType::Int));
        with_default.defaults.push(None);
        with_default.defaults.push(Some(crate::parser::ast::Expr {
            kind: crate::parser::ast::ExprKind::IntLiteral(2),
            span: crate::span::Span::new(1, 1),
        }));

        let mut variadic = void_signature();
        variadic.params.push(("a".to_string(), PhpType::Int));
        variadic
            .params
            .push(("rest".to_string(), PhpType::Array(Box::new(PhpType::Int))));
        variadic.defaults.push(None);
        variadic.defaults.push(None);
        variadic.variadic = Some("rest".to_string());

        for (class_id, class_name, signature) in [
            (1u64, "Exact", required_only),
            (2, "Optional", with_default),
            (3, "Variadic", variadic),
        ] {
            let mut class_info = minimal_class_info(class_id);
            class_info.methods.insert(method_key.clone(), signature);
            class_info
                .method_impl_classes
                .insert(method_key.clone(), class_name.to_string());
            module.class_infos.insert(class_name.to_string(), class_info);
        }

        assert_eq!(
            super::super::classes::mixed_method_arity_failures(
                &module,
                &method_key,
                &PhpType::Mixed,
                0
            ),
            vec![
                super::super::classes::MixedMethodArityFailure {
                    class_id: 1,
                    class_name: "Exact".to_string(),
                    required: 2,
                    exact: true,
                },
                super::super::classes::MixedMethodArityFailure {
                    class_id: 2,
                    class_name: "Optional".to_string(),
                    required: 1,
                    exact: false,
                },
                super::super::classes::MixedMethodArityFailure {
                    class_id: 3,
                    class_name: "Variadic".to_string(),
                    required: 1,
                    exact: true,
                },
            ],
            "php-src's own counts and wording for the three shapes"
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

        let candidates = super::super::classes::mixed_method_candidates(
            &module,
            &method_key,
            &PhpType::Mixed,
            0,
        );
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].1, "Concrete");
    }

    /// A receiver that names a class keeps only that class's subtree as a dispatch candidate.
    ///
    /// The unrelated class here shares the method NAME and nothing else, which is exactly the
    /// prelude collision (`DateInterval::format` against a user `Money::format`) that refused a
    /// program whose receiver could never have been a `DateInterval`.
    #[test]
    fn a_union_receiver_drops_the_candidates_its_static_type_cannot_hold() {
        let method_key = crate::names::php_symbol_key("format");
        let mut module = Module::new(Target::wasm());
        for (index, name) in ["Money", "Unrelated"].iter().enumerate() {
            let mut class_info = minimal_class_info(index as u64 + 1);
            class_info
                .methods
                .insert(method_key.clone(), void_signature());
            module.class_infos.insert((*name).to_string(), class_info);
        }

        let every = super::super::classes::mixed_method_candidates(
            &module,
            &method_key,
            &PhpType::Mixed,
            0,
        );
        assert_eq!(every.len(), 2, "a mixed receiver names no class at all");

        let narrowed = super::super::classes::mixed_method_candidates(
            &module,
            &method_key,
            &PhpType::Union(vec![PhpType::Int, PhpType::Object("Money".to_string())]),
            0,
        );
        assert_eq!(narrowed.len(), 1);
        assert_eq!(narrowed[0].1, "Money");

        // A receiver that admits no class at all is not evidence: the unnarrowed list stands.
        let scalars = super::super::classes::mixed_method_candidates(
            &module,
            &method_key,
            &PhpType::Union(vec![PhpType::Int, PhpType::Str]),
            0,
        );
        assert_eq!(scalars.len(), 2);
    }

    /// A class php-src could not ENTER with this many arguments is not this dispatch's arm.
    ///
    /// This is what a `mixed` receiver depends on: its type narrows nothing, so the only thing
    /// separating a user `Money::format()` from a prelude `DateInterval::format(string)` is that
    /// calling the latter with no arguments is an `ArgumentCountError` in every implementation.
    ///
    /// The filter runs ONE WAY. Passing an argument too many does not disqualify anything — a
    /// user method ignores surplus arguments — so the one-argument call keeps both classes and
    /// leaves the shape audit to decide `Money`, which is the second half of this test.
    #[test]
    fn a_candidate_php_could_not_call_with_this_arity_leaves_the_dispatch_ladder() {
        let method_key = crate::names::php_symbol_key("format");
        let mut module = Module::new(Target::wasm());
        let mut money = minimal_class_info(1);
        money
            .methods
            .insert(method_key.clone(), void_signature());
        module.class_infos.insert("Money".to_string(), money);

        let mut interval = minimal_class_info(2);
        let mut demanding = void_signature();
        demanding.params.push(("format".to_string(), PhpType::Str));
        demanding.defaults.push(None);
        demanding.ref_params.push(false);
        demanding.declared_params.push(true);
        demanding.param_type_exprs.push(None);
        demanding.param_attributes.push(Vec::new());
        interval.methods.insert(method_key.clone(), demanding);
        module
            .class_infos
            .insert("DateInterval".to_string(), interval);

        let no_arguments =
            super::super::classes::mixed_method_candidates(&module, &method_key, &PhpType::Mixed, 0);
        assert_eq!(no_arguments.len(), 1);
        assert_eq!(no_arguments[0].1, "Money");
        assert_eq!(
            super::super::classes::mixed_method_arity_failures(
                &module,
                &method_key,
                &PhpType::Mixed,
                0
            )
            .iter()
            .map(|failure| failure.class_name.as_str())
            .collect::<Vec<_>>(),
            vec!["DateInterval"],
            "the class php-src refuses to enter keeps an arm of its own"
        );

        let one_argument =
            super::super::classes::mixed_method_candidates(&module, &method_key, &PhpType::Mixed, 1);
        assert_eq!(
            one_argument
                .iter()
                .map(|(_, class_name, _)| class_name.as_str())
                .collect::<Vec<_>>(),
            vec!["Money", "DateInterval"],
            "a surplus argument disqualifies nothing: php-src enters Money::format() too"
        );
        assert!(
            super::super::classes::mixed_method_arity_failures(
                &module,
                &method_key,
                &PhpType::Mixed,
                1
            )
            .is_empty(),
            "and neither class raises ArgumentCountError at this arity"
        );
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
                vec![(IrType::Str, PhpType::Float)],
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
        // The destructor is reachable through the release ladder, and the closure body through
        // the runtime that references every body. `main` builds the first-class callable but
        // never invokes it and never creates the closure, so the two wrappers are planned but
        // not written out: only an invoke ladder calls them.
        let reachable_symbols = [
            super::super::symbols::method_symbol("PlanClass::__destruct"),
            super::super::symbols::closure_body_symbol("__eir_closure_plan_combo_0"),
        ];
        for symbol in reachable_symbols {
            assert!(wat.contains(&format!("${symbol}")), "missing ${symbol}: {wat}");
        }
        let unreachable_symbols = [
            super::super::symbols::closure_wrapper_symbol("__eir_closure_plan_combo_0"),
            super::super::symbols::fcc_wrapper_symbol("plan_fcc"),
        ];
        for symbol in unreachable_symbols {
            assert!(
                !wat.contains(&format!("(func ${symbol}")),
                "an unreachable ${symbol} was emitted"
            );
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
            // A cell to a TAGGED pair used to be the specimen here and is now a legal move —
            // the two carry the same two facts. What no conversion can reconcile is a
            // destination whose STORAGE and PHP type contradict each other, so that is what
            // the fixture hands the branch.
            (
                invalid_mixed_transfer_module(),
                "invalid wasm storage pair Str/Float",
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
            // `(string) $int` is supported, so the probe casts a STRING to a float instead —
            // a conversion the table still has no exact answer for.
            let text = builder.emit_const_str(method_data);
            let _ = builder.emit(
                Op::Cast,
                vec![text],
                Some(Immediate::CastTarget(IrType::F64)),
                IrType::F64,
                PhpType::Float,
                Ownership::NonHeap,
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
                        arguments: crate::ir::RuntimeArgumentLayout::Values,
                        strict_types: None,
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
        // Counted inside the generated function alone: runtime helpers call the same warning,
        // and a module-wide count would answer for them rather than for the reads under test.
        assert_eq!(
            generated_function_body(&wat, "_entry")
                .matches("call $__rt_warn_undefined_array_key_int")
                .count(),
            2,
            "only the two normal reads should call the warning helper: {wat}"
        );
    }

    /// The two untyped three-operand reads are told apart by their RECEIVER, and only by it.
    ///
    /// `$mixed[$key]` and `$obj[$key]` carry the same operand count, the same key and warn-flag
    /// shapes, and the same boxed result, so a lowering that keyed off arity alone would send an
    /// `ArrayAccess` object into `__rt_mixed_array_get` and read the object header as a Mixed
    /// cell. This pins the discrimination in both directions.
    #[test]
    fn array_access_and_mixed_reads_are_told_apart_by_their_receiver() {
        for (receiver_ir, receiver_php, is_array_access) in [
            (
                IrType::Heap(IrHeapKind::Object),
                PhpType::Object("Box".to_string()),
                true,
            ),
            (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, false),
        ] {
            let mut function =
                Function::new("reads".to_string(), IrType::Void, PhpType::Void);
            function.flags.is_main = true;
            let call = {
                let mut builder = Builder::new(&mut function);
                let entry = builder.create_named_block("entry", Vec::new());
                builder.set_entry(entry);
                builder.position_at_end(entry);
                let receiver = builder
                    .emit(
                        Op::ObjectNew,
                        Vec::new(),
                        Some(Immediate::Data(crate::ir::DataId::from_raw(0))),
                        receiver_ir,
                        receiver_php,
                        Ownership::Owned,
                    )
                    .expect("receiver");
                let key = builder.emit_const_str(crate::ir::DataId::from_raw(0));
                let warn = builder.emit_const_i64(1);
                let read = builder
                    .emit(
                        Op::RuntimeCall,
                        vec![receiver, key, warn],
                        None,
                        IrType::Heap(IrHeapKind::Mixed),
                        PhpType::Mixed,
                        Ownership::Owned,
                    )
                    .expect("read");
                let _ = read;
                builder.terminate(Terminator::Return { value: None });
                function
                    .instructions
                    .iter()
                    .find(|inst| inst.op == Op::RuntimeCall)
                    .cloned()
                    .expect("the read instruction")
            };
            assert_eq!(
                array_access_read_is_supported(&function, &call),
                is_array_access,
                "an {receiver_ir:?} receiver must {} the ArrayAccess path",
                if is_array_access { "take" } else { "not take" }
            );
        }
    }

    /// Every `__rt_warn_*` helper must open with the `@` suppression guard.
    ///
    /// Enumerated from the emitted WAT rather than from a hand-kept list: a new
    /// warning helper added without the guard would otherwise print through `@`,
    /// and nothing else in the suite would notice. The guard is asserted to come
    /// before the helper's first write, since a guard placed after one would
    /// leak a partial diagnostic.
    #[test]
    fn every_warning_helper_opens_with_the_suppression_guard() {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("_entry".to_string(), IrType::Void, PhpType::Void);
        // A warning-producing read is only admitted in a main-bearing command
        // module, which is also what pulls the warning helpers into the WAT.
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let index = builder.emit_const_i64(0);
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
                vec![array, index],
                None,
                IrType::TaggedScalar,
                PhpType::TaggedScalar,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let wat = validate_module(&module)
            .expect("a warning-emitting read must remain supported")
            .into_wat();

        let mut checked = 0usize;
        for chunk in wat.split("(func $").skip(1) {
            let name = chunk.split(|c: char| c.is_whitespace() || c == '(').next().unwrap_or("");
            if !name.starts_with("__rt_warn_") {
                continue;
            }
            checked += 1;
            let guard = chunk
                .find(DIAG_SUPPRESS_GUARD.trim())
                .unwrap_or_else(|| panic!("`{name}` is missing the @ suppression guard: {chunk}"));
            if let Some(write) = chunk.find("$__rt_wasi_write") {
                assert!(
                    guard < write,
                    "`{name}` guards after its first write, which would print a partial diagnostic"
                );
            }
        }
        assert!(
            checked >= 2,
            "fixture emitted {checked} warning helper(s); it must exercise the real ones: {wat}"
        );
    }

    /// Returns the WAT text of one generated function, so a call-site count answers for the
    /// code under test rather than for any runtime helper that happens to use the same symbol.
    fn generated_function_body<'a>(wat: &'a str, name: &str) -> &'a str {
        let start = wat
            .find(&format!("(func ${name} "))
            .or_else(|| wat.find(&format!("(func ${name}\n")))
            .unwrap_or_else(|| panic!("generated function ${name} must be in the module:\n{wat}"));
        let rest = &wat[start + 1..];
        let end = rest.find("\n  (func ").map_or(rest.len(), |offset| offset);
        &rest[..end]
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
        // Counted inside the generated function alone, for the same reason as the array case:
        // runtime helpers share these symbols and would answer for themselves otherwise.
        let body = generated_function_body(&wat, "_entry");
        assert_eq!(
            body.matches("call $__rt_warn_undefined_array_key_int").count(),
            3,
            "only normal reads should call the integer warning arm: {wat}"
        );
        assert_eq!(
            body.matches("call $__rt_warn_undefined_array_key_str").count(),
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
            // An empty module is enough here: these cases never reach the main-bearing gate,
            // which only the declared-return coercion consults.
            super::cast_shape_issue(&Module::new(Target::wasm()), function, cast)
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
        // A TAGGED scalar is the int-or-null an `array<int>` read answers, and PHP renders its
        // null arm as the empty string, so the pair is exact rather than approximate.
        assert_eq!(
            shape_issue(
                IrType::TaggedScalar,
                PhpType::TaggedScalar,
                Ownership::MaybeOwned
            ),
            None
        );
        // The TAG has to agree with the storage: a raw I64 claiming to be tagged does not.
        assert!(
            shape_issue(IrType::I64, PhpType::TaggedScalar, Ownership::MaybeOwned).is_some()
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
                .is_none(),
                "two Mixed cells are answered by the deep, order-sensitive array walk"
            );

            // An inline `?int` pair is comparable against a concrete side and against another
            // pair: its tag is 0 or 8 and nothing else, so every such comparison is decidable.
            // What it may NOT meet is a runtime-tagged cell, whose own tag is dynamic too.
            for concrete in [
                (IrType::I64, PhpType::Int, Ownership::NonHeap),
                (IrType::I64, PhpType::Void, Ownership::NonHeap),
                (IrType::I64, PhpType::Bool, Ownership::NonHeap),
                (IrType::F64, PhpType::Float, Ownership::NonHeap),
                (IrType::Str, PhpType::Str, Ownership::Borrowed),
                (
                    IrType::TaggedScalar,
                    PhpType::TaggedScalar,
                    Ownership::NonHeap,
                ),
            ] {
                assert!(
                    shape_issue(
                        op,
                        vec![
                            (
                                IrType::TaggedScalar,
                                PhpType::Union(vec![PhpType::Int, PhpType::Void]),
                                Ownership::NonHeap
                            ),
                            concrete.clone()
                        ],
                        None,
                        (IrType::I64, PhpType::Bool, Ownership::NonHeap)
                    )
                    .is_none(),
                    "a nullable int compares against {concrete:?}"
                );
            }
            assert!(
                shape_issue(
                    op,
                    vec![
                        (
                            IrType::TaggedScalar,
                            PhpType::TaggedScalar,
                            Ownership::NonHeap
                        ),
                        (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned),
                    ],
                    None,
                    (IrType::I64, PhpType::Bool, Ownership::NonHeap)
                )
                .is_some(),
                "a runtime-tagged cell needs the mixed/concrete path, not this one"
            );
            // A union that is NOT `int|null` never reaches the pair representation, so admitting
            // it here would compare two different storages.
            assert!(
                shape_issue(
                    op,
                    vec![
                        (
                            IrType::TaggedScalar,
                            PhpType::Union(vec![PhpType::Int, PhpType::Str]),
                            Ownership::NonHeap
                        ),
                        (IrType::I64, PhpType::Int, Ownership::NonHeap),
                    ],
                    None,
                    (IrType::I64, PhpType::Bool, Ownership::NonHeap)
                )
                .is_some(),
                "only int|null folds to the tagged pair"
            );

            for invalid in [
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
        // Truthiness is no longer among them. Its per-tag ANSWERS were always exact — the
        // seventeen arms are verified against php-src in
        // `codegen::cli::test_cli_wasm_coerces_a_boxed_arithmetic_operand`'s sibling probe —
        // and the only thing that was missing, the warning a NaN raises on its way to `true`,
        // is now emitted. This module bears a `main`, so it can carry that warning.
        assert!(
            !message.contains("truthiness"),
            "a command module carries the NaN warning and needs no truthiness refusal: {message}"
        );
    }

    /// A REACTOR keeps the truthiness refusal, because it has no stderr for the NaN warning.
    ///
    /// The answers would be right without it, which is exactly the trap: answering silently
    /// where php-src speaks is a divergence, not a partial implementation. So the rule follows
    /// the module kind rather than the value.
    #[test]
    fn float_truthiness_still_needs_a_command_module_for_its_warning() {
        let mut module = Module::new(Target::wasm());
        let mut function = Function::new("truthy".to_string(), IrType::I64, PhpType::Bool);
        // Deliberately NOT `flags.is_main`: this is a reactor.
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let float = builder.emit_const_f64(1.5);
            let truthy = builder.emit(
                Op::IsTruthy,
                vec![float],
                None,
                IrType::I64,
                PhpType::Bool,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Return { value: truthy });
        }
        module.add_function(function);

        let message = validate_module(&module)
            .expect_err("a reactor cannot warn, so it must refuse")
            .to_string();
        assert!(
            message.contains("needs a command module for its NaN diagnostic"),
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
