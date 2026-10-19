//! Purpose:
//! Lowers `__elephc_dynamic_property_push($obj, $name, $value)`, the parser's desugar of PHP's
//! `$obj->{$name}[] = $value` (Mailable.php's `$this->{$property}[] = [...]`).
//!
//! Called from:
//! - `crate::ir_lower::expr::dynamic_class_access::lower_dynamic_class_member_call()`.
//!
//! Key details:
//! - The object, the name and the value are evaluated once, in that order. On a receiver whose
//!   class is known, the name is matched (case-sensitively, like PHP property names) against
//!   every property visible and writable from the lowering scope, and each arm pushes onto THAT
//!   slot with its own static type: a list slot pushes in place (the checker refined its element
//!   type, see `refine_array_slots_for_dynamic_push`), an associative or boxed slot goes through
//!   a read/append/write-back with its static name, an ArrayAccess object slot gets
//!   `offsetSet(null, $value)`. No slot changes representation, so the class's other code is
//!   lowered exactly as without the push.
//! - A slot PHP cannot append to throws PHP's own `Error` (`[] operator not supported for
//!   strings`, `Cannot use a scalar value as an array`, `Cannot use object of type X as array`,
//!   `Cannot modify readonly property`); a slot whose storage cannot take the new array (a `bool`
//!   or never-assigned slot, a declared element type that refuses the value) throws an explicit
//!   elephc `Error` instead of storing into the wrong representation.
//! - An inaccessible property throws PHP's visibility `Error`; a hooked one an explicit elephc
//!   `Error`. Any other name on a class that declares properties would create a dynamic
//!   property, which throws an explicit elephc `Error` (the runtime-named access would store into
//!   a declared slot's raw representation). Only a class without declared properties (stdClass)
//!   or a receiver of unknown class takes the generic runtime-named read/append/write-back.

use super::dynamic_class_access::{conditional_branch, literal, scope_can_access, throw_error_and_merge};
use super::*;

/// What one name arm does with its slot.
enum SlotPush {
    /// `$obj->p[] = v` on a list slot.
    InPlace,
    /// `$t = $obj->p; $t[] = v; $obj->p = $t;` with a local of the slot's type.
    WriteBack(PhpType),
    /// `$t = $obj->p; $t[] = v;` on an ArrayAccess object (`offsetSet(null, v)`).
    ArrayAccess(PhpType),
    /// `throw new Error(message)`.
    Throw(String),
}

