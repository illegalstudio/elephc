//! Purpose:
//! Lowers variadic array construction from named argument sources.
//! Works with the shared call-argument plan to preserve PHP named-argument semantics.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args::named`
//!
//! Key details:
//! - Side effects occur in source order, while final argument materialization follows parameter and ABI order.

use crate::codegen::emit::Emitter;
use crate::codegen::{abi, context::Context, data_section::DataSection};
use crate::types::PhpType;

use super::temps::{load_source_temp_to_result, source_temp_offset};
use super::{FinalArgSource, PrefixVariadicTail, VariadicArgSource};
use super::super::{
    array_element_stride, load_array_element_to_result, spread_source_elem_ty,
    store_current_array_element, variadic_container_elem_ty,
};

pub(super) fn emit_variadic_array_arg_from_sources(
    variadic_sources: &[VariadicArgSource],
    prefix_variadic_tail: Option<&PrefixVariadicTail>,
    source_temp_types: &[PhpType],
    final_pushed_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if variadic_sources.iter().any(|source| source.key.is_some()) {
        return emit_variadic_assoc_arg_from_sources(
            variadic_sources,
            prefix_variadic_tail,
            source_temp_types,
            final_pushed_bytes,
            emitter,
            ctx,
            data,
        );
    }

    let elem_count = variadic_sources.len();
    let first_elem_ty = match variadic_sources.first() {
        Some(VariadicArgSource {
            source: FinalArgSource::SourceTemp(temp_idx),
            ..
        }) => source_temp_types[*temp_idx].clone(),
        _ => PhpType::Int,
    };
    let container_elem_ty = variadic_container_elem_ty(&first_elem_ty);
    let elem_size = match container_elem_ty.codegen_repr() {
        PhpType::Str => 16,
        _ => 8,
    };
    let (capacity_reg, elem_size_reg, peek_reg, len_reg) = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => ("x0", "x1", "x9", "x10"),
        crate::codegen::platform::Arch::X86_64 => ("rdi", "rsi", "r11", "r10"),
    };

    emitter.comment(&format!("build variadic array ({} elements)", elem_count));
    abi::emit_load_int_immediate(emitter, capacity_reg, elem_count as i64);
    abi::emit_load_int_immediate(emitter, elem_size_reg, elem_size as i64);
    abi::emit_call_label(emitter, "__rt_array_new");
    abi::emit_push_result_value(emitter, &PhpType::Array(Box::new(container_elem_ty.clone())));

    for (idx, source) in variadic_sources.iter().enumerate() {
        let mut elem_ty = match &source.source {
            FinalArgSource::SourceTemp(temp_idx) => load_source_temp_to_result(
                *temp_idx,
                source_temp_types,
                final_pushed_bytes + 16,
                emitter,
            ),
            _ => PhpType::Int,
        };
        let boxed_for_container = if matches!(container_elem_ty, PhpType::Mixed)
            && !matches!(elem_ty, PhpType::Mixed | PhpType::Union(_))
        {
            crate::codegen::emit_box_current_value_as_mixed(emitter, &elem_ty);
            elem_ty = PhpType::Mixed;
            true
        } else {
            false
        };
        if !boxed_for_container {
            abi::emit_incref_if_refcounted(emitter, &elem_ty.codegen_repr());
        }
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("ldr {}, [sp]", peek_reg));        // peek the variadic array pointer without removing it from the stack
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", peek_reg)); // peek the variadic array pointer without removing it from the stack
            }
        }
        if idx == 0 {
            super::super::super::super::arrays::emit_array_value_type_stamp(emitter, peek_reg, &elem_ty);
        }
        store_current_array_element(emitter, peek_reg, idx, &elem_ty);
        abi::emit_load_int_immediate(emitter, len_reg, (idx + 1) as i64);
        abi::emit_store_to_address(emitter, len_reg, peek_reg, 0);
    }

    PhpType::Array(Box::new(container_elem_ty))
}

