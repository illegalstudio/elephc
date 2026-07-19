//! Purpose:
//! Emits user-assembly helpers that let libelephc-magician access properties on
//! native objects using the current module's class metadata.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` when an EIR module uses eval.
//!
//! Key details:
//! - The cacheable runtime object cannot know user class ids or property
//!   offsets, so these C-ABI symbols are emitted into the user assembly.
//! - Supported slots include public properties plus protected/private
//!   properties when the active eval class scope satisfies PHP visibility.

use std::collections::BTreeMap;

use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime_value_tag;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::codegen::{abi, emit_box_current_value_as_mixed};
use crate::ir::{Function, LocalKind, Module};
use crate::parser::ast::Visibility;
use crate::types::{ClassInfo, PhpType};

/// Property slot metadata needed by eval property bridge dispatch.
#[derive(Clone)]
struct EvalPropertySlot {
    class_id: u64,
    class_name: String,
    declaring_class: String,
    allowed_scopes: Vec<String>,
    property: String,
    visibility: Visibility,
    offset: usize,
    ty: PhpType,
    is_declared: bool,
    is_hidden_shadow: bool,
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
    emit_property_is_initialized_helper(module, emitter, data, &slots);
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
    function.locals.iter().any(|local| {
        matches!(
            local.kind,
            LocalKind::EvalContext | LocalKind::EvalScope | LocalKind::EvalGlobalScope
        )
    })
}

/// Collects declared properties with storage layouts and visibility rules the bridge can access.
fn collect_eval_property_slots(module: &Module) -> Vec<EvalPropertySlot> {
    let mut slots = Vec::new();
    let mut classes = module.class_infos.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        collect_class_property_slots(module, class_name, class_info, &mut slots);
    }
    slots
}

/// Adds bridge-supported properties for one class.
fn collect_class_property_slots(
    module: &Module,
    class_name: &str,
    class_info: &ClassInfo,
    slots: &mut Vec<EvalPropertySlot>,
) {
    for (index, (property, ty)) in class_info.properties.iter().enumerate() {
        let visible_index = class_info.visible_property_index(property);
        let (declaring_class, visibility, is_hidden_shadow) = if visible_index == Some(index) {
            let declaring_class = class_info
                .property_declaring_classes
                .get(property)
                .map(String::as_str)
                .unwrap_or(class_name)
                .to_string();
            (
                declaring_class,
                property_visibility(class_info, property).clone(),
                false,
            )
        } else {
            let Some((declaring_class, visibility)) =
                hidden_private_property_slot_metadata(module, class_name, index, property)
            else {
                continue;
            };
            (declaring_class, visibility, true)
        };
        if !property_visibility_supported(&visibility) || !property_type_supported(ty) {
            continue;
        }
        slots.push(EvalPropertySlot {
            class_id: class_info.class_id,
            class_name: class_name.to_string(),
            declaring_class: declaring_class.clone(),
            allowed_scopes: visibility_scope_names(module, &declaring_class, &visibility),
            property: property.clone(),
            visibility,
            offset: 8 + index * 16,
            ty: ty.codegen_repr(),
            is_declared: class_info.property_slot_is_declared(index, property),
            is_hidden_shadow,
        });
    }
}

/// Returns metadata for a private parent slot hidden by a same-named child property.
fn hidden_private_property_slot_metadata(
    module: &Module,
    class_name: &str,
    index: usize,
    property: &str,
) -> Option<(String, Visibility)> {
    for (ancestor_name, ancestor_info) in class_ancestry(module, class_name) {
        let Some((ancestor_property, _)) = ancestor_info.properties.get(index) else {
            continue;
        };
        if ancestor_property != property
            || ancestor_info.visible_property_index(property) != Some(index)
        {
            continue;
        }
        let visibility = property_visibility(ancestor_info, property).clone();
        if visibility != Visibility::Private {
            continue;
        }
        let declaring_class = ancestor_info
            .property_declaring_classes
            .get(property)
            .cloned()
            .unwrap_or_else(|| ancestor_name.to_string());
        return Some((declaring_class, visibility));
    }
    None
}

/// Returns class metadata from root parent to the requested class.
fn class_ancestry<'a>(module: &'a Module, class_name: &'a str) -> Vec<(&'a str, &'a ClassInfo)> {
    let mut chain = Vec::new();
    collect_class_ancestry(module, class_name, &mut chain);
    chain
}

/// Recursively collects class metadata from parent to child.
fn collect_class_ancestry<'a>(
    module: &'a Module,
    class_name: &'a str,
    chain: &mut Vec<(&'a str, &'a ClassInfo)>,
) {
    let Some(class_info) = module.class_infos.get(class_name) else {
        return;
    };
    if let Some(parent) = class_info.parent.as_deref() {
        collect_class_ancestry(module, parent, chain);
    }
    chain.push((class_name, class_info));
}

/// Returns the declared property visibility, defaulting to public metadata.
fn property_visibility<'a>(class_info: &'a ClassInfo, property: &str) -> &'a Visibility {
    class_info
        .property_visibilities
        .get(property)
        .unwrap_or(&Visibility::Public)
}

/// Returns true when the eval property bridge can enforce this visibility.
fn property_visibility_supported(visibility: &Visibility) -> bool {
    matches!(
        visibility,
        Visibility::Public | Visibility::Protected | Visibility::Private
    )
}

/// Returns true for property storage shapes the bridge can box and update.
/// `Void` slots (untyped properties never assigned by AOT code) are read-only:
/// they box as PHP null and report initialized, while writes fail in the store
/// bodies because the slot has no value storage.
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
            | PhpType::Void
    )
}

/// Emits `__elephc_eval_value_property_get(Mixed*, name, len, scope, scope_len) -> Mixed*`.
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

/// Emits `__elephc_eval_value_property_is_initialized(Mixed*, name, len, scope, scope_len) -> bool`.
fn emit_property_is_initialized_helper(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
) {
    emitter.blank();
    emitter.comment("--- eval bridge: user property initialization probe ---");
    label_c_global(
        module,
        emitter,
        "__elephc_eval_value_property_is_initialized",
    );
    match module.target.arch {
        Arch::AArch64 => emit_property_is_initialized_aarch64(module, emitter, data, slots),
        Arch::X86_64 => emit_property_is_initialized_x86_64(module, emitter, data, slots),
    }
}