/// Expands `__elephc_dynamic_property_push($obj, $name, $value)`.
pub(super) fn lower_dynamic_property_push(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let [object, name, value] = args else {
        return None;
    };
    let span = expr.span;
    let object_local = lower_into_local(ctx, object, span);
    let object_type = ctx.local_type(&object_local);
    let name_local = ctx.declare_synthetic_php_local(PhpType::Str);
    store_expr_into_temp(
        ctx,
        &name_local,
        PhpType::Str,
        &Expr::new(ExprKind::Cast { target: CastType::String, expr: Box::new(name.clone()) }, span),
        span,
    );
    let value_local = lower_into_local(ctx, value, span);
    let value_type = ctx.local_type(&value_local);

    let result_temp = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let merge = ctx.builder.create_named_block("dyn_prop_push.merge", Vec::new());
    let split_initialized = ctx.initialized_slots_snapshot();
    let var = |name: &str| Expr::new(ExprKind::Variable(name.to_string()), span);

    if let PhpType::Object(class_name) = object_type.codegen_repr() {
        let class_name = class_name.trim_start_matches('\\').to_string();
        // The receiver's own visible properties first, then the ones only a subclass declares
        // (deepest subclass first), each guarded by `instanceof` on the runtime class.
        let mut arms: Vec<(String, Option<String>, SlotPush)> = slot_pushes(ctx, &class_name, &value_type)
            .into_iter()
            .map(|(property, push)| (property, None, push))
            .collect();
        let known: HashSet<String> = arms.iter().map(|(property, _, _)| property.clone()).collect();
        for subclass in subclasses_deepest_first(ctx, &class_name) {
            for (property, push) in slot_pushes(ctx, &subclass, &value_type) {
                if !known.contains(&property) {
                    arms.push((property, Some(subclass.clone()), push));
                }
            }
        }
        for (property, subclass, push) in arms {
            let arm = ctx.builder.create_named_block("dyn_prop_push.arm", Vec::new());
            let skip = ctx.builder.create_named_block("dyn_prop_push.skip", Vec::new());
            // `strcmp($name, 'p') === 0`: PHP property names are case-sensitive.
            let compare = Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("strcmp"),
                    args: vec![var(&name_local), literal(&property, span)],
                },
                span,
            );
            let mut condition = Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(compare),
                    op: BinOp::StrictEq,
                    right: Box::new(Expr::new(ExprKind::IntLiteral(0), span)),
                },
                span,
            );
            if let Some(subclass) = &subclass {
                condition = Expr::new(
                    ExprKind::BinaryOp {
                        left: Box::new(condition),
                        op: BinOp::And,
                        right: Box::new(Expr::new(
                            ExprKind::InstanceOf {
                                value: Box::new(var(&object_local)),
                                target: InstanceOfTarget::Name(Name::from(subclass.clone())),
                            },
                            span,
                        )),
                    },
                    span,
                );
            }
            let matches = lower_expr(ctx, &condition);
            conditional_branch(ctx, matches.value, arm, skip);
            ctx.builder.position_at_end(arm);
            ctx.restore_initialized_slots(split_initialized.clone());
            // A subclass-only slot is addressed through a local of the subclass's type.
            let receiver_local = match &subclass {
                Some(subclass) => {
                    let ty = PhpType::Object(subclass.clone());
                    let local = ctx.declare_synthetic_php_local(ty.clone());
                    store_expr_into_temp(ctx, &local, ty, &var(&object_local), span);
                    local
                }
                None => object_local.clone(),
            };
            let object_local = receiver_local;
            let slot = Expr::new(
                ExprKind::PropertyAccess { object: Box::new(var(&object_local)), property: property.clone() },
                span,
            );
            match push {
                SlotPush::Throw(message) => {
                    throw_error_and_merge(ctx, "Error", literal(&message, span), &result_temp, merge, span);
                    ctx.builder.position_at_end(skip);
                    continue;
                }
                SlotPush::InPlace => crate::ir_lower::stmt::lower_stmt(
                    ctx,
                    &Stmt::new(
                        StmtKind::PropertyArrayPush {
                            object: Box::new(var(&object_local)),
                            property: property.clone(),
                            value: var(&value_local),
                        },
                        span,
                    ),
                ),
                SlotPush::WriteBack(slot_type) => {
                    let container = ctx.declare_synthetic_php_local(slot_type.clone());
                    store_expr_into_temp(ctx, &container, slot_type, &slot, span);
                    push_onto_local(ctx, &container, &value_local, span);
                    crate::ir_lower::stmt::lower_stmt(
                        ctx,
                        &Stmt::new(
                            StmtKind::PropertyAssign {
                                object: Box::new(var(&object_local)),
                                property: property.clone(),
                                value: var(&container),
                            },
                            span,
                        ),
                    );
                }
                SlotPush::ArrayAccess(slot_type) => {
                    let container = ctx.declare_synthetic_php_local(slot_type.clone());
                    store_expr_into_temp(ctx, &container, slot_type, &slot, span);
                    push_onto_local(ctx, &container, &value_local, span);
                }
            }
            store_expr_into_temp(ctx, &result_temp, PhpType::Mixed, &Expr::new(ExprKind::Null, span), span);
            branch_to(ctx, merge);
            ctx.builder.position_at_end(skip);
        }
        ctx.restore_initialized_slots(split_initialized.clone());
    }

    // Any other name on a class that declares properties would create a dynamic property, which
    // the runtime-named access cannot do there without storing into a declared slot's raw
    // representation: refuse it explicitly. A class without declared properties (stdClass, a
    // bag) and a receiver whose class is unknown take the generic runtime-named path.
    let declares_properties = match object_type.codegen_repr() {
        PhpType::Object(class_name) => ctx
            .classes
            .get(class_name.trim_start_matches('\\'))
            .is_some_and(|info| !info.properties.is_empty()),
        _ => false,
    };
    if declares_properties {
        throw_error_and_merge(
            ctx,
            "Error",
            literal(
                "elephc: appending to an undeclared property through a runtime name (`$obj->{$name}[] = v`) is not supported on a class that declares properties; declare the property",
                span,
            ),
            &result_temp,
            merge,
            span,
        );
        ctx.builder.position_at_end(merge);
        return Some(take_owned_temp(ctx, &result_temp, span));
    }
    // The generic runtime-named read, append and write-back.
    let dynamic_slot = Expr::new(
        ExprKind::DynamicPropertyAccess {
            object: Box::new(var(&object_local)),
            property: Box::new(var(&name_local)),
        },
        span,
    );
    let container = ctx.declare_synthetic_php_local(PhpType::Mixed);
    store_expr_into_temp(ctx, &container, PhpType::Mixed, &dynamic_slot, span);
    push_onto_local(ctx, &container, &value_local, span);
    crate::ir_lower::stmt::lower_stmt(
        ctx,
        &Stmt::new(
            StmtKind::ExprStmt(Expr::new(
                ExprKind::Assignment {
                    target: Box::new(dynamic_slot),
                    value: Box::new(var(&container)),
                    result_target: None,
                    prelude: Vec::new(),
                    conditional_value_temp: None,
                },
                span,
            )),
            span,
        ),
    );
    store_expr_into_temp(ctx, &result_temp, PhpType::Mixed, &Expr::new(ExprKind::Null, span), span);
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    Some(take_owned_temp(ctx, &result_temp, span))
}

