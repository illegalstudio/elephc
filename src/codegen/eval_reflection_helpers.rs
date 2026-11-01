//! Purpose:
//! Emits user-assembly helpers that let libelephc-magician materialize selected
//! synthetic reflection objects using the current module's private layouts.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` when an EIR module uses eval.
//!
//! Key details:
//! - `ReflectionAttribute` stores private implementation slots that eval cannot
//!   populate through public property writes, plus its PHP-visible `name` mirror.
//! - Eval-declared attributes do not have compile-time factories, so the helper
//!   writes factory id 0; `ReflectionAttribute::newInstance()` then returns null.
//! - Target and repetition metadata are passed by the Rust eval bridge and
//!   stored in the same private layout used by generated reflection objects.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::ir::Module;
use crate::types::ClassInfo;


/// Fixed object slot layout for the synthetic `ReflectionAttribute` class.
struct ReflectionAttributeLayout {
    class_id: u64,
    property_count: usize,
    public_name_lo: usize,
    public_name_hi: usize,
    name_lo: usize,
    name_hi: usize,
    args_lo: usize,
    args_hi: usize,
    factory_lo: usize,
    factory_hi: usize,
    target_lo: usize,
    target_hi: usize,
    repeated_lo: usize,
    repeated_hi: usize,
    string_lo: usize,
    string_hi: usize,
}

/// Emits eval reflection helpers when the runtime bridge can materialize metadata owners.
pub(super) fn emit_eval_reflection_helpers(module: &Module, emitter: &mut Emitter) {
    if !module.required_runtime_features.eval_bridge {
        return;
    }
    emitter.blank();
    emitter.comment("--- eval bridge: reflection helpers ---");
    label_c_global(module, emitter, "__elephc_eval_reflection_attribute_new");
    let Some(layout) = reflection_attribute_layout(module) else {
        emit_reflection_attribute_new_stub(emitter);
        return;
    };
    match module.target.arch {
        Arch::AArch64 => emit_reflection_attribute_new_aarch64(emitter, &layout),
        Arch::X86_64 => emit_reflection_attribute_new_x86_64(emitter, &layout),
    }
}

/// Returns the synthetic `ReflectionAttribute` object layout from class metadata.
fn reflection_attribute_layout(module: &Module) -> Option<ReflectionAttributeLayout> {
    let info = module.class_infos.get("ReflectionAttribute")?;
    let public_name_lo = reflection_property_offset(info, "name")?;
    let name_lo = reflection_property_offset(info, "__name")?;
    let args_lo = reflection_property_offset(info, "__args")?;
    let factory_lo = reflection_property_offset(info, "__factory")?;
    let target_lo = reflection_property_offset(info, "__target")?;
    let repeated_lo = reflection_property_offset(info, "__is_repeated")?;
    let string_lo = reflection_property_offset(info, "__string")?;
    Some(ReflectionAttributeLayout {
        class_id: info.class_id,
        property_count: info.properties.len(),
        public_name_lo,
        public_name_hi: public_name_lo + 8,
        name_lo,
        name_hi: name_lo + 8,
        args_lo,
        args_hi: args_lo + 8,
        factory_lo,
        factory_hi: factory_lo + 8,
        target_lo,
        target_hi: target_lo + 8,
        repeated_lo,
        repeated_hi: repeated_lo + 8,
        string_lo,
        string_hi: string_lo + 8,
    })
}

/// Returns one declared property offset from the synthetic reflection class layout.
fn reflection_property_offset(info: &ClassInfo, property: &str) -> Option<usize> {
    info.property_offsets.get(property).copied()
}

/// Emits a fail-closed helper when reflection metadata is unavailable.
fn emit_reflection_attribute_new_stub(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, xzr");                                 // report helper failure when ReflectionAttribute metadata is missing
            emitter.instruction("ret");                                         // return the null pointer to Rust
        }
        Arch::X86_64 => {
            emitter.instruction("xor eax, eax");                                // report helper failure when ReflectionAttribute metadata is missing
            emitter.instruction("ret");                                         // return the null pointer to Rust
        }
    }
}