/// Emits `__elephc_eval_value_property_set(Mixed*, name, len, Mixed*, scope, scope_len) -> bool`.
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
    let fail_label = "__elephc_eval_value_property_get_fail";
    let done_label = "__elephc_eval_value_property_get_done";
    emitter.instruction("sub sp, sp, #80"); // reserve helper frame for saved inputs, object, scope, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #64]"); // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #64"); // establish a stable helper frame pointer
    emitter.instruction("str x1, [sp, #0]"); // save the requested property-name pointer
    emitter.instruction("str x2, [sp, #8]"); // save the requested property-name length
    emitter.instruction("str x0, [sp, #24]"); // save the boxed receiver for stdClass fallback reads
    emitter.instruction("str x3, [sp, #32]"); // save the active eval class-scope pointer
    emitter.instruction("str x4, [sp, #40]"); // save the active eval class-scope length
    emitter.instruction(&format!("cbz x0, {}", null_label)); // null Mixed receiver reads as PHP null
    emitter.instruction("bl __rt_mixed_unbox"); // expose receiver tag and object payload
    emitter.instruction("cmp x0, #6"); // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("b.ne {}", null_label)); // non-object receivers read as PHP null
    emitter.instruction("str x1, [sp, #16]"); // save the unboxed object pointer for property loads
    emitter.instruction("ldr x9, [x1]"); // load the object's runtime class id
    emit_aarch64_property_dispatch(module, emitter, data, slots, "get", fail_label);
    emit_aarch64_stdclass_property_get_fallback(emitter);
    emitter.instruction(&format!("b {}", done_label)); // return after stdClass fallback get or null result
    emit_aarch64_get_slot_bodies(module, emitter, slots, done_label);
    emitter.label(fail_label);
    emitter.instruction("mov x0, xzr"); // report an inaccessible declared property read to Rust
    emitter.instruction(&format!("b {}", done_label)); // join the helper epilogue after access failure
    emitter.label(null_label);
    let null_symbol = module.target.extern_symbol("__elephc_eval_value_null");
    abi::emit_call_label(emitter, &null_symbol);
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #64]"); // restore the Rust caller frame
    emitter.instruction("add sp, sp, #80"); // release the helper frame
    emitter.instruction("ret"); // return the boxed property value to Rust
}

/// Emits the x86_64 property-get helper body.
fn emit_property_get_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
) {
    let null_label = "__elephc_eval_value_property_get_null_x";
    let fail_label = "__elephc_eval_value_property_get_fail_x";
    let done_label = "__elephc_eval_value_property_get_done_x";
    emitter.instruction("push rbp"); // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp"); // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48"); // reserve aligned slots for name, length, object, and scope
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi"); // save the requested property-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx"); // save the requested property-name length
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi"); // save the boxed receiver for stdClass fallback reads
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx"); // save the active eval class-scope pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], r8"); // save the active eval class-scope length
    emitter.instruction(&format!("test rdi, rdi")); // check whether the boxed receiver pointer is null
    emitter.instruction(&format!("jz {}", null_label)); // null Mixed receiver reads as PHP null
    emitter.instruction("mov rax, rdi"); // move the receiver into the mixed-unbox input register
    emitter.instruction("call __rt_mixed_unbox"); // expose receiver tag and object payload
    emitter.instruction("cmp rax, 6"); // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("jne {}", null_label)); // non-object receivers read as PHP null
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi"); // save the unboxed object pointer for property loads
    emitter.instruction("mov r11, QWORD PTR [rdi]"); // load the object's runtime class id
    emit_x86_64_property_dispatch(module, emitter, data, slots, "get", fail_label);
    emit_x86_64_stdclass_property_get_fallback(emitter);
    emitter.instruction(&format!("jmp {}", done_label)); // return after stdClass fallback get or null result
    emit_x86_64_get_slot_bodies(module, emitter, slots, done_label);
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax"); // report an inaccessible declared property read to Rust
    emitter.instruction(&format!("jmp {}", done_label)); // join the helper epilogue after access failure
    emitter.label(null_label);
    let null_symbol = module.target.extern_symbol("__elephc_eval_value_null");
    abi::emit_call_label(emitter, &null_symbol);
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp"); // discard helper spill slots
    emitter.instruction("pop rbp"); // restore the Rust caller frame pointer
    emitter.instruction("ret"); // return the boxed property value to Rust
}

/// Emits the ARM64 property-initialization helper body.
fn emit_property_is_initialized_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
) {
    let fail_label = "__elephc_eval_value_property_is_initialized_fail";
    let done_label = "__elephc_eval_value_property_is_initialized_done";
    emitter.instruction("sub sp, sp, #80"); // reserve helper frame for saved inputs, object, scope, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #64]"); // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #64"); // establish a stable helper frame pointer
    emitter.instruction("str x1, [sp, #0]"); // save the requested property-name pointer
    emitter.instruction("str x2, [sp, #8]"); // save the requested property-name length
    emitter.instruction("str x3, [sp, #32]"); // save the active eval class-scope pointer
    emitter.instruction("str x4, [sp, #40]"); // save the active eval class-scope length
    emitter.instruction(&format!("cbz x0, {}", fail_label)); // null Mixed receiver cannot have an initialized declared property
    emitter.instruction("bl __rt_mixed_unbox"); // expose receiver tag and object payload
    emitter.instruction("cmp x0, #6"); // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("b.ne {}", fail_label)); // non-object receivers cannot have initialized declared properties
    emitter.instruction("str x1, [sp, #16]"); // save the unboxed object pointer for marker loads
    emitter.instruction("ldr x9, [x1]"); // load the object's runtime class id
    emit_aarch64_property_dispatch(module, emitter, data, slots, "is_initialized", fail_label);
    emitter.instruction(&format!("b {}", fail_label)); // no supported declared property matched the request
    emit_aarch64_initialized_slot_bodies(module, emitter, slots, done_label);
    emitter.label(fail_label);
    emitter.instruction("mov x0, #0"); // report an initialization miss to Rust
    emitter.instruction(&format!("b {}", done_label)); // join the helper epilogue after a miss
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #64]"); // restore the Rust caller frame
    emitter.instruction("add sp, sp, #80"); // release the helper frame
    emitter.instruction("ret"); // return the initialization flag to Rust
}

