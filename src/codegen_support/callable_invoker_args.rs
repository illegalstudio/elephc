//! Purpose:
//! Shared helpers for descriptor-invoker argument containers.
//! Normalizes indexed/associative argument arrays to boxed Mixed containers.
//!
//! Called from:
//! - `crate::codegen` EIR callable lowerers and legacy callable builtin support.
//!
//! Key details:
//! - Descriptor invokers consume boxed `Mixed` argument containers.
//! - Ref-cell markers use a private tag that must not overlap PHP-visible Mixed tags.

use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::{abi, value_boxing};
use crate::types::PhpType;

/// Internal boxed-Mixed tag used only inside descriptor-invoker argument arrays.
pub(crate) const INVOKER_ARG_REF_CELL_TAG: i64 = 11;

/// Clones a boxed runtime Mixed argument container into a normalized boxed Mixed container.
pub(crate) fn emit_clone_runtime_mixed_invoker_arg_as_mixed(
    dest_reg: &str,
    emitter: &mut Emitter,
    next_label: &mut dyn FnMut(&str) -> String,
    data: &mut DataSection,
) {
    let tag_reg = abi::secondary_scratch_reg(emitter);
    let payload_reg = abi::tertiary_scratch_reg(emitter);
    let indexed_label = next_label("invoker_normalize_mixed_indexed");
    let assoc_label = next_label("invoker_normalize_mixed_assoc");
    let done_label = next_label("invoker_normalize_mixed_done");
    let indexed_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let assoc_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };

    abi::emit_load_from_address(emitter, tag_reg, dest_reg, 0);
    abi::emit_load_from_address(emitter, payload_reg, dest_reg, 8);
    abi::emit_push_reg(emitter, payload_reg);                                   // preserve the unboxed runtime argument container while normalizing by Mixed tag
    emit_branch_if_mixed_arg_tag(
        tag_reg,
        value_boxing::runtime_value_tag(&indexed_ty),
        &indexed_label,
        emitter,
    );
    emit_branch_if_mixed_arg_tag(
        tag_reg,
        value_boxing::runtime_value_tag(&assoc_ty),
        &assoc_label,
        emitter,
    );
    emit_call_user_func_array_invalid_mixed_args_abort(emitter, data);

    emitter.label(&indexed_label);
    abi::emit_load_temporary_stack_slot(emitter, dest_reg, 0);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the borrowed indexed-array pointer after loading it for cloning
    emit_clone_indexed_array_for_invoker_with_runtime_tag(dest_reg, emitter);
    emit_box_invoker_arg_clone_as_mixed(dest_reg, &indexed_ty, emitter);
    abi::emit_jump(emitter, &done_label);

    emitter.label(&assoc_label);
    abi::emit_load_temporary_stack_slot(emitter, dest_reg, 0);
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the borrowed hash pointer after loading it for cloning
    emit_clone_assoc_array_for_invoker(dest_reg, emitter);
    emit_box_invoker_arg_clone_as_mixed(dest_reg, &assoc_ty, emitter);

    emitter.label(&done_label);
}

/// Clones and converts an indexed callback argument array to boxed Mixed slots.
pub(crate) fn emit_clone_indexed_array_for_invoker(
    dest_reg: &str,
    elem_ty: &PhpType,
    emitter: &mut Emitter,
) {
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let tag_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let result_reg = abi::int_result_reg(emitter);
    if array_arg_reg != dest_reg {
        emitter.instruction(&format!("mov {}, {}", array_arg_reg, dest_reg));   // pass the callback-argument array to the clone helper without mutating caller storage
    }
    abi::emit_call_label(emitter, "__rt_array_clone_shallow");
    if array_arg_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", array_arg_reg, result_reg)); // pass the cloned argument array to the Mixed-slot conversion helper
    }
    abi::emit_load_int_immediate(
        emitter,
        tag_arg_reg,
        value_boxing::runtime_value_tag(&elem_ty.codegen_repr()) as i64,
    );
    abi::emit_call_label(emitter, "__rt_array_to_mixed");
    if dest_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", dest_reg, result_reg));      // keep the normalized Mixed argument array in the invoker ABI register
    }
}

/// Clones and converts a runtime-typed indexed callback argument array to boxed Mixed slots.
pub(crate) fn emit_clone_indexed_array_for_invoker_with_runtime_tag(
    dest_reg: &str,
    emitter: &mut Emitter,
) {
    let array_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let tag_arg_reg = abi::int_arg_reg_name(emitter.target, 1);
    let result_reg = abi::int_result_reg(emitter);
    if array_arg_reg != dest_reg {
        emitter.instruction(&format!("mov {}, {}", array_arg_reg, dest_reg));   // pass the runtime-typed callback array to the clone helper without mutating caller storage
    }
    abi::emit_call_label(emitter, "__rt_array_clone_shallow");
    if array_arg_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", array_arg_reg, result_reg)); // pass the cloned runtime-typed array to the Mixed-slot conversion helper
    }
    emit_load_indexed_array_runtime_value_type_tag(array_arg_reg, tag_arg_reg, emitter);
    abi::emit_call_label(emitter, "__rt_array_to_mixed");
    if dest_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", dest_reg, result_reg));      // keep the normalized Mixed argument array in the invoker ABI register
    }
}

