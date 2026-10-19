//! Purpose:
//! Lowers a reference binding whose place is a runtime-named property: `$o->{$k} = &$v`,
//! `$o->$k = &$v`, `$o->{$k}['x'] = &$v` and `$o->{$k}[] = &$v` (Laravel's
//! serializable-closure `Native::mapPointers`).
//!
//! Called from:
//! - `crate::ir_lower::expr::reference_places::lower_ref_assign_place()`.
//!
//! Key details:
//! - The object and the name are evaluated once, then the name is matched (case-sensitively,
//!   like PHP property names) against every property of the receiver's class and of its
//!   subclasses (those guarded by `instanceof`). Each arm lowers the statically named binding
//!   (`$o->p = &$v`, `$o->p['x'] = &$v`), so a declared property keeps every rule a named bind
//!   has. An arm whose property cannot share the reference throws instead: PHP's visibility and
//!   readonly `Error`s, or an explicit elephc `Error` for a hooked property and for a typed
//!   property whose storage is not boxed (the checker promoted every property that can share).
//! - A name that selects no declared property is a property-hash entry: a `stdClass` property, or
//!   a dynamic property of a class with hash storage. `BindDynamicPropRefCell` binds the entry to
//!   the source's cell; an element place first takes the entry's own cell
//!   (`LoadDynamicPropRefCell`) into a hidden local and binds the element through it. A class
//!   whose subclasses place the hash at different offsets, or that has no hash, throws an
//!   explicit elephc `Error` rather than writing the wrong storage.
//! - A receiver of unknown class is dispatched on `instanceof stdClass`; any other runtime class
//!   throws an explicit elephc `Error`.

use super::dynamic_class_access::{conditional_branch, literal, scope_can_access, visibility_word};
use super::*;

/// Returns the runtime-named property (`$o->{$k}`) a place is, or that its element chain starts
/// from (`$o->{$k}['x']`), if any.
pub(crate) fn dynamic_place_root(place: &Expr) -> Option<(&Expr, &Expr)> {
    match &place.kind {
        ExprKind::DynamicPropertyAccess { object, property } => Some((object, property)),
        ExprKind::ArrayAccess { array, .. } => dynamic_place_root(array),
        _ => None,
    }
}

/// Rewrites a place rooted at `$s->p`, where `$s` is a local statically holding a `stdClass`,
/// to the runtime-named form `$s->{'p'}`; `None` for any other place.
pub(crate) fn stdclass_named_place(ctx: &LoweringContext<'_, '_>, place: &Expr) -> Option<Expr> {
    match &place.kind {
        ExprKind::PropertyAccess { object, property } => {
            let ExprKind::Variable(name) = &object.kind else { return None };
            if !ctx.has_local_slot(name) {
                return None;
            }
            let PhpType::Object(class) = ctx.local_type(name).codegen_repr() else { return None };
            if !crate::types::checker::builtin_stdclass::is_stdclass(class.trim_start_matches('\\')) {
                return None;
            }
            Some(Expr::new(
                ExprKind::DynamicPropertyAccess {
                    object: object.clone(),
                    property: Box::new(literal(property, place.span)),
                },
                place.span,
            ))
        }
        ExprKind::ArrayAccess { array, index } => Some(Expr::new(
            ExprKind::ArrayAccess { array: Box::new(stdclass_named_place(ctx, array)?), index: index.clone() },
            place.span,
        )),
        _ => None,
    }
}

/// What one declared-property arm does.
enum Arm {
    /// Lower the statically named binding.
    Bind,
    /// `throw new Error(message)`.
    Throw(String),
}