/// Emits the x86_64 property-initialization helper body.
fn emit_property_is_initialized_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
) {
    let fail_label = "__elephc_eval_value_property_is_initialized_fail_x";
    let done_label = "__elephc_eval_value_property_is_initialized_done_x";
    emitter.instruction("push rbp"); // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp"); // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 48"); // reserve aligned slots for name, object, and scope
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi"); // save the requested property-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx"); // save the requested property-name length
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx"); // save the active eval class-scope pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], r8"); // save the active eval class-scope length
    emitter.instruction("test rdi, rdi"); // check whether the boxed receiver pointer is null
    emitter.instruction(&format!("jz {}", fail_label)); // null Mixed receiver cannot have an initialized declared property
    emitter.instruction("mov rax, rdi"); // move the receiver into the mixed-unbox input register
    emitter.instruction("call __rt_mixed_unbox"); // expose receiver tag and object payload
    emitter.instruction("cmp rax, 6"); // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("jne {}", fail_label)); // non-object receivers cannot have initialized declared properties
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi"); // save the unboxed object pointer for marker loads
    emitter.instruction("mov r11, QWORD PTR [rdi]"); // load the object's runtime class id
    emit_x86_64_property_dispatch(module, emitter, data, slots, "is_initialized", fail_label);
    emitter.instruction(&format!("jmp {}", fail_label)); // no supported declared property matched the request
    emit_x86_64_initialized_slot_bodies(module, emitter, slots, done_label);
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax"); // report an initialization miss to Rust
    emitter.instruction(&format!("jmp {}", done_label)); // join the helper epilogue after a miss
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp"); // discard helper spill slots
    emitter.instruction("pop rbp"); // restore the Rust caller frame pointer
    emitter.instruction("ret"); // return the initialization flag to Rust
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
    emitter.instruction("sub sp, sp, #96"); // reserve helper frame for inputs, object, scope, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #80]"); // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #80"); // establish a stable helper frame pointer
    emitter.instruction("str x1, [sp, #0]"); // save the requested property-name pointer
    emitter.instruction("str x2, [sp, #8]"); // save the requested property-name length
    emitter.instruction("str x3, [sp, #24]"); // save the boxed value being assigned
    emitter.instruction("str x0, [sp, #32]"); // save the boxed receiver for stdClass fallback writes
    emitter.instruction("str x4, [sp, #40]"); // save the active eval class-scope pointer
    emitter.instruction("str x5, [sp, #48]"); // save the active eval class-scope length
    emitter.instruction(&format!("cbz x0, {}", fail_label)); // null Mixed receiver cannot accept a property write
    emitter.instruction("bl __rt_mixed_unbox"); // expose receiver tag and object payload
    emitter.instruction("cmp x0, #6"); // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("b.ne {}", fail_label)); // non-object receivers reject the property write
    emitter.instruction("str x1, [sp, #16]"); // save the unboxed object pointer for property stores
    emitter.instruction("ldr x9, [x1]"); // load the object's runtime class id
    emit_aarch64_property_dispatch(module, emitter, data, slots, "set", fail_label);
    emit_aarch64_stdclass_property_set_fallback(module, emitter, fail_label, done_label);
    emit_aarch64_set_slot_bodies(module, emitter, data, slots, done_label, fail_label);
    emitter.label(fail_label);
    emitter.instruction("mov x0, #0"); // report a failed eval property write to Rust
    emitter.instruction(&format!("b {}", done_label)); // join the helper epilogue after failure
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #80]"); // restore the Rust caller frame
    emitter.instruction("add sp, sp, #96"); // release the helper frame
    emitter.instruction("ret"); // return the write-status flag to Rust
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
    emitter.instruction("push rbp"); // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp"); // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 64"); // reserve aligned slots for name, length, object, value, and scope
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi"); // save the requested property-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx"); // save the requested property-name length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx"); // save the boxed value being assigned
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi"); // save the boxed receiver for stdClass fallback writes
    emitter.instruction("mov QWORD PTR [rbp - 48], r8"); // save the active eval class-scope pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], r9"); // save the active eval class-scope length
    emitter.instruction("test rdi, rdi"); // check whether the boxed receiver pointer is null
    emitter.instruction(&format!("jz {}", fail_label)); // null Mixed receiver cannot accept a property write
    emitter.instruction("mov rax, rdi"); // move the receiver into the mixed-unbox input register
    emitter.instruction("call __rt_mixed_unbox"); // expose receiver tag and object payload
    emitter.instruction("cmp rax, 6"); // runtime tag 6 means the Mixed receiver is an object
    emitter.instruction(&format!("jne {}", fail_label)); // non-object receivers reject the property write
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi"); // save the unboxed object pointer for property stores
    emitter.instruction("mov r11, QWORD PTR [rdi]"); // load the object's runtime class id
    emit_x86_64_property_dispatch(module, emitter, data, slots, "set", fail_label);
    emit_x86_64_stdclass_property_set_fallback(module, emitter, fail_label, done_label);
    emit_x86_64_set_slot_bodies(module, emitter, data, slots, done_label, fail_label);
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax"); // report a failed eval property write to Rust
    emitter.instruction(&format!("jmp {}", done_label)); // join the helper epilogue after failure
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp"); // discard helper spill slots
    emitter.instruction("pop rbp"); // restore the Rust caller frame pointer
    emitter.instruction("ret"); // return the write-status flag to Rust
}

/// Emits an ARM64 fallback read for stdClass dynamic properties.
fn emit_aarch64_stdclass_property_get_fallback(emitter: &mut Emitter) {
    emitter.instruction("ldr x0, [sp, #24]"); // reload the boxed receiver for the Mixed stdClass getter
    emitter.instruction("ldr x1, [sp, #0]"); // reload requested property-name pointer
    emitter.instruction("ldr x2, [sp, #8]"); // reload requested property-name length
    emitter.instruction("bl __rt_mixed_property_get"); // read stdClass dynamic property or return Mixed(null)
}

/// Emits an x86_64 fallback read for stdClass dynamic properties.
fn emit_x86_64_stdclass_property_get_fallback(emitter: &mut Emitter) {
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]"); // reload the boxed receiver for the Mixed stdClass getter
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]"); // reload requested property-name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]"); // reload requested property-name length
    emitter.instruction("call __rt_mixed_property_get"); // read stdClass dynamic property or return Mixed(null)
}

/// Emits an ARM64 fallback write for stdClass dynamic properties.
fn emit_aarch64_stdclass_property_set_fallback(
    module: &Module,
    emitter: &mut Emitter,
    fail_label: &str,
    done_label: &str,
) {
    let Some(class_id) = stdclass_class_id(module) else {
        emitter.instruction(&format!("b {}", fail_label)); // reject writes when stdClass metadata is unavailable
        return;
    };
    emitter.instruction("ldr x9, [sp, #16]"); // reload the unboxed object pointer for stdClass class check
    emitter.instruction("ldr x9, [x9]"); // load the object's runtime class id
    abi::emit_load_int_immediate(emitter, "x10", class_id as i64);
    emitter.instruction("cmp x9, x10"); // check whether the receiver is stdClass
    emitter.instruction(&format!("b.ne {}", fail_label)); // non-stdClass misses remain unsupported eval writes
    emitter.instruction("ldr x0, [sp, #32]"); // reload the boxed receiver for the Mixed stdClass setter
    emitter.instruction("ldr x1, [sp, #0]"); // reload requested property-name pointer
    emitter.instruction("ldr x2, [sp, #8]"); // reload requested property-name length
    emitter.instruction("ldr x3, [sp, #24]"); // reload the boxed value being assigned
    emitter.instruction("bl __rt_mixed_property_set"); // write the stdClass dynamic property
    emitter.instruction("mov x0, #1"); // report a successful eval property write to Rust
    emitter.instruction(&format!("b {}", done_label)); // join the helper epilogue after stdClass write
}

