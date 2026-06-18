//! Purpose:
//! Emits user-assembly helpers that let libelephc-eval materialize
//! ReflectionClass, ReflectionMethod, ReflectionProperty, ReflectionClassConstant,
//! and ReflectionEnum* objects with private metadata slots populated from
//! runtime eval declarations.
//!
//! Called from:
//! - `crate::codegen_ir::finalize_user_asm()` when an EIR module uses eval.
//!
//! Key details:
//! - Reflection owner objects store private metadata slots such as `__attrs`,
//!   `__name`, and the ReflectionClass relation-name arrays.
//! - The helper retains supplied array payloads for object ownership.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::ir::{Function, LocalKind, Module};
use crate::types::ClassInfo;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Fixed object layout for one synthetic Reflection owner class.
struct ReflectionOwnerLayout {
    class_id: u64,
    property_count: usize,
    name_lo: Option<usize>,
    name_hi: Option<usize>,
    short_name_lo: Option<usize>,
    short_name_hi: Option<usize>,
    namespace_name_lo: Option<usize>,
    namespace_name_hi: Option<usize>,
    interface_names_lo: Option<usize>,
    interface_names_hi: Option<usize>,
    trait_names_lo: Option<usize>,
    trait_names_hi: Option<usize>,
    attrs_lo: usize,
    attrs_hi: usize,
    is_final_lo: Option<usize>,
    is_final_hi: Option<usize>,
    is_abstract_lo: Option<usize>,
    is_abstract_hi: Option<usize>,
    is_interface_lo: Option<usize>,
    is_interface_hi: Option<usize>,
    is_trait_lo: Option<usize>,
    is_trait_hi: Option<usize>,
    is_enum_lo: Option<usize>,
    is_enum_hi: Option<usize>,
    modifiers_lo: Option<usize>,
    modifiers_hi: Option<usize>,
    in_namespace_lo: Option<usize>,
    in_namespace_hi: Option<usize>,
}

/// Layouts for the Reflection owner classes eval can materialize.
struct ReflectionOwnerLayouts {
    class: ReflectionOwnerLayout,
    method: ReflectionOwnerLayout,
    property: ReflectionOwnerLayout,
    class_constant: ReflectionOwnerLayout,
    enum_unit_case: ReflectionOwnerLayout,
    enum_backed_case: ReflectionOwnerLayout,
}

/// Emits eval Reflection owner helpers when any lowered function owns an eval context.
pub(super) fn emit_eval_reflection_owner_helpers(module: &Module, emitter: &mut Emitter) {
    if !module_uses_eval(module) {
        return;
    }
    emitter.blank();
    emitter.comment("--- eval bridge: reflection owner helpers ---");
    label_c_global(module, emitter, "__elephc_eval_reflection_owner_new");
    let Some(layouts) = reflection_owner_layouts(module) else {
        emit_reflection_owner_new_stub(emitter);
        return;
    };
    match module.target.arch {
        Arch::AArch64 => emit_reflection_owner_new_aarch64(emitter, &layouts),
        Arch::X86_64 => emit_reflection_owner_new_x86_64(emitter, &layouts),
    }
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

/// Returns the Reflection owner object layouts from class metadata.
fn reflection_owner_layouts(module: &Module) -> Option<ReflectionOwnerLayouts> {
    Some(ReflectionOwnerLayouts {
        class: reflection_owner_layout(module.class_infos.get("ReflectionClass")?, true)?,
        method: reflection_owner_layout(module.class_infos.get("ReflectionMethod")?, true)?,
        property: reflection_owner_layout(module.class_infos.get("ReflectionProperty")?, true)?,
        class_constant: reflection_owner_layout(
            module.class_infos.get("ReflectionClassConstant")?,
            true,
        )?,
        enum_unit_case: reflection_owner_layout(
            module.class_infos.get("ReflectionEnumUnitCase")?,
            true,
        )?,
        enum_backed_case: reflection_owner_layout(
            module.class_infos.get("ReflectionEnumBackedCase")?,
            true,
        )?,
    })
}

/// Returns one Reflection owner layout from class metadata.
fn reflection_owner_layout(info: &ClassInfo, has_name: bool) -> Option<ReflectionOwnerLayout> {
    let attrs_lo = reflection_property_offset(info, "__attrs")?;
    let name_lo = has_name
        .then(|| reflection_property_offset(info, "__name"))
        .flatten();
    let short_name_lo = reflection_property_offset(info, "__short_name");
    let namespace_name_lo = reflection_property_offset(info, "__namespace_name");
    let interface_names_lo = reflection_property_offset(info, "__interface_names");
    let trait_names_lo = reflection_property_offset(info, "__trait_names");
    let is_final_lo = reflection_property_offset(info, "__is_final");
    let is_abstract_lo = reflection_property_offset(info, "__is_abstract");
    let is_interface_lo = reflection_property_offset(info, "__is_interface");
    let is_trait_lo = reflection_property_offset(info, "__is_trait");
    let is_enum_lo = reflection_property_offset(info, "__is_enum");
    let modifiers_lo = reflection_property_offset(info, "__modifiers");
    let in_namespace_lo = reflection_property_offset(info, "__in_namespace");
    Some(ReflectionOwnerLayout {
        class_id: info.class_id,
        property_count: info.properties.len(),
        name_lo,
        name_hi: name_lo.map(|offset| offset + 8),
        short_name_lo,
        short_name_hi: short_name_lo.map(|offset| offset + 8),
        namespace_name_lo,
        namespace_name_hi: namespace_name_lo.map(|offset| offset + 8),
        interface_names_lo,
        interface_names_hi: interface_names_lo.map(|offset| offset + 8),
        trait_names_lo,
        trait_names_hi: trait_names_lo.map(|offset| offset + 8),
        attrs_lo,
        attrs_hi: attrs_lo + 8,
        is_final_lo,
        is_final_hi: is_final_lo.map(|offset| offset + 8),
        is_abstract_lo,
        is_abstract_hi: is_abstract_lo.map(|offset| offset + 8),
        is_interface_lo,
        is_interface_hi: is_interface_lo.map(|offset| offset + 8),
        is_trait_lo,
        is_trait_hi: is_trait_lo.map(|offset| offset + 8),
        is_enum_lo,
        is_enum_hi: is_enum_lo.map(|offset| offset + 8),
        modifiers_lo,
        modifiers_hi: modifiers_lo.map(|offset| offset + 8),
        in_namespace_lo,
        in_namespace_hi: in_namespace_lo.map(|offset| offset + 8),
    })
}

/// Returns one declared property offset from the synthetic reflection class layout.
fn reflection_property_offset(info: &ClassInfo, property: &str) -> Option<usize> {
    info.property_offsets.get(property).copied()
}

/// Emits a fail-closed helper when Reflection owner metadata is unavailable.
fn emit_reflection_owner_new_stub(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, xzr");                                 // report helper failure when Reflection owner metadata is missing
            emitter.instruction("ret");                                         // return the null pointer to Rust
        }
        Arch::X86_64 => {
            emitter.instruction("xor eax, eax");                                // report helper failure when Reflection owner metadata is missing
            emitter.instruction("ret");                                         // return the null pointer to Rust
        }
    }
}

