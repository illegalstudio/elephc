//! Purpose:
//! Preserves magic accessors when initialized-read policy observes a removed declared slot.
//!
//! Called from:
//! - Named physical property reads and intentional property probes.
//!
//! Key details:
//! - The existing property marker distinguishes initialized null from a removed slot.
//! - Shared owner pins keep receivers alive across accessor side effects and throws.

use super::*;

/// Selects the ordinary slot or getter without asking an existence accessor first.
pub(super) fn magic_read(ctx: &mut LoweringContext<'_, '_>, object: LoweredValue,
    property: &str, op: Op, expr: &Expr,
) -> Option<LoweredValue> {
    if op != Op::PropGet { return None; }
    select(ctx, object, property, None, expr)
}

/// Selects an initialized slot or the language construct's magic existence protocol.
pub(super) fn magic_probe(ctx: &mut LoweringContext<'_, '_>, object: LoweredValue,
    property: &str, mode: PropertyFetchMode, expr: &Expr,
) -> Option<LoweredValue> {
    select(ctx, object, property, Some(mode), expr)
}

/// Shares source eligibility, presence branching and receiver retirement for named accesses.
fn select(ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    probe: Option<PropertyFetchMode>, expr: &Expr,
) -> Option<LoweredValue> {
    if !crate::ir_lower::initialization::enabled(ctx) { return None; }
    let ty = ctx.builder.value_php_type(object.value);
    let (class, nullable) = singular_object_class(&ty)?;
    if nullable { return None; }
    if class_declares_hook_accessor(ctx, object.value, &property_hook_get_method(property))
        || !matches!(crate::types::resolve_property_name(ctx.classes, class, property, ctx.current_class.as_deref()),
            crate::types::PropertyNameResolution::Visible | crate::types::PropertyNameResolution::ScopePrivate { .. }) {
        return None;
    }
    let info = ctx.classes.get(class)?;
    if info.declared_properties.contains(property) || !info.methods.contains_key("__get")
        || !info.properties.iter().any(|(name, _)| name == property) { return None; }
    let has_isset = info.methods.contains_key("__isset");
    let (object, owner) = root_value_for_later_use(ctx, object, expr.span);
    let receiver = ctx.emit_value(Op::Borrow, vec![object.value], None, ty,
        Op::Borrow.default_effects(), Some(expr.span));
    ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
    let data = ctx.intern_string(property);
    let initialized = ctx.emit_value(Op::PropInitialized, vec![receiver.value],
        Some(Immediate::Data(data)), PhpType::Bool, Op::PropInitialized.default_effects(), Some(expr.span));
    let result = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let present = ctx.builder.create_named_block("property.initialized", Vec::new());
    let missing = ctx.builder.create_named_block("property.unset.accessor", Vec::new());
    let merge = ctx.builder.create_named_block("property.read.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: initialized.value,
        then_target: present, then_args: Vec::new(), else_target: missing, else_args: Vec::new() });
    let mut join = crate::ir_lower::stmt::ExprBranchJoin::at_split(ctx);
    ctx.builder.position_at_end(present);
    join.enter_arm(ctx);
    let value = super::property_access::lower_direct_property_read(ctx, receiver, property, Op::PropGet, expr);
    store_value_into_temp(ctx, &result, PhpType::Mixed, value, expr.span);
    join.leave_arm(ctx);
    ctx.builder.position_at_end(missing);
    join.enter_arm(ctx);
    let value = match probe {
        None => accessor(ctx, receiver, property, "__get", expr),
        Some(mode) => missing_probe(ctx, receiver, property, mode, has_isset, expr),
    };
    store_value_into_temp(ctx, &result, PhpType::Mixed, value, expr.span);
    join.leave_arm(ctx);
    join.finish(ctx, merge, expr.span);
    let result = take_owned_temp(ctx, &result, expr.span);
    if let Some(owner) = owner {
        super::call_operand_owners::retire_owned_call_operand_preserving_result(ctx, owner, result, expr.span);
    }
    Some(result)
}

/// Calls one accessor through ordinary method lowering with a side-effect-free name operand.
fn accessor(ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    method: &str, expr: &Expr,
) -> LoweredValue {
    let name = Expr::new(ExprKind::StringLiteral(property.to_string()), expr.span);
    lower_method_call_with_receiver(ctx, object, method, &[name], Op::MethodCall, expr)
}

/// Existence probes ask __isset once; coalescing can call a getter without an existence hook.
fn missing_probe(ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    mode: PropertyFetchMode, has_isset: bool, expr: &Expr,
) -> LoweredValue {
    if !has_isset {
        return if mode == PropertyFetchMode::Probe { accessor(ctx, object, property, "__get", expr) }
            else { lower_boxed_null(ctx, expr) };
    }
    let exists = accessor(ctx, object, property, "__isset", expr);
    let exists = ctx.condition_truthy_consuming(exists, Some(expr.span));
    let result = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let yes = ctx.builder.create_named_block("property.probe.exists", Vec::new());
    let no = ctx.builder.create_named_block("property.probe.absent", Vec::new());
    let merge = ctx.builder.create_named_block("property.probe.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr { cond: exists.value,
        then_target: yes, then_args: Vec::new(), else_target: no, else_args: Vec::new() });
    let mut join = crate::ir_lower::stmt::ExprBranchJoin::at_split(ctx);
    ctx.builder.position_at_end(yes);
    join.enter_arm(ctx);
    let value = if mode == PropertyFetchMode::Isset { lower_bool_literal(ctx, true, expr) }
        else { accessor(ctx, object, property, "__get", expr) };
    store_value_into_temp(ctx, &result, PhpType::Mixed, value, expr.span);
    join.leave_arm(ctx);
    ctx.builder.position_at_end(no);
    join.enter_arm(ctx);
    let value = lower_boxed_null(ctx, expr);
    store_value_into_temp(ctx, &result, PhpType::Mixed, value, expr.span);
    join.leave_arm(ctx);
    join.finish(ctx, merge, expr.span);
    take_owned_temp(ctx, &result, expr.span)
}
