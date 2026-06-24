//! Purpose:
//! Emits user-assembly helpers that let libelephc-magician access properties on
//! native objects using the current module's class metadata.
//!
//! Called from:
//! - `crate::codegen_ir::finalize_user_asm()` when an EIR module uses eval.
//!
//! Key details:
//! - The cacheable runtime object cannot know user class ids or property
//!   offsets, so these C-ABI symbols are emitted into the user assembly.
//! - The first slice supports public declared properties with scalar/Mixed
//!   storage, which covers `$this->prop` inside eval-called native methods.

use std::collections::BTreeMap;

use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime_value_tag;
use crate::ir::{Function, LocalKind, Module};
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, PhpType};

/// Property slot metadata needed by eval property bridge dispatch.
#[derive(Clone)]
struct EvalPropertySlot {
    class_id: u64,
    class_name: String,
    property: String,
    offset: usize,
    ty: PhpType,
}

/// Emits eval property helpers when any lowered function owns an eval context.
pub(super) fn emit_eval_property_helpers(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    if !module_uses_eval(module) {
        return;
    }
    let slots = collect_eval_property_slots(module);
    emit_property_get_helper(module, emitter, data, &slots);
    emit_property_set_helper(module, emitter, data, &slots);
}

/// Returns true when the EIR module contains a function that can call eval.
fn module_uses_eval(module: &Module) -> bool {
    all_module_functions(module).any(function_uses_eval)
}

/// Iterates every EIR function body emitted or inspected by the backend.
fn all_module_functions(module: &Module) -> impl Iterator<Item = &Function> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
}

/// Returns true when a function has hidden eval state locals.
fn function_uses_eval(function: &Function) -> bool {
    function
        .locals
        .iter()
        .any(|local| {
            matches!(
                local.kind,
                LocalKind::EvalContext | LocalKind::EvalScope | LocalKind::EvalGlobalScope
            )
        })
}

/// Collects public declared properties with storage layouts the bridge can access.
fn collect_eval_property_slots(module: &Module) -> Vec<EvalPropertySlot> {
    let mut slots = Vec::new();
    let mut classes = module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_class_property_slots(class_name, class_info, &mut slots);
    }
    slots
}

/// Adds bridge-supported public properties for one class.
fn collect_class_property_slots(
    class_name: &str,
    class_info: &ClassInfo,
    slots: &mut Vec<EvalPropertySlot>,
) {
    for (index, (property, ty)) in class_info.properties.iter().enumerate() {
        if class_info.visible_property_index(property) != Some(index) {
            continue;
        }
        if !property_is_public(class_info, property) || !property_type_supported(ty) {
            continue;
        }
        slots.push(EvalPropertySlot {
            class_id: class_info.class_id,
            class_name: class_name.to_string(),
            property: property.clone(),
            offset: 8 + index * 16,
            ty: ty.codegen_repr(),
        });
    }
}

/// Returns true when the property is visible to PHP eval from method scope.
fn property_is_public(class_info: &ClassInfo, property: &str) -> bool {
    class_info
        .property_visibilities
        .get(property)
        .is_none_or(|visibility| matches!(visibility, Visibility::Public))
}

/// Returns true for property storage shapes the bridge can box and update.
fn property_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::TaggedScalar
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Object(_)
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
    )
}

/// Emits `__elephc_eval_value_property_get(Mixed*, name, len) -> Mixed*`.
fn emit_property_get_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user property get ---");
    label_c_global(module, emitter, "__elephc_eval_value_property_get");
    match module.target.arch {
        Arch::AArch64 => emit_property_get_aarch64(module, emitter, data, slots),
        Arch::X86_64 => emit_property_get_x86_64(module, emitter, data, slots),
    }
}

/// Emits `__elephc_eval_value_property_set(Mixed*, name, len, Mixed*) -> bool`.
fn emit_property_set_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user property set ---");
    label_c_global(module, emitter, "__elephc_eval_value_property_set");
    match module.target.arch {
        Arch::AArch64 => emit_property_set_aarch64(module, emitter, data, slots),
        Arch::X86_64 => emit_property_set_x86_64(module, emitter, data, slots),
    }
}