/// Emits the ARM64 Reflection owner materializer helper body.
fn emit_reflection_owner_new_aarch64(emitter: &mut Emitter, layouts: &ReflectionOwnerLayouts) {
    let fail_label = "__elephc_eval_reflection_owner_new_fail";
    let done_label = "__elephc_eval_reflection_owner_new_done";
    let box_label = "__elephc_eval_reflection_owner_new_box";
    let class_label = "__elephc_eval_reflection_owner_new_class";
    let method_label = "__elephc_eval_reflection_owner_new_method";
    let property_label = "__elephc_eval_reflection_owner_new_property";
    let class_constant_label = "__elephc_eval_reflection_owner_new_class_constant";
    let enum_unit_case_label = "__elephc_eval_reflection_owner_new_enum_unit_case";
    let enum_backed_case_label = "__elephc_eval_reflection_owner_new_enum_backed_case";
    emitter.instruction("sub sp, sp, #128");                                    // reserve helper frame for inputs, object, arrays, scratch, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #112]");                            // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #112");                                   // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the Reflection owner kind
    emitter.instruction("str x1, [sp, #8]");                                    // save the reflected-name pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the reflected-name length
    emitter.instruction("str x3, [sp, #24]");                                   // save the boxed ReflectionAttribute array
    emitter.instruction("str x4, [sp, #80]");                                   // save the boxed ReflectionClass interface-name array
    emitter.instruction("str x5, [sp, #88]");                                   // save the boxed ReflectionClass trait-name array
    emitter.instruction("str x6, [sp, #48]");                                   // save ReflectionClass modifier flags
    emitter.instruction("str x7, [sp, #96]");                                   // save ReflectionClass getModifiers bitmask
    emitter.instruction("cmp x0, #0");                                          // owner kind 0 means ReflectionClass
    emitter.instruction(&format!("b.eq {}", class_label));                      // allocate a ReflectionClass owner
    emitter.instruction("cmp x0, #1");                                          // owner kind 1 means ReflectionMethod
    emitter.instruction(&format!("b.eq {}", method_label));                     // allocate a ReflectionMethod owner
    emitter.instruction("cmp x0, #2");                                          // owner kind 2 means ReflectionProperty
    emitter.instruction(&format!("b.eq {}", property_label));                   // allocate a ReflectionProperty owner
    emitter.instruction("cmp x0, #3");                                          // owner kind 3 means ReflectionClassConstant
    emitter.instruction(&format!("b.eq {}", class_constant_label));             // allocate a ReflectionClassConstant owner
    emitter.instruction("cmp x0, #4");                                          // owner kind 4 means ReflectionEnumUnitCase
    emitter.instruction(&format!("b.eq {}", enum_unit_case_label));             // allocate a ReflectionEnumUnitCase owner
    emitter.instruction("cmp x0, #5");                                          // owner kind 5 means ReflectionEnumBackedCase
    emitter.instruction(&format!("b.eq {}", enum_backed_case_label));           // allocate a ReflectionEnumBackedCase owner
    emitter.instruction(&format!("b {}", fail_label));                          // reject unknown owner kinds
    emit_aarch64_owner_kind_body(
        emitter,
        class_label,
        &layouts.class,
        true,
        fail_label,
        box_label,
    );
    emit_aarch64_owner_kind_body(
        emitter,
        method_label,
        &layouts.method,
        true,
        fail_label,
        box_label,
    );
    emit_aarch64_owner_kind_body(
        emitter,
        property_label,
        &layouts.property,
        true,
        fail_label,
        box_label,
    );
    emit_aarch64_owner_kind_body(
        emitter,
        class_constant_label,
        &layouts.class_constant,
        true,
        fail_label,
        box_label,
    );
    emit_aarch64_owner_kind_body(
        emitter,
        enum_unit_case_label,
        &layouts.enum_unit_case,
        true,
        fail_label,
        box_label,
    );
    emit_aarch64_owner_kind_body(
        emitter,
        enum_backed_case_label,
        &layouts.enum_backed_case,
        true,
        fail_label,
        box_label,
    );
    emitter.label(box_label);
    emitter.instruction("mov x0, #6");                                          // runtime tag 6 = object
    emitter.instruction("ldr x1, [sp, #32]");                                   // move the Reflection owner object pointer into the Mixed payload
    emitter.instruction("mov x2, xzr");                                         // object payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the Reflection owner object for eval
    emitter.instruction(&format!("b {}", done_label));                          // skip the fail-closed return path after boxing
    emitter.label(fail_label);
    emitter.instruction("mov x0, xzr");                                         // return a null pointer so Rust reports runtime failure
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore the Rust caller frame
    emitter.instruction("add sp, sp, #128");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed reflection owner to Rust
}

