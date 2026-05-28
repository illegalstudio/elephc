//! Purpose:
//! Lowers property reads, magic access paths, and nullable object field loads.
//! Produces object-related expression results while respecting runtime metadata and ownership rules.
//!
//! Called from:
//! - `crate::codegen::expr::objects`
//!
//! Key details:
//! - Object handles, property storage, and class ids must stay consistent with emitted class tables.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::functions;
use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::super::{coerce_result_to_type, emit_expr};

/// Lowers `$obj->property` where the receiver type is known at compile time.
pub(super) fn emit_property_access(
    object: &Expr,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    // Resolve the receiver's static class up-front so a nullable object
    // union (`?Foo`) routes through the same path as a direct object type.
    // Direct object receivers produce a raw object pointer, while nullable
    // unions produce a boxed mixed cell that must be checked and unboxed
    // before the normal property load.
    let static_obj_ty = functions::infer_contextual_type(object, ctx);
    let static_class = functions::singular_object_class(&static_obj_ty)
        .map(|name| name.to_string());
    let obj_ty = emit_expr(object, emitter, ctx, data);
    if let Some(class_name) = static_class.as_ref() {
        if matches!(obj_ty, PhpType::Mixed | PhpType::Union(_)) {
            return emit_nullable_object_property_access(class_name, property, emitter, ctx, data);
        }
        if matches!(obj_ty, PhpType::Object(_)) {
            return emit_loaded_object_property_access(class_name, property, emitter, ctx, data);
        }
    }
    let (class_name, prop_ty, offset, needs_deref, is_reference) = match &obj_ty {
        PhpType::Object(class_name) => {
            return emit_loaded_object_property_access(class_name, property, emitter, ctx, data);
        }
        PhpType::Mixed => {
            return emit_mixed_property_access(property, emitter, ctx, data);
        }
        PhpType::Pointer(Some(class_name)) if ctx.extern_classes.contains_key(class_name) => {
            let class_info = match ctx.extern_classes.get(class_name).cloned() {
                Some(c) => c,
                None => {
                    emitter.comment(&format!("WARNING: undefined extern class {}", class_name));
                    return PhpType::Int;
                }
            };

            let field = match class_info
                .fields
                .iter()
                .find(|field| field.name == property)
            {
                Some(field) => field.clone(),
                None => {
                    emitter.comment(&format!("WARNING: undefined extern field {}", property));
                    return PhpType::Int;
                }
            };

            (class_name.clone(), field.php_type, field.offset, true, false)
        }
        PhpType::Pointer(Some(class_name)) if ctx.packed_classes.contains_key(class_name) => {
            let class_info = match ctx.packed_classes.get(class_name).cloned() {
                Some(c) => c,
                None => {
                    emitter.comment(&format!("WARNING: undefined packed class {}", class_name));
                    return PhpType::Int;
                }
            };

            let field = match class_info
                .fields
                .iter()
                .find(|field| field.name == property)
            {
                Some(field) => field.clone(),
                None => {
                    emitter.comment(&format!("WARNING: undefined packed field {}", property));
                    return PhpType::Int;
                }
            };

            (class_name.clone(), field.php_type, field.offset, true, false)
        }
        _ => {
            emitter.comment("WARNING: property access on non-object");
            return PhpType::Int;
        }
    };

    if needs_deref {
        abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");               // abort with fatal error on null pointer dereference
        emitter.comment(&format!(
            "->{} via ptr<{}> (offset {})",
            property, class_name, offset
        ));
    } else {
        emitter.comment(&format!("->{}  (offset {})", property, offset));
    }

    let object_reg = abi::int_result_reg(emitter);

    if is_reference {
        let pointer_reg = abi::symbol_scratch_reg(emitter);
        abi::emit_load_from_address(emitter, pointer_reg, object_reg, offset);
        match &prop_ty {
            PhpType::Str => {
                let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                abi::emit_load_from_address(emitter, ptr_reg, pointer_reg, 0);
                abi::emit_load_from_address(emitter, len_reg, pointer_reg, 8);
            }
            PhpType::Float => {
                abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), pointer_reg, 0);
            }
            PhpType::Bool | PhpType::Int | PhpType::Void | PhpType::Never | PhpType::Resource(_) => {
                abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0);
            }
            PhpType::Iterable
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Buffer(_)
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_) => {
                abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0);
            }
        }
        return prop_ty;
    }

    match &prop_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            let base_reg = abi::symbol_scratch_reg(emitter);
            emitter.instruction(&format!("mov {}, {}", base_reg, object_reg));  // preserve the object base pointer while loading the two-word string property payload
            abi::emit_load_from_address(emitter, ptr_reg, base_reg, offset);
            abi::emit_load_from_address(emitter, len_reg, base_reg, offset + 8);
        }
        PhpType::Float => {
            abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), object_reg, offset);
        }
        PhpType::Bool | PhpType::Int | PhpType::Void | PhpType::Never | PhpType::Resource(_) => {
            abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
        }
        PhpType::Iterable
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
        }
    }

    prop_ty
}