/// Emits the ARM64 `ReflectionAttribute` materializer helper body.
fn emit_reflection_attribute_new_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionAttributeLayout,
) {
    let fail_label = "__elephc_eval_reflection_attribute_new_fail";
    let done_label = "__elephc_eval_reflection_attribute_new_done";
    emitter.instruction("sub sp, sp, #96");                                     // reserve helper frame for inputs, object, rendering, and fp/lr
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the Rust caller frame across runtime calls
    emitter.instruction("add x29, sp, #80");                                    // establish a stable helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the attribute-name pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the attribute-name length
    emitter.instruction("str x2, [sp, #16]");                                   // save the boxed eval argument array
    emitter.instruction("str x3, [sp, #40]");                                   // save the PHP attribute target bitmask
    emitter.instruction("str x4, [sp, #48]");                                   // save whether this attribute is repeated
    emitter.instruction("str x5, [sp, #64]");                                   // save the rendered __toString pointer
    emitter.instruction("str x6, [sp, #72]");                                   // save the rendered __toString length
    emit_alloc_reflection_attribute_object_aarch64(emitter, layout);
    emitter.instruction("str x0, [sp, #24]");                                   // save the unboxed ReflectionAttribute object pointer
    emit_set_name_property_aarch64(emitter, layout);
    emit_set_args_property_aarch64(emitter, layout, fail_label);
    emit_set_factory_property_aarch64(emitter, layout);
    emit_set_target_property_aarch64(emitter, layout);
    emit_set_repeated_property_aarch64(emitter, layout);
    emit_set_string_property_aarch64(emitter, layout);
    emitter.instruction("mov x0, #6");                                          // runtime tag 6 = object
    emitter.instruction("ldr x1, [sp, #24]");                                   // move the ReflectionAttribute object pointer into the Mixed payload
    emitter.instruction("mov x2, xzr");                                         // object payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the ReflectionAttribute object for eval
    // Boxing an object RETAINS it, so the box owns one reference and the helper still owns the
    // one the allocation gave it. Handing that one back is what lets the box's release free the
    // object: it leaked every materialized attribute (with its name, arguments and rendering),
    // 24 blocks per `new ReflectionFunction($closure)` that carried parameter attributes.
    abi::emit_push_reg(emitter, "x0");                                          // keep the boxed attribute across the release
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the object pointer, past the 16-byte save slot
    abi::emit_call_label(emitter, "__rt_decref_object");                        // drop the allocation reference now held only by the box
    abi::emit_pop_reg(emitter, "x0");                                           // restore the boxed attribute as the helper result
    emitter.instruction(&format!("b {}", done_label));                          // skip the fail-closed return path after boxing
    emitter.label(fail_label);
    emitter.instruction("mov x0, xzr");                                         // return a null pointer so Rust reports runtime failure
    emitter.label(done_label);
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the Rust caller frame
    emitter.instruction("add sp, sp, #96");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the boxed reflection attribute to Rust
}

/// Emits the x86_64 `ReflectionAttribute` materializer helper body.
fn emit_reflection_attribute_new_x86_64(emitter: &mut Emitter, layout: &ReflectionAttributeLayout) {
    let fail_label = "__elephc_eval_reflection_attribute_new_fail_x";
    let done_label = "__elephc_eval_reflection_attribute_new_done_x";
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable helper frame pointer
    emitter.instruction("sub rsp, 80");                                         // reserve slots for inputs, object, unboxed args, and the rendering
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the attribute-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the attribute-name length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the boxed eval argument array
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the PHP attribute target bitmask
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save whether this attribute is repeated
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // save the rendered __toString pointer
    emitter.instruction("mov rax, QWORD PTR [rbp + 16]");                       // the seventh integer argument arrives on the stack
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the rendered __toString length
    emit_alloc_reflection_attribute_object_x86_64(emitter, layout);
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the unboxed ReflectionAttribute object pointer
    emit_set_name_property_x86_64(emitter, layout);
    emit_set_args_property_x86_64(emitter, layout, fail_label);
    emit_set_factory_property_x86_64(emitter, layout);
    emit_set_target_property_x86_64(emitter, layout);
    emit_set_repeated_property_x86_64(emitter, layout);
    emit_set_string_property_x86_64(emitter, layout);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // move the ReflectionAttribute object pointer into the Mixed payload
    emitter.instruction("xor esi, esi");                                        // object payloads do not use a high word
    emitter.instruction("mov eax, 6");                                          // runtime tag 6 = object
    emitter.instruction("call __rt_mixed_from_value");                          // box the ReflectionAttribute object for eval
    // See the ARM64 helper: boxing retained the object, so give the allocation reference back.
    abi::emit_push_reg(emitter, "rax");                                         // keep the boxed attribute across the release
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the object pointer for the release
    abi::emit_call_label(emitter, "__rt_decref_object");                        // drop the allocation reference now held only by the box
    abi::emit_pop_reg(emitter, "rax");                                          // restore the boxed attribute as the helper result
    emitter.instruction(&format!("jmp {}", done_label));                        // skip the fail-closed return path after boxing
    emitter.label(fail_label);
    emitter.instruction("xor eax, eax");                                        // return a null pointer so Rust reports runtime failure
    emitter.label(done_label);
    emitter.instruction("mov rsp, rbp");                                        // discard helper spill slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the boxed reflection attribute to Rust
}

