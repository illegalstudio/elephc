//! Purpose:
//! Preserves exact local contracts after promotion to canonical boxed reference storage.
//!
//! Called from:
//! - Final module lowering after user functions and their reference promotions are complete.
//!
//! Key details:
//! - Protected callbacks borrow the original candidate and return an independently owned box.
//! - Local declarations share the existing reference coordinator and its ownership protocol.

use crate::{ir::{Builder, Function, Immediate, IrHeapKind, IrType, LocalSlotId, Module,
    Op, Ownership, PhpTypePredicate, Terminator}, types::PhpType};

/// Generates one typed callback per selected numeric binding contract, without PHP entry metadata.
pub(super) fn lower_requested(module: &mut Module) -> Result<(), super::LoweringError> {
    let mut contracts = Vec::new();
    for function in module.functions.iter().chain(&module.class_methods).chain(&module.closures)
        .chain(&module.fiber_wrappers).chain(&module.callback_wrappers)
        .chain(&module.runtime_callable_invokers).chain(&module.extern_callback_trampolines)
    {
        for instruction in &function.instructions {
            if instruction.op == Op::BindLocalRefContract && !contracts.contains(&instruction.result_php_type) {
                contracts.push(instruction.result_php_type.clone());
            }
        }
    }
    contracts.sort_by_key(ToString::to_string);
    if contracts.is_empty() { return Ok(()); }
    super::reference_property_validators::enable_native_reference_validation(module)?;
    for (index, contract) in contracts.into_iter().enumerate() {
        let message = module.data.intern_string(&format!(
            "--strict-locals requires {contract} without implicit conversion"));
        module.add_function(lower_callback(index, contract, message));
    }
    Ok(())
}

/// Checks original PHP kinds before acquiring the independently owned successful candidate.
fn lower_callback(index: usize, contract: PhpType, message: crate::ir::DataId) -> Function {
    let boxed = IrType::Heap(IrHeapKind::Mixed);
    let mut function = super::reference_property_validators::new_callback(
        format!("__eir_reference_local_{index}"));
    function.native_local_reference_contract = Some(contract.clone());
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", Vec::new());
    let accepted = builder.create_named_block("accepted", Vec::new());
    let rejected = builder.create_named_block("rejected", Vec::new());
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let original = builder.emit(Op::LoadLocal, Vec::new(),
        Some(Immediate::LocalSlot(LocalSlotId::from_raw(1))), boxed, PhpType::Mixed,
        Ownership::Borrowed).expect("original candidate is borrowed");
    let kinds = crate::semantics::conversions::implicit_contract(&contract)
        .expect("checked numeric local has an exact membership contract");
    use elephc_semantics::conversions::ValueKind;
    let mut condition = None;
    for (kind, predicate) in [(ValueKind::Integer, Some(PhpTypePredicate::Int)),
        (ValueKind::Float, Some(PhpTypePredicate::Float)), (ValueKind::Null, None)]
    {
        if !kinds.allows(kind) { continue; }
        let (op, immediate) = predicate.map_or((Op::IsNull, None),
            |predicate| (Op::TypePredicate, Some(Immediate::TypePredicate(predicate))));
        let next = builder.emit(op, vec![original], immediate, IrType::I64, PhpType::Bool,
            Ownership::NonHeap).expect("kind predicate has a boolean result");
        condition = Some(match condition {
            None => next,
            Some(previous) => builder.emit(Op::IBitOr, vec![previous, next], None,
                IrType::I64, PhpType::Bool, Ownership::NonHeap).expect("kind union is boolean"),
        });
    }
    builder.terminate(Terminator::CondBr { cond: condition.expect("numeric contract has a member"),
        then_target: accepted, then_args: Vec::new(), else_target: rejected, else_args: Vec::new() });
    builder.position_at_end(rejected);
    builder.emit(Op::ThrowTypeError, Vec::new(), Some(Immediate::Data(message)),
        IrType::Void, PhpType::Void, Ownership::NonHeap);
    builder.terminate(Terminator::Unreachable);
    builder.position_at_end(accepted);
    let candidate = builder.emit(Op::MixedClone, vec![original], None, boxed, PhpType::Mixed,
        Ownership::Owned).expect("successful candidate owns its box");
    builder.terminate(Terminator::Return { value: Some(candidate) });
    function
}