/// Lower a `$obj->name` read where `$obj` has type `Object("stdClass")`.
///
/// stdClass has no static property layout, so route the access through the
/// runtime helper `__rt_stdclass_get`. The receiver is already in
/// int_result_reg (x0/rax) at this point thanks to `emit_property_access`.
fn emit_stdclass_property_access(
    property: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) -> PhpType {
    emit_named_dynamic_property_access(
        property,
        emitter,
        data,
        "stdClass",
        "__rt_stdclass_get",
    )
}

/// Lower a `$obj->name` read where `$obj` has type `Mixed`.
///
/// The runtime helper unboxes the Mixed cell, validates that it carries a
/// stdClass instance, and routes to `__rt_stdclass_get`. Other payloads
/// return Mixed(null), matching PHP's "property access on non-object"
/// warning behaviour for the most common idiom (`json_decode($json)->name`).
pub(super) fn emit_mixed_property_access(
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let candidates = declared_property_candidates(property, ctx);
    if candidates.is_empty() {
        return emit_named_dynamic_property_access(
            property,
            emitter,
            data,
            "mixed",
            "__rt_mixed_property_get",
        );
    }

    emitter.comment(&format!("mixed->{}  (class-id dispatch)", property));
    let null_label = ctx.next_label("mixed_prop_null");
    let done_label = ctx.next_label("mixed_prop_done");
    let stdclass_label = ctx.next_label("mixed_prop_stdclass");
    let match_labels: Vec<String> = candidates
        .iter()
        .map(|(class_name, _, _)| {
            ctx.next_label(&format!("mixed_prop_{}", label_fragment(class_name)))
        })
        .collect();

    abi::emit_call_label(emitter, "__rt_mixed_unbox");                          // inspect the boxed receiver before reading a declared property
    emit_object_payload_or_null_branch(&null_label, emitter);
    emit_branch_to_declared_property_candidates(&candidates, &match_labels, emitter);
    emit_branch_to_stdclass_fallback(&stdclass_label, emitter);
    abi::emit_jump(emitter, &null_label);                                      // unknown object class falls through to the null result path

    for ((class_name, prop_ty, _), label) in candidates.into_iter().zip(match_labels) {
        emitter.label(&label);
        let loaded_ty = emit_loaded_object_property_access(&class_name, property, emitter, ctx, data);
        box_dynamic_property_result(&loaded_ty, emitter);
        abi::emit_jump(emitter, &done_label);                                  // finish the mixed property read after boxing the declared slot value
        let _ = prop_ty;
    }

    emitter.label(&stdclass_label);
    emit_static_stdclass_get_from_loaded_object(property, emitter, data);
    abi::emit_jump(emitter, &done_label);                                      // finish after stdClass hash lookup

    emitter.label(&null_label);
    super::emit_boxed_null(emitter);

    emitter.label(&done_label);
    PhpType::Mixed
}