/// Emits the x86_64 Reflection owner materializer helper body.
fn emit_reflection_owner_new_x86_64(emitter: &mut Emitter, layouts: &ReflectionOwnerLayouts) {
    let fail_label = "__elephc_eval_reflection_owner_new_fail_x";
    let done_label = "__elephc_eval_reflection_owner_new_done_x";
    let box_label = "__elephc_eval_reflection_owner_new_box_x";
    let class_label = "__elephc_eval_reflection_owner_new_class_x";
    let method_label = "__elephc_eval_reflection_owner_new_method_x";
    let property_label = "__elephc_eval_reflection_owner_new_property_x";
    let class_constant_label = "__elephc_eval_reflection_owner_new_class_constant_x";
    let enum_unit_case_label = "__elephc_eval_reflection_owner_new_enum_unit_case_x";
    let enum_backed_case_label = "__elephc_eval_reflection_owner_new_enum_backed_case_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 112");                                        // reserve slots for inputs, object, relation arrays, and name parts
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the Reflection owner kind
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the reflected-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the reflected-name length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the boxed ReflectionAttribute array
    emitter.instruction("mov QWORD PTR [rbp - 88], r8");                        // save the boxed ReflectionClass interface-name array
    emitter.instruction("mov QWORD PTR [rbp - 96], r9");                        // save the boxed ReflectionClass trait-name array
    emitter.instruction("mov rax, QWORD PTR [rbp + 16]");                       // load ReflectionClass modifier flags from the first stack argument
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save ReflectionClass modifier flags
    emitter.instruction("mov rax, QWORD PTR [rbp + 24]");                       // load ReflectionClass getModifiers bitmask from the second stack argument
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // save ReflectionClass getModifiers bitmask
    emitter.instruction("cmp rdi, 0");                                          // owner kind 0 means ReflectionClass
    emitter.instruction(&format!("je {}", class_label));                        // allocate a ReflectionClass owner
    emitter.instruction("cmp rdi, 1");                                          // owner kind 1 means ReflectionMethod
    emitter.instruction(&format!("je {}", method_label));                       // allocate a ReflectionMethod owner
    emitter.instruction("cmp rdi, 2");                                          // owner kind 2 means ReflectionProperty
    emitter.instruction(&format!("je {}", property_label));                     // allocate a ReflectionProperty owner
    emitter.instruction("cmp rdi, 3");                                          // owner kind 3 means ReflectionClassConstant
    emitter.instruction(&format!("je {}", class_constant_label));               // allocate a ReflectionClassConstant owner
    emitter.instruction("cmp rdi, 4");                                          // owner kind 4 means ReflectionEnumUnitCase
    emitter.instruction(&format!("je {}", enum_unit_case_label));               // allocate a ReflectionEnumUnitCase owner
    emitter.instruction("cmp rdi, 5");                                          // owner kind 5 means ReflectionEnumBackedCase
    emitter.instruction(&format!("je {}", enum_backed_case_label));             // allocate a ReflectionEnumBackedCase owner
    emitter.instruction(&format!("jmp {}", fail_label));                        // reject unknown owner kinds
    emit_x86_64_owner_kind_body(
        emitter,
        class_label,
        &layouts.class,
        true,
        fail_label,
        box_label,
    );
    emit_x86_64_owner_kind_body(
        emitter,
        method_label,
        &layouts.method,
        true,
        fail_label,
        box_label,
    );
    emit_x86_64_owner_kind_body(
        emitter,
        property_label,
        &layouts.property,
        true,
        fail_label,
        box_label,
    );
    emit_x86_64_owner_kind_body(
        emitter,
        class_constant_label,
        &layouts.class_constant,
        true,
        fail_label,
        box_label,
    );
    emit_x86_64_owner_kind_body(
        emitter,
        enum_unit_case_label,
        &layouts.enum_unit_case,
        true,
        fail_label,
        box_label,
    );
    emit_x86_64_owner_kind_body(
        emitter,
        enum_backed_case_label,
        &layouts.enum_backed_case,
        true,
        fail_label,
        box_label,
    );
    emitter.label(box_label);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // move the Reflection owner object pointer into the Mixed payload
    emitter.instruction("xor esi, esi");                                        // object payloads do not use a high word
    emitter.instruction("mov eax, 6");                                          // runtime tag 6 = object
    emitter.instruction("call __rt_mixed_from_value");                          // box the Reflection owner object for eval
    emitter.instruction(&format!("jmp {}", done_label));                        // skip the fail-closed return path after boxing
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax");                                        // return a null pointer so Rust reports runtime failure
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed reflection owner to Rust
}