/// Lowers `<place rooted at $o->{$k}> = &$source`.
pub(crate) fn lower_dynamic_ref_place(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    source_name: &str,
    span: Span,
) {
    let Some((object, name)) = dynamic_place_root(target) else {
        return;
    };
    let direct = matches!(target.kind, ExprKind::DynamicPropertyAccess { .. });
    let var = |name: &str| Expr::new(ExprKind::Variable(name.to_string()), span);

    let object_value = lower_expr(ctx, object);
    let object_type = normalize_value_php_type(ctx.builder.value_php_type(object_value.value));
    let object_local = ctx.declare_synthetic_php_local(object_type.clone());
    store_value_into_temp(ctx, &object_local, object_type.clone(), object_value, span);
    let name_local = ctx.declare_synthetic_php_local(PhpType::Str);
    store_expr_into_temp(
        ctx,
        &name_local,
        PhpType::Str,
        &Expr::new(ExprKind::Cast { target: CastType::String, expr: Box::new(name.clone()) }, span),
        span,
    );
    // Give the source its cell before the dispatch, so every arm shares one definition of it
    // (an undefined source is created as null here, like PHP's `&$v`).
    let Some(_) = super::reference_places::source_reference_cell(ctx, source_name, &PhpType::Mixed, span)
    else {
        return;
    };

    let merge = ctx.builder.create_named_block("dyn_ref_place.merge", Vec::new());
    let split_initialized = ctx.initialized_slots_snapshot();
    // A receiver typed `object` (or with a class this program does not declare) dispatches like
    // a receiver of unknown class.
    let receiver_repr = match object_type.codegen_repr() {
        PhpType::Object(class_name) if dispatches_like_unknown_class(ctx, &class_name) => PhpType::Mixed,
        other => other,
    };
    match receiver_repr {
        PhpType::Object(class_name) => {
            let class_name = class_name.trim_start_matches('\\').to_string();
            let mut arms: Vec<(String, Option<String>, Arm)> = declared_arms(ctx, &class_name, direct)
                .into_iter()
                .map(|(property, arm)| (property, None, arm))
                .collect();
            let known: HashSet<String> = arms.iter().map(|(property, _, _)| property.clone()).collect();
            for subclass in subclasses_of(ctx, &class_name) {
                for (property, arm) in declared_arms(ctx, &subclass, direct) {
                    if !known.contains(&property) {
                        arms.push((property, Some(subclass.clone()), arm));
                    }
                }
            }
            for (property, subclass, arm) in arms {
                let arm_block = ctx.builder.create_named_block("dyn_ref_place.arm", Vec::new());
                let skip = ctx.builder.create_named_block("dyn_ref_place.skip", Vec::new());
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
                conditional_branch(ctx, matches.value, arm_block, skip);
                ctx.builder.position_at_end(arm_block);
                ctx.restore_initialized_slots(split_initialized.clone());
                match arm {
                    Arm::Throw(message) => crate::ir_lower::stmt::lower_throw_access_error(ctx, &message, span),
                    Arm::Bind => {
                        let receiver = match &subclass {
                            Some(subclass) => {
                                let ty = PhpType::Object(subclass.clone());
                                let local = ctx.declare_synthetic_php_local(ty.clone());
                                store_expr_into_temp(ctx, &local, ty, &var(&object_local), span);
                                local
                            }
                            None => object_local.clone(),
                        };
                        let place = replace_root(
                            target,
                            Expr::new(
                                ExprKind::PropertyAccess { object: Box::new(var(&receiver)), property },
                                span,
                            ),
                        );
                        crate::ir_lower::stmt::lower_stmt(
                            ctx,
                            &Stmt::new(StmtKind::RefAssignPlace { target: place, source: var(source_name) }, span),
                        );
                    }
                }
                branch_to(ctx, merge);
                ctx.builder.position_at_end(skip);
                ctx.restore_initialized_slots(split_initialized.clone());
            }
            lower_hash_arm(ctx, target, &object_local, &class_name, &name_local, source_name, direct, span);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            let arm_block = ctx.builder.create_named_block("dyn_ref_place.stdclass", Vec::new());
            let other = ctx.builder.create_named_block("dyn_ref_place.other", Vec::new());
            let matches = lower_expr(ctx, &exact_stdclass_condition(&object_local, span));
            conditional_branch(ctx, matches.value, arm_block, other);
            ctx.builder.position_at_end(arm_block);
            ctx.restore_initialized_slots(split_initialized.clone());
            let receiver = unbox_stdclass(ctx, &object_local, span);
            lower_hash_arm(ctx, target, &receiver, "stdClass", &name_local, source_name, direct, span);
            branch_to(ctx, merge);
            ctx.builder.position_at_end(other);
            ctx.restore_initialized_slots(split_initialized.clone());
            crate::ir_lower::stmt::lower_throw_access_error(
                ctx,
                "elephc: a reference bound through a runtime property name (`$obj->{$name} = &$v`) \
                 on a receiver whose class is not known at compile time is only supported for \
                 stdClass; give the receiver a class type",
                span,
            );
        }
        other => {
            crate::ir_lower::diagnostics::refuse(
                span,
                &format!("Reference assignment to a runtime-named property needs an object, got {other:?}"),
            );
        }
    }
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(split_initialized);
}

