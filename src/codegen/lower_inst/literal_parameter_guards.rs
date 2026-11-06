//! Purpose:
//! Validates boxed false/null parameters before callable invokers erase their source type.
//! Builds catchable argument diagnostics from the selected descriptor and runtime value.
//!
//! Called from:
//! - `crate::codegen::runtime_callable_invoker::owned_value_args::coerce()`.
//!
//! Key details:
//! - The argument is borrowed; its invoker owns any temporary box and exception cleanup.
//! - A successful check returns the canonical zero word for false or null.
//! - Descriptor names remain dynamic because invoker bodies can share a signature.

use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;

use super::{abi, Arch};

/// Validates the boxed result and raises TypeError before the callable entry can run.
pub(in crate::codegen) fn emit_boxed_literal_parameter_guard(
    emitter: &mut Emitter,
    data: &mut DataSection,
    descriptor_offset: usize,
    parameter: (usize, &str),
    null_only: bool,
    label_prefix: &str,
) {
    let accepted = format!("{label_prefix}_accepted");
    let boolean = format!("{label_prefix}_boolean");
    let tag_reg = abi::int_result_reg(emitter);
    let payload = crate::codegen_support::mixed_unbox_payload_reg(emitter.target);
    let null_tag = crate::codegen_support::sentinels::TAGGED_SCALAR_TAG_NULL;
    let prefix = format!(
        "(): Argument #{} (${}) must be of type {}, ",
        parameter.0 + 1, parameter.1, if null_only { "null" } else { "false" },
    );

    // -- inspect the original boxed value before a literal ABI conversion --
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    if null_only {
        branch_tag(emitter, tag_reg, null_tag, &accepted);
    }
    branch_tag(emitter, tag_reg, i64::from(super::objects::BOOL_TAG), &boolean);
    let refusals = super::objects::RUNTIME_TAGS.iter()
        .filter(|(tag, _)| *tag != super::objects::BOOL_TAG
            && (!null_only || i64::from(*tag) != null_tag))
        .map(|(tag, name)| (*tag, *name, format!("{label_prefix}_tag_{tag}")))
        .collect::<Vec<_>>();
    for (tag, _, label) in &refusals {
        branch_tag(emitter, tag_reg, i64::from(*tag), label);
    }
    refuse_static(emitter, data, descriptor_offset, &prefix, "mixed");

    for (tag, name, label) in refusals {
        emitter.label(&label);
        if tag == super::objects::OBJECT_TAG {
            let fallback = data.add_string(b"object");
            super::runtime_class_messages::emit_class_name_lookup(
                emitter, payload, &fallback,
                &format!("{label_prefix}_object_fallback"),
                &format!("{label_prefix}_object_done"),
            );
            refuse_loaded_type(emitter, data, descriptor_offset, &prefix);
        } else {
            refuse_static(emitter, data, descriptor_offset, &prefix, name);
        }
    }

    // -- boolean payload zero is accepted only by the false declaration --
    emitter.label(&boolean);
    abi::emit_reg_move(emitter, abi::int_result_reg(emitter), payload);
    if null_only {
        let false_value = format!("{label_prefix}_false");
        abi::emit_branch_if_int_result_zero(emitter, &false_value);
        refuse_static(emitter, data, descriptor_offset, &prefix, "true");
        emitter.label(&false_value);
        refuse_static(emitter, data, descriptor_offset, &prefix, "false");
    } else {
        abi::emit_branch_if_int_result_zero(emitter, &accepted);
        refuse_static(emitter, data, descriptor_offset, &prefix, "true");
    }
    emitter.label(&accepted);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
}

/// Branches on a runtime Mixed tag without assuming the target's result register.
fn branch_tag(emitter: &mut Emitter, register: &str, tag: i64, label: &str) {
    // -- compare the unboxed tag with this literal guard arm --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {register}, #{tag}"));            // compare the source tag
            emitter.instruction(&format!("b.eq {label}"));                      // select the matching value category
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {register}, {tag}"));             // compare the source tag
            emitter.instruction(&format!("je {label}"));                        // select the matching value category
        }
    }
}