/// Emits one ARM64 owner-kind allocation and slot-population body.
fn emit_aarch64_owner_kind_body(
    emitter: &mut Emitter,
    label: &str,
    layout: &ReflectionOwnerLayout,
    set_name: bool,
    fail_label: &str,
    box_label: &str,
) {
    emitter.label(label);
    emit_alloc_reflection_owner_object_aarch64(emitter, layout);
    emitter.instruction("str x0, [sp, #32]");                                   // save the unboxed Reflection owner object pointer
    if set_name {
        emit_set_owner_name_property_aarch64(emitter, layout);
    }
    emit_set_owner_class_flags_property_aarch64(emitter, layout);
    emit_set_owner_relation_arrays_property_aarch64(emitter, layout, fail_label);
    emit_set_owner_attrs_property_aarch64(emitter, layout, fail_label);
    emitter.instruction(&format!("b {}", box_label));                           // box this populated Reflection owner object
}

/// Emits one x86_64 owner-kind allocation and slot-population body.
fn emit_x86_64_owner_kind_body(
    emitter: &mut Emitter,
    label: &str,
    layout: &ReflectionOwnerLayout,
    set_name: bool,
    fail_label: &str,
    box_label: &str,
) {
    emitter.label(label);
    emit_alloc_reflection_owner_object_x86_64(emitter, layout);
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the unboxed Reflection owner object pointer
    if set_name {
        emit_set_owner_name_property_x86_64(emitter, layout);
    }
    emit_set_owner_class_flags_property_x86_64(emitter, layout);
    emit_set_owner_relation_arrays_property_x86_64(emitter, layout, fail_label);
    emit_set_owner_attrs_property_x86_64(emitter, layout, fail_label);
    emitter.instruction(&format!("jmp {}", box_label));                         // box this populated Reflection owner object
}

/// Allocates a zero-initialized ARM64 Reflection owner object payload.
fn emit_alloc_reflection_owner_object_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let payload_size = 8 + layout.property_count * 16;
    emitter.instruction(&format!("mov x0, #{}", payload_size));                 // request Reflection owner object payload storage
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction("mov x9, #4");                                          // heap kind 4 marks the payload as an object
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the object heap header before the payload
    emitter.instruction(&format!("mov x10, #{}", layout.class_id));             // materialize the Reflection owner class id
    emitter.instruction("str x10, [x0]");                                       // store the class id at object payload offset zero
    for index in 0..layout.property_count {
        let offset = 8 + index * 16;
        abi::emit_store_zero_to_address(emitter, "x0", offset);
        abi::emit_store_zero_to_address(emitter, "x0", offset + 8);
    }
}

/// Allocates a zero-initialized x86_64 Reflection owner object payload.
fn emit_alloc_reflection_owner_object_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let payload_size = 8 + layout.property_count * 16;
    emitter.instruction(&format!("mov rax, {}", payload_size));                 // request Reflection owner object payload storage
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize the x86_64 object heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the object heap header before the payload
    emitter.instruction(&format!("mov r10, {}", layout.class_id));              // materialize the Reflection owner class id
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the class id at object payload offset zero
    for index in 0..layout.property_count {
        let offset = 8 + index * 16;
        abi::emit_store_zero_to_address(emitter, "rax", offset);
        abi::emit_store_zero_to_address(emitter, "rax", offset + 8);
    }
}