/// Lowers `$target = &$o->{$k}` (and `$x = &$x->{$k}`): every arm produces the selected
/// property's managed cell, the cells meet in one merge-block parameter, and the target is bound
/// to it once. A declared property gives its reference-property cell (`LoadPropRefCell`), a name
/// that selects none gives its property-hash entry's cell (`LoadDynamicPropRefCell`, which creates
/// a missing property as null, as PHP's fetch for writing does).
pub(crate) fn lower_ref_assign_dynamic_property(
    ctx: &mut LoweringContext<'_, '_>,
    target: &str,
    source: &Expr,
    span: Span,
) {
    let ExprKind::DynamicPropertyAccess { object, property: name } = &source.kind else {
        return;
    };
    let var = |name: &str| Expr::new(ExprKind::Variable(name.to_string()), span);
    let object_value = lower_expr(ctx, object);
    let object_type = normalize_value_php_type(ctx.builder.value_php_type(object_value.value));
    let object_local = ctx.declare_synthetic_php_local(object_type.clone());
    store_value_into_temp(ctx, &object_local, object_type.clone(), object_value, span);
    let name_local = ctx.declare_synthetic_php_local(PhpType::Str);
    store_expr_into_temp(
        ctx,
        &name_local,
        PhpType::Str,
        &Expr::new(ExprKind::Cast { target: CastType::String, expr: Box::new((**name).clone()) }, span),
        span,
    );
    let pointer = PhpType::Pointer(None);
    let merge = ctx
        .builder
        .create_named_block("dyn_ref_source.merge", vec![(IrType::I64, pointer.clone())]);
    let split_initialized = ctx.initialized_slots_snapshot();
    let mut reached = false;
    let receiver_repr = match object_type.codegen_repr() {
        PhpType::Object(class_name) if dispatches_like_unknown_class(ctx, &class_name) => PhpType::Mixed,
        other => other,
    };
    match receiver_repr {
        PhpType::Object(class_name) => {
            let class_name = class_name.trim_start_matches('\\').to_string();
            let mut arms: Vec<(String, Option<String>, Arm)> = declared_arms(ctx, &class_name, true)
                .into_iter()
                .map(|(property, arm)| (property, None, arm))
                .collect();
            let known: HashSet<String> = arms.iter().map(|(property, _, _)| property.clone()).collect();
            for subclass in subclasses_of(ctx, &class_name) {
                for (property, arm) in declared_arms(ctx, &subclass, true) {
                    if !known.contains(&property) {
                        arms.push((property, Some(subclass.clone()), arm));
                    }
                }
            }
            for (property, subclass, arm) in arms {
                let arm_block = ctx.builder.create_named_block("dyn_ref_source.arm", Vec::new());
                let skip = ctx.builder.create_named_block("dyn_ref_source.skip", Vec::new());
                let condition = name_condition(&name_local, &object_local, &property, subclass.as_deref(), span);
                let matches = lower_expr(ctx, &condition);
                conditional_branch(ctx, matches.value, arm_block, skip);
                ctx.builder.position_at_end(arm_block);
                ctx.restore_initialized_slots(split_initialized.clone());
                match arm {
                    Arm::Throw(message) => crate::ir_lower::stmt::lower_throw_access_error(ctx, &message, span),
                    Arm::Bind => {
                        let receiver = match &subclass {
                            Some(subclass) => {
                                let ty = PhpType::Object(subclass.clone());
                                let local = ctx.declare_synthetic_php_local(ty.clone());
                                store_expr_into_temp(ctx, &local, ty, &var(&object_local), span);
                                local
                            }
                            None => object_local.clone(),
                        };
                        let receiver = lower_expr(ctx, &var(&receiver));
                        let data = ctx.intern_string(&property);
                        let cell = ctx.emit_value(
                            Op::LoadPropRefCell,
                            vec![receiver.value],
                            Some(Immediate::Data(data)),
                            pointer.clone(),
                            Op::LoadPropRefCell.default_effects(),
                            Some(span),
                        );
                        ctx.builder.terminate(Terminator::Br { target: merge, args: vec![cell.value] });
                        reached = true;
                    }
                }
                ctx.builder.position_at_end(skip);
                ctx.restore_initialized_slots(split_initialized.clone());
            }
            reached |= hash_source_arm(ctx, &object_local, &class_name, &name_local, merge, span);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            let arm_block = ctx.builder.create_named_block("dyn_ref_source.stdclass", Vec::new());
            let other = ctx.builder.create_named_block("dyn_ref_source.other", Vec::new());
            let matches = lower_expr(ctx, &exact_stdclass_condition(&object_local, span));
            conditional_branch(ctx, matches.value, arm_block, other);
            ctx.builder.position_at_end(arm_block);
            ctx.restore_initialized_slots(split_initialized.clone());
            let receiver = unbox_stdclass(ctx, &object_local, span);
            reached |= hash_source_arm(ctx, &receiver, "stdClass", &name_local, merge, span);
            ctx.builder.position_at_end(other);
            ctx.restore_initialized_slots(split_initialized.clone());
            crate::ir_lower::stmt::lower_throw_access_error(
                ctx,
                "elephc: a reference taken through a runtime property name (`$r = &$obj->{$name}`) \
                 on a receiver whose class is not known at compile time is only supported for \
                 stdClass; give the receiver a class type",
                span,
            );
        }
        other => {
            crate::ir_lower::diagnostics::refuse(
                span,
                &format!("Reference to a runtime-named property needs an object, got {other:?}"),
            );
            return;
        }
    }
    if !ctx.builder.insertion_block_is_terminated() {
        crate::ir_lower::stmt::lower_throw_access_error(ctx, "elephc: unreachable runtime-named reference", span);
    }
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(split_initialized);
    if !reached {
        ctx.builder.terminate(Terminator::Unreachable);
        let after = ctx.builder.create_named_block("dyn_ref_source.after", Vec::new());
        ctx.builder.position_at_end(after);
        return;
    }
    let cell = LoweredValue { value: ctx.builder.block_param(merge, 0), ir_type: IrType::I64 };
    ctx.bind_owned_local_ref_cell_ptr(target, cell, PhpType::Mixed, Some(span));
}

