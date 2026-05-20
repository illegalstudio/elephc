//! Purpose:
//! Lowers direct object property assignment including nullable and magic-set paths.
//! Shares receiver and property metadata with object expression lowering.
//!
//! Called from:
//! - `crate::codegen::stmt::assignments::properties`
//!
//! Key details:
//! - Property writes must respect declared types, visibility checks, and runtime object layout.

use super::{dynamic_props, magic_set, references, storage, target};
use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_result_to_type, emit_expr, objects};
use crate::codegen::platform::Arch;
use crate::codegen::stmt::helpers;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub(crate) fn emit_property_assign_stmt(
    object: &Expr,
    property: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("->{}  = ...", property));
    if let Some((current, default)) =
        crate::codegen::stmt::null_coalesce_property_target(object, property, value)
    {
        if matches!(default.kind, ExprKind::Null) {
            emitter.comment("literal null fallback leaves the property unchanged");
            return;
        }
        let current_ty = emit_expr(current, emitter, ctx, data);
        if current_ty != PhpType::Void {
            let keep_label = ctx.next_label("nca_keep");
            crate::codegen::stmt::emit_branch_if_result_non_null(
                &current_ty,
                &keep_label,
                emitter,
            );
            emit_property_assign_stmt(object, property, default, emitter, ctx, data);
            emitter.label(&keep_label);
        } else {
            emit_property_assign_stmt(object, property, default, emitter, ctx, data);
        }
        return;
    }

    if is_stdclass_receiver(object, ctx) {
        emit_dynamic_property_assign_stmt(
            object, property, value, emitter, ctx, data, "__rt_stdclass_set",
        );
        return;
    }
    if is_mixed_receiver(object, ctx) {
        emit_dynamic_property_assign_stmt(
            object,
            property,
            value,
            emitter,
            ctx,
            data,
            "__rt_mixed_property_set",
        );
        return;
    }
    let magic_set_class = magic_set::resolve_magic_set_target(object, property, ctx);
    let declared_target_ty = declared_property_type(object, property, ctx);
    if let Some(class_name) = nullable_object_class(object, ctx) {
        emit_nullable_property_assign_stmt(
            object,
            &class_name,
            property,
            value,
            emitter,
            ctx,
            data,
        );
        return;
    }
    if references::is_reference_property(object, property, ctx) {
        if let Some(var_name) = references::promoted_reference_bind_var(object, property, value, ctx) {
            references::emit_property_reference_bind(&var_name, object, property, emitter, ctx, data);
        } else {
            references::emit_property_reference_write(value, object, property, emitter, ctx, data);
        }
        return;
    }

    let mut val_ty = if let Some(target_ty) = declared_target_ty.as_ref() {
        emit_indexed_literal_as_assoc_property_target(value, target_ty, emitter, ctx, data)
            .unwrap_or_else(|| emit_expr(value, emitter, ctx, data))
    } else {
        emit_expr(value, emitter, ctx, data)
    };
    let boxed_to_mixed = declared_target_ty.as_ref().is_some_and(|target_ty| {
        matches!(target_ty, PhpType::Mixed | PhpType::Union(_))
            && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_))
    });
    if let Some(target_ty) = &declared_target_ty {
        if crate::codegen::expr::can_coerce_result_to_type(&val_ty, target_ty) {
            let release_mixed_after_coerce =
                helpers::should_release_owned_mixed_after_coerce(value, &val_ty, target_ty);
            if release_mixed_after_coerce {
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
            }
            coerce_result_to_type(emitter, ctx, data, &val_ty, target_ty);
            if release_mixed_after_coerce {
                helpers::release_preserved_mixed_after_coercion(emitter, target_ty);
            }
            val_ty = target_ty.clone();
        }
    }
    if magic_set_class.is_none() && !boxed_to_mixed {
        helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }
    abi::emit_push_result_value(emitter, &val_ty);

    let obj_ty = emit_expr(object, emitter, ctx, data);
    let target = match target::resolve_property_assign_target(
        &obj_ty,
        property,
        magic_set_class.as_deref(),
        emitter,
        ctx,
    ) {
        target::PropertyAssignResolution::Resolved(target) => target,
        target::PropertyAssignResolution::UseMagicSet(class_name) => {
            magic_set::emit_magic_set_call(&class_name, property, &val_ty, emitter, ctx, data);
            return;
        }
        target::PropertyAssignResolution::UseDynamicProperty {
            class_name: _,
            dyn_slot_offset,
        } => {
            dynamic_props::emit_dynamic_property_set(
                property,
                &val_ty,
                dyn_slot_offset,
                emitter,
                ctx,
                data,
            );
            return;
        }
        target::PropertyAssignResolution::Abort => {
            abi::emit_release_temporary_stack(emitter, pushed_value_temp_bytes(&val_ty)); // discard the saved RHS value when the property target cannot be resolved
            return;
        }
    };

    if target.needs_deref {
        abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");
        emitter.comment(&format!(
            "store extern field {}::{} at offset {}",
            target.class_name, property, target.offset
        ));
    }

    let object_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", object_reg, abi::int_result_reg(emitter))); // keep the object pointer in a scratch register while property storage is updated
    if !target.needs_deref {
        storage::release_previous_property_value(emitter, object_reg, &target.prop_ty, target.offset);
    }

    storage::store_property_value(emitter, object_reg, &val_ty, target.offset);
}