/// Stores the incoming ARM64 reflected class name into ReflectionClass.
fn emit_set_owner_name_property_aarch64(emitter: &mut Emitter, layout: &ReflectionOwnerLayout) {
    let Some(name_lo) = layout.name_lo else {
        return;
    };
    let Some(name_hi) = layout.name_hi else {
        return;
    };
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the reflected-name pointer for persistence
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the reflected-name length for persistence
    emitter.instruction("bl __rt_str_persist");                                 // copy the eval-owned name bytes for object ownership
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", name_lo);
    abi::emit_store_to_address(emitter, "x2", "x9", name_hi);
    let (
        Some(short_name_lo),
        Some(short_name_hi),
        Some(namespace_name_lo),
        Some(namespace_name_hi),
        Some(in_namespace_lo),
        Some(in_namespace_hi),
    ) = (
        layout.short_name_lo,
        layout.short_name_hi,
        layout.namespace_name_lo,
        layout.namespace_name_hi,
        layout.in_namespace_lo,
        layout.in_namespace_hi,
    )
    else {
        return;
    };
    let scan_loop_label = "__elephc_eval_reflection_owner_name_scan_loop";
    let found_label = "__elephc_eval_reflection_owner_name_scan_found";
    let no_namespace_label = "__elephc_eval_reflection_owner_name_scan_none";
    let store_parts_label = "__elephc_eval_reflection_owner_name_store_parts";
    emitter.instruction("ldr x3, [sp, #8]");                                    // reload the original reflected-name pointer for splitting
    emitter.instruction("ldr x4, [sp, #16]");                                   // reload the original reflected-name length for splitting
    emitter.instruction("mov x5, x4");                                          // start scanning from one byte past the final name byte
    emitter.instruction(&format!("cbz x5, {}", no_namespace_label));            // empty names have no namespace component
    emitter.label(scan_loop_label);
    emitter.instruction("sub x5, x5, #1");                                      // move the scan cursor to the previous byte
    emitter.instruction("ldrb w6, [x3, x5]");                                   // read one reflected-name byte from the scan cursor
    emitter.instruction("cmp w6, #92");                                         // compare against PHP namespace separator '\\'
    emitter.instruction(&format!("b.eq {}", found_label));                      // split at the final namespace separator
    emitter.instruction(&format!("cbnz x5, {}", scan_loop_label));              // keep scanning until the first byte has been checked
    emitter.label(no_namespace_label);
    emitter.instruction("str x3, [sp, #56]");                                   // short-name pointer is the original name pointer
    emitter.instruction("str x4, [sp, #64]");                                   // short-name length is the full name length
    emitter.instruction("str xzr, [sp, #72]");                                  // namespace length is zero for global names
    emitter.instruction(&format!("b {}", store_parts_label));                   // skip the namespaced split path
    emitter.label(found_label);
    emitter.instruction("add x6, x5, #1");                                      // compute the short-name byte offset after the separator
    emitter.instruction("add x7, x3, x6");                                      // compute the short-name pointer
    emitter.instruction("sub x8, x4, x6");                                      // compute the short-name length
    emitter.instruction("str x7, [sp, #56]");                                   // save the short-name pointer across persistence calls
    emitter.instruction("str x8, [sp, #64]");                                   // save the short-name length across persistence calls
    emitter.instruction("str x5, [sp, #72]");                                   // namespace length is the separator offset
    emitter.label(store_parts_label);
    emitter.instruction("ldr x1, [sp, #8]");                                    // use the original name pointer for namespace persistence
    emitter.instruction("ldr x2, [sp, #72]");                                   // reload the namespace byte length
    emitter.instruction("bl __rt_str_persist");                                 // copy the namespace bytes for ReflectionClass storage
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", namespace_name_lo);
    abi::emit_store_to_address(emitter, "x2", "x9", namespace_name_hi);
    emitter.instruction("cmp x2, #0");                                          // detect whether a namespace component was present
    emitter.instruction("cset x10, ne");                                        // materialize ReflectionClass::inNamespace()
    abi::emit_store_to_address(emitter, "x10", "x9", in_namespace_lo);
    abi::emit_store_zero_to_address(emitter, "x9", in_namespace_hi);
    emitter.instruction("ldr x1, [sp, #56]");                                   // reload the short-name pointer
    emitter.instruction("ldr x2, [sp, #64]");                                   // reload the short-name byte length
    emitter.instruction("bl __rt_str_persist");                                 // copy the short-name bytes for ReflectionClass storage
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", short_name_lo);
    abi::emit_store_to_address(emitter, "x2", "x9", short_name_hi);
}

