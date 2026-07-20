//! Purpose:
//! Emits user-assembly helpers that let libelephc-magician access native static
//! properties through symbol-backed storage.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` when an EIR module uses eval.
//!
//! Key details:
//! - The cacheable runtime object cannot know user static-property symbols, so
//!   these C-ABI bridge symbols are emitted into the user assembly.
//! - Supported slots include public static properties plus protected/private
//!   static properties when the active eval class scope satisfies PHP visibility.
//! - Null helper returns mean "no bridge match"; boxed PHP null is returned as
//!   a real Mixed cell pointer.

use std::collections::BTreeMap;

use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::ir::{Function, LocalKind, Module};
use crate::names::static_property_symbol;
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, PhpType};

/// Static property slot metadata needed by eval bridge dispatch.
#[derive(Clone)]
struct EvalStaticPropertySlot {
    class_name: String,
    declaring_class: String,
    allowed_scopes: Vec<String>,
    property: String,
    visibility: Visibility,
    symbol: String,
    ty: PhpType,
    is_declared: bool,
}

/// Emits eval static-property helpers when any lowered function owns an eval context.
pub(super) fn emit_eval_static_property_helpers(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    if !module_uses_eval(module) {
        return;
    }
    let slots = collect_eval_static_property_slots(module);
    emit_static_property_get_helper(module, emitter, data, &slots);
    emit_static_property_is_initialized_helper(module, emitter, data, &slots);
    emit_static_property_set_helper(module, emitter, data, &slots);
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
    function.locals.iter().any(|local| {
        matches!(
            local.kind,
            LocalKind::EvalContext | LocalKind::EvalScope | LocalKind::EvalGlobalScope
        )
    })
}

/// Collects static properties with storage layouts and visibility rules the bridge can access.
fn collect_eval_static_property_slots(module: &Module) -> Vec<EvalStaticPropertySlot> {
    let mut slots = Vec::new();
    let mut classes = module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_class_static_property_slots(module, class_name, class_info, &mut slots);
    }
    slots
}

/// Adds bridge-supported static properties visible from one class.
fn collect_class_static_property_slots(
    module: &Module,
    class_name: &str,
    class_info: &ClassInfo,
    slots: &mut Vec<EvalStaticPropertySlot>,
) {
    let mut properties = class_info.static_properties.iter().collect::<Vec<_>>();
    properties.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (property, ty) in properties {
        let visibility = static_property_visibility(class_info, property);
        if !static_property_visibility_supported(visibility)
            || !static_property_type_supported(ty)
        {
            continue;
        }
        let declaring_class = class_info
            .static_property_declaring_classes
            .get(property)
            .map(String::as_str)
            .unwrap_or(class_name);
        let Some(declaring_info) = module.class_infos.get(declaring_class) else {
            continue;
        };
        slots.push(EvalStaticPropertySlot {
            class_name: class_name.to_string(),
            declaring_class: declaring_class.to_string(),
            allowed_scopes: visibility_scope_names(module, declaring_class, visibility),
            property: property.clone(),
            visibility: visibility.clone(),
            symbol: static_property_symbol(declaring_class, property),
            ty: ty.codegen_repr(),
            is_declared: declaring_info.declared_static_properties.contains(property),
        });
    }
}

/// Returns the declared static-property visibility, defaulting to public metadata.
fn static_property_visibility<'a>(class_info: &'a ClassInfo, property: &str) -> &'a Visibility {
    class_info
        .static_property_visibilities
        .get(property)
        .unwrap_or(&Visibility::Public)
}

/// Returns true when the eval static-property bridge can enforce this visibility.
fn static_property_visibility_supported(visibility: &Visibility) -> bool {
    matches!(
        visibility,
        Visibility::Public | Visibility::Protected | Visibility::Private
    )
}

/// Returns true for static-property storage shapes the bridge can box and update.
fn static_property_type_supported(ty: &PhpType) -> bool {
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

/// Emits `__elephc_eval_value_static_property_get(class, name, scope, scope_len) -> Mixed*`.
fn emit_static_property_get_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user static property get ---");
    label_c_global(module, emitter, "__elephc_eval_value_static_property_get");
    match module.target.arch {
        Arch::AArch64 => emit_static_property_get_aarch64(module, emitter, data, slots),
        Arch::X86_64 => emit_static_property_get_x86_64(module, emitter, data, slots),
    }
}

/// Emits `__elephc_eval_value_static_property_is_initialized(class, name, scope, scope_len) -> bool`.
fn emit_static_property_is_initialized_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user static-property initialization probe ---");
    label_c_global(
        module,
        emitter,
        "__elephc_eval_value_static_property_is_initialized",
    );
    match module.target.arch {
        Arch::AArch64 => emit_static_property_is_initialized_aarch64(module, emitter, data, slots),
        Arch::X86_64 => emit_static_property_is_initialized_x86_64(module, emitter, data, slots),
    }
}

/// Emits `__elephc_eval_value_static_property_set(class, name, Mixed*, scope, scope_len) -> bool`.
fn emit_static_property_set_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user static property set ---");
    label_c_global(module, emitter, "__elephc_eval_value_static_property_set");
    match module.target.arch {
        Arch::AArch64 => emit_static_property_set_aarch64(module, emitter, data, slots),
        Arch::X86_64 => emit_static_property_set_x86_64(module, emitter, data, slots),
    }
}