/// Emits an x86_64 fallback write for stdClass dynamic properties.
fn emit_x86_64_stdclass_property_set_fallback(
    module: &Module,
    emitter: &mut Emitter,
    fail_label: &str,
    done_label: &str,
) {
    let Some(class_id) = stdclass_class_id(module) else {
        emitter.instruction(&format!("jmp {}", fail_label)); // reject writes when stdClass metadata is unavailable
        return;
    };
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]"); // reload the unboxed object pointer for stdClass class check
    emitter.instruction("mov r11, QWORD PTR [r11]"); // load the object's runtime class id
    abi::emit_load_int_immediate(emitter, "r10", class_id as i64);
    emitter.instruction("cmp r11, r10"); // check whether the receiver is stdClass
    emitter.instruction(&format!("jne {}", fail_label)); // non-stdClass misses remain unsupported eval writes
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]"); // reload the boxed receiver for the Mixed stdClass setter
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]"); // reload requested property-name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]"); // reload requested property-name length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]"); // reload the boxed value being assigned
    emitter.instruction("call __rt_mixed_property_set"); // write the stdClass dynamic property
    emitter.instruction("mov rax, 1"); // report a successful eval property write to Rust
    emitter.instruction(&format!("jmp {}", done_label)); // join the helper epilogue after stdClass write
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
    fail_label: &str,
) {
    for (class_id, class_slots) in grouped_slots(slots) {
        let class_label = format!("__elephc_eval_property_{}_class_{}", mode, class_id);
        let next_label = format!("__elephc_eval_property_{}_next_{}", mode, class_id);
        abi::emit_load_int_immediate(emitter, "x10", class_id as i64);
        emitter.instruction("cmp x9, x10"); // compare receiver class id against this eval bridge class
        emitter.instruction(&format!("b.ne {}", next_label)); // try the next class when ids differ
        for slot in class_slots {
            emit_aarch64_property_name_compare(module, emitter, data, slot, mode, fail_label);
        }
        emitter.label(&class_label);
        emitter.instruction(&format!("b {}", next_label)); // fall through to the next class after a name miss
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
    fail_label: &str,
) {
    for (class_id, class_slots) in grouped_slots(slots) {
        let class_label = format!("__elephc_eval_property_{}_class_{}_x", mode, class_id);
        let next_label = format!("__elephc_eval_property_{}_next_{}_x", mode, class_id);
        abi::emit_load_int_immediate(emitter, "r10", class_id as i64);
        emitter.instruction("cmp r11, r10"); // compare receiver class id against this eval bridge class
        emitter.instruction(&format!("jne {}", next_label)); // try the next class when ids differ
        for slot in class_slots {
            emit_x86_64_property_name_compare(module, emitter, data, slot, mode, fail_label);
        }
        emitter.label(&class_label);
        emitter.instruction(&format!("jmp {}", next_label)); // fall through to the next class after a name miss
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
    fail_label: &str,
) {
    let (label, len) = data.add_string(slot.property.as_bytes());
    emitter.instruction("ldr x1, [sp, #0]"); // reload requested property-name pointer
    emitter.instruction("ldr x2, [sp, #8]"); // reload requested property-name length
    abi::emit_symbol_address(emitter, "x3", &label);
    abi::emit_load_int_immediate(emitter, "x4", len as i64);
    emitter.instruction("bl __rt_str_eq"); // compare requested property name with this declared property
    let target_label = slot_body_label(module, slot, mode);
    if matches!(slot.visibility, Visibility::Public) {
        emitter.instruction(&format!("cbnz x0, {}", target_label)); // dispatch to the property body when the names match
        return;
    }
    let miss_label = slot_access_miss_label(module, slot, mode);
    emitter.instruction(&format!("cbz x0, {}", miss_label)); // continue property dispatch when names differ
    let scope_ok_label = slot_scope_ok_label(module, slot, mode);
    let scope_fail_label = if slot.is_hidden_shadow {
        miss_label.as_str()
    } else {
        fail_label
    };
    emit_aarch64_property_scope_check(emitter, data, slot, mode, &scope_ok_label, scope_fail_label);
    emitter.label(&scope_ok_label);
    emitter.instruction(&format!("b {}", target_label)); // dispatch after scoped visibility is satisfied
    emitter.label(&miss_label);
}

/// Emits one x86_64 property-name comparison and branch to the matching body.
fn emit_x86_64_property_name_compare(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    mode: &str,
    fail_label: &str,
) {
    let (label, len) = data.add_string(slot.property.as_bytes());
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]"); // reload requested property-name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]"); // reload requested property-name length
    abi::emit_symbol_address(emitter, "rdx", &label);
    abi::emit_load_int_immediate(emitter, "rcx", len as i64);
    emitter.instruction("call __rt_str_eq"); // compare requested property name with this declared property
    emitter.instruction("test rax, rax"); // check whether the property names matched
    let target_label = slot_body_label(module, slot, mode);
    if matches!(slot.visibility, Visibility::Public) {
        emitter.instruction(&format!("jne {}", target_label)); // dispatch to the property body when the names match
        return;
    }
    let miss_label = slot_access_miss_label(module, slot, mode);
    emitter.instruction(&format!("je {}", miss_label)); // continue property dispatch when names differ
    let scope_ok_label = slot_scope_ok_label(module, slot, mode);
    let scope_fail_label = if slot.is_hidden_shadow {
        miss_label.as_str()
    } else {
        fail_label
    };
    emit_x86_64_property_scope_check(emitter, data, slot, mode, &scope_ok_label, scope_fail_label);
    emitter.label(&scope_ok_label);
    emitter.instruction(&format!("jmp {}", target_label)); // dispatch after scoped visibility is satisfied
    emitter.label(&miss_label);
}