/// Emits the ARM64 property-get helper body.
fn emit_property_get_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
) {
    let null_label = "__elephc_eval_value_property_get_null";
    let done_label = "__elephc_eval_value_property_get_done";
    emitter.instruction("sub sp, sp, #64");                                     // reserve helper frame for saved name/object and fp/lr
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the requested property-name pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the requested property-name length
    emitter.instruction("str x0, [sp, #24]");                                   // save the boxed receiver for stdClass fallback reads
    emitter.instruction(&format!("cbz x0, {}", null_label));                    // null Mixed receiver reads as PHP null
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose receiver tag and object payload
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("b.ne {}", null_label));                       // non-object receivers read as PHP null
    emitter.instruction("str x1, [sp, #16]");                                   // save the unboxed object pointer for property loads
    emitter.instruction("ldr x9, [x1]");                                        // load the object's runtime class id
    emit_aarch64_property_dispatch(module, emitter, data, slots, "get");
    emit_aarch64_stdclass_property_get_fallback(emitter);
    emitter.instruction(&format!("b {}", done_label));                          // return after stdClass fallback get or null result
    emit_aarch64_get_slot_bodies(module, emitter, slots, done_label);
    emitter.label(null_label);
    let null_symbol = module.target.extern_symbol("__elephc_eval_value_null");
    abi::emit_call_label(emitter, &null_symbol);
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed property value to Rust
}

/// Emits the x86_64 property-get helper body.
fn emit_property_get_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
) {
    let null_label = "__elephc_eval_value_property_get_null_x";
    let done_label = "__elephc_eval_value_property_get_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for name, length, and object
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the requested property-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the requested property-name length
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the boxed receiver for stdClass fallback reads
    emitter.instruction(&format!("test rdi, rdi"));                             // check whether the boxed receiver pointer is null
    emitter.instruction(&format!("jz {}", null_label));                         // null Mixed receiver reads as PHP null
    emitter.instruction("mov rax, rdi");                                        // move the receiver into the mixed-unbox input register
    emitter.instruction("call __rt_mixed_unbox");                               // expose receiver tag and object payload
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("jne {}", null_label));                        // non-object receivers read as PHP null
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the unboxed object pointer for property loads
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // load the object's runtime class id
    emit_x86_64_property_dispatch(module, emitter, data, slots, "get");
    emit_x86_64_stdclass_property_get_fallback(emitter);
    emitter.instruction(&format!("jmp {}", done_label));                        // return after stdClass fallback get or null result
    emit_x86_64_get_slot_bodies(module, emitter, slots, done_label);
    emitter.label(null_label);
    let null_symbol = module.target.extern_symbol("__elephc_eval_value_null");
    abi::emit_call_label(emitter, &null_symbol);
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed property value to Rust
}