/// Emits the ARM64 static-property get helper body.
fn emit_static_property_get_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
) {
    let done_label = "__elephc_eval_value_static_property_get_done";
    emitter.instruction("sub sp, sp, #64");                                     // reserve helper frame for class/property/scope slices and fp/lr
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    emitter.instruction("str x2, [sp, #16]");                                   // save the requested property-name pointer
    emitter.instruction("str x3, [sp, #24]");                                   // save the requested property-name length
    emitter.instruction("str x4, [sp, #32]");                                   // save the active eval class-scope pointer
    emitter.instruction("str x5, [sp, #40]");                                   // save the active eval class-scope length
    emit_aarch64_static_property_dispatch(module, emitter, data, slots, "get");
    emitter.instruction("mov x0, xzr");                                         // report bridge miss with a null pointer
    emitter.instruction(&format!("b {}", done_label));                          // join the helper epilogue after a miss
    emit_aarch64_get_slot_bodies(module, emitter, slots, done_label);
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed static property value to Rust
}

/// Emits the x86_64 static-property get helper body.
fn emit_static_property_get_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
) {
    let done_label = "__elephc_eval_value_static_property_get_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for class, property, and scope slices
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested property-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the requested property-name length
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the active eval class-scope pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the active eval class-scope length
    emit_x86_64_static_property_dispatch(module, emitter, data, slots, "get");
    emitter.instruction("xor eax, eax");                                        // report bridge miss with a null pointer
    emitter.instruction(&format!("jmp {}", done_label));                        // join the helper epilogue after a miss
    emit_x86_64_get_slot_bodies(module, emitter, slots, done_label);
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed static property value to Rust
}

/// Emits the ARM64 static-property-initialization helper body.
fn emit_static_property_is_initialized_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
) {
    let done_label = "__elephc_eval_value_static_property_is_initialized_done";
    emitter.instruction("sub sp, sp, #64");                                     // reserve helper frame for class/property/scope slices and fp/lr
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #48");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    emitter.instruction("str x2, [sp, #16]");                                   // save the requested property-name pointer
    emitter.instruction("str x3, [sp, #24]");                                   // save the requested property-name length
    emitter.instruction("str x4, [sp, #32]");                                   // save the active eval class-scope pointer
    emitter.instruction("str x5, [sp, #40]");                                   // save the active eval class-scope length
    emit_aarch64_static_property_dispatch(module, emitter, data, slots, "is_initialized");
    emitter.instruction("mov x0, #0");                                          // report an initialization miss to Rust
    emitter.instruction(&format!("b {}", done_label));                          // join the helper epilogue after a miss
    emit_aarch64_static_initialized_slot_bodies(module, emitter, slots, done_label);
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the initialization flag to Rust
}

/// Emits the x86_64 static-property-initialization helper body.
fn emit_static_property_is_initialized_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
) {
    let done_label = "__elephc_eval_value_static_property_is_initialized_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned slots for class, property, and scope slices
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested property-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the requested property-name length
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the active eval class-scope pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the active eval class-scope length
    emit_x86_64_static_property_dispatch(module, emitter, data, slots, "is_initialized");
    emitter.instruction("xor eax, eax");                                        // report an initialization miss to Rust
    emitter.instruction(&format!("jmp {}", done_label));                        // join the helper epilogue after a miss
    emit_x86_64_static_initialized_slot_bodies(module, emitter, slots, done_label);
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the initialization flag to Rust
}

/// Emits the ARM64 static-property set helper body.
fn emit_static_property_set_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
) {
    let fail_label = "__elephc_eval_value_static_property_set_fail";
    let done_label = "__elephc_eval_value_static_property_set_done";
    emitter.instruction("sub sp, sp, #80");                                     // reserve helper frame for class/property, value, scope, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #64");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested class-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested class-name length
    emitter.instruction("str x2, [sp, #16]");                                   // save the requested property-name pointer
    emitter.instruction("str x3, [sp, #24]");                                   // save the requested property-name length
    emitter.instruction("str x4, [sp, #32]");                                   // save the boxed value being assigned
    emitter.instruction("str x5, [sp, #40]");                                   // save the active eval class-scope pointer
    emitter.instruction("str x6, [sp, #48]");                                   // save the active eval class-scope length
    emit_aarch64_static_property_dispatch(module, emitter, data, slots, "set");
    emitter.instruction(&format!("b {}", fail_label));                          // no supported static property matched the request
    emit_aarch64_set_slot_bodies(module, emitter, data, slots, done_label, fail_label);
    emitter.label(fail_label);
    emitter.instruction("mov x0, #0");                                          // report a failed eval static-property write to Rust
    emitter.instruction(&format!("b {}", done_label));                          // join the helper epilogue after failure
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the write-status flag to Rust
}

/// Emits the x86_64 static-property set helper body.
fn emit_static_property_set_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
) {
    let fail_label = "__elephc_eval_value_static_property_set_fail_x";
    let done_label = "__elephc_eval_value_static_property_set_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve aligned slots for class, property, value, and scope
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested class-name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the requested property-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the requested property-name length
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the boxed value being assigned
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the active eval class-scope pointer
    emitter.instruction("mov rax, QWORD PTR [rbp + 16]");                       // load the active eval class-scope length stack argument
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the active eval class-scope length
    emit_x86_64_static_property_dispatch(module, emitter, data, slots, "set");
    emitter.instruction(&format!("jmp {}", fail_label));                        // no supported static property matched the request
    emit_x86_64_set_slot_bodies(module, emitter, data, slots, done_label, fail_label);
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax");                                        // report a failed eval static-property write to Rust
    emitter.instruction(&format!("jmp {}", done_label));                        // join the helper epilogue after failure
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the write-status flag to Rust
}

