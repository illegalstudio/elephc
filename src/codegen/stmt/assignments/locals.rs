//! Purpose:
//! Lowers local variable assignment, compound assignment, and null-coalescing local writes.
//! Evaluates values, coerces to local slot types, and updates ownership for overwritten locals.
//!
//! Called from:
//! - `crate::codegen::stmt::assignments`
//!
//! Key details:
//! - Local writes must release replaced heap values only when the frame owns the previous value.

use super::super::super::abi;
use super::super::super::context::{Context, HeapOwnership};
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::expr::{coerce_result_to_type, emit_expr};
use super::super::super::functions;
use super::super::PhpType;
use crate::names::Name;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver};

pub(crate) fn emit_assign_stmt(
    name: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if let ExprKind::NullCoalesce {
        value: current,
        default,
    } = &value.kind
    {
        if matches!(&current.kind, ExprKind::Variable(current_name) if current_name == name) {
            emit_null_coalesce_assign_stmt(name, current, default, emitter, ctx, data);
            return;
        }
    }

    emitter.blank();
    emitter.comment(&format!("${} = ...", name));
    let saved_self_ref_var = prepare_self_ref_closure_capture(name, value, ctx);
    let target_static_ty = ctx.variables.get(name).map(|var| var.static_ty.clone());
    let assoc_array_target = assoc_array_literal_target_type(value, target_static_ty.as_ref());
    let static_ty = assoc_array_target.clone().unwrap_or_else(|| {
        target_static_ty
            .clone()
            .filter(|ty| matches!(ty, PhpType::Union(_)))
            .unwrap_or_else(|| functions::infer_contextual_type(value, ctx))
    });
    let mut ty = if let Some(target_ty) = assoc_array_target {
        emit_indexed_literal_as_assoc_target(value, &target_ty, emitter, ctx, data)
    } else {
        emit_expr(value, emitter, ctx, data)
    };
    restore_self_ref_closure_capture(name, saved_self_ref_var, ctx);
    let dest_needs_mixed_box = ctx.variables.get(name).is_some_and(|var| {
        !ctx.ref_params.contains(name)
            && matches!(var.ty, PhpType::Mixed)
            && !matches!(ty, PhpType::Mixed | PhpType::Union(_))
    });
    if dest_needs_mixed_box {
        super::super::super::emit_box_current_value_as_mixed(emitter, &ty);
        ty = PhpType::Mixed;
    }

    if ctx.extern_globals.contains_key(name) {
        super::super::emit_extern_global_store(emitter, name, &ty);
    } else if ctx.global_vars.contains(name) {
        if !dest_needs_mixed_box {
            super::super::helpers::retain_borrowed_heap_result(emitter, value, &ty);
        }
        super::super::emit_global_store(emitter, ctx, name, &ty);
    } else if ctx.ref_params.contains(name) {
        let var = match ctx.variables.get(name) {
            Some(v) => v,
            None => {
                emitter.comment(&format!("WARNING: undefined variable ${}", name));
                return;
            }
        };
        let offset = var.stack_offset;
        let old_ty = var.ty.clone();
        let pointer_reg = abi::symbol_scratch_reg(emitter);
        let ref_needs_mixed_box =
            matches!(old_ty, PhpType::Mixed) && !matches!(ty, PhpType::Mixed | PhpType::Union(_));
        if ref_needs_mixed_box {
            super::super::super::emit_box_current_value_as_mixed(emitter, &ty);
            ty = PhpType::Mixed;
        } else if matches!(ty, PhpType::Mixed | PhpType::Union(_))
            && !matches!(old_ty, PhpType::Mixed | PhpType::Union(_))
            && super::super::super::expr::can_coerce_result_to_type(&ty, &old_ty)
        {
            let release_mixed_after_coerce =
                super::super::helpers::should_release_owned_mixed_after_coerce(
                    value,
                    &ty,
                    &old_ty,
                );
            if release_mixed_after_coerce {
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
            }
            coerce_result_to_type(emitter, ctx, data, &ty, &old_ty);
            if release_mixed_after_coerce {
                super::super::helpers::release_preserved_mixed_after_coercion(
                    emitter,
                    &old_ty,
                );
            }
            ty = old_ty.clone();
        } else {
            super::super::helpers::retain_borrowed_heap_result(emitter, value, &ty);
        }
        emitter.comment(&format!("write through ref ${}", name));
        abi::load_at_offset(emitter, pointer_reg, offset);                            // load pointer to referenced variable
        if old_ty.is_refcounted() {
            abi::emit_push_reg(emitter, pointer_reg);                                 // preserve the referenced-slot address across decref helper calls
            let needs_save_x0 = !matches!(&ty, PhpType::Str | PhpType::Float);
            if needs_save_x0 {
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));            // preserve the incoming boxed/scalar result across decref helper calls
            }
            abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0); // load previous heap pointer from ref target
            abi::emit_decref_if_refcounted(emitter, &old_ty);
            if needs_save_x0 {
                abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));             // restore the incoming boxed/scalar result after decref helper calls
            }
            abi::emit_pop_reg(emitter, pointer_reg);                                  // restore the referenced-slot address after decref helper calls
        }
        match &ty {
            PhpType::Bool | PhpType::Int => {
                abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0); // store int/bool through reference pointer
            }
            PhpType::Float => {
                abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), pointer_reg, 0); // store float through reference pointer
            }
            PhpType::Str => {
                abi::emit_push_reg(emitter, pointer_reg);                             // preserve the referenced-slot address across string persistence
                abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0); // load old string ptr from ref target
                abi::emit_call_label(emitter, "__rt_heap_free_safe");                // free old string if on heap
                abi::emit_call_label(emitter, "__rt_str_persist");                   // persist new string to heap
                abi::emit_pop_reg(emitter, pointer_reg);                              // restore the referenced-slot address after string persistence
                let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                abi::emit_store_to_address(emitter, ptr_reg, pointer_reg, 0);         // store heap string pointer through ref
                abi::emit_store_to_address(emitter, len_reg, pointer_reg, 8);         // store string length through ref
            }
            _ => {
                abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0); // store value through reference pointer
            }
        }
    } else {
        let var = match ctx.variables.get(name) {
            Some(v) => v,
            None => {
                emitter.comment(&format!("WARNING: undefined variable ${}", name));
                return;
            }
        };
        let offset = var.stack_offset;
        let old_ty = var.ty.clone();
        if matches!(ty, PhpType::Mixed | PhpType::Union(_))
            && !matches!(old_ty, PhpType::Mixed | PhpType::Union(_))
            && super::super::super::expr::can_coerce_result_to_type(&ty, &old_ty)
        {
            let release_mixed_after_coerce =
                super::super::helpers::should_release_owned_mixed_after_coerce(
                    value,
                    &ty,
                    &old_ty,
                );
            if release_mixed_after_coerce {
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
            }
            coerce_result_to_type(emitter, ctx, data, &ty, &old_ty);
            if release_mixed_after_coerce {
                super::super::helpers::release_preserved_mixed_after_coercion(
                    emitter,
                    &old_ty,
                );
            }
            ty = old_ty.clone();
        }

        if ctx.static_vars.contains(name) {
            if !dest_needs_mixed_box {
                super::super::helpers::retain_borrowed_heap_result(emitter, value, &ty);
            }
            super::super::emit_static_store(emitter, ctx, name, &ty);
        } else {
            if !dest_needs_mixed_box {
                super::super::helpers::retain_borrowed_heap_result(emitter, value, &ty);
            }
            super::super::helpers::release_owned_slot(emitter, &old_ty, offset, &ty);
        }

        abi::emit_store(emitter, &ty, offset);
        ctx.update_var_type_static_and_ownership(
            name,
            ty.clone(),
            static_ty.clone(),
            super::super::helpers::local_slot_ownership_after_store(&ty),
        );

        if ctx.in_main && ctx.all_global_var_names.contains(name) {
            if ty.is_refcounted() {
                abi::emit_incref_if_refcounted(emitter, &ty);                        // global storage becomes a second owner alongside the local slot
            }
            super::super::emit_global_store(emitter, ctx, name, &ty);
        }
    }

    match &value.kind {
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_) => {
            let last_wrapper_label = ctx.deferred_closures.last().map(|d| d.label.clone());
            if let Some(deferred) = ctx.deferred_closures.last() {
                ctx.closure_sigs.insert(name.to_string(), deferred.sig.clone());
                if !deferred.captures.is_empty() {
                    ctx.closure_captures
                        .insert(name.to_string(), deferred.captures.clone());
                } else {
                    ctx.closure_captures.remove(name);
                }
            }
            if let ExprKind::FirstClassCallable(target) = &value.kind {
                if let Some(resolved) = resolve_storable_target(target, ctx) {
                    ctx.first_class_callable_targets
                        .insert(name.to_string(), resolved);
                } else {
                    ctx.first_class_callable_targets.remove(name);
                }
                if let Some(label) = last_wrapper_label {
                    // Track which wrapper backs this local so `emit_variable`
                    // can later mark it `needed` if the FCC value escapes.
                    ctx.variable_fcc_label
                        .insert(name.to_string(), label.clone());
                    // The wrapper is dead code unless and until that escape is
                    // detected — flip the flag now so the emission loop emits
                    // a stub instead of the full body.
                    if let Some(deferred) =
                        ctx.deferred_closures.iter_mut().find(|d| d.label == label)
                    {
                        deferred.needed = false;
                    }
                } else {
                    ctx.variable_fcc_label.remove(name);
                }
            } else {
                ctx.first_class_callable_targets.remove(name);
                ctx.variable_fcc_label.remove(name);
            }
        }
        ExprKind::Variable(src_name) if ty == PhpType::Callable => {
            if let Some(sig) = ctx.closure_sigs.get(src_name).cloned() {
                ctx.closure_sigs.insert(name.to_string(), sig);
            } else {
                ctx.closure_sigs.remove(name);
            }
            if let Some(captures) = ctx.closure_captures.get(src_name).cloned() {
                ctx.closure_captures.insert(name.to_string(), captures);
            } else {
                ctx.closure_captures.remove(name);
            }
            if let Some(target) = ctx.first_class_callable_targets.get(src_name).cloned() {
                ctx.first_class_callable_targets
                    .insert(name.to_string(), target);
            } else {
                ctx.first_class_callable_targets.remove(name);
            }
            if let Some(label) = ctx.variable_fcc_label.get(src_name).cloned() {
                ctx.variable_fcc_label.insert(name.to_string(), label);
            } else {
                ctx.variable_fcc_label.remove(name);
            }
        }
        _ => {
            ctx.closure_sigs.remove(name);
            ctx.closure_captures.remove(name);
            ctx.first_class_callable_targets.remove(name);
            ctx.variable_fcc_label.remove(name);
        }
    }

    if let Some(var) = ctx.variables.get(name) {
        if var.ty != ty {
            ctx.update_var_type_static_and_ownership(
                name,
                ty.clone(),
                static_ty,
                super::super::helpers::local_slot_ownership_after_store(&ty),
            );
        }
    }
}