/// Emits the ARM64 property-set helper body.
fn emit_property_set_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
) {
    let fail_label = "__elephc_eval_value_property_set_fail";
    let done_label = "__elephc_eval_value_property_set_done";
    emitter.instruction("sub sp, sp, #80");                                     // reserve helper frame for inputs, object, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #64");                                    // establish a stable helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the requested property-name pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the requested property-name length
    emitter.instruction("str x3, [sp, #24]");                                   // save the boxed value being assigned
    emitter.instruction("str x0, [sp, #32]");                                   // save the boxed receiver for stdClass fallback writes
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // null Mixed receiver cannot accept a property write
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose receiver tag and object payload
    emitter.instruction("cmp x0, #6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("b.ne {}", fail_label));                       // non-object receivers reject the property write
    emitter.instruction("str x1, [sp, #16]");                                   // save the unboxed object pointer for property stores
    emitter.instruction("ldr x9, [x1]");                                        // load the object's runtime class id
    emit_aarch64_property_dispatch(module, emitter, data, slots, "set");
    emit_aarch64_stdclass_property_set_fallback(module, emitter, fail_label, done_label);
    emit_aarch64_set_slot_bodies(module, emitter, slots, done_label);
    emitter.label(fail_label);
    emitter.instruction("mov x0, #0");                                          // report a failed eval property write to Rust
    emitter.instruction(&format!("b {}", done_label));                          // join the helper epilogue after failure
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the write-status flag to Rust
}

/// Emits the x86_64 property-set helper body.
fn emit_property_set_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
) {
    let fail_label = "__elephc_eval_value_property_set_fail_x";
    let done_label = "__elephc_eval_value_property_set_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for name, length, object, and value
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the requested property-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the requested property-name length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the boxed value being assigned
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save the boxed receiver for stdClass fallback writes
    emitter.instruction("test rdi, rdi");                                       // check whether the boxed receiver pointer is null
    emitter.instruction(&format!("jz {}", fail_label));                         // null Mixed receiver cannot accept a property write
    emitter.instruction("mov rax, rdi");                                        // move the receiver into the mixed-unbox input register
    emitter.instruction("call __rt_mixed_unbox");                               // expose receiver tag and object payload
    emitter.instruction("cmp rax, 6");                                          // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("jne {}", fail_label));                        // non-object receivers reject the property write
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the unboxed object pointer for property stores
    emitter.instruction("mov r11, QWORD PTR [rdi]");                            // load the object's runtime class id
    emit_x86_64_property_dispatch(module, emitter, data, slots, "set");
    emit_x86_64_stdclass_property_set_fallback(module, emitter, fail_label, done_label);
    emit_x86_64_set_slot_bodies(module, emitter, slots, done_label);
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax");                                        // report a failed eval property write to Rust
    emitter.instruction(&format!("jmp {}", done_label));                        // join the helper epilogue after failure
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the write-status flag to Rust
}

/// Emits an ARM64 fallback read for stdClass dynamic properties.
fn emit_aarch64_stdclass_property_get_fallback(emitter: &mut Emitter) {
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed receiver for the Mixed stdClass getter
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload requested property-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload requested property-name length
    emitter.instruction("bl __rt_mixed_property_get");                          // read stdClass dynamic property or return Mixed(null)
}

/// Emits an x86_64 fallback read for stdClass dynamic properties.
fn emit_x86_64_stdclass_property_get_fallback(emitter: &mut Emitter) {
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the boxed receiver for the Mixed stdClass getter
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload requested property-name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload requested property-name length
    emitter.instruction("call __rt_mixed_property_get");                        // read stdClass dynamic property or return Mixed(null)
}

/// Emits an ARM64 fallback write for stdClass dynamic properties.
fn emit_aarch64_stdclass_property_set_fallback(
    module: &Module,
    emitter: &mut Emitter,
    fail_label: &str,
    done_label: &str,
) {
    let Some(class_id) = stdclass_class_id(module) else {
        emitter.instruction(&format!("b {}", fail_label));                      // reject writes when stdClass metadata is unavailable
        return;
    };
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the unboxed object pointer for stdClass class check
    emitter.instruction("ldr x9, [x9]");                                        // load the object's runtime class id
    abi::emit_load_int_immediate(emitter, "x10", class_id as i64);
    emitter.instruction("cmp x9, x10");                                         // check whether the receiver is stdClass
    emitter.instruction(&format!("b.ne {}", fail_label));                       // non-stdClass misses remain unsupported eval writes
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the boxed receiver for the Mixed stdClass setter
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload requested property-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload requested property-name length
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload the boxed value being assigned
    emitter.instruction("bl __rt_mixed_property_set");                          // write the stdClass dynamic property
    emitter.instruction("mov x0, #1");                                          // report a successful eval property write to Rust
    emitter.instruction(&format!("b {}", done_label));                          // join the helper epilogue after stdClass write
}