/// Emits ARM64 class-name and property-name dispatch for static property helpers.
fn emit_aarch64_static_property_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
    mode: &str,
) {
    for (class_name, class_slots) in grouped_slots(slots) {
        let next_label = format!(
            "__elephc_eval_static_property_{}_next_{}",
            mode,
            label_fragment(class_name)
        );
        emit_aarch64_static_class_name_compare(emitter, data, class_name, &next_label);
        for slot in class_slots {
            emit_aarch64_static_property_name_compare(module, emitter, data, slot, mode);
        }
        emitter.label(&next_label);
    }
}

/// Emits x86_64 class-name and property-name dispatch for static property helpers.
fn emit_x86_64_static_property_dispatch(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
    mode: &str,
) {
    for (class_name, class_slots) in grouped_slots(slots) {
        let next_label = format!(
            "__elephc_eval_static_property_{}_next_{}_x",
            mode,
            label_fragment(class_name)
        );
        emit_x86_64_static_class_name_compare(emitter, data, class_name, &next_label);
        for slot in class_slots {
            emit_x86_64_static_property_name_compare(module, emitter, data, slot, mode);
        }
        emitter.label(&next_label);
    }
}

/// Emits one ARM64 case-insensitive class-name comparison for a static property group.
fn emit_aarch64_static_class_name_compare(
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_name: &str,
    next_label: &str,
) {
    let (label, len) = data.add_string(class_name.as_bytes());
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload requested class-name pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload requested class-name length
    abi::emit_symbol_address(emitter, "x3", &label);
    abi::emit_load_int_immediate(emitter, "x4", len as i64);
    emitter.instruction("bl __rt_strcasecmp");                                  // compare class names with PHP case-insensitive rules
    emitter.instruction(&format!("cbnz x0, {}", next_label));                   // try the next class when names differ
}

/// Emits one x86_64 case-insensitive class-name comparison for a static property group.
fn emit_x86_64_static_class_name_compare(
    emitter: &mut Emitter,
    data: &mut DataSection,
    class_name: &str,
    next_label: &str,
) {
    let (label, len) = data.add_string(class_name.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload requested class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload requested class-name length
    abi::emit_symbol_address(emitter, "rdx", &label);
    abi::emit_load_int_immediate(emitter, "rcx", len as i64);
    emitter.instruction("call __rt_strcasecmp");                                // compare class names with PHP case-insensitive rules
    emitter.instruction("test rax, rax");                                       // check whether the class names matched
    emitter.instruction(&format!("jne {}", next_label));                        // try the next class when names differ
}

/// Emits one ARM64 property-name comparison and branch to the matching body.
fn emit_aarch64_static_property_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalStaticPropertySlot,
    mode: &str,
) {
    let (label, len) = data.add_string(slot.property.as_bytes());
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload requested property-name pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload requested property-name length
    abi::emit_symbol_address(emitter, "x3", &label);
    abi::emit_load_int_immediate(emitter, "x4", len as i64);
    emitter.instruction("bl __rt_str_eq");                                      // compare property names with PHP case-sensitive rules
    let target_label = slot_body_label(module, slot, mode);
    if matches!(slot.visibility, Visibility::Public) {
        emitter.instruction(&format!("cbnz x0, {}", target_label));             // dispatch to the static property body when names match
        return;
    }
    let miss_label = slot_access_miss_label(module, slot, mode);
    emitter.instruction(&format!("cbz x0, {}", miss_label));                    // continue static-property dispatch when names differ
    emit_aarch64_static_property_scope_check(emitter, data, slot, mode, &target_label);
    emitter.label(&miss_label);
}