/// Emits named dynamic property access for this module.
fn emit_named_dynamic_property_access(
    property: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
    receiver_label: &str,
    runtime_symbol: &str,
) -> PhpType {
    emitter.comment(&format!("{}->{}  (dynamic)", receiver_label, property));
    let (label, len) = data.add_string(property.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", &label);
            abi::emit_load_int_immediate(emitter, "x2", len as i64);
            emitter.instruction(&format!("bl {}", runtime_symbol));             // call the dynamic-property reader; result Mixed* lands in x0
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // shift the receiver into the SysV first-arg register
            abi::emit_symbol_address(emitter, "rsi", &label);
            abi::emit_load_int_immediate(emitter, "rdx", len as i64);
            emitter.instruction(&format!("call {}", runtime_symbol));           // call the dynamic-property reader; result Mixed* lands in rax
        }
    }
    PhpType::Mixed
}

/// Lowers `$obj->{$expr}` where property name is a dynamic expression.
pub(super) fn emit_dynamic_property_access(
    object: &Expr,
    property: &Expr,
    nullsafe: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let static_obj_ty = functions::infer_contextual_type(object, ctx);
    let static_class = functions::singular_object_class(&static_obj_ty)
        .map(|name| name.to_string());
    let obj_ty = emit_expr(object, emitter, ctx, data);

    if nullsafe && matches!(obj_ty.codegen_repr(), PhpType::Void) {
        super::emit_boxed_null(emitter);
        return PhpType::Mixed;
    }

    let null_label = nullsafe.then(|| ctx.next_label("dynamic_prop_null"));
    let done_label = nullsafe.then(|| ctx.next_label("dynamic_prop_done"));
    if nullsafe && matches!(obj_ty.codegen_repr(), PhpType::Mixed) {
        super::emit_unbox_mixed_object_or_null_branch(
            null_label
                .as_deref()
                .expect("nullsafe dynamic access must have a null label"),
            emitter,
        );
    }

    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // preserve the receiver while the dynamic property-name expression is evaluated
    let property_ty = emit_expr(property, emitter, ctx, data);
    if property_ty != PhpType::Str {
        coerce_result_to_type(emitter, ctx, data, &property_ty, &PhpType::Str);
    }
    let (name_ptr_reg, name_len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, name_ptr_reg, name_len_reg);              // preserve the evaluated property name for each runtime-name comparison

    if let Some(class_name) = static_class
        .or_else(|| match &obj_ty {
            PhpType::Object(class_name) => Some(class_name.clone()),
            _ => None,
        })
    {
        emit_dynamic_declared_property_lookup(&class_name, emitter, ctx, data);
    } else {
        emit_runtime_dynamic_property_get_from_saved_receiver(
            "__rt_mixed_property_get",
            emitter,
        );
    }

    if let (Some(null_label), Some(done_label)) = (null_label, done_label) {
        abi::emit_jump(emitter, &done_label);                                  // skip the nullsafe null branch after a real dynamic-property lookup
        emitter.label(&null_label);
        super::emit_boxed_null(emitter);
        emitter.label(&done_label);
    }

    PhpType::Mixed
}