/// Emits an x86_64 fallback write for stdClass dynamic properties.
fn emit_x86_64_stdclass_property_set_fallback(
    module: &Module,
    emitter: &mut Emitter,
    fail_label: &str,
    done_label: &str,
) {
    let Some(class_id) = stdclass_class_id(module) else {
        emitter.instruction(&format!("jmp {}", fail_label));                    // reject writes when stdClass metadata is unavailable
        return;
    };
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the unboxed object pointer for stdClass class check
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the object's runtime class id
    abi::emit_load_int_immediate(emitter, "r10", class_id as i64);
    emitter.instruction("cmp r11, r10");                                        // check whether the receiver is stdClass
    emitter.instruction(&format!("jne {}", fail_label));                        // non-stdClass misses remain unsupported eval writes
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the boxed receiver for the Mixed stdClass setter
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload requested property-name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload requested property-name length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the boxed value being assigned
    emitter.instruction("call __rt_mixed_property_set");                        // write the stdClass dynamic property
    emitter.instruction("mov rax, 1");                                          // report a successful eval property write to Rust
    emitter.instruction(&format!("jmp {}", done_label));                        // join the helper epilogue after stdClass write
}

/// Returns the runtime class id for builtin `stdClass` in this module.
fn stdclass_class_id(module: &Module) -> Option<u64> {
    module
        .class_infos
        .iter()
        .find(|(class_name, _)| crate::types::checker::builtin_stdclass::is_stdclass(class_name))
        .map(|(_, class_info)| class_info.class_id)
}

/// Emits ARM64 class-id and property-name dispatch for helper slot bodies.
fn emit_aarch64_property_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    mode: &str,
) {
    for (class_id, class_slots) in grouped_slots(slots) {
        let class_label = format!("__elephc_eval_property_{}_class_{}", mode, class_id);
        let next_label = format!("__elephc_eval_property_{}_next_{}", mode, class_id);
        abi::emit_load_int_immediate(emitter, "x10", class_id as i64);
        emitter.instruction("cmp x9, x10");                                     // compare receiver class id against this eval bridge class
        emitter.instruction(&format!("b.ne {}", next_label));                   // try the next class when ids differ
        for slot in class_slots {
            emit_aarch64_property_name_compare(module, emitter, data, slot, mode);
        }
        emitter.label(&class_label);
        emitter.instruction(&format!("b {}", next_label));                      // fall through to the next class after a name miss
        emitter.label(&next_label);
    }
}

/// Emits x86_64 class-id and property-name dispatch for helper slot bodies.
fn emit_x86_64_property_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    mode: &str,
) {
    for (class_id, class_slots) in grouped_slots(slots) {
        let class_label = format!("__elephc_eval_property_{}_class_{}_x", mode, class_id);
        let next_label = format!("__elephc_eval_property_{}_next_{}_x", mode, class_id);
        abi::emit_load_int_immediate(emitter, "r10", class_id as i64);
        emitter.instruction("cmp r11, r10");                                    // compare receiver class id against this eval bridge class
        emitter.instruction(&format!("jne {}", next_label));                    // try the next class when ids differ
        for slot in class_slots {
            emit_x86_64_property_name_compare(module, emitter, data, slot, mode);
        }
        emitter.label(&class_label);
        emitter.instruction(&format!("jmp {}", next_label));                    // fall through to the next class after a name miss
        emitter.label(&next_label);
    }
}

/// Emits one ARM64 property-name comparison and branch to the matching body.
fn emit_aarch64_property_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    mode: &str,
) {
    let (label, len) = data.add_string(slot.property.as_bytes());
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload requested property-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload requested property-name length
    abi::emit_symbol_address(emitter, "x3", &label);
    abi::emit_load_int_immediate(emitter, "x4", len as i64);
    emitter.instruction("bl __rt_str_eq");                                      // compare requested property name with this declared property
    let target_label = slot_body_label(module, slot, mode);
    emitter.instruction(&format!("cbnz x0, {}", target_label));                 // dispatch to the property body when the names match
}