fn emit_indexed_literal_as_assoc_property_target(
    value: &Expr,
    target_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let ExprKind::ArrayLiteral(elems) = &value.kind else {
        return None;
    };
    let PhpType::AssocArray {
        key: target_key_ty,
        value: target_value_ty,
    } = target_ty
    else {
        return None;
    };
    if elems.is_empty() {
        return Some(crate::codegen::expr::arrays::emit_empty_assoc_array_literal(
            *target_key_ty.clone(),
            *target_value_ty.clone(),
            emitter,
        ));
    }

    let pairs: Vec<(Expr, Expr)> = elems
        .iter()
        .enumerate()
        .map(|(idx, elem)| {
            (
                Expr::new(ExprKind::IntLiteral(idx as i64), elem.span),
                elem.clone(),
            )
        })
        .collect();
    Some(crate::codegen::expr::arrays::emit_assoc_array_literal(
        &pairs, emitter, ctx, data,
    ))
}

fn is_stdclass_receiver(object: &Expr, ctx: &Context) -> bool {
    let obj_ty = crate::codegen::functions::infer_contextual_type(object, ctx);
    crate::codegen::functions::singular_object_class(&obj_ty)
        .is_some_and(crate::types::checker::builtin_stdclass::is_stdclass)
}

fn is_mixed_receiver(object: &Expr, ctx: &Context) -> bool {
    let obj_ty = crate::codegen::functions::infer_contextual_type(object, ctx);
    matches!(obj_ty, PhpType::Mixed)
}

/// Lower `$obj->name = $value` for receivers whose property storage lives in
/// a runtime hash (stdClass directly, or a Mixed receiver that resolves to a
/// stdClass at runtime). Boxes the RHS into a Mixed cell with the existing
/// helper, evaluates the receiver, and dispatches to the named runtime
/// helper which performs the write.
fn emit_dynamic_property_assign_stmt(
    object: &Expr,
    property: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    runtime_symbol: &str,
) {
    emitter.comment(&format!("{} = ... via {}", property, runtime_symbol));

    let val_ty = emit_expr(value, emitter, ctx, data);
    crate::codegen::emit_box_current_value_as_mixed(emitter, &val_ty);          // ensure the RHS is a Mixed* before it crosses object evaluation
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // park the boxed Mixed pointer while we evaluate the receiver

    let _obj_ty = emit_expr(object, emitter, ctx, data);                        // receiver pointer lands in int_result_reg

    let (label, len) = data.add_string(property.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            // x0 already holds the receiver after object eval.
            abi::emit_symbol_address(emitter, "x1", &label);
            abi::emit_load_int_immediate(emitter, "x2", len as i64);
            abi::emit_pop_reg(emitter, "x3");                                   // value Mixed* into x3 for the dynamic setter ABI
            emitter.instruction(&format!("bl {}", runtime_symbol));             // store the boxed Mixed pointer in the dynamic-property hash
        }
        Arch::X86_64 => {
            // Move the receiver from rax (int_result_reg) into rdi for SysV.
            emitter.instruction("mov rdi, rax");                                // shift the receiver into the SysV first-arg register
            abi::emit_symbol_address(emitter, "rsi", &label);
            abi::emit_load_int_immediate(emitter, "rdx", len as i64);
            abi::emit_pop_reg(emitter, "rcx");                                  // value Mixed* into rcx for the dynamic setter ABI
            emitter.instruction(&format!("call {}", runtime_symbol));           // store the boxed Mixed pointer in the dynamic-property hash
        }
    }
}

fn nullable_object_class(object: &Expr, ctx: &Context) -> Option<String> {
    let obj_ty = crate::codegen::functions::infer_contextual_type(object, ctx);
    if !matches!(obj_ty, PhpType::Union(_)) {
        return None;
    }
    crate::codegen::functions::singular_object_class(&obj_ty).map(str::to_string)
}