/// Emits a runtime dispatch over all classes that declare `property`.
///
/// Scans `ctx.classes` for every class that has `property` as a declared
/// property, builds a match table keyed by class id, and falls through to
/// `emit_dynamic_property_miss` when no declared name matches the evaluated
/// dynamic property name.
fn emit_dynamic_declared_property_lookup(
    class_name: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
        emit_runtime_dynamic_property_get_from_saved_receiver("__rt_stdclass_get", emitter);
        return;
    }

    let Some(class_info) = ctx.classes.get(class_name).cloned() else {
        emit_dynamic_property_miss(emitter);
        return;
    };
    let done_label = ctx.next_label("dyn_prop_done");
    let miss_label = ctx.next_label("dyn_prop_miss");
    let candidates: Vec<(String, PhpType)> = class_info
        .properties
        .iter()
        .map(|(name, ty)| (name.clone(), ty.clone()))
        .collect();
    let match_labels: Vec<String> = candidates
        .iter()
        .map(|(name, _)| ctx.next_label(&format!("dyn_prop_{}", label_fragment(name))))
        .collect();

    for ((property_name, _), label) in candidates.iter().zip(match_labels.iter()) {
        emit_branch_if_dynamic_name_matches(property_name, label, emitter, data);
    }
    abi::emit_jump(emitter, &miss_label);                                      // no declared property name matched the evaluated dynamic name

    for ((property_name, _), label) in candidates.into_iter().zip(match_labels) {
        emitter.label(&label);
        abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
        let loaded_ty =
            emit_loaded_object_property_access(class_name, &property_name, emitter, ctx, data);
        box_dynamic_property_result(&loaded_ty, emitter);
        abi::emit_release_temporary_stack(emitter, 32);
        abi::emit_jump(emitter, &done_label);                                  // finish after loading the matching declared property
    }

    emitter.label(&miss_label);
    emit_dynamic_property_miss(emitter);
    emitter.label(&done_label);
}

/// Emits cleanup for a failed dynamic property lookup and returns boxed null.
///
/// Releases the temporary stack slot (32 bytes) and emits a boxed null as the
/// result of a dynamic property access that matched no declared property name.
fn emit_dynamic_property_miss(emitter: &mut Emitter) {
    abi::emit_release_temporary_stack(emitter, 32);
    super::emit_boxed_null(emitter);
}

/// Emits a runtime dynamic property read from a receiver saved on the temporary stack.
///
/// Loads the object pointer (offset 16), property name pointer (offset 0), and
/// name length (offset 8) from the temporary stack and calls `runtime_symbol`
/// (`__rt_mixed_property_get` or `__rt_stdclass_get`). Releases 32 bytes of
/// temporary stack after the call. Result lands in int_result_reg.
fn emit_runtime_dynamic_property_get_from_saved_receiver(
    runtime_symbol: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x0", 16);
            abi::emit_load_temporary_stack_slot(emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(emitter, "x2", 8);
            emitter.instruction(&format!("bl {}", runtime_symbol));             // read the runtime-named property through the object helper
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", 16);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", 0);
            abi::emit_load_temporary_stack_slot(emitter, "rdx", 8);
            emitter.instruction(&format!("call {}", runtime_symbol));           // read the runtime-named property through the object helper
        }
    }
    abi::emit_release_temporary_stack(emitter, 32);
}

/// Emits a runtime string comparison and conditional branch for a declared property name.
///
/// Compares the evaluated dynamic property name (loaded from temporary stack at
/// offsets 0 and 8) against `property` using `__rt_str_eq`. On match, branches
/// to `target_label`. Uses target-specific calling convention for the comparison
/// helper (x0/x1/x2 on ARM64, rdi/rsi/rdx on x86_64).
fn emit_branch_if_dynamic_name_matches(
    property: &str,
    target_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (label, len) = data.add_string(property.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(emitter, "x2", 8);
            abi::emit_symbol_address(emitter, "x3", &label);
            abi::emit_load_int_immediate(emitter, "x4", len as i64);
            emitter.instruction("bl __rt_str_eq");                              // compare the evaluated property name against a declared property name
            emitter.instruction(&format!("cbnz x0, {}", target_label));         // dispatch to the declared property load when the names match
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "rdi", 0);
            abi::emit_load_temporary_stack_slot(emitter, "rsi", 8);
            abi::emit_symbol_address(emitter, "rdx", &label);
            abi::emit_load_int_immediate(emitter, "rcx", len as i64);
            emitter.instruction("call __rt_str_eq");                            // compare the evaluated property name against a declared property name
            emitter.instruction("test rax, rax");                               // check whether the runtime string comparison matched
            emitter.instruction(&format!("jne {}", target_label));              // dispatch to the declared property load when the names match
        }
    }
}