/// Emits one x86_64 property-name comparison and branch to the matching body.
fn emit_x86_64_property_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    mode: &str,
) {
    let (label, len) = data.add_string(slot.property.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload requested property-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload requested property-name length
    abi::emit_symbol_address(emitter, "rdx", &label);
    abi::emit_load_int_immediate(emitter, "rcx", len as i64);
    emitter.instruction("call __rt_str_eq");                                    // compare requested property name with this declared property
    emitter.instruction("test rax, rax");                                       // check whether the property names matched
    emitter.instruction(&format!("jne {}", slot_body_label(module, slot, mode))); //dispatch to the property body when the names match
}

/// Emits ARM64 property-get bodies for every bridge-supported property slot.
fn emit_aarch64_get_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalPropertySlot],
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "get"));
        emit_aarch64_box_property_slot(emitter, slot);
        emitter.instruction(&format!("b {}", done_label));                      // return after boxing the declared property value
    }
}

/// Emits x86_64 property-get bodies for every bridge-supported property slot.
fn emit_x86_64_get_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalPropertySlot],
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "get"));
        emit_x86_64_box_property_slot(emitter, slot);
        emitter.instruction(&format!("jmp {}", done_label));                    // return after boxing the declared property value
    }
}

/// Emits ARM64 property-set bodies for every bridge-supported property slot.
fn emit_aarch64_set_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalPropertySlot],
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "set"));
        emit_aarch64_store_property_slot(emitter, slot);
        emitter.instruction("mov x0, #1");                                      // report a successful eval property write to Rust
        emitter.instruction(&format!("b {}", done_label));                      // return after storing the declared property value
    }
}

/// Emits x86_64 property-set bodies for every bridge-supported property slot.
fn emit_x86_64_set_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalPropertySlot],
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "set"));
        emit_x86_64_store_property_slot(emitter, slot);
        emitter.instruction("mov rax, 1");                                      // report a successful eval property write to Rust
        emitter.instruction(&format!("jmp {}", done_label));                    // return after storing the declared property value
    }
}

/// Boxes a property value loaded from an ARM64 object slot into a Mixed cell.
fn emit_aarch64_box_property_slot(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the unboxed object pointer
    match slot.ty.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } => {
            emitter.instruction(&format!("ldr x1, [x9, #{}]", slot.offset));    // load the property payload low word
            emitter.instruction("mov x2, xzr");                                 // heap/scalar property payloads do not use a high word here
            abi::emit_load_int_immediate(emitter, "x0", runtime_value_tag(&slot.ty) as i64);
            emitter.instruction("bl __rt_mixed_from_value");                    // box the property payload as a Mixed cell
        }
        PhpType::Float => {
            emitter.instruction(&format!("ldr d0, [x9, #{}]", slot.offset));    // load the floating property payload
            emitter.instruction("fmov x1, d0");                                 // move float bits into the Mixed low payload word
            emitter.instruction("mov x2, xzr");                                 // float payloads do not use a high word
            emitter.instruction("mov x0, #2");                                  // runtime tag 2 = float
            emitter.instruction("bl __rt_mixed_from_value");                    // box the floating property payload as Mixed
        }
        PhpType::Str => {
            emitter.instruction(&format!("ldr x1, [x9, #{}]", slot.offset));    // load the string property pointer
            emitter.instruction(&format!("ldr x2, [x9, #{}]", slot.offset + 8)); //load the string property length
            emitter.instruction("mov x0, #1");                                  // runtime tag 1 = string
            emitter.instruction("bl __rt_mixed_from_value");                    // persist and box the string property payload
        }
        PhpType::TaggedScalar => {
            emitter.instruction(&format!("ldr x0, [x9, #{}]", slot.offset));    // load the nullable integer property payload
            emitter.instruction(&format!("ldr x1, [x9, #{}]", slot.offset + 8)); //load the nullable integer property tag
            emit_box_current_value_as_mixed(emitter, &PhpType::TaggedScalar);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            let null_label = format!("{}_mixed_null", label_fragment(&slot_body_label_raw(slot, "get")));
            let done_label = format!("{}_mixed_done", label_fragment(&slot_body_label_raw(slot, "get")));
            emitter.instruction(&format!("ldr x0, [x9, #{}]", slot.offset));    // load the stored Mixed property cell
            emitter.instruction(&format!("cbz x0, {}", null_label));            // null property storage reads as PHP null
            emitter.instruction("bl __rt_incref");                              // retain the stored Mixed cell for the eval caller
            emitter.instruction(&format!("b {}", done_label));                  // skip null materialization after a retained hit
            emitter.label(&null_label);
            let null_symbol = emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(emitter, &null_symbol);
            emitter.label(&done_label);
        }
        _ => {
            let null_symbol = emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(emitter, &null_symbol);
        }
    }
}