/// Allocates a zero-initialized ARM64 `ReflectionAttribute` object payload.
fn emit_alloc_reflection_attribute_object_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionAttributeLayout,
) {
    let payload_size = 8 + layout.property_count * 16;
    emitter.instruction(&format!("mov x0, #{}", payload_size));                 // request ReflectionAttribute object payload storage
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction("mov x9, #4");                                          // heap kind 4 marks ReflectionAttribute as an object
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the object heap header before the payload
    emitter.instruction("bl __rt_object_handle_acquire");                       // bind the new object to its PHP object handle
    emitter.instruction(&format!("mov x10, #{}", layout.class_id));             // materialize the ReflectionAttribute class id
    emitter.instruction("str x10, [x0]");                                       // store the class id at object payload offset zero
    for index in 0..layout.property_count {
        let offset = 8 + index * 16;
        abi::emit_store_zero_to_address(emitter, "x0", offset);
        abi::emit_store_zero_to_address(emitter, "x0", offset + 8);
    }
}

/// Allocates a zero-initialized x86_64 `ReflectionAttribute` object payload.
fn emit_alloc_reflection_attribute_object_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionAttributeLayout,
) {
    let payload_size = 8 + layout.property_count * 16;
    emitter.instruction(&format!("mov rax, {}", payload_size));                 // request ReflectionAttribute object payload storage
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    emitter.instruction(&format!(
        "mov r10, 0x{:x}",
        crate::codegen_support::sentinels::x86_64_heap_kind_word(4)
    ));                                                                         // materialize the x86_64 object heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the object heap header before the payload
    emitter.instruction("call __rt_object_handle_acquire");                     // bind the new object to its PHP object handle
    emitter.instruction(&format!("mov r10, {}", layout.class_id));              // materialize the ReflectionAttribute class id
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the class id at object payload offset zero
    for index in 0..layout.property_count {
        let offset = 8 + index * 16;
        abi::emit_store_zero_to_address(emitter, "rax", offset);
        abi::emit_store_zero_to_address(emitter, "rax", offset + 8);
    }
}

/// Stores the incoming ARM64 attribute name into the private slot and public mirror.
fn emit_set_name_property_aarch64(emitter: &mut Emitter, layout: &ReflectionAttributeLayout) {
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the attribute-name pointer for persistence
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the attribute-name length for persistence
    emitter.instruction("bl __rt_str_persist");                                 // copy the eval-owned name bytes for object ownership
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the ReflectionAttribute object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", layout.name_lo);
    abi::emit_store_to_address(emitter, "x2", "x9", layout.name_hi);
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the original name pointer for the public mirror copy
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the original name length for the public mirror copy
    emitter.instruction("bl __rt_str_persist");                                 // give the mutable public mirror independent string storage
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the ReflectionAttribute object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", layout.public_name_lo);
    abi::emit_store_to_address(emitter, "x2", "x9", layout.public_name_hi);
}

/// Stores the incoming x86_64 attribute name into the private slot and public mirror.
fn emit_set_name_property_x86_64(emitter: &mut Emitter, layout: &ReflectionAttributeLayout) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the attribute-name pointer for persistence
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the attribute-name length for persistence
    emitter.instruction("call __rt_str_persist");                               // copy the eval-owned name bytes for object ownership
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the ReflectionAttribute object pointer
    abi::emit_store_to_address(emitter, "rax", "r10", layout.name_lo);
    abi::emit_store_to_address(emitter, "rdx", "r10", layout.name_hi);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the original name pointer for the public mirror copy
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the original name length for the public mirror copy
    emitter.instruction("call __rt_str_persist");                               // give the mutable public mirror independent string storage
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the ReflectionAttribute object pointer
    abi::emit_store_to_address(emitter, "rax", "r10", layout.public_name_lo);
    abi::emit_store_to_address(emitter, "rdx", "r10", layout.public_name_hi);
}