/// Emits one x86_64 property-name comparison and branch to the matching body.
fn emit_x86_64_static_property_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalStaticPropertySlot,
    mode: &str,
) {
    let (label, len) = data.add_string(slot.property.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload requested property-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload requested property-name length
    abi::emit_symbol_address(emitter, "rdx", &label);
    abi::emit_load_int_immediate(emitter, "rcx", len as i64);
    emitter.instruction("call __rt_str_eq");                                    // compare property names with PHP case-sensitive rules
    emitter.instruction("test rax, rax");                                       // check whether the property names matched
    let target_label = slot_body_label(module, slot, mode);
    if matches!(slot.visibility, Visibility::Public) {
        emitter.instruction(&format!("jne {}", target_label));                  // dispatch to the static property body when names match
        return;
    }
    let miss_label = slot_access_miss_label(module, slot, mode);
    emitter.instruction(&format!("je {}", miss_label));                         // continue static-property dispatch when names differ
    emit_x86_64_static_property_scope_check(emitter, data, slot, mode, &target_label);
    emitter.label(&miss_label);
}

/// Emits ARM64 visibility checks for a protected/private static-property bridge hit.
fn emit_aarch64_static_property_scope_check(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalStaticPropertySlot,
    mode: &str,
    target_label: &str,
) {
    let (scope_ptr_offset, scope_len_offset) = aarch64_scope_offsets(mode);
    emitter.instruction(&format!("ldr x1, [sp, #{}]", scope_ptr_offset));       // reload the active eval class-scope pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", scope_len_offset));       // reload the active eval class-scope length
    emitter.instruction("cbz x1, 1f");                                          // skip scoped dispatch outside a class scope
    for scope_name in &slot.allowed_scopes {
        let (label, len) = data.add_string(scope_name.as_bytes());
        emitter.instruction(&format!("ldr x1, [sp, #{}]", scope_ptr_offset));   // reload the active eval class-scope pointer
        emitter.instruction(&format!("ldr x2, [sp, #{}]", scope_len_offset));   // reload the active eval class-scope length
        abi::emit_symbol_address(emitter, "x3", &label);
        abi::emit_load_int_immediate(emitter, "x4", len as i64);
        emitter.instruction("bl __rt_strcasecmp");                              // compare current eval scope with an allowed class
        emitter.instruction(&format!("cbz x0, {}", target_label));              // dispatch when scoped visibility is satisfied
    }
    emitter.label("1");
}

/// Emits x86_64 visibility checks for a protected/private static-property bridge hit.
fn emit_x86_64_static_property_scope_check(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalStaticPropertySlot,
    mode: &str,
    target_label: &str,
) {
    let (scope_ptr_offset, scope_len_offset) = x86_64_scope_offsets(mode);
    emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", scope_ptr_offset)); // reload the active eval class-scope pointer
    emitter.instruction(&format!("mov rsi, QWORD PTR [rbp - {}]", scope_len_offset)); // reload the active eval class-scope length
    emitter.instruction("test rdi, rdi");                                       // check whether eval is executing inside a class scope
    emitter.instruction("jz 1f");                                               // skip scoped dispatch outside a class scope
    for scope_name in &slot.allowed_scopes {
        let (label, len) = data.add_string(scope_name.as_bytes());
        emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", scope_ptr_offset)); // reload the active eval class-scope pointer
        emitter.instruction(&format!("mov rsi, QWORD PTR [rbp - {}]", scope_len_offset)); // reload the active eval class-scope length
        abi::emit_symbol_address(emitter, "rdx", &label);
        abi::emit_load_int_immediate(emitter, "rcx", len as i64);
        emitter.instruction("call __rt_strcasecmp");                            // compare current eval scope with an allowed class
        emitter.instruction("test rax, rax");                                   // check whether the current scope matched
        emitter.instruction(&format!("je {}", target_label));                   // dispatch when scoped visibility is satisfied
    }
    emitter.label("1");
}

/// Returns ARM64 stack offsets for the class-scope pointer and length.
fn aarch64_scope_offsets(mode: &str) -> (usize, usize) {
    match mode {
        "get" | "is_initialized" => (32, 40),
        "set" => (40, 48),
        _ => unreachable!("eval static property helpers only use get/set/is_initialized modes"),
    }
}

/// Returns x86_64 frame offsets for the class-scope pointer and length.
fn x86_64_scope_offsets(mode: &str) -> (usize, usize) {
    match mode {
        "get" | "is_initialized" => (40, 48),
        "set" => (48, 56),
        _ => unreachable!("eval static property helpers only use get/set/is_initialized modes"),
    }
}

/// Emits ARM64 get bodies for every bridge-supported static property slot.
fn emit_aarch64_get_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalStaticPropertySlot],
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "get"));
        emit_aarch64_uninitialized_guard(emitter, slot, done_label);
        emit_aarch64_box_static_property_slot(emitter, slot);
        emitter.instruction(&format!("b {}", done_label));                      // return after boxing the static property value
    }
}

/// Emits x86_64 get bodies for every bridge-supported static property slot.
fn emit_x86_64_get_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalStaticPropertySlot],
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "get"));
        emit_x86_64_uninitialized_guard(emitter, slot, done_label);
        emit_x86_64_box_static_property_slot(emitter, slot);
        emitter.instruction(&format!("jmp {}", done_label));                    // return after boxing the static property value
    }
}

/// Emits ARM64 static-property-initialization bodies for every supported slot.
fn emit_aarch64_static_initialized_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalStaticPropertySlot],
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "is_initialized"));
        emit_aarch64_static_property_initialized_flag(emitter, slot);
        emitter.instruction(&format!("b {}", done_label));                      // return after materializing the static initialization flag
    }
}

/// Emits x86_64 static-property-initialization bodies for every supported slot.
fn emit_x86_64_static_initialized_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalStaticPropertySlot],
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "is_initialized"));
        emit_x86_64_static_property_initialized_flag(emitter, slot);
        emitter.instruction(&format!("jmp {}", done_label));                    // return after materializing the static initialization flag
    }
}

/// Emits ARM64 set bodies for every bridge-supported static property slot.
fn emit_aarch64_set_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
    done_label: &str,
    fail_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "set"));
        emit_aarch64_store_static_property_slot(module, emitter, data, slot, fail_label);
        emitter.instruction("mov x0, #1");                                      // report a successful eval static-property write to Rust
        emitter.instruction(&format!("b {}", done_label));                      // return after storing the static property value
    }
}

/// Emits x86_64 set bodies for every bridge-supported static property slot.
fn emit_x86_64_set_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalStaticPropertySlot],
    done_label: &str,
    fail_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "set"));
        emit_x86_64_store_static_property_slot(module, emitter, data, slot, fail_label);
        emitter.instruction("mov rax, 1");                                      // report a successful eval static-property write to Rust
        emitter.instruction(&format!("jmp {}", done_label));                    // return after storing the static property value
    }
}