fn prepare_self_ref_closure_capture(
    name: &str,
    value: &Expr,
    ctx: &mut Context,
) -> Option<(PhpType, PhpType, HeapOwnership, bool)> {
    if !closure_captures_name_by_ref(value, name) {
        return None;
    }
    let var = ctx.variables.get_mut(name)?;
    let saved = (
        var.ty.clone(),
        var.static_ty.clone(),
        var.ownership,
        var.epilogue_cleanup_safe,
    );
    var.ty = PhpType::Callable;
    var.static_ty = PhpType::Callable;
    var.ownership = HeapOwnership::NonHeap;
    Some(saved)
}

fn restore_self_ref_closure_capture(
    name: &str,
    saved: Option<(PhpType, PhpType, HeapOwnership, bool)>,
    ctx: &mut Context,
) {
    let Some((ty, static_ty, ownership, epilogue_cleanup_safe)) = saved else {
        return;
    };
    if let Some(var) = ctx.variables.get_mut(name) {
        var.ty = ty;
        var.static_ty = static_ty;
        var.ownership = ownership;
        var.epilogue_cleanup_safe = epilogue_cleanup_safe;
    }
}

fn closure_captures_name_by_ref(value: &Expr, name: &str) -> bool {
    matches!(
        &value.kind,
        ExprKind::Closure {
            captures,
            capture_refs,
            ..
        } if captures.iter().any(|capture| capture == name)
            && capture_refs.iter().any(|capture| capture == name)
    )
}