/// Boxes the loaded property value as `PhpType::Mixed` when the result type is not already `Mixed`.
///
/// Consults `result_ty.codegen_repr()` to determine whether boxing is needed;
/// when it is, calls `emit_box_current_value_as_mixed`. Used after loading a
/// declared property through a dynamic name to ensure the result type is consistent
/// with the broader dynamic property access path.
fn box_dynamic_property_result(result_ty: &PhpType, emitter: &mut Emitter) {
    if !matches!(result_ty.codegen_repr(), PhpType::Mixed) {
        crate::codegen::emit_box_current_value_as_mixed(emitter, result_ty);
    }
}

/// Collects all classes that declare `property` as a declared property, sorted by class id.
///
/// Scans `ctx.classes` for every class that has `property` in its `properties` map,
/// returning a vector of `(class_name, property_type, class_id)` sorted by class id.
/// Used by `emit_mixed_property_access` to build a runtime dispatch table for
/// declared property access on `Mixed` receivers.
fn declared_property_candidates(
    property: &str,
    ctx: &Context,
) -> Vec<(String, PhpType, u64)> {
    let mut candidates: Vec<(String, PhpType, u64)> = ctx
        .classes
        .iter()
        .filter_map(|(class_name, class_info)| {
            class_info
                .properties
                .iter()
                .find(|(name, _)| name == property)
                .map(|(_, ty)| (class_name.clone(), ty.clone(), class_info.class_id))
        })
        .collect();
    candidates.sort_by_key(|(_, _, class_id)| *class_id);
    candidates
}

/// Emits a branch to `null_label` when the mixed payload is not an object.
///
/// Unboxes the mixed value in `int_result_reg` (x0/rax) and checks whether the
/// runtime tag equals 6 (object). On non-object, branches to `null_label`; on
/// object, promotes the unboxed pointer to the result register (x0 = x1 on ARM64,
/// rax = rdi on x86_64).
fn emit_object_payload_or_null_branch(null_label: &str, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #6");                                  // runtime tag 6 means the mixed payload is an object
            emitter.instruction(&format!("b.ne {}", null_label));               // non-object mixed receivers read as null for property dispatch
            emitter.instruction("mov x0, x1");                                  // promote the unboxed object pointer into the normal result register
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 6");                                  // runtime tag 6 means the mixed payload is an object
            emitter.instruction(&format!("jne {}", null_label));                // non-object mixed receivers read as null for property dispatch
            emitter.instruction("mov rax, rdi");                                // promote the unboxed object pointer into the normal result register
        }
    }
}

/// Emits a runtime class-id dispatch over declared property candidates.
///
/// Loads the receiver's class id from `int_result_reg` and compares it against
/// each candidate's `class_id` (x9/x10 on ARM64, r11/r10 on x86_64). Jumps to
/// the corresponding `match_label` on equality, falling through when no candidate
/// matches. The caller is responsible for emitting the fallthrough path (typically
/// a miss label or stdclass fallback).
fn emit_branch_to_declared_property_candidates(
    candidates: &[(String, PhpType, u64)],
    match_labels: &[String],
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0]");                                // load the receiver class id for declared-property dispatch
            for ((_, _, class_id), label) in candidates.iter().zip(match_labels) {
                abi::emit_load_int_immediate(emitter, "x10", *class_id as i64);
                emitter.instruction("cmp x9, x10");                             // compare the receiver class id against a class that declares the property
                emitter.instruction(&format!("b.eq {}", label));                // jump to the matching declared-property load
            }
        }
        Arch::X86_64 => {
            emitter.instruction("mov r11, QWORD PTR [rax]");                    // load the receiver class id for declared-property dispatch
            for ((_, _, class_id), label) in candidates.iter().zip(match_labels) {
                abi::emit_load_int_immediate(emitter, "r10", *class_id as i64);
                emitter.instruction("cmp r11, r10");                            // compare the receiver class id against a class that declares the property
                emitter.instruction(&format!("je {}", label));                  // jump to the matching declared-property load
            }
        }
    }
}

