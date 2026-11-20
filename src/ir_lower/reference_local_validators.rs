//! Purpose:
//! Preserves local value contracts after promotion to canonical boxed reference storage.
//!
//! Called from:
//! - Final module lowering after user functions and their reference promotions are complete.
//!
//! Key details:
//! - Protected callbacks borrow the original candidate and return an independently owned box.
//! - Local declarations share the existing reference coordinator and its ownership protocol.

use crate::{ir::{Builder, Function, Immediate, IrHeapKind, IrType, LocalSlotId, Module,
    Op, Ownership, PhpTypePredicate, Terminator}, types::PhpType};

/// Exposes current owned value bindings through the canonical reference snapshot protocol.
pub(super) fn prepare_eval_bindings(ctx: &mut super::context::LoweringContext<'_, '_>, span: crate::span::Span) {
    let mut sources = ctx.local_store_contracts.eval_sources.iter()
        .filter(|((scope, at, name), _)| scope == &ctx.loop_storage_scope && *at == span
            && ctx.local_slots.get(name).is_some_and(|slot|
                ctx.builder.local_kind(*slot) == crate::ir::LocalKind::PhpLocal)
            && !crate::names::is_generated_local_name(name))
        .map(|((_, _, name), (ty, declared))| (name.clone(), ty.clone(), *declared))
        .collect::<Vec<_>>();
    sources.sort_by(|left, right| left.0.cmp(&right.0));
    for (name, ty, declared) in sources {
        let slot = ctx.local_slots[&name];
        super::local_binding_state::when_native_bound(ctx, slot, span, |ctx| {
            ctx.declare_eval_scope_local();
            let name = ctx.intern_global_name(&name);
            ctx.emit_void(Op::EvalScopeBindLocalType, Vec::new(),
                Some(Immediate::EvalScopeStore(Box::new(crate::ir::EvalScopeStoreImmediate {
                    name, contract: Some(ty), role: if declared { crate::ir::EvalBindingRole::DeclaredSource }
                        else { crate::ir::EvalBindingRole::Source },
                }))), Op::EvalScopeBindLocalType.default_effects(), Some(span));
        });
    }
    let mut names = ctx.local_slots.keys().filter(|name| {
        !crate::names::is_generated_local_name(name)
            && ctx.local_store_contracts.get(&(ctx.loop_storage_scope.clone(), span, (*name).clone()))
                .is_some_and(|contract| contract.feature == elephc_semantics::StrictFeature::Locals)
            && ctx.local_can_own_reference_cell(name)
            && (!ctx.is_ref_bound_local(name) || ctx.ref_cell_owner_slot(name).is_some_and(|owner|
                (ctx.slot_is_initialized(owner) || ctx.builder.local_php_type(owner) == PhpType::Mixed)
                    && crate::types::canonical_property_reference_payload(&ctx.builder.local_php_type(owner)).is_some()))
    }).cloned().collect::<Vec<_>>();
    names.sort();
    for name in names {
        let Some(contract) = ctx.local_store_contracts.get(
            &(ctx.loop_storage_scope.clone(), span, name.clone())).cloned() else { continue; };
        let slot = ctx.local_slots[&name];
        super::local_binding_state::when_native_bound(ctx, slot, span, |ctx| {
            if ctx.is_ref_bound_local(&name) && ctx.ref_cell_owner_slot(&name)
                .is_some_and(|owner| ctx.slot_is_initialized(owner)) {
                ctx.bind_local_reference_contract(&name, Some(span));
            } else { ctx.promote_local_mixed_ref_cell(&name, Some(span)); }
            ctx.declare_eval_scope_local();
            let name = ctx.intern_global_name(&name);
            ctx.emit_void(Op::EvalScopeBindLocalType, Vec::new(),
                Some(Immediate::EvalScopeStore(Box::new(crate::ir::EvalScopeStoreImmediate {
                    name, contract: Some(contract.ty), role: crate::ir::EvalBindingRole::Contract,
                }))), Op::EvalScopeBindLocalType.default_effects(), Some(span));
        });
    }
}

/// Publishes a caller's exact binding before a typed reference can observe widened frame storage.
pub(super) fn prepare_argument_binding(ctx: &mut super::context::LoweringContext<'_, '_>,
    arg: &crate::parser::ast::Expr) {
    let mut source = arg;
    while let crate::parser::ast::ExprKind::NamedArg { value, .. }
        | crate::parser::ast::ExprKind::ErrorSuppress(value) = &source.kind { source = value; }
    let crate::parser::ast::ExprKind::Variable(name) = &source.kind else { return; };
    ctx.prepare_eval_native_binding_store(name, source.span);
    if ctx.local_store_contracts.get(&(ctx.loop_storage_scope.clone(), source.span, name.clone()))
        .is_some_and(|contract| contract.feature == elephc_semantics::StrictFeature::Locals)
        && ctx.local_is_promotable_to_ref_cell(name)
        && (!ctx.is_ref_bound_local(name) || ctx.local_type(name).codegen_repr() == PhpType::Mixed)
    { ctx.promote_local_mixed_ref_cell(name, Some(source.span)); }
}