fn assoc_array_literal_target_type(value: &Expr, target_ty: Option<&PhpType>) -> Option<PhpType> {
    if !matches!(value.kind, ExprKind::ArrayLiteral(_)) {
        return None;
    }
    match target_ty {
        Some(PhpType::AssocArray { .. }) => target_ty.cloned(),
        _ => None,
    }
}

fn emit_indexed_literal_as_assoc_target(
    value: &Expr,
    target_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let ExprKind::ArrayLiteral(elems) = &value.kind else {
        unreachable!("assoc target literal lowering only accepts indexed array literals");
    };
    let PhpType::AssocArray {
        key: target_key_ty,
        value: target_value_ty,
    } = target_ty
    else {
        unreachable!("assoc target literal lowering requires an associative array target");
    };
    if elems.is_empty() {
        return crate::codegen::expr::arrays::emit_empty_assoc_array_literal(
            *target_key_ty.clone(),
            *target_value_ty.clone(),
            emitter,
        );
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
    crate::codegen::expr::arrays::emit_assoc_array_literal(&pairs, emitter, ctx, data)
}

/// Returns the callable target to store in `ctx.first_class_callable_targets` so
/// that subsequent `$cb(args)` invocations can short-circuit. `self::method(...)`
/// and `parent::method(...)` are resolved to their concrete class at storage time
/// (lexically — `ctx.current_class` at the assignment site). `static::method(...)`
/// is stored as-is: the short-circuit re-uses the caller scope's late-static
/// binding context the same way the closure wrapper would, via
/// `emit_forwarded_called_class_id`.
fn resolve_storable_target(target: &CallableTarget, ctx: &Context) -> Option<CallableTarget> {
    match target {
        CallableTarget::StaticMethod { receiver, method } => match receiver {
            StaticReceiver::Named(_) | StaticReceiver::Static => Some(target.clone()),
            StaticReceiver::Self_ => {
                let class = ctx.current_class.as_ref()?;
                Some(CallableTarget::StaticMethod {
                    receiver: StaticReceiver::Named(Name::unqualified(class)),
                    method: method.clone(),
                })
            }
            StaticReceiver::Parent => {
                let current = ctx.current_class.as_ref()?;
                let parent = ctx.classes.get(current).and_then(|info| info.parent.clone())?;
                Some(CallableTarget::StaticMethod {
                    receiver: StaticReceiver::Named(Name::unqualified(parent)),
                    method: method.clone(),
                })
            }
        },
        _ => Some(target.clone()),
    }
}

fn emit_null_coalesce_assign_stmt(
    name: &str,
    current: &Expr,
    default: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("${} ??= ...", name));
    if matches!(default.kind, ExprKind::Null) {
        emitter.comment("literal null fallback leaves the current value unchanged");
        return;
    }
    let current_ty = emit_expr(current, emitter, ctx, data);
    if current_ty != PhpType::Void {
        let keep_label = ctx.next_label("nca_keep");
        emit_branch_if_result_non_null(&current_ty, &keep_label, emitter);
        emit_assign_stmt(name, default, emitter, ctx, data);
        emitter.label(&keep_label);
    } else {
        emit_assign_stmt(name, default, emitter, ctx, data);
    }
}