fn emit_nullable_property_assign_stmt(
    object: &Expr,
    class_name: &str,
    property: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let magic_set_class = magic_set_target_for_class(class_name, property, ctx);
    let declared_target_ty = declared_property_type_for_class(class_name, property, ctx);

    let runtime_obj_ty = emit_expr(object, emitter, ctx, data);
    let guard_nullable = matches!(runtime_obj_ty, PhpType::Mixed | PhpType::Union(_));
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // save the nullable receiver before evaluating the RHS in PHP order

    let mut val_ty = emit_expr(value, emitter, ctx, data);
    let boxed_to_mixed = declared_target_ty.as_ref().is_some_and(|target_ty| {
        matches!(target_ty, PhpType::Mixed | PhpType::Union(_))
            && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_))
    });
    if let Some(target_ty) = &declared_target_ty {
        if crate::codegen::expr::can_coerce_result_to_type(&val_ty, target_ty) {
            let release_mixed_after_coerce =
                helpers::should_release_owned_mixed_after_coerce(value, &val_ty, target_ty);
            if release_mixed_after_coerce {
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
            }
            coerce_result_to_type(emitter, ctx, data, &val_ty, target_ty);
            if release_mixed_after_coerce {
                helpers::release_preserved_mixed_after_coercion(emitter, target_ty);
            }
            val_ty = target_ty.clone();
        }
    }
    if magic_set_class.is_none() && !boxed_to_mixed {
        helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }
    abi::emit_push_result_value(emitter, &val_ty);

    let value_temp_bytes = pushed_value_temp_bytes(&val_ty);
    abi::emit_load_temporary_stack_slot(
        emitter,
        abi::int_result_reg(emitter),
        value_temp_bytes,
    );
    if guard_nullable {
        let message = format!(
            "Fatal error: Attempt to assign property \"{}\" on null\n",
            property
        );
        objects::emit_unbox_mixed_object_or_fatal(message.as_bytes(), emitter, ctx, data);
    }

    let object_ty = PhpType::Object(class_name.to_string());
    let target = match target::resolve_property_assign_target(
        &object_ty,
        property,
        magic_set_class.as_deref(),
        emitter,
        ctx,
    ) {
        target::PropertyAssignResolution::Resolved(target) => target,
        target::PropertyAssignResolution::UseMagicSet(class_name) => {
            magic_set::emit_magic_set_call(&class_name, property, &val_ty, emitter, ctx, data);
            abi::emit_release_temporary_stack(emitter, 16);                     // discard the saved nullable receiver after __set consumes the RHS
            return;
        }
        target::PropertyAssignResolution::UseDynamicProperty {
            class_name: _,
            dyn_slot_offset,
        } => {
            dynamic_props::emit_dynamic_property_set(
                property,
                &val_ty,
                dyn_slot_offset,
                emitter,
                ctx,
                data,
            );
            abi::emit_release_temporary_stack(emitter, 16);                     // discard the saved nullable receiver after dynamic-property storage consumes the RHS
            return;
        }
        target::PropertyAssignResolution::Abort => {
            abi::emit_release_temporary_stack(emitter, value_temp_bytes + 16);  // discard the saved RHS and nullable receiver for unresolved targets
            return;
        }
    };

    let object_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", object_reg, abi::int_result_reg(emitter))); // keep the unboxed object pointer while property storage is updated
    if target.is_reference {
        let pointer_reg = abi::temp_int_reg(emitter.target);
        abi::emit_load_from_address(emitter, pointer_reg, object_reg, target.offset);
        storage::release_previous_referenced_value(emitter, pointer_reg, &target.prop_ty, &val_ty);
        storage::store_referenced_value(emitter, pointer_reg, &val_ty);
        abi::emit_release_temporary_stack(emitter, 16);                         // discard the saved nullable receiver after reference storage consumes the RHS
        return;
    }

    storage::release_previous_property_value(emitter, object_reg, &target.prop_ty, target.offset);
    storage::store_property_value(emitter, object_reg, &val_ty, target.offset);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the saved nullable receiver after property storage consumes the RHS
}

fn pushed_value_temp_bytes(val_ty: &PhpType) -> usize {
    if matches!(val_ty, PhpType::Void | PhpType::Never) {
        0
    } else {
        16
    }
}

fn declared_property_type(object: &Expr, property: &str, ctx: &Context) -> Option<PhpType> {
    let obj_ty = crate::codegen::functions::infer_contextual_type(object, ctx);
    let class_name = crate::codegen::functions::singular_object_class(&obj_ty)?;
    declared_property_type_for_class(class_name, property, ctx)
}

fn declared_property_type_for_class(class_name: &str, property: &str, ctx: &Context) -> Option<PhpType> {
    let class_info = ctx.classes.get(class_name)?;
    class_info
        .properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
}

fn magic_set_target_for_class(class_name: &str, property: &str, ctx: &Context) -> Option<String> {
    let class_info = ctx.classes.get(class_name)?;
    if class_info.properties.iter().any(|(name, _)| name == property) {
        return None;
    }
    class_info.methods.contains_key("__set").then_some(class_name.to_string())
}