/// Emits a branch to `label` when the receiver's class id matches stdClass's sentinel.
///
/// Reloads the class id from the object in `int_result_reg` and compares it
/// against the compile-time `_stdclass_class_id` sentinel via `emit_symbol_address`.
/// On match, branches to `label` to route the read through `__rt_stdclass_get`.
/// Used by `emit_mixed_property_access` to distinguish stdClass dynamic storage
/// from other object types before falling through to the null result path.
fn emit_branch_to_stdclass_fallback(label: &str, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x10, [x0]");                               // reload the receiver class id before the stdClass fallback check
            abi::emit_symbol_address(emitter, "x11", "_stdclass_class_id");
            emitter.instruction("ldr x11, [x11]");                              // load the compile-time stdClass class id sentinel
            emitter.instruction("cmp x10, x11");                                // check whether the object uses stdClass dynamic storage
            emitter.instruction(&format!("b.eq {}", label));                    // route stdClass property reads through the hash-backed helper
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rax]");                    // reload the receiver class id before the stdClass fallback check
            emitter.instruction("mov r11, QWORD PTR [rip + _stdclass_class_id]"); // load the compile-time stdClass class id sentinel
            emitter.instruction("cmp r10, r11");                                // check whether the object uses stdClass dynamic storage
            emitter.instruction(&format!("je {}", label));                      // route stdClass property reads through the hash-backed helper
        }
    }
}

/// Emits a static stdClass property read from an already-unboxed object.
///
/// The object pointer is expected in `int_result_reg` (x0/rax). Emits the property
/// name as a runtime string and calls `__rt_stdclass_get`. On ARM64 passes arguments
/// in x0/x1/x2; on x86_64 passes in rdi/rsi/rdx. Result is `PhpType::Mixed`.
fn emit_static_stdclass_get_from_loaded_object(
    property: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (label, len) = data.add_string(property.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", &label);
            abi::emit_load_int_immediate(emitter, "x2", len as i64);
            emitter.instruction("bl __rt_stdclass_get");                        // read the static property name from stdClass dynamic storage
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // pass the unboxed stdClass object pointer as the first helper argument
            abi::emit_symbol_address(emitter, "rsi", &label);
            abi::emit_load_int_immediate(emitter, "rdx", len as i64);
            emitter.instruction("call __rt_stdclass_get");                      // read the static property name from stdClass dynamic storage
        }
    }
}

