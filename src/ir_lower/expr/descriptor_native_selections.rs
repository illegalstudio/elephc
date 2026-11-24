//! Purpose:
//! Chooses native XML argument read context from a runtime callable's passing modes.
//!
//! Called from:
//! - Signature-unknown direct descriptor call argument construction.
//!
//! Key details:
//! - Modes are captured before argument effects; only the selected reference edge creates nodes.
//! - Empty managed cells are published before the argument container to keep unwind records LIFO.

use super::*;

/// One argument's preselected passing mode and independently owned writable cell.
pub(super) struct SelectionArgument {
    by_reference: LoweredValue,
    alias: String,
}

/// Captures modes and publishes possible reference cells before evaluating any argument.
pub(super) fn prepare(
    ctx: &mut LoweringContext<'_, '_>, callback: &mut RootedDescriptorCallback,
    args: &[Expr], span: Span,
) -> Vec<Option<SelectionArgument>> {
    // Compiler-generated helpers (`__elephc_usort_mixed`, ...) hand their callback elements of
    // plain PHP arrays, never SimpleXML selections, so the passing-mode query is provably useless.
    if ctx.owner_name().starts_with("__elephc_")
        || !args.iter().any(|arg| may_be_native_selection_reference_argument(ctx, arg))
    {
        return (0..args.len()).map(|_| None).collect();
    }
    // The passing-mode query asks the interpreter's bridge only for a callable the interpreter
    // owns. A closed world has none: the query is answered from the compiled callable's own
    // descriptor, so no eval context is declared (it would link the whole interpreter).
    if !crate::closed_world::is_enabled() {
        ctx.declare_eval_context_local();
    }
    let ty = ctx.builder.value_php_type(callback.value.value);
    let borrowed = ctx.emit_value(Op::Borrow, vec![callback.value.value], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    let boxed = ctx.box_value_as_mixed(borrowed, PhpType::Mixed, Some(span));
    let (boxed, owner) = root_owned_call_operand(ctx, boxed, span);
    let modes: Vec<_> = args.iter().enumerate().map(|(index, arg)| {
        if !may_be_native_selection_reference_argument(ctx, arg) { return None; }
        let name = match &arg.kind { ExprKind::NamedArg { name, .. } => name.as_str(), _ => "" };
        let name_expr = Expr::new(ExprKind::StringLiteral(name.to_string()), arg.span);
        let name = lower_expr(ctx, &name_expr);
        let position = ctx.builder.emit_const_i64(index as i64);
        Some(ctx.emit_value(Op::RuntimeCall, vec![boxed.value, name.value, position],
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::CallableParameterIsByRef)),
            PhpType::Bool, Op::RuntimeCall.default_effects(), Some(arg.span)))
    }).collect();
    if let Some(owner) = owner { retire_owned_call_operand(ctx, owner, span); }
    modes.into_iter().zip(args).map(|(mode, arg)| mode.map(|by_reference| {
        let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
        // Start every execution of this site from a pristine alias. A previous execution (a loop
        // body) may have left it reference-bound to a cell that a throwing callee's unwind
        // already retired, and the store below would then go through that stale cell.
        let alias_slot = ctx.local_slots[&alias];
        for op in [Op::UnsetLocal, Op::ZeroLocalSlot] {
            ctx.emit_void(op, Vec::new(), Some(Immediate::LocalSlot(alias_slot)),
                op.default_effects(), Some(arg.span));
        }
        let null = ctx.builder.emit_const_null();
        let null = LoweredValue { value: null, ir_type: IrType::I64 };
        store_value_into_temp(ctx, &alias, PhpType::Mixed, null, arg.span);
        ctx.promote_local_mixed_ref_cell(&alias, Some(arg.span));
        let slot = ctx.ref_cell_owner_slot(&alias).expect("managed selection cell");
        register_owned_call_operand(ctx, slot, arg.span);
        callback.selection_aliases.push((alias.clone(), arg.span));
        SelectionArgument { by_reference, alias }
    })).collect()
}

/// Evaluates exactly one argument in the passing context captured for its selected target.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, arg: &Expr, selection: &SelectionArgument,
) -> LoweredValue {
    let arg = if let ExprKind::NamedArg { value, .. } = &arg.kind { value.as_ref() } else { arg };
    let result = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let initial = ctx.initialized_slots_snapshot();
    let reference = ctx.builder.create_named_block("call.xml.reference", Vec::new());
    let read = ctx.builder.create_named_block("call.xml.value", Vec::new());
    let merge = ctx.builder.create_named_block("call.xml.argument", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: selection.by_reference.value,
        then_target: reference, then_args: Vec::new(), else_target: read, else_args: Vec::new() });
    ctx.builder.position_at_end(reference);
    let value = property_access::lower_runtime_reference_argument(ctx, arg, &selection.alias);
    store_value_into_temp(ctx, &result, PhpType::Mixed, value, arg.span);
    branch_to(ctx, merge);
    let reference_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(read);
    ctx.restore_initialized_slots(initial.clone());
    let value = lower_expr(ctx, arg);
    store_value_into_temp(ctx, &result, PhpType::Mixed, value, arg.span);
    branch_to(ctx, merge);
    let read_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial,
        reference_initialized, true, read_initialized, true));
    take_owned_temp(ctx, &result, arg.span)
}