/// Stores the incoming ARM64 `__toString` rendering into the object private slot.
///
/// The interpreter renders it from its own attribute metadata (see
/// `elephc_magician::eval_ir::attributes::render_reflection_attribute_string`) because only it
/// knows the attribute a runtime-declared class carries.
fn emit_set_string_property_aarch64(emitter: &mut Emitter, layout: &ReflectionAttributeLayout) {
    emitter.instruction("ldr x1, [sp, #64]");                                   // reload the rendered __toString pointer for persistence
    emitter.instruction("ldr x2, [sp, #72]");                                   // reload the rendered __toString length for persistence
    emitter.instruction("bl __rt_str_persist");                                 // copy the eval-owned rendering for object ownership
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the ReflectionAttribute object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", layout.string_lo);
    abi::emit_store_to_address(emitter, "x2", "x9", layout.string_hi);
}

/// Stores the incoming x86_64 `__toString` rendering into the object private slot.
fn emit_set_string_property_x86_64(emitter: &mut Emitter, layout: &ReflectionAttributeLayout) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the rendered __toString pointer for persistence
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // reload the rendered __toString length for persistence
    emitter.instruction("call __rt_str_persist");                               // copy the eval-owned rendering for object ownership
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the ReflectionAttribute object pointer
    abi::emit_store_to_address(emitter, "rax", "r10", layout.string_lo);
    abi::emit_store_to_address(emitter, "rdx", "r10", layout.string_hi);
}

/// Stores a retained ARM64 argument-array payload into the object private slot.
fn emit_set_args_property_aarch64(
    emitter: &mut Emitter,
    layout: &ReflectionAttributeLayout,
    fail_label: &str,
) {
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the boxed eval attribute-argument array
    emitter.instruction(&format!("cbz x0, {}", fail_label));                    // reject malformed null argument arrays
    emitter.instruction("bl __rt_mixed_unbox");                                 // expose the argument array tag and payload pointer
    emit_normalize_attribute_args_aarch64(emitter, fail_label);
    emitter.instruction("str x0, [sp, #32]");                                   // save the owned associative argument array
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload the owned associative argument array payload
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the ReflectionAttribute object pointer
    abi::emit_store_to_address(emitter, "x1", "x9", layout.args_lo);
    emitter.instruction("mov x10, #5");                                         // record associative storage for the argument-map property
    abi::emit_store_to_address(emitter, "x10", "x9", layout.args_hi);
}

/// Stores a retained x86_64 argument-array payload into the object private slot.
fn emit_set_args_property_x86_64(
    emitter: &mut Emitter,
    layout: &ReflectionAttributeLayout,
    fail_label: &str,
) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the boxed eval attribute-argument array
    emitter.instruction("test rax, rax");                                       // check whether the boxed argument array is null
    emitter.instruction(&format!("jz {}", fail_label));                         // reject malformed null argument arrays
    emitter.instruction("call __rt_mixed_unbox");                               // expose the argument array tag and payload pointer
    emit_normalize_attribute_args_x86_64(emitter, fail_label);
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the owned associative argument array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload the owned associative argument array payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the ReflectionAttribute object pointer
    abi::emit_store_to_address(emitter, "rdi", "r10", layout.args_lo);
    emitter.instruction("mov r11, 5");                                          // record associative storage for the argument-map property
    abi::emit_store_to_address(emitter, "r11", "r10", layout.args_hi);
}

/// Returns an owned associative ARM64 argument map from an unboxed eval array tag/payload.
fn emit_normalize_attribute_args_aarch64(emitter: &mut Emitter, fail_label: &str) {
    let indexed = "__elephc_eval_reflection_attribute_args_indexed";
    let done = "__elephc_eval_reflection_attribute_args_done";
    emitter.instruction("cmp x0, #4");                                          // does eval expose positional indexed arguments?
    emitter.instruction(&format!("b.eq {}", indexed));                          // convert positional arguments to integer-keyed hash storage
    emitter.instruction("cmp x0, #5");                                          // otherwise require an associative argument map
    emitter.instruction(&format!("b.ne {}", fail_label));                       // reject non-array argument metadata
    emitter.instruction("mov x0, x1");                                          // retain the borrowed associative argument hash
    emitter.instruction("bl __rt_incref");                                      // transfer one hash owner into ReflectionAttribute
    emitter.instruction(&format!("b {}", done));                                // skip indexed-array conversion
    emitter.label(indexed);
    emitter.instruction("mov x0, x1");                                          // pass the borrowed indexed argument array for conversion
    emitter.instruction("bl __rt_array_to_hash");                               // build an owned integer-keyed associative argument map
    emitter.label(done);
}