/// Emits ARM64 visibility checks for a protected/private property bridge hit.
fn emit_aarch64_property_scope_check(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    mode: &str,
    success_label: &str,
    fail_label: &str,
) {
    let (scope_ptr_offset, scope_len_offset) = aarch64_scope_offsets(mode);
    emitter.instruction(&format!("ldr x1, [sp, #{}]", scope_ptr_offset)); // reload the active eval class-scope pointer
    emitter.instruction(&format!("ldr x2, [sp, #{}]", scope_len_offset)); // reload the active eval class-scope length
    emitter.instruction(&format!("cbz x1, {}", fail_label)); // reject scoped property access outside a class scope
    for scope_name in &slot.allowed_scopes {
        let (label, len) = data.add_string(scope_name.as_bytes());
        emitter.instruction(&format!("ldr x1, [sp, #{}]", scope_ptr_offset)); // reload the active eval class-scope pointer
        emitter.instruction(&format!("ldr x2, [sp, #{}]", scope_len_offset)); // reload the active eval class-scope length
        abi::emit_symbol_address(emitter, "x3", &label);
        abi::emit_load_int_immediate(emitter, "x4", len as i64);
        emitter.instruction("bl __rt_strcasecmp"); // compare current eval scope with an allowed class
        emitter.instruction(&format!("cbz x0, {}", success_label)); // accept access when the current scope is allowed
    }
    emitter.instruction(&format!("b {}", fail_label)); // reject scoped property access from unrelated classes
}

/// Emits x86_64 visibility checks for a protected/private property bridge hit.
fn emit_x86_64_property_scope_check(
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    mode: &str,
    success_label: &str,
    fail_label: &str,
) {
    let (scope_ptr_offset, scope_len_offset) = x86_64_scope_offsets(mode);
    emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", scope_ptr_offset)); // reload the active eval class-scope pointer
    emitter.instruction(&format!("mov rsi, QWORD PTR [rbp - {}]", scope_len_offset)); // reload the active eval class-scope length
    emitter.instruction("test rdi, rdi"); // check whether eval is executing inside a class scope
    emitter.instruction(&format!("jz {}", fail_label)); // reject scoped property access outside a class scope
    for scope_name in &slot.allowed_scopes {
        let (label, len) = data.add_string(scope_name.as_bytes());
        emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {}]", scope_ptr_offset)); // reload the active eval class-scope pointer
        emitter.instruction(&format!("mov rsi, QWORD PTR [rbp - {}]", scope_len_offset)); // reload the active eval class-scope length
        abi::emit_symbol_address(emitter, "rdx", &label);
        abi::emit_load_int_immediate(emitter, "rcx", len as i64);
        emitter.instruction("call __rt_strcasecmp"); // compare current eval scope with an allowed class
        emitter.instruction("test rax, rax"); // check whether the current scope matched
        emitter.instruction(&format!("je {}", success_label)); // accept access when the current scope is allowed
    }
    emitter.instruction(&format!("jmp {}", fail_label)); // reject scoped property access from unrelated classes
}

/// Returns ARM64 stack offsets for the class-scope pointer and length.
fn aarch64_scope_offsets(mode: &str) -> (usize, usize) {
    match mode {
        "get" | "is_initialized" => (32, 40),
        "set" => (40, 48),
        _ => unreachable!("eval property helpers only use get/set/is_initialized modes"),
    }
}

/// Returns x86_64 frame offsets for the class-scope pointer and length.
fn x86_64_scope_offsets(mode: &str) -> (usize, usize) {
    match mode {
        "get" | "is_initialized" => (40, 48),
        "set" => (48, 56),
        _ => unreachable!("eval property helpers only use get/set/is_initialized modes"),
    }
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
        emit_aarch64_uninitialized_property_get_guard(emitter, slot, done_label);
        emit_aarch64_box_property_slot(emitter, slot);
        emitter.instruction(&format!("b {}", done_label)); // return after boxing the declared property value
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
        emit_x86_64_uninitialized_property_get_guard(emitter, slot, done_label);
        emit_x86_64_box_property_slot(emitter, slot);
        emitter.instruction(&format!("jmp {}", done_label)); // return after boxing the declared property value
    }
}

/// Emits ARM64 property-initialization bodies for every bridge-supported property slot.
fn emit_aarch64_initialized_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalPropertySlot],
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "is_initialized"));
        emit_aarch64_property_initialized_flag(emitter, slot);
        emitter.instruction(&format!("b {}", done_label)); // return after materializing the initialization flag
    }
}

/// Emits x86_64 property-initialization bodies for every bridge-supported property slot.
fn emit_x86_64_initialized_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    slots: &[EvalPropertySlot],
    done_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "is_initialized"));
        emit_x86_64_property_initialized_flag(emitter, slot);
        emitter.instruction(&format!("jmp {}", done_label)); // return after materializing the initialization flag
    }
}

/// Emits ARM64 property-set bodies for every bridge-supported property slot.
fn emit_aarch64_set_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    done_label: &str,
    fail_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "set"));
        emit_aarch64_store_property_slot(module, emitter, data, slot, fail_label);
        emitter.instruction("mov x0, #1"); // report a successful eval property write to Rust
        emitter.instruction(&format!("b {}", done_label)); // return after storing the declared property value
    }
}

/// Emits x86_64 property-set bodies for every bridge-supported property slot.
fn emit_x86_64_set_slot_bodies(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slots: &[EvalPropertySlot],
    done_label: &str,
    fail_label: &str,
) {
    for slot in slots {
        emitter.label(&slot_body_label(module, slot, "set"));
        emit_x86_64_store_property_slot(module, emitter, data, slot, fail_label);
        emitter.instruction("mov rax, 1"); // report a successful eval property write to Rust
        emitter.instruction(&format!("jmp {}", done_label)); // return after storing the declared property value
    }
}