/// Emits an ARM64 boolean for one static property's initialized state.
fn emit_aarch64_static_property_initialized_flag(
    emitter: &mut Emitter,
    slot: &EvalStaticPropertySlot,
) {
    if !slot.is_declared {
        emitter.instruction("mov x0, #1");                                      // non-typed declared static properties are always initialized
        return;
    }
    abi::emit_load_symbol_to_reg(emitter, "x10", &slot.symbol, 8);
    abi::emit_load_int_immediate(emitter, "x11", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp x10, x11");                                        // compare the static property marker against the uninitialized sentinel
    emitter.instruction("cset x0, ne");                                         // materialize true when the static property is initialized
}

/// Emits an x86_64 boolean for one static property's initialized state.
fn emit_x86_64_static_property_initialized_flag(
    emitter: &mut Emitter,
    slot: &EvalStaticPropertySlot,
) {
    if !slot.is_declared {
        emitter.instruction("mov rax, 1");                                      // non-typed declared static properties are always initialized
        return;
    }
    abi::emit_load_symbol_to_reg(emitter, "r10", &slot.symbol, 8);
    abi::emit_load_int_immediate(emitter, "r11", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp r10, r11");                                        // compare the static property marker against the uninitialized sentinel
    emitter.instruction("setne al");                                            // materialize true when the static property is initialized
    emitter.instruction("movzx rax, al");                                       // widen the initialization flag into the return register
}

/// Emits an ARM64 uninitialized typed-static-property guard before boxing.
fn emit_aarch64_uninitialized_guard(
    emitter: &mut Emitter,
    slot: &EvalStaticPropertySlot,
    done_label: &str,
) {
    if !slot.is_declared {
        return;
    }
    let initialized_label = format!(
        "{}_initialized",
        label_fragment(&slot_body_label_raw(slot, "get"))
    );
    abi::emit_load_symbol_to_reg(emitter, "x10", &slot.symbol, 8);
    abi::emit_load_int_immediate(emitter, "x11", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp x10, x11");                                        // check whether the typed static property is initialized
    emitter.instruction(&format!("b.ne {}", initialized_label));                // continue boxing once the static slot is initialized
    emitter.instruction("mov x0, xzr");                                         // report uninitialized static property as a bridge failure
    emitter.instruction(&format!("b {}", done_label));                          // return the failure to Rust without boxing storage
    emitter.label(&initialized_label);
}

/// Emits an x86_64 uninitialized typed-static-property guard before boxing.
fn emit_x86_64_uninitialized_guard(
    emitter: &mut Emitter,
    slot: &EvalStaticPropertySlot,
    done_label: &str,
) {
    if !slot.is_declared {
        return;
    }
    let initialized_label = format!(
        "{}_initialized_x",
        label_fragment(&slot_body_label_raw(slot, "get"))
    );
    abi::emit_load_symbol_to_reg(emitter, "r10", &slot.symbol, 8);
    abi::emit_load_int_immediate(emitter, "r11", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp r10, r11");                                        // check whether the typed static property is initialized
    emitter.instruction(&format!("jne {}", initialized_label));                 // continue boxing once the static slot is initialized
    emitter.instruction("xor eax, eax");                                        // report uninitialized static property as a bridge failure
    emitter.instruction(&format!("jmp {}", done_label));                        // return the failure to Rust without boxing storage
    emitter.label(&initialized_label);
}

/// Boxes an ARM64 static property symbol payload into a Mixed cell.
fn emit_aarch64_box_static_property_slot(emitter: &mut Emitter, slot: &EvalStaticPropertySlot) {
    match slot.ty.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } => {
            abi::emit_load_symbol_to_reg(emitter, "x0", &slot.symbol, 0);
            emit_box_current_value_as_mixed(emitter, &slot.ty);
        }
        PhpType::Float => {
            abi::emit_load_symbol_to_reg(emitter, "d0", &slot.symbol, 0);
            emit_box_current_value_as_mixed(emitter, &PhpType::Float);
        }
        PhpType::Str => {
            abi::emit_load_symbol_to_reg(emitter, "x1", &slot.symbol, 0);
            abi::emit_load_symbol_to_reg(emitter, "x2", &slot.symbol, 8);
            emit_box_current_value_as_mixed(emitter, &PhpType::Str);
        }
        PhpType::TaggedScalar => {
            abi::emit_load_symbol_to_reg(emitter, "x0", &slot.symbol, 0);
            abi::emit_load_symbol_to_reg(emitter, "x1", &slot.symbol, 8);
            emit_box_current_value_as_mixed(emitter, &PhpType::TaggedScalar);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            let null_label = format!(
                "{}_mixed_null",
                label_fragment(&slot_body_label_raw(slot, "get"))
            );
            let done_label = format!(
                "{}_mixed_done",
                label_fragment(&slot_body_label_raw(slot, "get"))
            );
            abi::emit_load_symbol_to_reg(emitter, "x0", &slot.symbol, 0);
            emitter.instruction(&format!("cbz x0, {}", null_label));            // null static storage reads as PHP null
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

/// Boxes an x86_64 static property symbol payload into a Mixed cell.
fn emit_x86_64_box_static_property_slot(emitter: &mut Emitter, slot: &EvalStaticPropertySlot) {
    match slot.ty.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } => {
            abi::emit_load_symbol_to_reg(emitter, "rax", &slot.symbol, 0);
            emit_box_current_value_as_mixed(emitter, &slot.ty);
        }
        PhpType::Float => {
            abi::emit_load_symbol_to_reg(emitter, "xmm0", &slot.symbol, 0);
            emit_box_current_value_as_mixed(emitter, &PhpType::Float);
        }
        PhpType::Str => {
            abi::emit_load_symbol_to_reg(emitter, "rax", &slot.symbol, 0);
            abi::emit_load_symbol_to_reg(emitter, "rdx", &slot.symbol, 8);
            emit_box_current_value_as_mixed(emitter, &PhpType::Str);
        }
        PhpType::TaggedScalar => {
            abi::emit_load_symbol_to_reg(emitter, "rax", &slot.symbol, 0);
            abi::emit_load_symbol_to_reg(emitter, "rdx", &slot.symbol, 8);
            emit_box_current_value_as_mixed(emitter, &PhpType::TaggedScalar);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            let null_label = format!(
                "{}_mixed_null_x",
                label_fragment(&slot_body_label_raw(slot, "get"))
            );
            let done_label = format!(
                "{}_mixed_done_x",
                label_fragment(&slot_body_label_raw(slot, "get"))
            );
            abi::emit_load_symbol_to_reg(emitter, "rax", &slot.symbol, 0);
            emitter.instruction("test rax, rax");                               // check whether static storage holds a Mixed cell
            emitter.instruction(&format!("jz {}", null_label));                 // null static storage reads as PHP null
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

/// Stores a boxed Mixed eval value into an ARM64 static property symbol.
fn emit_aarch64_store_static_property_slot(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalStaticPropertySlot,
    fail_label: &str,
) {
    match slot.ty.codegen_repr() {
        PhpType::Int => emit_aarch64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_int", "x0"),
        PhpType::Bool => emit_aarch64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_bool", "x0"),
        PhpType::Float => {
            emitter.instruction("ldr x0, [sp, #32]");                           // reload the boxed eval value for float coercion
            emitter.instruction("bl __rt_mixed_cast_float");                    // coerce the eval value to a PHP float
            abi::emit_store_reg_to_symbol(emitter, "d0", &slot.symbol, 0);
            clear_uninitialized_marker_after_static_store(emitter, slot);
        }
        PhpType::Str => {
            emitter.instruction("ldr x0, [sp, #32]");                           // reload the boxed eval value for string coercion
            emitter.instruction("bl __rt_mixed_cast_string");                   // coerce the eval value to a PHP string pair
            abi::emit_store_reg_to_symbol(emitter, "x1", &slot.symbol, 0);
            abi::emit_store_reg_to_symbol(emitter, "x2", &slot.symbol, 8);
        }
        PhpType::TaggedScalar => emit_aarch64_store_tagged_scalar_static_property(emitter, slot),
        PhpType::Array(_) => {
            emit_aarch64_store_heap_static_property_slot(emitter, slot, 4, fail_label);
        }
        PhpType::AssocArray { .. } => {
            emit_aarch64_store_heap_static_property_slot(emitter, slot, 5, fail_label);
        }
        PhpType::Object(class_name) => {
            emit_aarch64_store_object_static_property_slot(
                module,
                emitter,
                data,
                slot,
                &class_name,
                fail_label,
            );
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emitter.instruction("ldr x0, [sp, #32]");                           // reload the boxed eval value being assigned
            emitter.instruction("bl __rt_incref");                              // retain the Mixed cell for static-property ownership
            abi::emit_store_reg_to_symbol(emitter, "x0", &slot.symbol, 0);
            clear_uninitialized_marker_after_static_store(emitter, slot);
        }
        _ => {}
    }
}

/// Stores a boxed Mixed eval value into an x86_64 static property symbol.
fn emit_x86_64_store_static_property_slot(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalStaticPropertySlot,
    fail_label: &str,
) {
    match slot.ty.codegen_repr() {
        PhpType::Int => emit_x86_64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_int", "rax"),
        PhpType::Bool => emit_x86_64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_bool", "rax"),
        PhpType::Float => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval value for float coercion
            emitter.instruction("call __rt_mixed_cast_float");                  // coerce the eval value to a PHP float
            abi::emit_store_reg_to_symbol(emitter, "xmm0", &slot.symbol, 0);
            clear_uninitialized_marker_after_static_store(emitter, slot);
        }
        PhpType::Str => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval value for string coercion
            emitter.instruction("call __rt_mixed_cast_string");                 // coerce the eval value to a PHP string pair
            abi::emit_store_reg_to_symbol(emitter, "rax", &slot.symbol, 0);
            abi::emit_store_reg_to_symbol(emitter, "rdx", &slot.symbol, 8);
        }
        PhpType::TaggedScalar => emit_x86_64_store_tagged_scalar_static_property(emitter, slot),
        PhpType::Array(_) => {
            emit_x86_64_store_heap_static_property_slot(emitter, slot, 4, fail_label);
        }
        PhpType::AssocArray { .. } => {
            emit_x86_64_store_heap_static_property_slot(emitter, slot, 5, fail_label);
        }
        PhpType::Object(class_name) => {
            emit_x86_64_store_object_static_property_slot(
                module,
                emitter,
                data,
                slot,
                &class_name,
                fail_label,
            );
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 40]");               // reload the boxed eval value being assigned
            emitter.instruction("call __rt_incref");                            // retain the Mixed cell for static-property ownership
            abi::emit_store_reg_to_symbol(emitter, "rax", &slot.symbol, 0);
            clear_uninitialized_marker_after_static_store(emitter, slot);
        }
        _ => {}
    }
}

/// Emits an ARM64 scalar static-property store after Mixed coercion.
fn emit_aarch64_store_cast_scalar(
    emitter: &mut Emitter,
    slot: &EvalStaticPropertySlot,
    helper: &str,
    result_reg: &str,
) {
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the boxed eval value for scalar coercion
    emitter.instruction(&format!("bl {}", helper));                             // coerce the eval value to the declared static-property type
    abi::emit_store_reg_to_symbol(emitter, result_reg, &slot.symbol, 0);
    clear_uninitialized_marker_after_static_store(emitter, slot);
}

/// Emits an x86_64 scalar static-property store after Mixed coercion.
fn emit_x86_64_store_cast_scalar(
    emitter: &mut Emitter,
    slot: &EvalStaticPropertySlot,
    helper: &str,
    result_reg: &str,
) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval value for scalar coercion
    emitter.instruction(&format!("call {}", helper));                           // coerce the eval value to the declared static-property type
    abi::emit_store_reg_to_symbol(emitter, result_reg, &slot.symbol, 0);
    clear_uninitialized_marker_after_static_store(emitter, slot);
}

/// Stores a boxed ARM64 eval heap value into an array-like static property symbol.
fn emit_aarch64_store_heap_static_property_slot(
    emitter: &mut Emitter,
    slot: &EvalStaticPropertySlot,
    expected_tag: i64,
    fail_label: &str,
) {
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the boxed eval value for heap payload inspection
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the assigned heap value tag and payload pointer
    abi::emit_load_int_immediate(emitter, "x10", expected_tag);
    emitter.instruction("cmp x0, x10");                                         // compare the assigned value tag with the static-property ABI
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject heap values with an incompatible ABI shape
    emitter.instruction("mov x0, x1");                                          // move the unboxed heap pointer into the retained-result register
    abi::emit_incref_if_refcounted(emitter, &slot.ty.codegen_repr());
    abi::emit_store_reg_to_symbol(emitter, "x0", &slot.symbol, 0);
    clear_uninitialized_marker_after_static_store(emitter, slot);
}

/// Validates and stores a boxed ARM64 eval object into an object static property symbol.
fn emit_aarch64_store_object_static_property_slot(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalStaticPropertySlot,
    class_name: &str,
    fail_label: &str,
) {
    if !class_name.is_empty() {
        let (label, len) = data.add_string(class_name.as_bytes());
        let is_a_symbol = module.target.extern_symbol("__elephc_eval_value_is_a");
        emitter.instruction("ldr x0, [sp, #32]");                               // reload the boxed eval value for object type validation
        abi::emit_symbol_address(emitter, "x1", &label);
        abi::emit_load_int_immediate(emitter, "x2", len as i64);
        emitter.instruction("mov x3, xzr");                                     // allow exact class matches for object static-property hints
        abi::emit_call_label(emitter, &is_a_symbol);
        emitter.instruction(&format!("cbz x0, {}", fail_label));                // reject values that fail the object static-property type hint
    }
    emit_aarch64_store_heap_static_property_slot(emitter, slot, 6, fail_label);
}

/// Stores a boxed x86_64 eval heap value into an array-like static property symbol.
fn emit_x86_64_store_heap_static_property_slot(
    emitter: &mut Emitter,
    slot: &EvalStaticPropertySlot,
    expected_tag: i64,
    fail_label: &str,
) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval value for heap payload inspection
    emitter.instruction("call __rt_mixed_unbox");                               // expose the assigned heap value tag and payload pointer
    abi::emit_load_int_immediate(emitter, "r10", expected_tag);
    emitter.instruction("cmp rax, r10");                                        // compare the assigned value tag with the static-property ABI
    emitter.instruction(&format!("jne {}", fail_label));                        // reject heap values with an incompatible ABI shape
    emitter.instruction("mov rax, rdi");                                        // move the unboxed heap pointer into the retained-result register
    abi::emit_incref_if_refcounted(emitter, &slot.ty.codegen_repr());
    abi::emit_store_reg_to_symbol(emitter, "rax", &slot.symbol, 0);
    clear_uninitialized_marker_after_static_store(emitter, slot);
}

/// Validates and stores a boxed x86_64 eval object into an object static property symbol.
fn emit_x86_64_store_object_static_property_slot(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalStaticPropertySlot,
    class_name: &str,
    fail_label: &str,
) {
    if !class_name.is_empty() {
        let (label, len) = data.add_string(class_name.as_bytes());
        let is_a_symbol = module.target.extern_symbol("__elephc_eval_value_is_a");
        emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                   // reload the boxed eval value for object type validation
        abi::emit_symbol_address(emitter, "rsi", &label);
        abi::emit_load_int_immediate(emitter, "rdx", len as i64);
        emitter.instruction("xor ecx, ecx");                                    // allow exact class matches for object static-property hints
        abi::emit_call_label(emitter, &is_a_symbol);
        emitter.instruction("test rax, rax");                                   // check whether the value satisfied the static-property hint
        emitter.instruction(&format!("je {}", fail_label));                     // reject values that fail the object static-property type hint
    }
    emit_x86_64_store_heap_static_property_slot(emitter, slot, 6, fail_label);
}

/// Stores a boxed eval value into an ARM64 nullable-int static property symbol.
fn emit_aarch64_store_tagged_scalar_static_property(
    emitter: &mut Emitter,
    slot: &EvalStaticPropertySlot,
) {
    let null_label = format!(
        "{}_tagged_scalar_null",
        label_fragment(&slot_body_label_raw(slot, "set"))
    );
    let done_label = format!(
        "{}_tagged_scalar_done",
        label_fragment(&slot_body_label_raw(slot, "set"))
    );
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the boxed eval value for nullable-int inspection
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the assigned value tag and payload words
    emitter.instruction("cmp x0, #8");                                          // runtime tag 8 means the assigned value is null
    emitter.instruction(&format!("b.eq {}", null_label));                       // materialize a tagged null for null static-property writes
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the boxed eval value for integer coercion
    emitter.instruction("bl __rt_mixed_cast_int");                              // coerce non-null eval values to a PHP int payload
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
    emitter.instruction(&format!("b {}", done_label));                          // skip tagged-null materialization after integer coercion
    emitter.label(&null_label);
    crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
    emitter.label(&done_label);
    abi::emit_store_reg_to_symbol(emitter, "x0", &slot.symbol, 0);
    abi::emit_store_reg_to_symbol(emitter, "x1", &slot.symbol, 8);
}

/// Stores a boxed eval value into an x86_64 nullable-int static property symbol.
fn emit_x86_64_store_tagged_scalar_static_property(
    emitter: &mut Emitter,
    slot: &EvalStaticPropertySlot,
) {
    let null_label = format!(
        "{}_tagged_scalar_null_x",
        label_fragment(&slot_body_label_raw(slot, "set"))
    );
    let done_label = format!(
        "{}_tagged_scalar_done_x",
        label_fragment(&slot_body_label_raw(slot, "set"))
    );
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval value for nullable-int inspection
    emitter.instruction("call __rt_mixed_unbox");                               // expose the assigned value tag and payload words
    emitter.instruction("cmp rax, 8");                                          // runtime tag 8 means the assigned value is null
    emitter.instruction(&format!("je {}", null_label));                         // materialize a tagged null for null static-property writes
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the boxed eval value for integer coercion
    emitter.instruction("call __rt_mixed_cast_int");                            // coerce non-null eval values to a PHP int payload
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
    emitter.instruction(&format!("jmp {}", done_label));                        // skip tagged-null materialization after integer coercion
    emitter.label(&null_label);
    crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
    emitter.label(&done_label);
    abi::emit_store_reg_to_symbol(emitter, "rax", &slot.symbol, 0);
    abi::emit_store_reg_to_symbol(emitter, "rdx", &slot.symbol, 8);
}

/// Clears the typed-property high word after a successful non-pair static store.
fn clear_uninitialized_marker_after_static_store(
    emitter: &mut Emitter,
    slot: &EvalStaticPropertySlot,
) {
    if !matches!(slot.ty.codegen_repr(), PhpType::Str | PhpType::TaggedScalar) {
        abi::emit_store_zero_to_symbol(emitter, &slot.symbol, 8);
    }
}

/// Groups static property slots by PHP-visible class name in deterministic order.
fn grouped_slots(slots: &[EvalStaticPropertySlot]) -> BTreeMap<&str, Vec<&EvalStaticPropertySlot>> {
    let mut grouped = BTreeMap::new();
    for slot in slots {
        grouped
            .entry(slot.class_name.as_str())
            .or_insert_with(Vec::new)
            .push(slot);
    }
    grouped
}

/// Returns a platform-safe body label for a get/set static property slot.
fn slot_body_label(module: &Module, slot: &EvalStaticPropertySlot, mode: &str) -> String {
    let suffix = match module.target.arch {
        Arch::AArch64 => "",
        Arch::X86_64 => "_x",
    };
    format!("{}{}", slot_body_label_raw(slot, mode), suffix)
}

/// Returns a platform-safe label for continuing after a scoped static-property name miss.
fn slot_access_miss_label(
    module: &Module,
    slot: &EvalStaticPropertySlot,
    mode: &str,
) -> String {
    format!("{}_access_miss", slot_body_label(module, slot, mode))
}

/// Returns the architecture-independent body label stem for a static property slot.
fn slot_body_label_raw(slot: &EvalStaticPropertySlot, mode: &str) -> String {
    format!(
        "__elephc_eval_static_property_{}_{}_{}_{}",
        mode,
        label_fragment(&slot.class_name),
        label_fragment(&slot.declaring_class),
        label_fragment(&slot.property)
    )
}

/// Returns class scopes that satisfy one member visibility for a declaring class.
fn visibility_scope_names(
    module: &Module,
    declaring_class: &str,
    visibility: &Visibility,
) -> Vec<String> {
    match visibility {
        Visibility::Public => Vec::new(),
        Visibility::Private => vec![declaring_class.to_string()],
        Visibility::Protected => related_class_scope_names(module, declaring_class),
    }
}

/// Returns AOT classes in the same inheritance line as `declaring_class`.
fn related_class_scope_names(module: &Module, declaring_class: &str) -> Vec<String> {
    let mut scopes = module
        .class_infos
        .keys()
        .filter(|class_name| {
            is_same_or_descendant(module, class_name, declaring_class)
                || is_same_or_descendant(module, declaring_class, class_name)
        })
        .cloned()
        .collect::<Vec<_>>();
    scopes.sort_by(|left, right| {
        class_id_for_scope(module, left)
            .cmp(&class_id_for_scope(module, right))
            .then_with(|| left.cmp(right))
    });
    scopes
}

/// Returns true when `class_name` is `ancestor` or descends from it.
fn is_same_or_descendant(module: &Module, class_name: &str, ancestor: &str) -> bool {
    let mut cursor = Some(class_name);
    while let Some(name) = cursor {
        if name == ancestor {
            return true;
        }
        cursor = module
            .class_infos
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

/// Returns the deterministic class id used to order generated scope checks.
fn class_id_for_scope(module: &Module, class_name: &str) -> u64 {
    module
        .class_infos
        .get(class_name)
        .map(|class_info| class_info.class_id)
        .unwrap_or(u64::MAX)
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