fn emit_branch_if_result_non_null(ty: &PhpType, keep_label: &str, emitter: &mut Emitter) {
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(emitter, "__rt_mixed_unbox");                      // inspect the boxed value tag before deciding whether ??= should store
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction("cmp x0, #8");                              // runtime tag 8 means the boxed value is null
                emitter.instruction(&format!("b.ne {}", keep_label));           // keep the existing value when the boxed payload is non-null
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction("cmp rax, 8");                              // runtime tag 8 means the boxed value is null
                emitter.instruction(&format!("jne {}", keep_label));            // keep the existing value when the boxed payload is non-null
            }
        }
        return;
    }

    let null_reg = abi::symbol_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, null_reg, 0x7fff_ffff_ffff_fffe_u64 as i64);
    if ty == &PhpType::Float {
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction("fmov x0, d0");                             // copy the float bits into x0 for the null-sentinel comparison
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction("movq rax, xmm0");                          // copy the float bits into rax for the null-sentinel comparison
            }
        }
    }
    let cmp_reg = if ty == &PhpType::Str {
        abi::string_result_regs(emitter).0
    } else {
        abi::int_result_reg(emitter)
    };
    emitter.instruction(&format!("cmp {}, {}", cmp_reg, null_reg));             // compare the current value with the shared null sentinel
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("b.ne {}", keep_label));               // keep the existing value when it is not null
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("jne {}", keep_label));                // keep the existing value when it is not null
        }
    }
}