/// Converts a property or class name into a label-safe fragment.
///
/// Replaces every non-alphanumeric character with `_` so the result can be used
/// in asm label names without冲突. Used to construct readable dispatch label
/// names like `mixed_prop_stdclass` or `dyn_prop_myProp`.
fn label_fragment(name: &str) -> String {
    name.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

/// Lowers `?Class->property` with a nullable receiver that may be null at runtime.
pub(super) fn emit_nullable_object_property_access(
    class_name: &str,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let null_label = ctx.next_label("nullable_prop_null");
    let done_label = ctx.next_label("nullable_prop_done");
    let message = format!("Warning: Attempt to read property \"{}\" on null\n", property);

    super::emit_unbox_mixed_object_or_null_branch(&null_label, emitter);
    let property_ty = emit_loaded_object_property_access(class_name, property, emitter, ctx, data);
    super::box_nullable_result(&property_ty, emitter);
    abi::emit_jump(emitter, &done_label);                                      // skip the nullable property null path after a real property read

    emitter.label(&null_label);
    super::emit_runtime_warning(message.as_bytes(), emitter, data);
    super::emit_boxed_null(emitter);

    emitter.label(&done_label);
    PhpType::Mixed
}

/// Lowers `$obj->property` where the class is loaded and property is declared.
pub(super) fn emit_loaded_object_property_access(
    class_name: &str,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if crate::types::checker::builtin_stdclass::is_stdclass(class_name) {
        return emit_stdclass_property_access(property, emitter, data);
    }
    let class_info = match ctx.classes.get(class_name).cloned() {
        Some(c) => c,
        None => {
            emitter.comment(&format!("WARNING: undefined class {}", class_name));
            return PhpType::Int;
        }
    };

    let prop_ty = match class_info
        .properties
        .iter()
        .find(|(n, _)| n == property)
        .map(|(_, t)| t.clone())
    {
        Some(v) => v,
        None => {
            if class_info.methods.contains_key("__get") {
                emitter.comment(&format!("magic __get('{}')", property));
                let object_reg = abi::symbol_scratch_reg(emitter);
                emitter.instruction(&format!("mov {}, {}", object_reg, abi::int_result_reg(emitter))); // preserve $this while the magic-property name setup clobbers normal result registers
                super::push_magic_property_name_arg(property, emitter, data);
                abi::emit_push_reg(emitter, object_reg);                      // push $this pointer for __get dispatch using the preserved object register
                return super::emit_method_call_with_pushed_args(
                    class_name,
                    "__get",
                    &[PhpType::Str],
                    emitter,
                    ctx,
                );
            }
            if class_info.allow_dynamic_properties {
                let dyn_slot_offset = 8 + class_info.properties.len() * 16;
                return crate::codegen::stmt::emit_dynamic_property_get(
                    property,
                    dyn_slot_offset,
                    emitter,
                    ctx,
                    data,
                );
            }
            emitter.comment(&format!("WARNING: undefined property {}", property));
            return PhpType::Int;
        }
    };
    let offset = match class_info.property_offsets.get(property) {
        Some(offset) => *offset,
        None => {
            emitter.comment(&format!("WARNING: missing property offset {}", property));
            return PhpType::Int;
        }
    };

    emit_loaded_object_property_value(
        class_name,
        property,
        prop_ty,
        offset,
        class_info.declared_properties.contains(property),
        false,
        class_info.reference_properties.contains(property),
        ctx,
        data,
        emitter,
    )
}

/// Lowers a declared property load for a known class and property offset.
///
/// Called by `emit_loaded_object_property_access` once the class info, property
/// type, and memory offset have all been resolved. Emits a comment describing
/// the access path, optionally guards uninitialized typed properties, handles
/// reference vs value semantics, and loads the property payload into the
/// appropriate result register(s) based on `prop_ty`. Returns the loaded `PhpType`.
fn emit_loaded_object_property_value(
    class_name: &str,
    property: &str,
    prop_ty: PhpType,
    offset: usize,
    is_declared: bool,
    needs_deref: bool,
    is_reference: bool,
    ctx: &mut Context,
    data: &mut DataSection,
    emitter: &mut Emitter,
) -> PhpType {
    if needs_deref {
        abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");               // abort with fatal error on null pointer dereference
        emitter.comment(&format!(
            "->{} via ptr<{}> (offset {})",
            property, class_name, offset
        ));
    } else {
        emitter.comment(&format!("->{}  (offset {})", property, offset));
    }

    let object_reg = abi::int_result_reg(emitter);

    if is_declared {
        emit_uninitialized_typed_property_guard(
            class_name, property, offset, object_reg, emitter, ctx, data,
        );
    }

    if is_reference {
        let pointer_reg = abi::symbol_scratch_reg(emitter);
        abi::emit_load_from_address(emitter, pointer_reg, object_reg, offset);
        match &prop_ty {
            PhpType::Str => {
                let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                abi::emit_load_from_address(emitter, ptr_reg, pointer_reg, 0);
                abi::emit_load_from_address(emitter, len_reg, pointer_reg, 8);
            }
            PhpType::Float => {
                abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), pointer_reg, 0);
            }
            PhpType::Bool | PhpType::Int | PhpType::Void | PhpType::Never | PhpType::Resource(_) => {
                abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0);
            }
            PhpType::Iterable
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Buffer(_)
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_) => {
                abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), pointer_reg, 0);
            }
        }
        return prop_ty;
    }

    match &prop_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            let base_reg = abi::symbol_scratch_reg(emitter);
            emitter.instruction(&format!("mov {}, {}", base_reg, object_reg));  // preserve the object base pointer while loading the two-word string property payload
            abi::emit_load_from_address(emitter, ptr_reg, base_reg, offset);
            abi::emit_load_from_address(emitter, len_reg, base_reg, offset + 8);
        }
        PhpType::Float => {
            abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), object_reg, offset);
        }
        PhpType::Bool | PhpType::Int | PhpType::Void | PhpType::Never | PhpType::Resource(_) => {
            abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
        }
        PhpType::Iterable
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), object_reg, offset);
        }
    }

    prop_ty
}