/// Classes extending `class_name` (not itself), deepest first.
fn subclasses_deepest_first(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Vec<String> {
    let depth = |name: &str| {
        let mut depth = 0usize;
        let mut current = ctx.classes.get(name).and_then(|info| info.parent.clone());
        while let Some(parent) = current {
            depth += 1;
            current = ctx.classes.get(parent.as_str()).and_then(|info| info.parent.clone());
        }
        depth
    };
    let mut subclasses: Vec<String> = ctx
        .classes
        .keys()
        .filter(|other| {
            other.as_str() != class_name
                && super::property_access::class_extends_class(ctx, other, class_name)
        })
        .cloned()
        .collect();
    subclasses.sort_by(|left, right| depth(right).cmp(&depth(left)).then_with(|| left.cmp(right)));
    subclasses
}

/// Evaluates `expr` once into a fresh synthetic local of its own type.
fn lower_into_local(ctx: &mut LoweringContext<'_, '_>, expr: &Expr, span: Span) -> String {
    let lowered = lower_expr(ctx, expr);
    let ty = normalize_value_php_type(ctx.builder.value_php_type(lowered.value));
    let local = ctx.declare_synthetic_php_local(ty.clone());
    store_value_into_temp(ctx, &local, ty, lowered, span);
    local
}

/// Lowers `$container[] = $value` on a synthetic local.
fn push_onto_local(ctx: &mut LoweringContext<'_, '_>, container: &str, value: &str, span: Span) {
    crate::ir_lower::stmt::lower_stmt(
        ctx,
        &Stmt::new(
            StmtKind::ArrayPush {
                array: container.to_string(),
                value: Expr::new(ExprKind::Variable(value.to_string()), span),
            },
            span,
        ),
    );
}

/// Decides, per property the name may select, how the push reaches that slot.
fn slot_pushes(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    value_type: &PhpType,
) -> Vec<(String, SlotPush)> {
    let Some(info) = ctx.classes.get(class_name) else {
        return Vec::new();
    };
    let mut names: Vec<String> = info.properties.iter().map(|(name, _)| name.clone()).collect();
    names.sort();
    names.dedup();
    let mut pushes = Vec::new();
    for property in names {
        let Some((_, (_, slot_type))) = info.visible_property(&property) else { continue };
        let declaring = info
            .property_declaring_classes
            .get(&property)
            .cloned()
            .unwrap_or_else(|| class_name.to_string());
        let readable = scope_can_access(ctx, info.property_visibilities.get(&property), &declaring);
        let writable = scope_can_access(
            ctx,
            info.property_set_visibilities
                .get(&property)
                .or(info.property_visibilities.get(&property)),
            &declaring,
        );
            let hooked = info.property_hooks.get(&property).is_some_and(|hooks| hooks.get || hooks.set);
        let visibility = if readable { info.property_set_visibilities.get(&property) } else { info.property_visibilities.get(&property) };
        let push = if !readable || !writable {
            SlotPush::Throw(format!(
                "Cannot {} {} property {class_name}::${property}",
                if readable { "modify" } else { "access" },
                super::dynamic_class_access::visibility_word(visibility),
            ))
        } else if hooked {
            SlotPush::Throw(format!(
                "elephc: appending through a runtime property name (`$obj->{{$name}}[] = v`) to the hooked property {class_name}::${property} is not supported; name the property"
            ))
        } else if info.readonly_properties.contains(&property) {
            SlotPush::Throw(format!("Cannot modify readonly property {class_name}::${property}"))
        } else {
            slot_push(ctx, info.visible_property_is_declared(&property), slot_type, value_type)
        };
        pushes.push((property, push));
    }
    pushes
}

/// How a push reaches one slot of `slot_type`.
fn slot_push(
    ctx: &LoweringContext<'_, '_>,
    declared: bool,
    slot_type: &PhpType,
    value_type: &PhpType,
) -> SlotPush {
    let unsupported = |what: &str| {
        SlotPush::Throw(format!(
            "elephc: appending through a runtime property name (`$obj->{{$name}}[] = v`) to {what} is not supported; name the property"
        ))
    };
    match slot_type.codegen_repr() {
        PhpType::Array(element) => {
            let element = element.codegen_repr();
            let value = value_type.codegen_repr();
            // An untyped slot's element type already absorbed the value (the checker merged it);
            // a declared one only takes a value of its own element representation.
            if declared && element != PhpType::Mixed && element != value {
                unsupported("a typed array property whose element type differs from the value")
            } else {
                SlotPush::InPlace
            }
        }
        PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_) => {
            SlotPush::WriteBack(slot_type.clone())
        }
        PhpType::Str => SlotPush::Throw("[] operator not supported for strings".to_string()),
        PhpType::Int | PhpType::Float => {
            SlotPush::Throw("Cannot use a scalar value as an array".to_string())
        }
        PhpType::Object(class_name) => {
            if super::type_satisfies_array_access_for_ir(ctx, slot_type) {
                SlotPush::ArrayAccess(slot_type.clone())
            } else {
                SlotPush::Throw(format!(
                    "Cannot use object of type {} as array",
                    class_name.trim_start_matches('\\')
                ))
            }
        }
        PhpType::Bool => unsupported("a bool property"),
        _ => unsupported("a property that was never assigned an array"),
    }
}