fn emit_prefix_tail_into_variadic_hash(
    tail: &PrefixVariadicTail,
    container_elem_ty: &PhpType,
    source_temp_types: &[PhpType],
    final_pushed_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    const SCRATCH_BYTES: usize = 48;
    const HASH_SLOT_BYTES: usize = 16;

    let source_elem_ty = spread_source_elem_ty(&source_temp_types[tail.prefix_temp_idx]);
    let elem_stride = array_element_stride(&source_elem_ty);
    let loop_start = ctx.next_label("named_variadic_tail_loop");
    let loop_done = ctx.next_label("named_variadic_tail_done");
    let tail_empty = ctx.next_label("named_variadic_tail_empty");
    let tail_ready = ctx.next_label("named_variadic_tail_ready");
    let result_reg = abi::int_result_reg(emitter);
    let hash_reg = abi::int_arg_reg_name(emitter.target, 0);
    let key_ptr_reg = abi::int_arg_reg_name(emitter.target, 1);
    let key_len_reg = abi::int_arg_reg_name(emitter.target, 2);
    let value_lo_reg = abi::int_arg_reg_name(emitter.target, 3);
    let value_hi_reg = abi::int_arg_reg_name(emitter.target, 4);
    let value_tag_reg = abi::int_arg_reg_name(emitter.target, 5);
    let zero_reg = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => "xzr",
        crate::codegen::platform::Arch::X86_64 => "0",
    };
    let stack_reg = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => "sp",
        crate::codegen::platform::Arch::X86_64 => "rsp",
    };

    emitter.comment("copy spread tail into named variadic array");
    abi::emit_reserve_temporary_stack(emitter, SCRATCH_BYTES);
    let prefix_offset = source_temp_offset(
        source_temp_types,
        tail.prefix_temp_idx,
        final_pushed_bytes + SCRATCH_BYTES + HASH_SLOT_BYTES,
    );

    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x10", prefix_offset);
            abi::emit_store_to_address(emitter, "x10", stack_reg, 32);
            emitter.instruction("ldr x9, [x10]");                               // load the evaluated spread prefix length before slicing its variadic tail
            abi::emit_load_int_immediate(emitter, "x11", tail.start_idx as i64);
            emitter.instruction("cmp x9, x11");                                 // check whether the prefix has values beyond the regular parameters
            emitter.instruction(&format!("b.le {}", tail_empty));               // no variadic tail exists when the prefix fits in regular parameters
            emitter.instruction("sub x9, x9, x11");                             // compute variadic tail length as prefix length minus regular parameter count
            emitter.instruction(&format!("b {}", tail_ready));                  // store the computed non-empty variadic tail length
            emitter.label(&tail_empty);
            emitter.instruction("mov x9, #0");                                  // use an empty variadic tail when the prefix has no remaining values
            emitter.label(&tail_ready);
            abi::emit_store_to_address(emitter, "x9", stack_reg, 16);
            abi::emit_store_to_address(emitter, "xzr", stack_reg, 0);

            emitter.label(&loop_start);
            abi::emit_load_temporary_stack_slot(emitter, "x8", 0);
            abi::emit_load_temporary_stack_slot(emitter, "x9", 16);
            emitter.instruction("cmp x8, x9");                                  // stop after every spread-tail element has been copied into ...$rest
            emitter.instruction(&format!("b.ge {}", loop_done));                // finish the dynamic variadic-tail copy loop
            abi::emit_load_temporary_stack_slot(emitter, "x10", 32);
            abi::emit_load_int_immediate(emitter, "x11", tail.start_idx as i64);
            emitter.instruction("add x11, x11, x8");                            // convert tail index to source prefix element index
            if elem_stride == 16 {
                emitter.instruction("lsl x11, x11, #4");                        // scale source prefix element index by the string slot width
            } else {
                emitter.instruction("lsl x11, x11, #3");                        // scale source prefix element index by the scalar slot width
            }
            emitter.instruction("add x10, x10, #24");                           // address the spread prefix payload after its array header
            emitter.instruction("add x10, x10, x11");                           // address the current spread-tail element payload slot
            load_array_element_to_result(emitter, &source_elem_ty, "x10", 0);
        }
        crate::codegen::platform::Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r12", prefix_offset);
            abi::emit_store_to_address(emitter, "r12", stack_reg, 32);
            emitter.instruction("mov r11, QWORD PTR [r12]");                    // load the evaluated spread prefix length before slicing its variadic tail
            abi::emit_load_int_immediate(emitter, "r10", tail.start_idx as i64);
            emitter.instruction("cmp r11, r10");                                // check whether the prefix has values beyond the regular parameters
            emitter.instruction(&format!("jle {}", tail_empty));                // no variadic tail exists when the prefix fits in regular parameters
            emitter.instruction("sub r11, r10");                                // compute variadic tail length as prefix length minus regular parameter count
            emitter.instruction(&format!("jmp {}", tail_ready));                // store the computed non-empty variadic tail length
            emitter.label(&tail_empty);
            emitter.instruction("mov r11, 0");                                  // use an empty variadic tail when the prefix has no remaining values
            emitter.label(&tail_ready);
            abi::emit_store_to_address(emitter, "r11", stack_reg, 16);
            abi::emit_store_to_address(emitter, "0", stack_reg, 0);

            emitter.label(&loop_start);
            abi::emit_load_temporary_stack_slot(emitter, "r10", 0);
            abi::emit_load_temporary_stack_slot(emitter, "r11", 16);
            emitter.instruction("cmp r10, r11");                                // stop after every spread-tail element has been copied into ...$rest
            emitter.instruction(&format!("jge {}", loop_done));                 // finish the dynamic variadic-tail copy loop
            abi::emit_load_temporary_stack_slot(emitter, "r12", 32);
            abi::emit_load_int_immediate(emitter, "r11", tail.start_idx as i64);
            emitter.instruction("add r11, r10");                                // convert tail index to source prefix element index
            emitter.instruction(&format!("imul r11, {}", elem_stride));         // scale source prefix element index by the payload slot width
            emitter.instruction("add r12, 24");                                 // address the spread prefix payload after its array header
            emitter.instruction("add r12, r11");                                // address the current spread-tail element payload slot
            load_array_element_to_result(emitter, &source_elem_ty, "r12", 0);
        }
    }

    let mut elem_ty = source_elem_ty.clone();
    let boxed_for_container = if matches!(container_elem_ty, PhpType::Mixed)
        && !matches!(elem_ty, PhpType::Mixed | PhpType::Union(_))
    {
        crate::codegen::emit_box_current_value_as_mixed(emitter, &elem_ty);
        elem_ty = PhpType::Mixed;
        true
    } else {
        false
    };
    if !boxed_for_container && matches!(elem_ty, PhpType::Str) {
        abi::emit_call_label(emitter, "__rt_str_persist");                      // persist spread-tail strings before storing them in the variadic hash
    } else if !boxed_for_container {
        abi::emit_incref_if_refcounted(emitter, &elem_ty.codegen_repr());
    }

    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x8", 0);
            emitter.instruction(&format!("mov {}, x8", key_ptr_reg));           // use the zero-based tail index as the numeric variadic key
            abi::emit_load_int_immediate(emitter, key_len_reg, -1);
        }
        crate::codegen::platform::Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r10", 0);
            emitter.instruction(&format!("mov {}, r10", key_ptr_reg));          // use the zero-based tail index as the numeric variadic key
            abi::emit_load_int_immediate(emitter, key_len_reg, -1);
        }
    }

    let (val_lo, val_hi) = match elem_ty.codegen_repr() {
        PhpType::Float => {
            let bits_reg = abi::temp_int_reg(emitter.target);
            match emitter.target.arch {
                crate::codegen::platform::Arch::AArch64 => {
                    emitter.instruction(&format!("fmov {}, {}", bits_reg, abi::float_result_reg(emitter))); // move variadic float bits into the hash value register
                }
                crate::codegen::platform::Arch::X86_64 => {
                    emitter.instruction(&format!("movq {}, {}", bits_reg, abi::float_result_reg(emitter))); // move variadic float bits into the hash value register
                }
            }
            (bits_reg, zero_reg)
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            (ptr_reg, len_reg)
        }
        _ => (result_reg, zero_reg),
    };
    emitter.instruction(&format!("mov {}, {}", value_lo_reg, val_lo));          // move the spread-tail value low word into the hash-set ABI register
    emitter.instruction(&format!("mov {}, {}", value_hi_reg, val_hi));          // move the spread-tail value high word into the hash-set ABI register
    abi::emit_load_int_immediate(
        emitter,
        value_tag_reg,
        crate::codegen::runtime_value_tag(&elem_ty) as i64,
    );
    abi::emit_load_temporary_stack_slot(emitter, hash_reg, SCRATCH_BYTES);
    abi::emit_call_label(emitter, "__rt_hash_set");
    abi::emit_store_to_address(emitter, result_reg, stack_reg, SCRATCH_BYTES);

    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(emitter, "x8", 0);
            emitter.instruction("add x8, x8, #1");                              // advance to the next spread-tail variadic element
            abi::emit_store_to_address(emitter, "x8", stack_reg, 0);
            emitter.instruction(&format!("b {}", loop_start));                  // continue copying spread-tail elements into ...$rest
        }
        crate::codegen::platform::Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(emitter, "r10", 0);
            emitter.instruction("add r10, 1");                                  // advance to the next spread-tail variadic element
            abi::emit_store_to_address(emitter, "r10", stack_reg, 0);
            emitter.instruction(&format!("jmp {}", loop_start));                // continue copying spread-tail elements into ...$rest
        }
    }

    emitter.label(&loop_done);
    abi::emit_release_temporary_stack(emitter, SCRATCH_BYTES);
}