/// Stores the incoming x86_64 reflected class name into ReflectionClass.
fn emit_set_owner_name_property_x86_64(emitter: &mut Emitter, layout: &ReflectionOwnerLayout) {
    let Some(name_lo) = layout.name_lo else {
        return;
    };
    let Some(name_hi) = layout.name_hi else {
        return;
    };
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the reflected-name pointer for persistence
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the reflected-name length for persistence
    emitter.instruction("call __rt_str_persist");                               // copy the eval-owned name bytes for object ownership
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "rax", "r10", name_lo);
    abi::emit_store_to_address(emitter, "rdx", "r10", name_hi);
    let (
        Some(short_name_lo),
        Some(short_name_hi),
        Some(namespace_name_lo),
        Some(namespace_name_hi),
        Some(in_namespace_lo),
        Some(in_namespace_hi),
    ) = (
        layout.short_name_lo,
        layout.short_name_hi,
        layout.namespace_name_lo,
        layout.namespace_name_hi,
        layout.in_namespace_lo,
        layout.in_namespace_hi,
    )
    else {
        return;
    };
    let scan_loop_label = "__elephc_eval_reflection_owner_name_scan_loop_x";
    let found_label = "__elephc_eval_reflection_owner_name_scan_found_x";
    let no_namespace_label = "__elephc_eval_reflection_owner_name_scan_none_x";
    let store_parts_label = "__elephc_eval_reflection_owner_name_store_parts_x";
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the original reflected-name pointer for splitting
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the original reflected-name length for splitting
    emitter.instruction("mov r11, r9");                                         // start scanning from one byte past the final name byte
    emitter.instruction("test r11, r11");                                       // check whether the reflected name is empty
    emitter.instruction(&format!("jz {}", no_namespace_label));                 // empty names have no namespace component
    emitter.label(scan_loop_label);
    emitter.instruction("sub r11, 1");                                          // move the scan cursor to the previous byte
    emitter.instruction("movzx eax, BYTE PTR [r8 + r11]");                      // read one reflected-name byte from the scan cursor
    emitter.instruction("cmp eax, 92");                                         // compare against PHP namespace separator '\\'
    emitter.instruction(&format!("je {}", found_label));                        // split at the final namespace separator
    emitter.instruction("test r11, r11");                                       // check whether the first byte has been examined
    emitter.instruction(&format!("jnz {}", scan_loop_label));                   // keep scanning until the first byte has been checked
    emitter.label(no_namespace_label);
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // short-name pointer is the original name pointer
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // short-name length is the full name length
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // namespace length is zero for global names
    emitter.instruction(&format!("jmp {}", store_parts_label));                 // skip the namespaced split path
    emitter.label(found_label);
    emitter.instruction("lea rax, [r11 + 1]");                                  // compute the short-name byte offset after the separator
    emitter.instruction("lea r10, [r8 + rax]");                                 // compute the short-name pointer
    emitter.instruction("mov rcx, r9");                                         // copy the full name length before subtracting the prefix
    emitter.instruction("sub rcx, rax");                                        // compute the short-name length
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // save the short-name pointer across persistence calls
    emitter.instruction("mov QWORD PTR [rbp - 72], rcx");                       // save the short-name length across persistence calls
    emitter.instruction("mov QWORD PTR [rbp - 80], r11");                       // namespace length is the separator offset
    emitter.label(store_parts_label);
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // use the original name pointer for namespace persistence
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // reload the namespace byte length
    emitter.instruction("call __rt_str_persist");                               // copy the namespace bytes for ReflectionClass storage
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "rax", "r10", namespace_name_lo);
    abi::emit_store_to_address(emitter, "rdx", "r10", namespace_name_hi);
    emitter.instruction("test rdx, rdx");                                       // detect whether a namespace component was present
    emitter.instruction("setne al");                                            // materialize ReflectionClass::inNamespace()
    emitter.instruction("movzx eax, al");                                       // widen the namespace boolean to a full word
    abi::emit_store_to_address(emitter, "rax", "r10", in_namespace_lo);
    abi::emit_store_zero_to_address(emitter, "r10", in_namespace_hi);
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the short-name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // reload the short-name byte length
    emitter.instruction("call __rt_str_persist");                               // copy the short-name bytes for ReflectionClass storage
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "rax", "r10", short_name_lo);
    abi::emit_store_to_address(emitter, "rdx", "r10", short_name_hi);
}

/// Stores incoming ARM64 ReflectionClass boolean modifier flags.
fn emit_set_owner_class_flags_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let Some(is_final_lo) = layout.is_final_lo else {
        return;
    };
    let Some(is_final_hi) = layout.is_final_hi else {
        return;
    };
    let Some(is_abstract_lo) = layout.is_abstract_lo else {
        return;
    };
    let Some(is_abstract_hi) = layout.is_abstract_hi else {
        return;
    };
    let Some(is_interface_lo) = layout.is_interface_lo else {
        return;
    };
    let Some(is_interface_hi) = layout.is_interface_hi else {
        return;
    };
    let Some(is_trait_lo) = layout.is_trait_lo else {
        return;
    };
    let Some(is_trait_hi) = layout.is_trait_hi else {
        return;
    };
    let Some(is_enum_lo) = layout.is_enum_lo else {
        return;
    };
    let Some(is_enum_hi) = layout.is_enum_hi else {
        return;
    };
    let Some(modifiers_lo) = layout.modifiers_lo else {
        return;
    };
    let Some(modifiers_hi) = layout.modifiers_hi else {
        return;
    };
    emitter.instruction("ldr x11, [sp, #48]");                                  // reload ReflectionClass modifier flags
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    emitter.instruction("and x10, x11, #1");                                    // extract the final-class flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_final_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_final_hi);
    emitter.instruction("lsr x10, x11, #1");                                    // move the abstract-class bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the abstract-class flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_abstract_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_abstract_hi);
    emitter.instruction("lsr x10, x11, #2");                                    // move the interface bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the interface flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_interface_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_interface_hi);
    emitter.instruction("lsr x10, x11, #3");                                    // move the trait bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the trait flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_trait_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_trait_hi);
    emitter.instruction("lsr x10, x11, #4");                                    // move the enum bit into position
    emitter.instruction("and x10, x10, #1");                                    // extract the enum flag as a boolean
    abi::emit_store_to_address(emitter, "x10", "x9", is_enum_lo);
    abi::emit_store_zero_to_address(emitter, "x9", is_enum_hi);
    emitter.instruction("ldr x10, [sp, #96]");                                  // reload PHP ReflectionClass::getModifiers() bitmask
    abi::emit_store_to_address(emitter, "x10", "x9", modifiers_lo);
    abi::emit_store_zero_to_address(emitter, "x9", modifiers_hi);
}