/// Emits an ARM64 boolean for one declared property's initialized state.
fn emit_aarch64_property_initialized_flag(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    if !slot.is_declared {
        emitter.instruction("mov x0, #1"); // non-typed declared properties are always initialized
        return;
    }
    emitter.instruction("ldr x10, [sp, #16]"); // reload the unboxed object pointer
    emitter.instruction(&format!("ldr x11, [x10, #{}]", slot.offset + 8)); // load the typed-property initialization marker
    abi::emit_load_int_immediate(emitter, "x12", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp x11, x12"); // compare the property marker against the uninitialized sentinel
    emitter.instruction("cset x0, ne"); // materialize true when the instance property is initialized
}

/// Emits an x86_64 boolean for one declared property's initialized state.
fn emit_x86_64_property_initialized_flag(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    if !slot.is_declared {
        emitter.instruction("mov rax, 1"); // non-typed declared properties are always initialized
        return;
    }
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]"); // reload the unboxed object pointer
    emitter.instruction(&format!("mov rax, QWORD PTR [r11 + {}]", slot.offset + 8)); // load the typed-property initialization marker
    abi::emit_load_int_immediate(emitter, "r10", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp rax, r10"); // compare the property marker against the uninitialized sentinel
    emitter.instruction("setne al"); // materialize true when the instance property is initialized
    emitter.instruction("movzx rax, al"); // widen the initialization flag into the return register
}

/// Emits an ARM64 typed-property guard before boxing an eval bridge property read.
fn emit_aarch64_uninitialized_property_get_guard(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    done_label: &str,
) {
    if !slot.is_declared {
        return;
    }
    let initialized_label = format!(
        "{}_initialized",
        label_fragment(&slot_body_label_raw(slot, "get"))
    );
    emitter.instruction("ldr x10, [sp, #16]"); // reload the unboxed object pointer for marker inspection
    emitter.instruction(&format!("ldr x11, [x10, #{}]", slot.offset + 8)); // load the typed-property initialization marker
    abi::emit_load_int_immediate(emitter, "x12", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp x11, x12"); // compare the property marker against the uninitialized sentinel
    emitter.instruction(&format!("b.ne {}", initialized_label)); // continue boxing once the instance property is initialized
    emitter.instruction("mov x0, xzr"); // report uninitialized property reads as bridge failures
    emitter.instruction(&format!("b {}", done_label)); // return the failure to Rust without boxing storage
    emitter.label(&initialized_label);
}

/// Emits an x86_64 typed-property guard before boxing an eval bridge property read.
fn emit_x86_64_uninitialized_property_get_guard(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    done_label: &str,
) {
    if !slot.is_declared {
        return;
    }
    let initialized_label = format!(
        "{}_initialized_x",
        label_fragment(&slot_body_label_raw(slot, "get"))
    );
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]"); // reload the unboxed object pointer for marker inspection
    emitter.instruction(&format!("mov rax, QWORD PTR [r10 + {}]", slot.offset + 8)); // load the typed-property initialization marker
    abi::emit_load_int_immediate(emitter, "r11", UNINITIALIZED_TYPED_PROPERTY_SENTINEL);
    emitter.instruction("cmp rax, r11"); // compare the property marker against the uninitialized sentinel
    emitter.instruction(&format!("jne {}", initialized_label)); // continue boxing once the instance property is initialized
    emitter.instruction("xor eax, eax"); // report uninitialized property reads as bridge failures
    emitter.instruction(&format!("jmp {}", done_label)); // return the failure to Rust without boxing storage
    emitter.label(&initialized_label);
}

/// Boxes a property value loaded from an ARM64 object slot into a Mixed cell.
fn emit_aarch64_box_property_slot(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    emitter.instruction("ldr x9, [sp, #16]"); // reload the unboxed object pointer
    match slot.ty.codegen_repr() {
        PhpType::Int
        | PhpType::Bool
        | PhpType::Object(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. } => {
            emitter.instruction(&format!("ldr x1, [x9, #{}]", slot.offset)); // load the property payload low word
            emitter.instruction("mov x2, xzr"); // heap/scalar property payloads do not use a high word here
            abi::emit_load_int_immediate(emitter, "x0", runtime_value_tag(&slot.ty) as i64);
            emitter.instruction("bl __rt_mixed_from_value"); // box the property payload as a Mixed cell
        }
        PhpType::Float => {
            emitter.instruction(&format!("ldr d0, [x9, #{}]", slot.offset)); // load the floating property payload
            emitter.instruction("fmov x1, d0"); // move float bits into the Mixed low payload word
            emitter.instruction("mov x2, xzr"); // float payloads do not use a high word
            emitter.instruction("mov x0, #2"); // runtime tag 2 = float
            emitter.instruction("bl __rt_mixed_from_value"); // box the floating property payload as Mixed
        }
        PhpType::Str => {
            emitter.instruction(&format!("ldr x1, [x9, #{}]", slot.offset));    // load the string property pointer
            emitter.instruction(&format!("ldr x2, [x9, #{}]", slot.offset + 8)); // load the string property length
            emitter.instruction("mov x0, #1");                                  // runtime tag 1 = string
            emitter.instruction("bl __rt_mixed_from_value");                    // persist and box the string property payload
        }
        PhpType::TaggedScalar => {
            emitter.instruction(&format!("ldr x0, [x9, #{}]", slot.offset)); // load the nullable integer property payload
            emitter.instruction(&format!("ldr x1, [x9, #{}]", slot.offset + 8)); //load the nullable integer property tag
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
            emitter.instruction(&format!("ldr x0, [x9, #{}]", slot.offset)); // load the stored Mixed property cell
            emitter.instruction(&format!("cbz x0, {}", null_label)); // null property storage reads as PHP null
            emitter.instruction("bl __rt_incref"); // retain the stored Mixed cell for the eval caller
            emitter.instruction(&format!("b {}", done_label)); // skip null materialization after a retained hit
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
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]"); // reload the unboxed object pointer
    match slot.ty.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::Object(_) | PhpType::Array(_) | PhpType::AssocArray { .. } => {
            emitter.instruction(&format!("mov rdi, QWORD PTR [r11 + {}]", slot.offset)); // load the property payload low word
            emitter.instruction("xor esi, esi");                                // heap/scalar property payloads do not use a high word here
            abi::emit_load_int_immediate(emitter, "rax", runtime_value_tag(&slot.ty) as i64);
            emitter.instruction("call __rt_mixed_from_value"); // box the property payload as a Mixed cell
        }
        PhpType::Float => {
            emitter.instruction(&format!("movsd xmm0, QWORD PTR [r11 + {}]", slot.offset)); // load the floating property payload
            emitter.instruction("movq rdi, xmm0");                              // move float bits into the Mixed low payload word
            emitter.instruction("xor esi, esi");                                // float payloads do not use a high word
            emitter.instruction("mov eax, 2");                                  // runtime tag 2 = float
            emitter.instruction("call __rt_mixed_from_value");                  // box the floating property payload as Mixed
        }
        PhpType::Str => {
            emitter.instruction(&format!("mov rdi, QWORD PTR [r11 + {}]", slot.offset)); // load the string property pointer
            emitter.instruction(&format!("mov rsi, QWORD PTR [r11 + {}]", slot.offset + 8)); // load the string property length
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
            emitter.instruction(&format!("mov rax, QWORD PTR [r11 + {}]", slot.offset)); // load the stored Mixed property cell
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
fn emit_aarch64_store_property_slot(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    fail_label: &str,
) {
    match slot.ty.codegen_repr() {
        PhpType::Int => emit_aarch64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_int", "x0"),
        PhpType::Bool => {
            emit_aarch64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_bool", "x0")
        }
        PhpType::Float => {
            emitter.instruction("ldr x0, [sp, #24]"); // reload the boxed eval value for float coercion
            emitter.instruction("bl __rt_mixed_cast_float"); // coerce the eval value to a PHP float
            emitter.instruction("ldr x9, [sp, #16]"); // reload the unboxed object pointer for the store
            emitter.instruction(&format!("str d0, [x9, #{}]", slot.offset)); // store the coerced float into the property slot
            emit_aarch64_clear_scalar_property_marker(emitter, slot);
        }
        PhpType::Str => {
            emitter.instruction("ldr x0, [sp, #24]");                           // reload the boxed eval value for string coercion
            emitter.instruction("bl __rt_mixed_cast_string");                   // coerce the eval value to a PHP string pair
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the unboxed object pointer for the store
            emitter.instruction(&format!("str x1, [x9, #{}]", slot.offset));    // store the coerced string pointer into the property slot
            emitter.instruction(&format!("str x2, [x9, #{}]", slot.offset + 8)); // store the coerced string length into the property slot
        }
        PhpType::TaggedScalar => emit_aarch64_store_tagged_scalar_property(emitter, slot),
        PhpType::Array(_) => emit_aarch64_store_heap_property_slot(emitter, slot, 4, fail_label),
        PhpType::AssocArray { .. } => {
            emit_aarch64_store_heap_property_slot(emitter, slot, 5, fail_label);
        }
        PhpType::Object(class_name) => {
            emit_aarch64_store_object_property_slot(
                module,
                emitter,
                data,
                slot,
                &class_name,
                fail_label,
            );
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emitter.instruction("ldr x0, [sp, #24]");                           // reload the boxed eval value being assigned
            emitter.instruction("bl __rt_incref");                              // retain the Mixed cell for property ownership
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the unboxed object pointer for the store
            emitter.instruction(&format!("str x0, [x9, #{}]", slot.offset));    // store the retained Mixed cell into the property slot
            emitter.instruction(&format!("str xzr, [x9, #{}]", slot.offset + 8)); // clear the unused property high word
        }
        PhpType::Void => {
            emitter.instruction(&format!("b {}", fail_label)); // Void slots have no value storage; report the eval write as unsupported
        }
        _ => {}
    }
}

/// Stores a boxed Mixed eval value into an x86_64 object property slot.
fn emit_x86_64_store_property_slot(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    fail_label: &str,
) {
    match slot.ty.codegen_repr() {
        PhpType::Int => emit_x86_64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_int", "rax"),
        PhpType::Bool => {
            emit_x86_64_store_cast_scalar(emitter, slot, "__rt_mixed_cast_bool", "rax")
        }
        PhpType::Float => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // reload the boxed eval value for float coercion
            emitter.instruction("call __rt_mixed_cast_float");                  // coerce the eval value to a PHP float
            emitter.instruction("mov r11, QWORD PTR [rbp - 24]");               // reload the unboxed object pointer for the store
            emitter.instruction(&format!("movsd QWORD PTR [r11 + {}], xmm0", slot.offset)); // store the coerced float into the property slot
            emit_x86_64_clear_scalar_property_marker(emitter, slot);
        }
        PhpType::Str => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // reload the boxed eval value for string coercion
            emitter.instruction("call __rt_mixed_cast_string");                 // coerce the eval value to a PHP string pair
            emitter.instruction("mov r11, QWORD PTR [rbp - 24]");               // reload the unboxed object pointer for the store
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", slot.offset)); // store the coerced string pointer into the property slot
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rdx", slot.offset + 8)); // store the coerced string length into the property slot
        }
        PhpType::TaggedScalar => emit_x86_64_store_tagged_scalar_property(emitter, slot),
        PhpType::Array(_) => emit_x86_64_store_heap_property_slot(emitter, slot, 4, fail_label),
        PhpType::AssocArray { .. } => {
            emit_x86_64_store_heap_property_slot(emitter, slot, 5, fail_label);
        }
        PhpType::Object(class_name) => {
            emit_x86_64_store_object_property_slot(
                module,
                emitter,
                data,
                slot,
                &class_name,
                fail_label,
            );
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emitter.instruction("mov rax, QWORD PTR [rbp - 32]");               // reload the boxed eval value being assigned
            emitter.instruction("call __rt_incref");                            // retain the Mixed cell for property ownership
            emitter.instruction("mov r11, QWORD PTR [rbp - 24]");               // reload the unboxed object pointer for the store
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", slot.offset)); // store the retained Mixed cell into the property slot
            emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", slot.offset + 8)); // clear the unused property high word
        }
        PhpType::Void => {
            emitter.instruction(&format!("jmp {}", fail_label)); // Void slots have no value storage; report the eval write as unsupported
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
    emitter.instruction(&format!("str {}, [x9, #{}]", result_reg, slot.offset)); // store the coerced scalar into the property slot
    emit_aarch64_clear_scalar_property_marker(emitter, slot);
}

/// Clears an ARM64 one-word scalar typed-property marker after a successful store.
fn emit_aarch64_clear_scalar_property_marker(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    if slot.is_declared {
        emitter.instruction(&format!("str xzr, [x9, #{}]", slot.offset + 8)); // clear the typed-property initialization marker
    }
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
    emitter.instruction("ldr x0, [sp, #24]"); // reload the boxed eval value for nullable-int inspection
    emitter.instruction("bl __rt_mixed_unbox"); // expose the assigned value tag and payload words
    emitter.instruction("cmp x0, #8"); // runtime tag 8 means the assigned value is null
    emitter.instruction(&format!("b.eq {}", null_label)); // materialize a tagged null for null property writes
    emitter.instruction("ldr x0, [sp, #24]"); // reload the boxed eval value for integer coercion
    emitter.instruction("bl __rt_mixed_cast_int"); // coerce non-null eval values to a PHP int payload
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
    emitter.instruction(&format!("b {}", done_label)); // skip tagged-null materialization after integer coercion
    emitter.label(&null_label);
    crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
    emitter.label(&done_label);
    emitter.instruction("ldr x9, [sp, #16]"); // reload the unboxed object pointer for the store
    emitter.instruction(&format!("str x0, [x9, #{}]", slot.offset)); // store the nullable integer payload into the property slot
    emitter.instruction(&format!("str x1, [x9, #{}]", slot.offset + 8)); // store the nullable integer tag into the property slot
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
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], {}", slot.offset, result_reg)); // store the coerced scalar into the property slot
    emit_x86_64_clear_scalar_property_marker(emitter, slot);
}

/// Clears an x86_64 one-word scalar typed-property marker after a successful store.
fn emit_x86_64_clear_scalar_property_marker(emitter: &mut Emitter, slot: &EvalPropertySlot) {
    if slot.is_declared {
        emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", slot.offset + 8));
        // clear the typed-property initialization marker
    }
}

/// Stores a boxed ARM64 eval heap value into an array-like property slot.
fn emit_aarch64_store_heap_property_slot(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    expected_tag: i64,
    fail_label: &str,
) {
    emitter.instruction("ldr x0, [sp, #24]"); // reload the boxed eval value for heap payload inspection
    emitter.instruction("bl __rt_mixed_unbox"); // expose the assigned heap value tag and payload pointer
    abi::emit_load_int_immediate(emitter, "x10", expected_tag);
    emitter.instruction("cmp x0, x10"); // compare the assigned value tag with the property storage ABI
    emitter.instruction(&format!("b.ne {}", fail_label)); // reject heap values with an incompatible ABI shape
    emitter.instruction("mov x0, x1"); // move the unboxed heap pointer into the retained-result register
    abi::emit_incref_if_refcounted(emitter, &slot.ty.codegen_repr());
    emitter.instruction("ldr x9, [sp, #16]"); // reload the unboxed object pointer for the heap store
    emitter.instruction(&format!("str x0, [x9, #{}]", slot.offset)); // store the retained heap pointer into the property slot
    emitter.instruction(&format!("str xzr, [x9, #{}]", slot.offset + 8)); // clear the typed-property initialization marker
}

/// Validates and stores a boxed ARM64 eval object into an object property slot.
fn emit_aarch64_store_object_property_slot(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    class_name: &str,
    fail_label: &str,
) {
    if !class_name.is_empty() {
        let (label, len) = data.add_string(class_name.as_bytes());
        let is_a_symbol = module.target.extern_symbol("__elephc_eval_value_is_a");
        emitter.instruction("ldr x0, [sp, #24]"); // reload the boxed eval value for object type validation
        abi::emit_symbol_address(emitter, "x1", &label);
        abi::emit_load_int_immediate(emitter, "x2", len as i64);
        emitter.instruction("mov x3, xzr"); // allow exact class matches for object property type hints
        abi::emit_call_label(emitter, &is_a_symbol);
        emitter.instruction(&format!("cbz x0, {}", fail_label)); // reject values that fail the object property type hint
    }
    emit_aarch64_store_heap_property_slot(emitter, slot, 6, fail_label);
}

/// Stores a boxed x86_64 eval heap value into an array-like property slot.
fn emit_x86_64_store_heap_property_slot(
    emitter: &mut Emitter,
    slot: &EvalPropertySlot,
    expected_tag: i64,
    fail_label: &str,
) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]"); // reload the boxed eval value for heap payload inspection
    emitter.instruction("call __rt_mixed_unbox"); // expose the assigned heap value tag and payload pointer
    abi::emit_load_int_immediate(emitter, "r10", expected_tag);
    emitter.instruction("cmp rax, r10"); // compare the assigned value tag with the property storage ABI
    emitter.instruction(&format!("jne {}", fail_label)); // reject heap values with an incompatible ABI shape
    emitter.instruction("mov rax, rdi"); // move the unboxed heap pointer into the retained-result register
    abi::emit_incref_if_refcounted(emitter, &slot.ty.codegen_repr());
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]"); // reload the unboxed object pointer for the heap store
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", slot.offset)); //store the retained heap pointer into the property slot
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], 0", slot.offset + 8));
    //clear the typed-property initialization marker
}