fn emit_variadic_assoc_arg_from_sources(
    variadic_sources: &[VariadicArgSource],
    prefix_variadic_tail: Option<&PrefixVariadicTail>,
    source_temp_types: &[PhpType],
    final_pushed_bytes: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let elem_count = variadic_sources.len();
    let first_elem_ty = if let Some(tail) = prefix_variadic_tail {
        spread_source_elem_ty(&source_temp_types[tail.prefix_temp_idx])
    } else {
        match variadic_sources.first() {
        Some(VariadicArgSource {
            source: FinalArgSource::SourceTemp(temp_idx),
            ..
        }) => source_temp_types[*temp_idx].clone(),
        _ => PhpType::Int,
        }
    };
    let container_elem_ty = variadic_container_elem_ty(&first_elem_ty);
    let hash_capacity_reg = abi::int_arg_reg_name(emitter.target, 0);
    let key_ptr_reg = abi::int_arg_reg_name(emitter.target, 1);
    let key_len_reg = abi::int_arg_reg_name(emitter.target, 2);
    let value_lo_reg = abi::int_arg_reg_name(emitter.target, 3);
    let value_hi_reg = abi::int_arg_reg_name(emitter.target, 4);
    let value_tag_reg = abi::int_arg_reg_name(emitter.target, 5);
    let tag_reg = abi::int_arg_reg_name(emitter.target, 1);
    let result_reg = abi::int_result_reg(emitter);
    let stack_reg = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => "sp",
        crate::codegen::platform::Arch::X86_64 => "rsp",
    };
    let zero_reg = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => "xzr",
        crate::codegen::platform::Arch::X86_64 => "0",
    };

    emitter.comment(&format!("build named variadic array ({} elements)", elem_count));
    abi::emit_load_int_immediate(
        emitter,
        hash_capacity_reg,
        std::cmp::max(elem_count * 2, 16) as i64,
    );
    abi::emit_load_int_immediate(
        emitter,
        tag_reg,
        crate::codegen::runtime_value_tag(&container_elem_ty) as i64,
    );
    abi::emit_call_label(emitter, "__rt_hash_new");
    abi::emit_push_result_value(emitter, &PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(container_elem_ty.clone()),
    });

    if let Some(tail) = prefix_variadic_tail {
        emit_prefix_tail_into_variadic_hash(
            tail,
            &container_elem_ty,
            source_temp_types,
            final_pushed_bytes,
            emitter,
            ctx,
        );
    }

    for (idx, source) in variadic_sources.iter().enumerate() {
        match &source.key {
            Some(key) => {
                let (key_label, key_len) = data.add_string(key.as_bytes());
                abi::emit_symbol_address(emitter, key_ptr_reg, &key_label);
                abi::emit_load_int_immediate(emitter, key_len_reg, key_len as i64);
            }
            None => {
                abi::emit_load_int_immediate(emitter, key_ptr_reg, idx as i64);
                abi::emit_load_int_immediate(emitter, key_len_reg, -1);
            }
        }
        abi::emit_push_reg_pair(emitter, key_ptr_reg, key_len_reg);             // preserve the variadic hash key while loading the saved argument value
        let mut elem_ty = match &source.source {
            FinalArgSource::SourceTemp(temp_idx) => load_source_temp_to_result(
                *temp_idx,
                source_temp_types,
                final_pushed_bytes + 32,
                emitter,
            ),
            _ => PhpType::Int,
        };
        let boxed_for_container = if matches!(container_elem_ty, PhpType::Mixed)
            && !matches!(elem_ty, PhpType::Mixed | PhpType::Union(_))
        {
            crate::codegen::emit_box_current_value_as_mixed(emitter, &elem_ty);
            elem_ty = PhpType::Mixed;
            true
        } else {
            false
        };
        if !boxed_for_container && matches!(elem_ty, PhpType::Str) {
            abi::emit_call_label(emitter, "__rt_str_persist");                  // persist variadic strings before storing them in the hash table
        } else if !boxed_for_container {
            abi::emit_incref_if_refcounted(emitter, &elem_ty.codegen_repr());
        }
        let (val_lo, val_hi) = match elem_ty.codegen_repr() {
            PhpType::Float => {
                let bits_reg = abi::temp_int_reg(emitter.target);
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("fmov {}, {}", bits_reg, abi::float_result_reg(emitter))); // move variadic float bits into the hash value register
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("movq {}, {}", bits_reg, abi::float_result_reg(emitter))); // move variadic float bits into the hash value register
                    }
                }
                (bits_reg, zero_reg)
            }
            PhpType::Str => {
                let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                (ptr_reg, len_reg)
            }
            _ => (result_reg, zero_reg),
        };
        emitter.instruction(&format!("mov {}, {}", value_lo_reg, val_lo));      // move the variadic value low word into the hash-set ABI register
        emitter.instruction(&format!("mov {}, {}", value_hi_reg, val_hi));      // move the variadic value high word into the hash-set ABI register
        abi::emit_load_int_immediate(
            emitter,
            value_tag_reg,
            crate::codegen::runtime_value_tag(&elem_ty) as i64,
        );
        abi::emit_pop_reg_pair(emitter, key_ptr_reg, key_len_reg);              // restore the variadic hash key into the hash-set ABI registers
        abi::emit_load_temporary_stack_slot(emitter, hash_capacity_reg, 0);
        abi::emit_call_label(emitter, "__rt_hash_set");
        abi::emit_store_to_address(emitter, result_reg, stack_reg, 0);
    }

    PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(container_elem_ty),
    }
}