/// The property-hash arm of a runtime-named reference source; returns whether it reaches `merge`.
fn hash_source_arm(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &str,
    class_name: &str,
    name_local: &str,
    merge: BlockId,
    span: Span,
) -> bool {
    if let Some(message) = hash_storage_refusal(ctx, class_name) {
        crate::ir_lower::stmt::lower_throw_access_error(ctx, &message, span);
        return false;
    }
    let var = |name: &str| Expr::new(ExprKind::Variable(name.to_string()), span);
    let object = lower_expr(ctx, &var(receiver));
    let name = lower_expr(ctx, &var(name_local));
    let cell = ctx.emit_value(
        Op::LoadDynamicPropRefCell,
        vec![object.value, name.value],
        None,
        PhpType::Pointer(None),
        Op::LoadDynamicPropRefCell.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::Br { target: merge, args: vec![cell.value] });
    true
}

/// `strcmp($name, 'p') === 0`, and `&& $object instanceof Sub` for a subclass-only property.
fn name_condition(name_local: &str, object_local: &str, property: &str, subclass: Option<&str>, span: Span) -> Expr {
    let var = |name: &str| Expr::new(ExprKind::Variable(name.to_string()), span);
    let mut condition = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("strcmp"),
                    args: vec![var(name_local), literal(property, span)],
                },
                span,
            )),
            op: BinOp::StrictEq,
            right: Box::new(Expr::new(ExprKind::IntLiteral(0), span)),
        },
        span,
    );
    if let Some(subclass) = subclass {
        condition = Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(condition),
                op: BinOp::And,
                right: Box::new(Expr::new(
                    ExprKind::InstanceOf {
                        value: Box::new(var(object_local)),
                        target: InstanceOfTarget::Name(Name::from(subclass.to_string())),
                    },
                    span,
                )),
            },
            span,
        );
    }
    condition
}

/// Unboxes a `mixed` local already known to hold a `stdClass` into a `stdClass`-typed local.
fn unbox_stdclass(ctx: &mut LoweringContext<'_, '_>, object_local: &str, span: Span) -> String {
    let ty = PhpType::Object("stdClass".to_string());
    if !matches!(ctx.local_type(object_local).codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        // An object-typed receiver routed here (a class extends stdClass) is already a pointer.
        let receiver = ctx.declare_synthetic_php_local(ty.clone());
        store_expr_into_temp(ctx, &receiver, ty, &Expr::new(ExprKind::Variable(object_local.to_string()), span), span);
        return receiver;
    }
    let boxed = lower_expr(ctx, &Expr::new(ExprKind::Variable(object_local.to_string()), span));
    let unboxed = ctx.emit_owned_value(
        Op::MixedUnbox,
        vec![boxed.value],
        None,
        ty.clone(),
        Op::mixed_unbox_effects(&ty),
        Some(span),
    );
    crate::ir_lower::ownership::release_if_owned(ctx, boxed, Some(span));
    let receiver = ctx.declare_synthetic_php_local(ty.clone());
    store_value_into_temp(ctx, &receiver, ty, unboxed, span);
    receiver
}