/// Boxes a property value loaded from an x86_64 object slot into a Mixed cell.
fn emit_x86_64_box_property_slot(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the unboxed object pointer
    match slot.ty.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } => {
            emitter.instruction(&format!("mov rdi, QWORD PTR [r11 + {}]", slot.offset)); //load the property payload low word
            emitter.instruction("xor esi, esi");                                // heap/scalar property payloads do not use a high word here
            abi::emit_load_int_immediate(emitter, "rax", runtime_value_tag(&slot.ty) as i64);
            emitter.instruction("call __rt_mixed_from_value");                  // box the property payload as a Mixed cell
        }
        PhpType::Float => {
            emitter.instruction(&format!("movsd xmm0, QWORD PTR [r11 + {}]", slot.offset)); //load the floating property payload
            emitter.instruction("movq rdi, xmm0");                              // move float bits into the Mixed low payload word
            emitter.instruction("xor esi, esi");                                // float payloads do not use a high word
            emitter.instruction("mov eax, 2");                                  // runtime tag 2 = float
            emitter.instruction("call __rt_mixed_from_value");                  // box the floating property payload as Mixed
        }
        PhpType::Str => {
            emitter.instruction(&format!("mov rdi, QWORD PTR [r11 + {}]", slot.offset)); //load the string property pointer
            emitter.instruction(&format!("mov rsi, QWORD PTR [r11 + {}]", slot.offset + 8)); //load the string property length
            emitter.instruction("mov eax, 1");                                  // runtime tag 1 = string
            emitter.instruction("call __rt_mixed_from_value");                  // persist and box the string property payload
        }
        PhpType::TaggedScalar => {
            emitter.instruction(&format!("mov rax, QWORD PTR [r11 + {}]", slot.offset)); //load the nullable integer property payload
            emitter.instruction(&format!("mov rdx, QWORD PTR [r11 + {}]", slot.offset + 8)); //load the nullable integer property tag
            emit_box_current_value_as_mixed(emitter, &PhpType::TaggedScalar);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            let null_label = format!("{}_mixed_null_x", label_fragment(&slot_body_label_raw(slot, "get")));
            let done_label = format!("{}_mixed_done_x", label_fragment(&slot_body_label_raw(slot, "get")));
            emitter.instruction(&format!("mov rax, QWORD PTR [r11 + {}]", slot.offset)); //load the stored Mixed property cell
            emitter.instruction("test rax, rax");                               // check whether the property storage is initialized
            emitter.instruction(&format!("jz {}", null_label));                 // null property storage reads as PHP null
            emitter.instruction("call __rt_incref");                            // retain the stored Mixed cell for the eval caller
            emitter.instruction(&format!("jmp {}", done_label));                // skip null materialization after a retained hit
            emitter.label(&null_label);
            let null_symbol = emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(emitter, &null_symbol);
            emitter.label(&done_label);
        }
        _ => {
            let null_symbol = emitter.target.extern_symbol("__elephc_eval_value_null");
            abi::emit_call_label(emitter, &null_symbol);
        }
    }
}