/// Emits a guard that aborts when a typed property has not been initialized.
///
/// Loads the marker word at `offset + 8` from `object_reg` and compares it against
/// `UNINITIALIZED_TYPED_PROPERTY_SENTINEL`. When the sentinel is detected, falls
/// through to `emit_uninitialized_typed_property_fatal`; otherwise jumps to
/// `initialized_label` to continue the property read. Used for typed properties
/// that must not be accessed before initialization, matching PHP's own runtime
/// behavior.
fn emit_uninitialized_typed_property_guard(
    class_name: &str,
    property: &str,
    offset: usize,
    object_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let initialized_label = ctx.next_label("typed_prop_initialized");
    let marker_reg = abi::secondary_scratch_reg(emitter);
    let sentinel_reg = abi::tertiary_scratch_reg(emitter);
    abi::emit_load_from_address(emitter, marker_reg, object_reg, offset + 8);
    abi::emit_load_int_immediate(emitter, sentinel_reg, UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // check whether the typed property still carries the uninitialized marker
            emitter.instruction(&format!("b.ne {}", initialized_label));        // continue the property read once the slot has been initialized
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", marker_reg, sentinel_reg)); // check whether the typed property still carries the uninitialized marker
            emitter.instruction(&format!("jne {}", initialized_label));         // continue the property read once the slot has been initialized
        }
    }
    emit_uninitialized_typed_property_fatal(class_name, property, emitter, data);
    emitter.label(&initialized_label);
}

/// Emits a fatal runtime error and terminates the program.
///
/// Formats the message "Fatal error: Typed property {class_name}::{property} must
/// not be accessed before initialization" and emits it to stderr via the `write`
/// syscall, then calls `exit(1)`. Emits platform-specific syscalls directly (ARM64
/// uses x0=fd, x1=buf, x2=len with syscall 4/1; x86_64 uses rdi=fd, rsi=buf,
/// rdx=len with syscall 1/60). Called by `emit_uninitialized_typed_property_guard`
/// when the sentinel marker is detected.
fn emit_uninitialized_typed_property_fatal(
    class_name: &str,
    property: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let message = format!(
        "Fatal error: Typed property {}::${} must not be accessed before initialization\n",
        class_name, property
    );
    let (label, len) = data.add_string(message.as_bytes());
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // fd = stderr for the typed-property initialization fatal
            abi::emit_symbol_address(emitter, "x1", &label);                    // point write() at the typed-property initialization diagnostic
            emitter.instruction(&format!("mov x2, #{}", len));                  // pass the diagnostic byte length to write()
            emitter.syscall(4);
            emitter.instruction("mov x0, #1");                                  // exit status 1 indicates abnormal termination
            emitter.syscall(1);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", &label);                   // point write() at the typed-property initialization diagnostic
            emitter.instruction(&format!("mov edx, {}", len));                  // pass the diagnostic byte length to write()
            emitter.instruction("mov edi, 2");                                  // fd = stderr for the typed-property initialization fatal
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal diagnostic before terminating
            emitter.instruction("mov edi, 1");                                  // exit status 1 indicates abnormal termination
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall");                                     // terminate after the typed-property initialization fatal
        }
    }
}