/// Emits, before a runtime-named write `$obj->{$name} = $value`, one arm per declared slot that
/// takes the value's exact representation, each lowering the statically named
/// `$obj->slot = $value`. Returns the values the remaining runtime write uses (reloaded from the
/// locals the arms share) and the merge block every path continues at, or the original values
/// and `None` when no slot qualifies (nothing is emitted then).
pub(super) fn lower_dynamic_assign_static_arms(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: LoweredValue,
    value: LoweredValue,
    span: Span,
) -> (LoweredValue, LoweredValue, LoweredValue, Option<BlockId>) {
    let object_type = normalize_value_php_type(ctx.builder.value_php_type(object.value));
    let value_type = normalize_value_php_type(ctx.builder.value_php_type(value.value));
    let PhpType::Object(class_name) = object_type.codegen_repr() else {
        return (object, property, value, None);
    };
    let class_name = class_name.trim_start_matches('\\').to_string();
    let mut targets: Vec<(String, Option<String>)> = assign_targets(ctx, &class_name, &value_type)
        .into_iter()
        .map(|property| (property, None))
        .collect();
    let known: HashSet<String> = ctx
        .classes
        .get(class_name.as_str())
        .map(|info| info.properties.iter().map(|(name, _)| name.clone()).collect())
        .unwrap_or_default();
    for subclass in subclasses_deepest_first(ctx, &class_name) {
        for property in assign_targets(ctx, &subclass, &value_type) {
            if !known.contains(&property) {
                targets.push((property, Some(subclass.clone())));
            }
        }
    }
    if targets.is_empty() {
        return (object, property, value, None);
    }
    let object_local = ctx.declare_synthetic_php_local(object_type.clone());
    store_value_into_temp(ctx, &object_local, object_type, object, span);
    let name_local = ctx.declare_synthetic_php_local(PhpType::Str);
    store_value_into_temp(ctx, &name_local, PhpType::Str, property, span);
    let value_local = ctx.declare_synthetic_php_local(value_type.clone());
    store_value_into_temp(ctx, &value_local, value_type, value, span);
    let var = |name: &str| Expr::new(ExprKind::Variable(name.to_string()), span);
    let merge = ctx.builder.create_named_block("dyn_prop_assign.merge", Vec::new());
    let split_initialized = ctx.initialized_slots_snapshot();
    for (property, subclass) in targets {
        let arm = ctx.builder.create_named_block("dyn_prop_assign.arm", Vec::new());
        let skip = ctx.builder.create_named_block("dyn_prop_assign.skip", Vec::new());
        let mut condition = Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(Expr::new(
                    ExprKind::FunctionCall {
                        name: Name::unqualified("strcmp"),
                        args: vec![var(&name_local), literal(&property, span)],
                    },
                    span,
                )),
                op: BinOp::StrictEq,
                right: Box::new(Expr::new(ExprKind::IntLiteral(0), span)),
            },
            span,
        );
        if let Some(subclass) = &subclass {
            condition = Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(condition),
                    op: BinOp::And,
                    right: Box::new(Expr::new(
                        ExprKind::InstanceOf {
                            value: Box::new(var(&object_local)),
                            target: InstanceOfTarget::Name(Name::from(subclass.clone())),
                        },
                        span,
                    )),
                },
                span,
            );
        }
        let matches = lower_expr(ctx, &condition);
        conditional_branch(ctx, matches.value, arm, skip);
        ctx.builder.position_at_end(arm);
        ctx.restore_initialized_slots(split_initialized.clone());
        let receiver = match &subclass {
            Some(subclass) => {
                let ty = PhpType::Object(subclass.clone());
                let local = ctx.declare_synthetic_php_local(ty.clone());
                store_expr_into_temp(ctx, &local, ty, &var(&object_local), span);
                local
            }
            None => object_local.clone(),
        };
        crate::ir_lower::stmt::lower_stmt(
            ctx,
            &Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(var(&receiver)),
                    property,
                    value: var(&value_local),
                },
                span,
            ),
        );
        branch_to(ctx, merge);
        ctx.builder.position_at_end(skip);
    }
    ctx.restore_initialized_slots(split_initialized);
    let object = lower_expr(ctx, &var(&object_local));
    let property = lower_expr(ctx, &var(&name_local));
    let value = lower_expr(ctx, &var(&value_local));
    (object, property, value, Some(merge))
}