/// Starts an argument error with a statically known PHP type name.
fn refuse_static(
    emitter: &mut Emitter,
    data: &mut DataSection,
    descriptor_offset: usize,
    prefix: &str,
    type_name: &str,
) {
    let (label, len) = data.add_string(type_name.as_bytes());
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_symbol_address(emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(emitter, len_reg, len as i64);
    refuse_loaded_type(emitter, data, descriptor_offset, prefix);
}

/// Combines the runtime type, parameter identity, and descriptor name into an owned diagnostic.
fn refuse_loaded_type(
    emitter: &mut Emitter,
    data: &mut DataSection,
    descriptor_offset: usize,
    prefix: &str,
) {
    let (ptr, len) = abi::string_result_regs(emitter);
    let (right_ptr, right_len) = super::runtime_class_messages::concat_right_operand_regs(emitter);
    let (given, given_len) = data.add_string(b" given");
    abi::emit_symbol_address(emitter, right_ptr, &given);
    abi::emit_load_int_immediate(emitter, right_len, given_len as i64);
    abi::emit_call_label(emitter, "__rt_concat");

    let (prefix_label, prefix_len) = data.add_string(prefix.as_bytes());
    abi::emit_reg_move(emitter, right_ptr, ptr);
    abi::emit_reg_move(emitter, right_len, len);
    abi::emit_symbol_address(emitter, ptr, &prefix_label);
    abi::emit_load_int_immediate(emitter, len, prefix_len as i64);
    abi::emit_call_label(emitter, "__rt_concat");

    abi::emit_reg_move(emitter, right_ptr, ptr);
    abi::emit_reg_move(emitter, right_len, len);
    let descriptor = abi::symbol_scratch_reg(emitter);
    abi::load_at_offset(emitter, descriptor, descriptor_offset);
    abi::emit_load_from_address(
        emitter, ptr, descriptor,
        crate::codegen_support::callable_descriptor::CALLABLE_DESC_NAME_OFFSET,
    );
    abi::emit_load_from_address(
        emitter, len, descriptor,
        crate::codegen_support::callable_descriptor::CALLABLE_DESC_NAME_LENGTH_OFFSET,
    );
    let (anonymous, anonymous_len) = data.add_string(b"{closure}");
    let fallback_ptr = abi::symbol_scratch_reg(emitter);
    let fallback_len = abi::secondary_scratch_reg(emitter);
    abi::emit_symbol_address(emitter, fallback_ptr, &anonymous);
    abi::emit_load_int_immediate(emitter, fallback_len, anonymous_len as i64);
    // -- anonymous descriptors still identify their closure in argument errors --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {len}, #0"));                     // does the descriptor have a PHP name?
            emitter.instruction(&format!(
                "csel {ptr}, {fallback_ptr}, {ptr}, eq"
            ));                                                                 // select the closure name when absent
            emitter.instruction(&format!(
                "csel {len}, {fallback_len}, {len}, eq"
            ));                                                                 // select the matching byte length
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {len}, {len}"));                 // does the descriptor have a PHP name?
            emitter.instruction(&format!("cmovz {ptr}, {fallback_ptr}"));       // select the closure name when absent
            emitter.instruction(&format!("cmovz {len}, {fallback_len}"));       // select the matching byte length
        }
    }
    abi::emit_call_label(emitter, "__rt_concat");
    abi::emit_call_label(emitter, "__rt_str_persist");
    throw_type_error(emitter);
}

/// Transfers the persisted diagnostic to a compact TypeError and the standard unwinder.
fn throw_type_error(emitter: &mut Emitter) {
    let (ptr, len) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr, len);
    // -- construct a managed Throwable while preserving the diagnostic --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #56");                                 // allocate the compact Throwable payload
            emitter.instruction("bl __rt_heap_alloc");                          // acquire the exception's owned block
            emitter.instruction("mov x9, #6");                                  // classify the allocation as a Throwable
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the object heap kind
            emitter.instruction("bl __rt_object_handle_acquire");               // acquire its PHP object handle
            abi::emit_load_symbol_to_reg(emitter, "x9", "_spl_type_error_class_id", 0);
            emitter.instruction("str x9, [x0]");                                // identify the TypeError class
            abi::emit_pop_reg_pair(emitter, ptr, len);
            emitter.instruction("str x1, [x0, #8]");                            // transfer the persisted diagnostic pointer
            emitter.instruction("str x2, [x0, #16]");                           // store the diagnostic byte length
            emitter.instruction("str xzr, [x0, #24]");                          // initialize the exception code
            crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, "x0");
            emitter.instruction("str xzr, [x0, #40]");                          // initialize the previous exception
            abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);
            emitter.instruction("b __rt_throw_current");                        // unwind argument owners before the PHP catch
        }
        Arch::X86_64 => {
            emitter.instruction("mov eax, 56");                                 // allocate the compact Throwable payload
            emitter.instruction("call __rt_heap_alloc");                        // acquire the exception's owned block
            abi::emit_load_int_immediate(
                emitter, "r10",
                crate::codegen_support::sentinels::x86_64_heap_kind_word(6) as i64,
            );
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the canonical Throwable heap kind
            emitter.instruction("call __rt_object_handle_acquire");             // acquire its PHP object handle
            abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_type_error_class_id", 0);
            emitter.instruction("mov QWORD PTR [rax], r10");                    // identify the TypeError class
            let diagnostic_ptr = abi::symbol_scratch_reg(emitter);
            let diagnostic_len = abi::secondary_scratch_reg(emitter);
            abi::emit_pop_reg_pair(emitter, diagnostic_ptr, diagnostic_len);
            emitter.instruction(&format!(
                "mov QWORD PTR [rax + 8], {diagnostic_ptr}"
            ));                                                                 // transfer the persisted diagnostic pointer
            emitter.instruction(&format!(
                "mov QWORD PTR [rax + 16], {diagnostic_len}"
            ));                                                                 // store the diagnostic byte length
            emitter.instruction("mov QWORD PTR [rax + 24], 0");                 // initialize the exception code
            crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(emitter, "rax");
            emitter.instruction("mov QWORD PTR [rax + 40], 0");                 // initialize the previous exception
            abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
            emitter.instruction("jmp __rt_throw_current");                      // unwind argument owners before the PHP catch
        }
    }
}