/// Binds a name that selects no declared property: an entry of the receiver's property hash.
#[allow(clippy::too_many_arguments)]
fn lower_hash_arm(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    receiver: &str,
    class_name: &str,
    name_local: &str,
    source_name: &str,
    direct: bool,
    span: Span,
) {
    if let Some(message) = hash_storage_refusal(ctx, class_name) {
        crate::ir_lower::stmt::lower_throw_access_error(ctx, &message, span);
        return;
    }
    let var = |name: &str| Expr::new(ExprKind::Variable(name.to_string()), span);
    let object = lower_expr(ctx, &var(receiver));
    let name = lower_expr(ctx, &var(name_local));
    if direct {
        let Some(cell) = super::reference_places::source_reference_cell(ctx, source_name, &PhpType::Mixed, span)
        else {
            return;
        };
        ctx.emit_void(
            Op::BindDynamicPropRefCell,
            vec![object.value, name.value, cell.value],
            None,
            Op::BindDynamicPropRefCell.default_effects(),
            Some(span),
        );
        return;
    }
    let cell = ctx.emit_value(
        Op::LoadDynamicPropRefCell,
        vec![object.value, name.value],
        None,
        PhpType::Pointer(None),
        Op::LoadDynamicPropRefCell.default_effects(),
        Some(span),
    );
    let container = ctx.declare_synthetic_php_local(PhpType::Mixed);
    ctx.bind_owned_local_ref_cell_ptr(&container, cell, PhpType::Mixed, Some(span));
    let place = replace_root(target, var(&container));
    crate::ir_lower::stmt::lower_stmt(
        ctx,
        &Stmt::new(StmtKind::RefAssignPlace { target: place, source: var(source_name) }, span),
    );
    crate::ir_lower::stmt::lower_stmt(
        ctx,
        &Stmt::new(
            StmtKind::ExprStmt(Expr::new(
                ExprKind::FunctionCall { name: Name::unqualified("unset"), args: vec![var(&container)] },
                span,
            )),
            span,
        ),
    );
}

/// Whether a receiver of static class `class_name` must be dispatched on its runtime class: an
/// `object` type or a class this program does not declare, or `stdClass` while some class extends
/// it (its instances then have another layout and may define `__get`/`__set`).
fn dispatches_like_unknown_class(ctx: &LoweringContext<'_, '_>, class_name: &str) -> bool {
    let class_name = class_name.trim_start_matches('\\');
    if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
        return !subclasses_of(ctx, class_name).is_empty();
    }
    !ctx.classes.contains_key(class_name)
}

/// `$o instanceof stdClass && get_class($o) === 'stdClass'`: exactly a `stdClass`, never a class
/// extending it.
fn exact_stdclass_condition(object_local: &str, span: Span) -> Expr {
    let var = Expr::new(ExprKind::Variable(object_local.to_string()), span);
    let instance = Expr::new(
        ExprKind::InstanceOf {
            value: Box::new(var.clone()),
            target: InstanceOfTarget::Name(Name::from("stdClass".to_string())),
        },
        span,
    );
    let class = Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(Expr::new(
                ExprKind::FunctionCall { name: Name::unqualified("get_class"), args: vec![var] },
                span,
            )),
            op: BinOp::StrictEq,
            right: Box::new(literal("stdClass", span)),
        },
        span,
    );
    Expr::new(ExprKind::BinaryOp { left: Box::new(instance), op: BinOp::And, right: Box::new(class) }, span)
}

/// Whether `class_name` or one of its ancestors defines `__get` or `__set`.
fn has_magic_accessors(ctx: &LoweringContext<'_, '_>, class_name: &str) -> bool {
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        let Some(info) = ctx.classes.get(name.as_str()) else { return false };
        if info.methods.keys().any(|method| {
            let method = method.to_ascii_lowercase();
            method == "__get" || method == "__set"
        }) {
            return true;
        }
        current = info.parent.clone();
    }
    false
}

/// Whether any runtime class a `class_name` receiver can hold defines `__get` or `__set`.
fn subtree_has_magic_accessors(ctx: &LoweringContext<'_, '_>, class_name: &str) -> bool {
    has_magic_accessors(ctx, class_name)
        || subclasses_of(ctx, class_name).iter().any(|subclass| has_magic_accessors(ctx, subclass))
}