/// Generates one typed callback per selected binding contract, without PHP entry metadata.
pub(super) fn lower_requested(module: &mut Module) -> Result<(), super::LoweringError> {
    let mut contracts: Vec<PhpType> = Vec::new();
    let mut has_binding_copy = false;
    let mut has_binding_origin = false;
    for function in module.functions.iter().chain(&module.class_methods).chain(&module.closures)
        .chain(&module.fiber_wrappers).chain(&module.callback_wrappers)
        .chain(&module.runtime_callable_invokers).chain(&module.extern_callback_trampolines)
    {
        for instruction in &function.instructions {
            has_binding_copy |= instruction.op == Op::EvalScopeCopyBindingType;
            has_binding_origin |= matches!(instruction.op, Op::EvalScopeBindLocalType | Op::EvalScopeStartBinding);
            let contract = match &instruction.immediate {
                Some(Immediate::EvalScopeStore(metadata))
                    if metadata.role == crate::ir::EvalBindingRole::Contract => metadata.contract.as_ref(),
                _ if instruction.op == Op::BindLocalRefContract => Some(&instruction.result_php_type),
                _ if instruction.op == Op::EvalScopeCopyBindingType =>
                    crate::semantics::conversions::inferred_binding_contract(&instruction.result_php_type)
                        .as_ref().map(|_| &instruction.result_php_type),
                _ => None,
            };
            if let Some(contract) = contract.filter(|contract| !contracts.contains(contract)) {
                contracts.push(contract.clone());
            }
        }
    }
    contracts.sort_by_key(ToString::to_string);
    if contracts.is_empty() && !has_binding_copy && !has_binding_origin { return Ok(()); }
    super::reference_property_validators::enable_native_reference_validation(module)?;
    for (index, contract) in contracts.into_iter().enumerate() {
        let message = module.data.intern_string(&format!(
            "--strict-locals requires {contract} without implicit conversion"));
        let function = lower_callback(index, contract, message, &mut module.data);
        module.add_function(function);
    }
    Ok(())
}

/// Checks original PHP kinds before acquiring the independently owned successful candidate.
fn lower_callback(index: usize, contract: PhpType, message: crate::ir::DataId,
    data: &mut crate::ir::DataPool) -> Function {
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
        .expect("checked local has an exact membership contract");
    use elephc_semantics::conversions::ValueKind;
    let mut conditions = Vec::new();
    let false_only = crate::semantics::conversions::only_false(&contract);
    for (kind, predicate) in [(ValueKind::Integer, Some(PhpTypePredicate::Int)),
        (ValueKind::Float, Some(PhpTypePredicate::Float)),
        (ValueKind::Boolean, Some(PhpTypePredicate::Bool)),
        (ValueKind::String, Some(PhpTypePredicate::String)), (ValueKind::Null, None)]
    {
        if !kinds.allows(kind) || (false_only && kind == ValueKind::Boolean) { continue; }
        let (op, immediate) = predicate.map_or((Op::IsNull, None),
            |predicate| (Op::TypePredicate, Some(Immediate::TypePredicate(predicate))));
        let next = builder.emit(op, vec![original], immediate, IrType::I64, PhpType::Bool,
            Ownership::NonHeap).expect("kind predicate has a boolean result");
        conditions.push(next);
    }
    if false_only {
        let no = builder.emit(Op::ConstBool, Vec::new(), Some(Immediate::Bool(false)),
            IrType::I64, PhpType::Bool, Ownership::NonHeap).expect("false has a scalar value");
        conditions.push(builder.emit(Op::StrictEq, vec![original, no], None,
            IrType::I64, PhpType::Bool, Ownership::NonHeap).expect("literal membership is boolean"));
    }
    for class in crate::semantics::conversions::nominal_members(&contract) {
        let (op, immediate) = if class.is_empty() || class.eq_ignore_ascii_case("object") {
            (Op::TypePredicate, Immediate::TypePredicate(PhpTypePredicate::Object))
        } else { (Op::InstanceOf, Immediate::Data(data.intern_class_name(class))) };
        conditions.push(builder.emit(op, vec![original], Some(immediate),
            IrType::I64, PhpType::Bool, Ownership::NonHeap).expect("nominal membership is boolean"));
    }
    for constraint in crate::semantics::conversions::runtime_constraints(&contract) {
        use crate::semantics::conversions::RuntimeConstraint;
        let (op, immediate, effects) = match constraint {
            RuntimeConstraint::Callable => {
                let target = crate::ir::RuntimeFnId::IsCallable;
                (Op::RuntimeCall, Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target)), target.effects())
            }
            constraint => (Op::TypePredicate, Immediate::TypePredicate(match constraint {
                RuntimeConstraint::Array => PhpTypePredicate::Array,
                RuntimeConstraint::Iterable => PhpTypePredicate::Iterable,
                RuntimeConstraint::Resource => PhpTypePredicate::Resource,
                RuntimeConstraint::Callable => unreachable!(),
            }), Op::TypePredicate.default_effects()),
        };
        conditions.push(builder.emit_with_effects(op, vec![original], Some(immediate),
            IrType::I64, PhpType::Bool, Ownership::NonHeap, effects, None)
            .expect("structural membership is boolean"));
    }
    let condition = conditions.into_iter().reduce(|left, right|
        builder.emit(Op::IBitOr, vec![left, right], None, IrType::I64,
            PhpType::Bool, Ownership::NonHeap).expect("membership union is boolean"))
        .expect("binding contract has a member");
    builder.terminate(Terminator::CondBr { cond: condition,
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