/// Returns an owned associative x86_64 argument map from an unboxed eval array tag/payload.
fn emit_normalize_attribute_args_x86_64(emitter: &mut Emitter, fail_label: &str) {
    let indexed = "__elephc_eval_reflection_attribute_args_indexed_x";
    let done = "__elephc_eval_reflection_attribute_args_done_x";
    emitter.instruction("cmp rax, 4");                                          // does eval expose positional indexed arguments?
    emitter.instruction(&format!("je {}", indexed));                            // convert positional arguments to integer-keyed hash storage
    emitter.instruction("cmp rax, 5");                                          // otherwise require an associative argument map
    emitter.instruction(&format!("jne {}", fail_label));                        // reject non-array argument metadata
    emitter.instruction("mov rax, rdi");                                        // retain the borrowed associative argument hash
    emitter.instruction("call __rt_incref");                                    // transfer one hash owner into ReflectionAttribute
    emitter.instruction(&format!("jmp {}", done));                              // skip indexed-array conversion
    emitter.label(indexed);
    emitter.instruction("call __rt_array_to_hash");                             // build an owned integer-keyed associative argument map
    emitter.label(done);
}

/// Stores factory id 0 on the ARM64 reflection object.
fn emit_set_factory_property_aarch64(emitter: &mut Emitter, layout: &ReflectionAttributeLayout) {
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the ReflectionAttribute object pointer
    abi::emit_store_zero_to_address(emitter, "x9", layout.factory_lo);
    abi::emit_store_zero_to_address(emitter, "x9", layout.factory_hi);
}

/// Stores factory id 0 on the x86_64 reflection object.
fn emit_set_factory_property_x86_64(emitter: &mut Emitter, layout: &ReflectionAttributeLayout) {
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the ReflectionAttribute object pointer
    abi::emit_store_zero_to_address(emitter, "r10", layout.factory_lo);
    abi::emit_store_zero_to_address(emitter, "r10", layout.factory_hi);
}

/// Stores the eval-provided ARM64 target bitmask on the reflection object.
fn emit_set_target_property_aarch64(emitter: &mut Emitter, layout: &ReflectionAttributeLayout) {
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the ReflectionAttribute object pointer
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload the PHP attribute target bitmask
    abi::emit_store_to_address(emitter, "x10", "x9", layout.target_lo);
    abi::emit_store_zero_to_address(emitter, "x9", layout.target_hi);
}

/// Stores the eval-provided x86_64 target bitmask on the reflection object.
fn emit_set_target_property_x86_64(emitter: &mut Emitter, layout: &ReflectionAttributeLayout) {
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the ReflectionAttribute object pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the PHP attribute target bitmask
    abi::emit_store_to_address(emitter, "r11", "r10", layout.target_lo);
    abi::emit_store_zero_to_address(emitter, "r10", layout.target_hi);
}

/// Stores the eval-provided ARM64 repeated flag on the reflection object.
fn emit_set_repeated_property_aarch64(emitter: &mut Emitter, layout: &ReflectionAttributeLayout) {
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the ReflectionAttribute object pointer
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload whether this attribute is repeated
    abi::emit_store_to_address(emitter, "x10", "x9", layout.repeated_lo);
    abi::emit_store_zero_to_address(emitter, "x9", layout.repeated_hi);
}

/// Stores the eval-provided x86_64 repeated flag on the reflection object.
fn emit_set_repeated_property_x86_64(emitter: &mut Emitter, layout: &ReflectionAttributeLayout) {
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the ReflectionAttribute object pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload whether this attribute is repeated
    abi::emit_store_to_address(emitter, "r11", "r10", layout.repeated_lo);
    abi::emit_store_zero_to_address(emitter, "r10", layout.repeated_hi);
}

/// Emits a C-visible global label with target-specific symbol mangling.
fn label_c_global(module: &Module, emitter: &mut Emitter, name: &str) {
    let symbol = module.target.extern_symbol(name);
    emitter.label_global(&symbol);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{Platform, Target};

    /// Verifies ARM64 positional attribute args become an owned integer-keyed hash.
    #[test]
    fn arm64_attribute_args_normalize_indexed_storage_to_hash() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_normalize_attribute_args_aarch64(&mut emitter, "args_fail");
        let output = emitter.output();
        assert!(output.contains("cmp x0, #4"));
        assert!(output.contains("bl __rt_array_to_hash"));
        assert!(output.contains("bl __rt_incref"));
    }

    /// Verifies x86_64 positional attribute args become an owned integer-keyed hash.
    #[test]
    fn x86_64_attribute_args_normalize_indexed_storage_to_hash() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_normalize_attribute_args_x86_64(&mut emitter, "args_fail");
        let output = emitter.output();
        assert!(output.contains("cmp rax, 4"));
        assert!(output.contains("call __rt_array_to_hash"));
        assert!(output.contains("call __rt_incref"));
    }
}