/// Loads an indexed array's runtime value-type tag from its packed heap header.
fn emit_load_indexed_array_runtime_value_type_tag(
    array_reg: &str,
    tag_reg: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [{}, #-8]", tag_reg, array_reg)); // load the packed indexed-array metadata before Mixed-slot conversion
            emitter.instruction(&format!("lsr {}, {}, #8", tag_reg, tag_reg));  // move the indexed-array value_type tag into the low bits
            emitter.instruction(&format!("and {}, {}, #0x7f", tag_reg, tag_reg)); // isolate the runtime indexed-array value_type tag
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, QWORD PTR [{} - 8]", tag_reg, array_reg)); // load the packed indexed-array metadata before Mixed-slot conversion
            emitter.instruction(&format!("shr {}, 8", tag_reg));                // move the indexed-array value_type tag into the low bits
            emitter.instruction(&format!("and {}, 0x7f", tag_reg));             // isolate the runtime indexed-array value_type tag
        }
    }
}

/// Clones and converts an associative callback argument array to boxed Mixed entries.
pub(crate) fn emit_clone_assoc_array_for_invoker(dest_reg: &str, emitter: &mut Emitter) {
    emit_clone_assoc_array_for_invoker_with_value_type(dest_reg, &PhpType::Int, emitter);
}

/// Clones an associative callback argument array and boxes entries when needed.
pub(crate) fn emit_clone_assoc_array_for_invoker_with_value_type(
    dest_reg: &str,
    value_ty: &PhpType,
    emitter: &mut Emitter,
) {
    let hash_arg_reg = abi::int_arg_reg_name(emitter.target, 0);
    let result_reg = abi::int_result_reg(emitter);
    if hash_arg_reg != dest_reg {
        emitter.instruction(&format!("mov {}, {}", hash_arg_reg, dest_reg));    // pass the callback-argument hash to the clone helper without mutating caller storage
    }
    abi::emit_call_label(emitter, "__rt_hash_clone_shallow");
    if hash_arg_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", hash_arg_reg, result_reg));  // pass the cloned argument hash to the Mixed-entry conversion helper
    }
    if !matches!(value_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(emitter, "__rt_hash_to_mixed");
    }
    if dest_reg != result_reg {
        emitter.instruction(&format!("mov {}, {}", dest_reg, result_reg));      // keep the normalized Mixed argument hash in the invoker ABI register
    }
}

/// Boxes the normalized argument clone as `Mixed` and releases the caller-side clone owner.
pub(crate) fn emit_box_invoker_arg_clone_as_mixed(
    dest_reg: &str,
    container_ty: &PhpType,
    emitter: &mut Emitter,
) {
    let tag_reg = abi::secondary_scratch_reg(emitter);
    let zero_reg = abi::tertiary_scratch_reg(emitter);

    abi::emit_push_reg(emitter, dest_reg);                                      // preserve the cloned argument container while Mixed boxing retains it
    abi::emit_load_int_immediate(
        emitter,
        tag_reg,
        value_boxing::runtime_value_tag(container_ty) as i64,
    );
    abi::emit_load_int_immediate(emitter, zero_reg, 0);
    value_boxing::emit_box_runtime_payload_as_mixed(emitter, tag_reg, dest_reg, zero_reg);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the boxed Mixed argument while dropping the clone owner
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_decref_if_refcounted(emitter, container_ty);
    abi::emit_pop_reg(emitter, dest_reg);                                       // move the boxed Mixed argument into the invoker ABI register
    abi::emit_release_temporary_stack(emitter, 16);                             // discard the preserved clone slot after ownership transfer
}

/// Branches to `label` when a boxed invoker argument carries `expected_tag`.
pub(crate) fn emit_branch_if_mixed_arg_tag(
    tag_reg: &str,
    expected_tag: u8,
    label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #{}", tag_reg, expected_tag)); // check the runtime tag of the boxed invoker argument container
            emitter.instruction(&format!("b.eq {}", label));                    // dispatch to the handler for this argument-container shape
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", tag_reg, expected_tag)); // check the runtime tag of the boxed invoker argument container
            emitter.instruction(&format!("je {}", label));                      // dispatch to the handler for this argument-container shape
        }
    }
}

/// Emits assembly for a descriptor invoker argument-container type mismatch.
pub(crate) fn emit_call_user_func_array_invalid_mixed_args_abort(
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let (message_label, message_len) = data.add_string(
        b"Fatal error: callable descriptor invoker expected an indexed or associative argument array\n",
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the descriptor argument-shape diagnostic to stderr
            abi::emit_symbol_address(emitter, "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the descriptor argument-shape diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the descriptor argument-shape diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the descriptor argument-shape diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the descriptor argument-shape diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}