/// Declared, accessible, unhooked, writable slots of `class_name` whose representation is
/// exactly the value's, which a statically named store handles like PHP.
fn assign_targets(ctx: &LoweringContext<'_, '_>, class_name: &str, value_type: &PhpType) -> Vec<String> {
    let Some(info) = ctx.classes.get(class_name) else {
        return Vec::new();
    };
    let value_repr = value_type.codegen_repr();
    let mut names: Vec<String> = info.properties.iter().map(|(name, _)| name.clone()).collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .filter(|property| {
            let Some((_, (_, slot_type))) = info.visible_property(property) else { return false };
            let declaring = info
                .property_declaring_classes
                .get(property)
                .cloned()
                .unwrap_or_else(|| class_name.to_string());
            let readable = scope_can_access(ctx, info.property_visibilities.get(property), &declaring);
            let writable = scope_can_access(
                ctx,
                info.property_set_visibilities
                    .get(property)
                    .or(info.property_visibilities.get(property)),
                &declaring,
            );
            let hooked = info.property_hooks.get(property).is_some_and(|hooks| hooks.get || hooks.set);
            readable
                && writable
                && !hooked
                && !info.readonly_properties.contains(property)
                && matches!(slot_type.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. })
                && slot_type.codegen_repr() == value_repr
        })
        .collect()
}