/// Stores a boxed Mixed eval value into an ARM64 object property slot.
fn emit_aarch64_store_property_slot(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    match slot.ty.codegen_repr() {
        PhpType::Int => emit_aarch64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_int", "x0"),
        PhpType::Bool => emit_aarch64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_bool", "x0"),
        PhpType::Float => {
            emitter.instruction("ldr x0, [sp, #24]");                           // reload the boxed eval value for float coercion
            emitter.instruction("bl __rt_mixed_cast_float");                    // coerce the eval value to a PHP float
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the unboxed object pointer for the store
            emitter.instruction(&format!("str d0, [x9, #{}]", slot.offset));    // store the coerced float into the property slot
        }
        PhpType::Str => {
            emitter.instruction("ldr x0, [sp, #24]");                           // reload the boxed eval value for string coercion
            emitter.instruction("bl __rt_mixed_cast_string");                   // coerce the eval value to a PHP string pair
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the unboxed object pointer for the store
            emitter.instruction(&format!("str x1, [x9, #{}]", slot.offset));    // store the coerced string pointer into the property slot
            emitter.instruction(&format!("str x2, [x9, #{}]", slot.offset + 8)); //store the coerced string length into the property slot
        }
        PhpType::TaggedScalar => emit_aarch64_store_tagged_scalar_property(emitter, slot),
        PhpType::Mixed | PhpType::Union(_) => {
            emitter.instruction("ldr x0, [sp, #24]");                           // reload the boxed eval value being assigned
            emitter.instruction("bl __rt_incref");                              // retain the Mixed cell for property ownership
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the unboxed object pointer for the store
            emitter.instruction(&format!("str x0, [x9, #{}]", slot.offset));    // store the retained Mixed cell into the property slot
            emitter.instruction(&format!("str xzr, [x9, #{}]", slot.offset + 8)); //clear the unused property high word
        }
        _ => {}
    }
}

/// Stores a boxed Mixed eval value into an x86_64 object property slot.
fn emit_x86_64_store_property_slot(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    match slot.ty.codegen_repr() {
        PhpType::Int => emit_x86_64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_int", "rax"),
        PhpType::Bool => emit_x86_64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_bool", "rax"),
        PhpType::Float => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // reload the boxed eval value for float coercion
            emitter.instruction("call __rt_mixed_cast_float");                  // coerce the eval value to a PHP float
            emitter.instruction("mov r11, QWORD PTR [rbp - 24]");               // reload the unboxed object pointer for the store
            emitter.instruction(&format!("movsd QWORD PTR [r11 + {}], xmm0", slot.offset)); //store the coerced float into the property slot
        }
        PhpType::Str => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // reload the boxed eval value for string coercion
            emitter.instruction("call __rt_mixed_cast_string");                 // coerce the eval value to a PHP string pair
            emitter.instruction("mov r11, QWORD PTR [rbp - 24]");               // reload the unboxed object pointer for the store
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", slot.offset)); //store the coerced string pointer into the property slot
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rdx", slot.offset + 8)); //store the coerced string length into the property slot
        }
        PhpType::TaggedScalar => emit_x86_64_store_tagged_scalar_property(emitter, slot),
        PhpType::Mixed | PhpType::Union(_) => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // reload the boxed eval value being assigned
            emitter.instruction("call __rt_incref");                            // retain the Mixed cell for property ownership
            emitter.instruction("mov r11, QWORD PTR [rbp - 24]");               // reload the unboxed object pointer for the store
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", slot.offset)); //store the retained Mixed cell into the property slot
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", slot.offset + 8)); //clear the unused property high word
        }
        _ => {}
    }
}

/// Emits an ARM64 scalar property store after Mixed coercion.
fn emit_aarch64_store_cast_scalar(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    helper: &str,
    result_reg: &str,
) {
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed eval value for scalar coercion
    emitter.instruction(&format!("bl {}", helper));                             // coerce the eval value to the declared property type
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the unboxed object pointer for the store
    emitter.instruction(&format!("str {}, [x9, #{}]", result_reg, slot.offset)); //store the coerced scalar into the property slot
}