/// Stores incoming x86_64 ReflectionClass boolean modifier flags.
fn emit_set_owner_class_flags_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
) {
    let Some(is_final_lo) = layout.is_final_lo else {
        return;
    };
    let Some(is_final_hi) = layout.is_final_hi else {
        return;
    };
    let Some(is_abstract_lo) = layout.is_abstract_lo else {
        return;
    };
    let Some(is_abstract_hi) = layout.is_abstract_hi else {
        return;
    };
    let Some(is_interface_lo) = layout.is_interface_lo else {
        return;
    };
    let Some(is_interface_hi) = layout.is_interface_hi else {
        return;
    };
    let Some(is_trait_lo) = layout.is_trait_lo else {
        return;
    };
    let Some(is_trait_hi) = layout.is_trait_hi else {
        return;
    };
    let Some(is_enum_lo) = layout.is_enum_lo else {
        return;
    };
    let Some(is_enum_hi) = layout.is_enum_hi else {
        return;
    };
    let Some(modifiers_lo) = layout.modifiers_lo else {
        return;
    };
    let Some(modifiers_hi) = layout.modifiers_hi else {
        return;
    };
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload ReflectionClass modifier flags
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the final bit
    emitter.instruction("and rax, 1");                                          // extract the final-class flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_final_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_final_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the abstract bit
    emitter.instruction("shr rax, 1");                                          // move the abstract-class bit into position
    emitter.instruction("and rax, 1");                                          // extract the abstract-class flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_abstract_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_abstract_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the interface bit
    emitter.instruction("shr rax, 2");                                          // move the interface bit into position
    emitter.instruction("and rax, 1");                                          // extract the interface flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_interface_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_interface_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the trait bit
    emitter.instruction("shr rax, 3");                                          // move the trait bit into position
    emitter.instruction("and rax, 1");                                          // extract the trait flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_trait_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_trait_hi);
    emitter.instruction("mov rax, r11");                                        // copy flags before extracting the enum bit
    emitter.instruction("shr rax, 4");                                          // move the enum bit into position
    emitter.instruction("and rax, 1");                                          // extract the enum flag as a boolean
    abi::emit_store_to_address(emitter, "rax", "r10", is_enum_lo);
    abi::emit_store_zero_to_address(emitter, "r10", is_enum_hi);
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // reload PHP ReflectionClass::getModifiers() bitmask
    abi::emit_store_to_address(emitter, "rax", "r10", modifiers_lo);
    abi::emit_store_zero_to_address(emitter, "r10", modifiers_hi);
}

/// Stores incoming ARM64 ReflectionClass interface and trait name arrays.
fn emit_set_owner_relation_arrays_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (
        Some(interface_names_lo),
        Some(interface_names_hi),
        Some(trait_names_lo),
        Some(trait_names_hi),
    ) = (
        layout.interface_names_lo,
        layout.interface_names_hi,
        layout.trait_names_lo,
        layout.trait_names_hi,
    )
    else {
        return;
    };
    emit_set_owner_relation_array_slot_aarch64(
        emitter,
        80,
        interface_names_lo,
        interface_names_hi,
        fail_label,
    );
    emit_set_owner_relation_array_slot_aarch64(
        emitter,
        88,
        trait_names_lo,
        trait_names_hi,
        fail_label,
    );
}

/// Stores one retained ARM64 boxed relation-name array into a ReflectionClass slot.
fn emit_set_owner_relation_array_slot_aarch64(
    emitter: &mut Emitter,
    boxed_slot: usize,
    low_offset: usize,
    high_offset: usize,
    fail_label: &str,
) {
    emitter.instruction(&format!("ldr x0, [sp, #{}]", boxed_slot));             // reload the boxed ReflectionClass relation-name array
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // reject malformed null relation-name arrays
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the relation-name array tag and payload pointer
    emitter.instruction("cmp x0, #4");                                          // runtime tag 4 means indexed array
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject non-array relation-name metadata
    emitter.instruction("str x1, [sp, #40]");                                   // save the unboxed relation-name array across incref
    emitter.instruction("mov x0, x1");                                          // move the array payload into the incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain the relation-name array for ReflectionClass storage
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the retained relation-name array payload
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", low_offset);
    abi::emit_load_int_immediate(emitter, "x10", 4);
    abi::emit_store_to_address(emitter, "x10", "x9", high_offset);
}

