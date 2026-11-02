//! Purpose:
//! Emits repeated runtime-name Mixed property dispatch once per lexical scope and fetch mode.
//!
//! Called from:
//! - Module emission before user bodies and dynamic Mixed property-read lowering.
//!
//! Key details:
//! - Each ordinary synthetic EIR function reuses the existing DynamicPropGet implementation.
//! - Caller operands are already evaluated; the helper borrows them and returns one owned cell.
//! - Literal names, hook bodies, quiet fetches and single-site scopes retain their current paths.

use std::collections::HashMap;

use super::{abi, block_emit, context::FunctionContext, data_section::DataSection,
    emit::Emitter, shared_state::SharedCodegenState, Result};
use crate::ir::{Builder, Function, FunctionParam, Immediate, InstId, Instruction, IrHeapKind,
    IrType, LocalKind, Module, Op, Ownership, PropertyFetchMode, Terminator, ValueDef, ValueId};
use crate::types::PhpType;

const PREFIX: &str = "_eir_shared_property_read_";

/// One repeated dispatch whose access scope, fetch semantics and entry label agree.
#[derive(Clone)]
pub(super) struct ReadHelper {
    scope: Option<String>,
    mode: PropertyFetchMode,
    label: String,
}

/// Returns the instruction's fetch mode without changing the ordinary-read default.
fn fetch_mode(inst: &Instruction) -> PropertyFetchMode {
    match inst.immediate {
        Some(Immediate::PropertyFetchMode(mode)) => mode,
        _ => PropertyFetchMode::Read,
    }
}

/// Selects only boxed runtime-name reads whose result has the helper's owned Mixed ABI.
fn is_shared_site(function: &Function, inst: &Instruction) -> bool {
    if function.name.starts_with(PREFIX)
        || function.name.contains("::__propget_") || function.name.contains("::__propset_")
        || inst.op != Op::DynamicPropGet || fetch_mode(inst) == PropertyFetchMode::QuietRead
        || inst.result_php_type.codegen_repr() != PhpType::Mixed
    { return false; }
    let [object, name] = inst.operands.as_slice() else { return false; };
    if !function.value(*object).is_some_and(|value|
        matches!(value.php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)))
        || !function.value(*name).is_some_and(|value| value.php_type.codegen_repr() == PhpType::Str)
    { return false; }
    !function.value(*name).is_some_and(|value| match value.def {
        ValueDef::Instruction { inst, .. } => function.instruction(inst).is_some_and(|inst| inst.op == Op::ConstStr),
        _ => false,
    })
}

/// Computes stable helper entries once, so lowering each read does not rescan the module.
fn prepare(module: &Module, shared: &mut SharedCodegenState) {
    if shared.property_read_helpers.is_some() { return; }
    let mut counts = HashMap::new();
    for function in module.functions.iter().chain(&module.class_methods).chain(&module.closures) {
        for (index, inst) in function.instructions.iter().enumerate() {
            if !is_shared_site(function, inst) { continue; }
            let scope = function.inlined_scopes.get(&InstId::from_raw(index as u32))
                .cloned().or_else(|| function.lexical_class.clone());
            *counts.entry((scope, fetch_mode(inst))).or_insert(0_usize) += 1;
        }
    }
    let mut keys = counts.into_iter().filter_map(|(key, count)| (count >= 2).then_some(key))
        .collect::<Vec<_>>();
    keys.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.as_eir().cmp(right.1.as_eir())));
    shared.property_read_helpers = Some(keys.into_iter().enumerate().map(|(index, (scope, mode))|
        ReadHelper { scope, mode, label: format!("{PREFIX}{index}") }).collect());
}

/// Finds the same preplanned helper at every eligible call site, excluding helper recursion.
pub(super) fn helper_label(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Option<String> {
    if !is_shared_site(ctx.function, inst) { return None; }
    prepare(ctx.module, ctx.shared);
    let scope = ctx.lexical_class();
    ctx.shared.property_read_helpers.as_ref()?.iter().find(|helper|
        helper.scope.as_deref() == scope && helper.mode == fetch_mode(inst)).map(|helper| helper.label.clone())
}

/// Calls the ordinary PHP ABI after staging operands without re-evaluating either expression.
pub(super) fn emit_call(ctx: &mut FunctionContext<'_>, object: ValueId, name: ValueId, label: &str) -> Result<()> {
    let result = abi::int_result_reg(ctx.emitter);
    ctx.load_value_to_reg(object, result)?;
    abi::emit_push_reg(ctx.emitter, result);
    let (ptr, len) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(name, ptr, len)?;
    abi::emit_push_reg_pair(ctx.emitter, ptr, len);
    for (index, offset) in [(0, 16), (1, 0), (2, 8)] {
        let argument = abi::int_arg_reg_name(ctx.emitter.target, index);
        abi::emit_load_temporary_stack_slot(ctx.emitter, argument, offset);
    }
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    abi::emit_call_label(ctx.emitter, label);
    Ok(())
}

/// Builds a borrowed two-parameter EIR function around the original property-read opcode.
fn helper_function(helper: &ReadHelper) -> Function {
    let mixed = IrType::Heap(IrHeapKind::Mixed);
    let mut function = Function::new(helper.label.clone(), mixed, PhpType::Mixed);
    function.flags.is_synthetic = true;
    function.lexical_class = helper.scope.clone();
    for (name, ir_type, php_type) in [("object", mixed, PhpType::Mixed), ("name", IrType::Str, PhpType::Str)] {
        function.params.push(FunctionParam { name: name.to_string(), ir_type, php_type: php_type.clone(),
            by_ref: false, variadic: false });
        let slot = function.add_local(Some(name.to_string()), ir_type, php_type, LocalKind::BorrowedTemp);
        function.no_epilogue_cleanup_slots.insert(slot);
    }
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", Vec::new());
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let object = builder.emit_load_local(crate::ir::LocalSlotId::from_raw(0), mixed, PhpType::Mixed);
    let name = builder.emit_load_local(crate::ir::LocalSlotId::from_raw(1), IrType::Str, PhpType::Str);
    builder.set_value_ownership(object, Ownership::Borrowed);
    builder.set_value_ownership(name, Ownership::Borrowed);
    let result = builder.emit(Op::DynamicPropGet, vec![object, name], Some(Immediate::PropertyFetchMode(helper.mode)),
        mixed, PhpType::Mixed, Ownership::Owned).expect("property read produces a value");
    builder.terminate(Terminator::Return { value: Some(result) });
    function
}

/// Emits the same dispatch bodies with ordinary frame, argument and exception cleanup machinery.
pub(super) fn emit_helpers(module: &Module, emitter: &mut Emitter, data: &mut DataSection,
    shared: &mut SharedCodegenState) -> Result<()> {
    prepare(module, shared);
    let helpers = shared.property_read_helpers.clone().unwrap_or_default();
    for helper in helpers {
        let function = helper_function(&helper);
        crate::ir::validate_function(&function).map_err(|error|
            super::CodegenIrError::invalid_module(format!("shared property read: {error:?}")))?;
        emitter.comment(&format!("shared property read: scope={} mode={}",
            helper.scope.as_deref().unwrap_or("global"), helper.mode.as_eir()));
        block_emit::emit_synthetic_function_with_label(module, &function, &helper.label,
            emitter, data, shared, false)?;
    }
    Ok(())
}