/// Stores a boxed eval value into an ARM64 nullable-int tagged-scalar property slot.
fn emit_aarch64_store_tagged_scalar_property(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    let null_label = format!(
        "{}_tagged_scalar_null",
        label_fragment(&slot_body_label_raw(slot, "set"))
    );
    let done_label = format!(
        "{}_tagged_scalar_done",
        label_fragment(&slot_body_label_raw(slot, "set"))
    );
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed eval value for nullable-int inspection
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the assigned value tag and payload words
    emitter.instruction("cmp x0, #8");                                          // runtime tag 8 means the assigned value is null
    emitter.instruction(&format!("b.eq {}", null_label));                       // materialize a tagged null for null property writes
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed eval value for integer coercion
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce non-null eval values to a PHP int payload
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
    emitter.instruction(&format!("b {}", done_label));                          // skip tagged-null materialization after integer coercion
    emitter.label(&null_label);
    crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
    emitter.label(&done_label);
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the unboxed object pointer for the store
    emitter.instruction(&format!("str x0, [x9, #{}]", slot.offset));            // store the nullable integer payload into the property slot
    emitter.instruction(&format!("str x1, [x9, #{}]", slot.offset + 8));        // store the nullable integer tag into the property slot
}

/// Emits an x86_64 scalar property store after Mixed coercion.
fn emit_x86_64_store_cast_scalar(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    helper: &str,
    result_reg: &str,
) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed eval value for scalar coercion
    emitter.instruction(&format!("call {}", helper));                           // coerce the eval value to the declared property type
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the unboxed object pointer for the store
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], {}", slot.offset, result_reg)); //store the coerced scalar into the property slot
}

/// Stores a boxed eval value into an x86_64 nullable-int tagged-scalar property slot.
fn emit_x86_64_store_tagged_scalar_property(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    let null_label = format!(
        "{}_tagged_scalar_null_x",
        label_fragment(&slot_body_label_raw(slot, "set"))
    );
    let done_label = format!(
        "{}_tagged_scalar_done_x",
        label_fragment(&slot_body_label_raw(slot, "set"))
    );
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed eval value for nullable-int inspection
    emitter.instruction("call __rt_mixed_unbox");                               // expose the assigned value tag and payload words
    emitter.instruction("cmp rax, 8");                                          // runtime tag 8 means the assigned value is null
    emitter.instruction(&format!("je {}", null_label));                         // materialize a tagged null for null property writes
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed eval value for integer coercion
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce non-null eval values to a PHP int payload
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
    emitter.instruction(&format!("jmp {}", done_label));                        // skip tagged-null materialization after integer coercion
    emitter.label(&null_label);
    crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
    emitter.label(&done_label);
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the unboxed object pointer for the store
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", slot.offset)); //store the nullable integer payload into the property slot
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rdx", slot.offset + 8)); //store the nullable integer tag into the property slot
}

/// Groups property slots by class id while preserving sorted class order.
fn grouped_slots(slots: &[EvalPropertySlot]) -> BTreeMap<u64, Vec<&EvalPropertySlot>> {
    let mut grouped = BTreeMap::new();
    for slot in slots {
        grouped.entry(slot.class_id).or_insert_with(Vec::new).push(slot);
    }
    grouped
}

/// Returns a platform-safe body label for a get/set property slot.
fn slot_body_label(module: &Module, slot: &EvalPropertySlot, mode: &str) -> String {
    let suffix = match module.target.arch {
        Arch::AArch64 => "",
        Arch::X86_64 => "_x",
    };
    format!("{}{}", slot_body_label_raw(slot, mode), suffix)
}

/// Returns the architecture-independent body label stem for a property slot.
fn slot_body_label_raw(slot: &EvalPropertySlot, mode: &str) -> String {
    format!(
        "__elephc_eval_property_{}_{}_{}",
        mode,
        label_fragment(&slot.class_name),
        label_fragment(&slot.property)
    )
}

/// Converts arbitrary PHP metadata names into assembly-label-safe fragments.
fn label_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

/// Emits a C-visible global label with target-specific symbol mangling.
fn label_c_global(module: &Module, emitter: &mut Emitter, name: &str) {
    let symbol = module.target.extern_symbol(name);
    emitter.label_global(&symbol);
}