/// Stores incoming x86_64 ReflectionClass interface and trait name arrays.
fn emit_set_owner_relation_arrays_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    let (
        Some(interface_names_lo),
        Some(interface_names_hi),
        Some(trait_names_lo),
        Some(trait_names_hi),
    ) = (
        layout.interface_names_lo,
        layout.interface_names_hi,
        layout.trait_names_lo,
        layout.trait_names_hi,
    )
    else {
        return;
    };
    emit_set_owner_relation_array_slot_x86_64(
        emitter,
        -88,
        interface_names_lo,
        interface_names_hi,
        fail_label,
    );
    emit_set_owner_relation_array_slot_x86_64(
        emitter,
        -96,
        trait_names_lo,
        trait_names_hi,
        fail_label,
    );
}

/// Stores one retained x86_64 boxed relation-name array into a ReflectionClass slot.
fn emit_set_owner_relation_array_slot_x86_64(
    emitter: &mut Emitter,
    boxed_slot: isize,
    low_offset: usize,
    high_offset: usize,
    fail_label: &str,
) {
    let boxed_slot = if boxed_slot < 0 {
        format!("- {}", -boxed_slot)
    } else {
        format!("+ {}", boxed_slot)
    };
    emitter.instruction(&format!("mov rax, QWORD PTR [rbp {}]", boxed_slot));   // reload the boxed ReflectionClass relation-name array
    emitter.instruction("test rax, rax");                                       // check whether the boxed relation-name array is null
    emitter.instruction(&format!("jz {}", fail_label));                         // reject malformed null relation-name arrays
    emitter.instruction("call __rt_mixed_unbox");                               // expose the relation-name array tag and payload pointer
    emitter.instruction("cmp rax, 4");                                          // runtime tag 4 means indexed array
    emitter.instruction(&format!("jne {}", fail_label));                        // reject non-array relation-name metadata
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // save the unboxed relation-name array across incref
    emitter.instruction("mov rax, rdi");                                        // move the array payload into the incref argument register
    emitter.instruction("call __rt_incref");                                    // retain the relation-name array for ReflectionClass storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the retained relation-name array payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "rdi", "r10", low_offset);
    abi::emit_load_int_immediate(emitter, "r11", 4);
    abi::emit_store_to_address(emitter, "r11", "r10", high_offset);
}

/// Stores a retained ARM64 attribute-array payload into the owner private slot.
fn emit_set_owner_attrs_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the boxed ReflectionAttribute array
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // reject malformed null attribute arrays
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the attribute array tag and payload pointer
    emitter.instruction("cmp x0, #4");                                          // runtime tag 4 means indexed array
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject non-array attribute metadata
    emitter.instruction("str x1, [sp, #40]");                                   // save the unboxed attribute array across incref
    emitter.instruction("mov x0, x1");                                          // move the array payload into the incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain the attribute array for Reflection owner storage
    emitter.instruction("ldr x1, [sp, #40]");                                   // reload the retained attribute array payload
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", layout.attrs_lo);
    abi::emit_load_int_immediate(emitter, "x10", 4);
    abi::emit_store_to_address(emitter, "x10", "x9", layout.attrs_hi);
}

/// Stores a retained x86_64 attribute-array payload into the owner private slot.
fn emit_set_owner_attrs_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionOwnerLayout,
    fail_label: &str,
) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the boxed ReflectionAttribute array
    emitter.instruction("test rax, rax");                                       // check whether the boxed attribute array is null
    emitter.instruction(&format!("jz {}", fail_label));                         // reject malformed null attribute arrays
    emitter.instruction("call __rt_mixed_unbox");                               // expose the attribute array tag and payload pointer
    emitter.instruction("cmp rax, 4");                                          // runtime tag 4 means indexed array
    emitter.instruction(&format!("jne {}", fail_label));                        // reject non-array attribute metadata
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // save the unboxed attribute array across incref
    emitter.instruction("mov rax, rdi");                                        // move the array payload into the incref argument register
    emitter.instruction("call __rt_incref");                                    // retain the attribute array for Reflection owner storage
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the retained attribute array payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the Reflection owner object pointer
    abi::emit_store_to_address(emitter, "rdi", "r10", layout.attrs_lo);
    abi::emit_load_int_immediate(emitter, "r11", 4);
    abi::emit_store_to_address(emitter, "r11", "r10", layout.attrs_hi);
}

/// Emits a C-visible global label with target-specific symbol mangling.
fn label_c_global(module: &Module, emitter: &mut Emitter, name: &str) {
    let symbol = module.target.extern_symbol(name);
    emitter.label_global(&symbol);
}