/// Validates and stores a boxed x86_64 eval object into an object property slot.
fn emit_x86_64_store_object_property_slot(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    slot: &EvalPropertySlot,
    class_name: &str,
    fail_label: &str,
) {
    if !class_name.is_empty() {
        let (label, len) = data.add_string(class_name.as_bytes());
        let is_a_symbol = module.target.extern_symbol("__elephc_eval_value_is_a");
        emitter.instruction("mov rdi, QWORD PTR [rbp - 32]"); // reload the boxed eval value for object type validation
        abi::emit_symbol_address(emitter, "rsi", &label);
        abi::emit_load_int_immediate(emitter, "rdx", len as i64);
        emitter.instruction("xor ecx, ecx"); // allow exact class matches for object property type hints
        abi::emit_call_label(emitter, &is_a_symbol);
        emitter.instruction("test rax, rax"); // check whether the value satisfied the object property type hint
        emitter.instruction(&format!("je {}", fail_label)); // reject values that fail the object property type hint
    }
    emit_x86_64_store_heap_property_slot(emitter, slot, 6, fail_label);
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
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]"); // reload the boxed eval value for nullable-int inspection
    emitter.instruction("call __rt_mixed_unbox"); // expose the assigned value tag and payload words
    emitter.instruction("cmp rax, 8"); // runtime tag 8 means the assigned value is null
    emitter.instruction(&format!("je {}", null_label)); // materialize a tagged null for null property writes
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]"); // reload the boxed eval value for integer coercion
    emitter.instruction("call __rt_mixed_cast_int"); // coerce non-null eval values to a PHP int payload
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
    emitter.instruction(&format!("jmp {}", done_label)); // skip tagged-null materialization after integer coercion
    emitter.label(&null_label);
    crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
    emitter.label(&done_label);
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]"); // reload the unboxed object pointer for the store
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rax", slot.offset)); //store the nullable integer payload into the property slot
    emitter.instruction(&format!("mov QWORD PTR [r11 + {}], rdx", slot.offset + 8));
    //store the nullable integer tag into the property slot
}

/// Groups property slots by class id while preserving sorted class order.
fn grouped_slots(slots: &[EvalPropertySlot]) -> BTreeMap<u64, Vec<&EvalPropertySlot>> {
    let mut grouped = BTreeMap::new();
    for slot in slots {
        grouped
            .entry(slot.class_id)
            .or_insert_with(Vec::new)
            .push(slot);
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

/// Returns a platform-safe label for continuing after a scoped property name miss.
fn slot_access_miss_label(module: &Module, slot: &EvalPropertySlot, mode: &str) -> String {
    format!("{}_access_miss", slot_body_label(module, slot, mode))
}

/// Returns a platform-safe label for a successful scoped property visibility check.
fn slot_scope_ok_label(module: &Module, slot: &EvalPropertySlot, mode: &str) -> String {
    format!("{}_scope_ok", slot_body_label(module, slot, mode))
}

/// Returns the architecture-independent body label stem for a property slot.
fn slot_body_label_raw(slot: &EvalPropertySlot, mode: &str) -> String {
    format!(
        "__elephc_eval_property_{}_{}_{}_{}",
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