/// The explicit refusal for a reference through a runtime name that PHP would route to
/// `__get`/`__set` (an undeclared or inaccessible name on a class with magic accessors).
fn magic_refusal(class_name: &str) -> String {
    format!(
        "elephc: a reference through a runtime property name (`$obj->{{$name}}`) to an undeclared or \
         inaccessible property of {class_name}, which has __get/__set, is not supported; PHP would \
         call the magic accessor"
    )
}

/// Returns why the property hash of `class_name` cannot hold the reference, if it cannot.
fn hash_storage_refusal(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Option<String> {
    if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
        return None;
    }
    if subtree_has_magic_accessors(ctx, class_name) {
        return Some(magic_refusal(class_name));
    }
    let refuse = || {
        Some(format!(
            "elephc: binding a reference to an undeclared property of {class_name} through a \
             runtime name (`$obj->{{$name}} = &$v`) is not supported; declare the property"
        ))
    };
    let Some(info) = ctx.classes.get(class_name) else {
        return refuse();
    };
    if !info.dynamic_property_hash_is_name_addressable() {
        return refuse();
    }
    // The hash sits after the declared slots, so every runtime class the receiver can hold must
    // place it at the same offset and report the same creation notice.
    let uniform = subclasses_of(ctx, class_name).iter().all(|subclass| {
        ctx.classes.get(subclass).is_some_and(|sub| {
            sub.properties.len() == info.properties.len()
                && sub.dynamic_property_hash_is_name_addressable()
                && sub.dynamic_property_creation_is_deprecated() == info.dynamic_property_creation_is_deprecated()
        })
    });
    if uniform {
        None
    } else {
        refuse()
    }
}

/// Decides, per property of `class_name`, whether a runtime name selecting it binds or throws.
fn declared_arms(ctx: &LoweringContext<'_, '_>, class_name: &str, direct: bool) -> Vec<(String, Arm)> {
    let Some(info) = ctx.classes.get(class_name) else {
        return Vec::new();
    };
    let mut names: Vec<String> = info.properties.iter().map(|(name, _)| name.clone()).collect();
    names.sort();
    names.dedup();
    let mut arms = Vec::new();
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
        let visibility = if readable {
            info.property_set_visibilities.get(&property)
        } else {
            info.property_visibilities.get(&property)
        };
        let arm = if (!readable || !writable) && subtree_has_magic_accessors(ctx, class_name) {
            Arm::Throw(magic_refusal(class_name))
        } else if !readable || !writable {
            Arm::Throw(format!(
                "Cannot {} {} property {class_name}::${property}",
                if readable { "modify" } else { "access" },
                visibility_word(visibility),
            ))
        } else if info.readonly_properties.contains(&property) {
            let declaring = declaring.trim_start_matches('\\');
            Arm::Throw(format!("Cannot indirectly modify readonly property {declaring}::${property}"))
        } else if hooked {
            Arm::Throw(format!(
                "elephc: a reference to the hooked property {class_name}::${property} through a \
                 runtime name is not supported; name the property"
            ))
        } else if !info.owned_reference_properties.contains(&property) || !shares_reference(slot_type, direct) {
            Arm::Throw(format!(
                "elephc: the typed property {class_name}::${property} cannot share a reference \
                 bound through a runtime name (`$obj->{{$name}} = &$v`): its storage is not mixed; \
                 name the property or declare it mixed"
            ))
        } else {
            Arm::Bind
        };
        arms.push((property, arm));
    }
    arms
}

/// Whether a property stored as `slot_type` can hold the reference the place binds.
fn shares_reference(slot_type: &PhpType, direct: bool) -> bool {
    slot_type.codegen_repr() == PhpType::Mixed || (!direct && slot_type.is_php_array())
}

/// Classes extending `class_name` (not itself), deepest first.
fn subclasses_of(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Vec<String> {
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
            other.as_str() != class_name && super::property_access::class_extends_class(ctx, other, class_name)
        })
        .cloned()
        .collect();
    subclasses.sort_by(|left, right| depth(right).cmp(&depth(left)).then_with(|| left.cmp(right)));
    subclasses
}

/// Rebuilds `place` with its runtime-named root replaced by `root`.
fn replace_root(place: &Expr, root: Expr) -> Expr {
    match &place.kind {
        ExprKind::DynamicPropertyAccess { .. } => root,
        ExprKind::ArrayAccess { array, index } => Expr::new(
            ExprKind::ArrayAccess { array: Box::new(replace_root(array, root)), index: index.clone() },
            place.span,
        ),
        _ => place.clone(),
    }
}
