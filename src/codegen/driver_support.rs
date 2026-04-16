use crate::types::{EnumInfo, PhpType};

use super::abi;
use super::context::Context;
use super::data_section::DataSection;
use super::emit::Emitter;
use super::functions;
use super::platform::{Arch, Target};
use super::runtime;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

pub(super) fn emit_write_literal_stderr(emitter: &mut Emitter, label: &str, len: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp("x1", label);                                          // load the page address of the stderr literal on AArch64
            emitter.add_lo12("x1", "x1", label);                                // resolve the exact stderr literal address on AArch64
            emitter.instruction(&format!("mov x2, #{}", len));                  // materialize the stderr literal byte length in the AArch64 write-length register
            emitter.instruction("mov x0, #2");                                  // target the stderr file descriptor on AArch64
            emitter.syscall(4);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", label);
            emitter.instruction(&format!("mov edx, {}", len));                  // materialize the stderr literal byte length in the x86_64 write-length register
            emitter.instruction("mov edi, 2");                                  // target the stderr file descriptor on x86_64
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall number 1 = write
            emitter.instruction("syscall");                                     // write the requested literal bytes to stderr on x86_64
        }
    }
}

pub(super) fn emit_write_current_string_stderr(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // target the stderr file descriptor on AArch64
            emitter.syscall(4);
        }
        Arch::X86_64 => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            emitter.instruction(&format!("mov rsi, {}", ptr_reg));              // move the current string pointer into the x86_64 write buffer register
            emitter.instruction(&format!("mov rdx, {}", len_reg));              // move the current string length into the x86_64 write length register
            emitter.instruction("mov edi, 2");                                  // target the stderr file descriptor on x86_64
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall number 1 = write
            emitter.instruction("syscall");                                     // write the current string payload to stderr on x86_64
        }
    }
}

pub fn generate_runtime(heap_size: usize, target: Target) -> String {
    let mut emitter = Emitter::new(target);
    emitter.emit_text_prelude();
    runtime::emit_runtime(&mut emitter);
    let mut output = emitter.output();
    output.push('\n');
    output.push_str(&runtime::emit_runtime_data_fixed(heap_size));
    output
}

pub(super) fn emit_enum_singleton_initializers(
    emitter: &mut Emitter,
    data: &mut DataSection,
    ctx: &Context,
) {
    let mut sorted_enums: Vec<(&String, &EnumInfo)> = ctx.enums.iter().collect();
    sorted_enums.sort_by_key(|(name, _)| name.as_str());
    for (enum_name, enum_info) in sorted_enums {
        let Some(class_info) = ctx.classes.get(enum_name) else {
            continue;
        };
        for case in &enum_info.cases {
            emitter.comment(&format!("initialize enum singleton {}::{}", enum_name, case.name));
            let obj_size = 8 + class_info.properties.len() * 16;
            let result_reg = abi::int_result_reg(emitter);
            let object_reg = abi::symbol_scratch_reg(emitter);
            let temp_reg = abi::temp_int_reg(emitter.target);
            abi::emit_load_int_immediate(emitter, result_reg, obj_size as i64); // enum singleton object size in bytes in the heap allocator input register
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate enum singleton object storage
            abi::emit_load_int_immediate(emitter, temp_reg, 4);                 // heap kind 4 = object instance
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction(&format!("str {}, [{}, #-8]", temp_reg, result_reg)); // store object kind in the uniform heap header just before the payload pointer
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("mov {}, 0x{:x}", temp_reg, (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize the x86_64 object heap kind word with the uniform heap marker
                    emitter.instruction(&format!("mov QWORD PTR [{} - 8], {}", result_reg, temp_reg)); // store object kind in the x86_64 uniform heap header just before the payload pointer
                }
            }
            abi::emit_load_int_immediate(emitter, temp_reg, class_info.class_id as i64); // load compile-time enum class id
            abi::emit_store_to_address(emitter, temp_reg, result_reg, 0);       // store enum class id at object header
            abi::emit_push_reg(emitter, result_reg);                            // save singleton object pointer while initializing properties

            for i in 0..class_info.properties.len() {
                let offset = 8 + i * 16;
                abi::emit_load_temporary_stack_slot(emitter, object_reg, 0);    // peek enum singleton pointer from the temporary stack slot
                abi::emit_store_zero_to_address(emitter, object_reg, offset);   // zero-initialize the low property word
                abi::emit_store_zero_to_address(emitter, object_reg, offset + 8); // zero-initialize the high property word
            }

            if let Some(case_value) = &case.value {
                abi::emit_load_temporary_stack_slot(emitter, object_reg, 0);    // reload enum singleton pointer for backing-value initialization
                match case_value {
                    crate::types::EnumCaseValue::Int(value) => {
                        load_immediate(emitter, temp_reg, *value);              // materialize the enum int backing value
                        abi::emit_store_to_address(emitter, temp_reg, object_reg, 8); // store the int backing value in the first property slot
                        abi::emit_store_zero_to_address(emitter, object_reg, 16); // clear the metadata/high word for the int property
                    }
                    crate::types::EnumCaseValue::Str(value) => {
                        let (label, len) = data.add_string(value.as_bytes());
                        abi::emit_symbol_address(emitter, temp_reg, &label);    // materialize the enum string backing literal address
                        abi::emit_store_to_address(emitter, temp_reg, object_reg, 8); // store the string backing pointer in the first property slot
                        abi::emit_load_int_immediate(emitter, temp_reg, len as i64); // materialize the enum string backing length
                        abi::emit_store_to_address(emitter, temp_reg, object_reg, 16); // store the string backing length in the second property word
                    }
                }
            }

            abi::emit_pop_reg(emitter, result_reg);                             // pop initialized enum singleton pointer into the active integer result register
            let slot_label = crate::names::enum_case_symbol(enum_name, &case.name);
            abi::emit_store_reg_to_symbol(emitter, result_reg, &slot_label, 0); // publish the enum singleton pointer in its global slot
        }
    }
}

pub(super) fn emit_deferred_closures(
    emitter: &mut Emitter,
    data: &mut DataSection,
    ctx: &mut Context,
) {
    while !ctx.deferred_closures.is_empty() {
        let closures: Vec<_> = ctx.deferred_closures.drain(..).collect();
        for closure in closures {
            functions::emit_closure(
                emitter,
                data,
                &closure.label,
                &closure.sig,
                &closure.body,
                &ctx.functions,
                &ctx.constants,
                &ctx.interfaces,
                &ctx.classes,
                &ctx.packed_classes,
                &ctx.extern_functions,
                &ctx.extern_classes,
                &ctx.extern_globals,
            );
        }
    }
}

pub(super) fn emit_main_activation_record_push(
    emitter: &mut Emitter,
    ctx: &Context,
    cleanup_label: &str,
) {
    let prev_offset = ctx
        .activation_prev_offset
        .expect("codegen bug: missing main activation prev slot");
    let cleanup_offset = ctx
        .activation_cleanup_offset
        .expect("codegen bug: missing main activation cleanup slot");
    let frame_base_offset = ctx
        .activation_frame_base_offset
        .expect("codegen bug: missing main activation frame-base slot");

    emitter.comment("register main exception cleanup frame");
    let scratch = abi::temp_int_reg(emitter.target);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_exc_call_frame_top", 0);
    abi::store_at_offset(emitter, scratch, prev_offset);                        // save the previous call-frame pointer in the main activation record
    abi::emit_symbol_address(emitter, scratch, cleanup_label);
    abi::store_at_offset(emitter, scratch, cleanup_offset);                     // save the main cleanup callback address in the activation record
    abi::emit_copy_frame_pointer(emitter, scratch);
    abi::store_at_offset(emitter, scratch, frame_base_offset);                  // save the main frame pointer in the activation record
    abi::emit_store_zero_to_local_slot(emitter, ctx.pending_action_offset.expect("codegen bug: missing main pending-action slot")); // clear any stale finally action before running main
    abi::emit_frame_slot_address(emitter, scratch, prev_offset);                // compute the address of the main activation record's first slot
    abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
}

pub(super) fn emit_main_activation_record_pop(emitter: &mut Emitter, ctx: &Context) {
    let prev_offset = ctx
        .activation_prev_offset
        .expect("codegen bug: missing main activation prev slot");

    emitter.comment("unregister main exception cleanup frame");
    let scratch = abi::temp_int_reg(emitter.target);
    abi::load_at_offset(emitter, scratch, prev_offset);                         // reload the previous call-frame pointer from the main activation record
    abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
}

pub(super) fn emit_main_cleanup_callback(
    emitter: &mut Emitter,
    cleanup_label: &str,
    ctx: &Context,
) {
    emitter.label(cleanup_label);
    abi::emit_cleanup_callback_prologue(emitter, abi::int_arg_reg_name(emitter.target, 0));
    functions::emit_owned_local_epilogue_cleanup(emitter, ctx);
    abi::emit_cleanup_callback_epilogue(emitter);
    emitter.blank();
}

pub(crate) fn runtime_value_tag(ty: &PhpType) -> u8 {
    match ty {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed => 7,
        PhpType::Union(_) => 7,
        PhpType::Void => 8,
        PhpType::Callable | PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => 0,
    }
}

pub(crate) fn emit_box_runtime_payload_as_mixed(
    emitter: &mut Emitter,
    value_tag_reg: &str,
    value_lo_reg: &str,
    value_hi_reg: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, {}", value_tag_reg));         // x0 = runtime value tag for the mixed boxing helper
            emitter.instruction(&format!("mov x1, {}", value_lo_reg));          // x1 = low payload word for the mixed boxing helper
            emitter.instruction(&format!("mov x2, {}", value_hi_reg));          // x2 = high payload word for the mixed boxing helper
            emitter.instruction("bl __rt_mixed_from_value");                    // retain/persist the payload as needed and return a boxed mixed cell
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", value_tag_reg));        // rax = runtime value tag for the mixed boxing helper
            emitter.instruction(&format!("mov rdi, {}", value_lo_reg));         // rdi = low payload word for the mixed boxing helper
            emitter.instruction(&format!("mov rsi, {}", value_hi_reg));         // rsi = high payload word for the mixed boxing helper
            emitter.instruction("call __rt_mixed_from_value");                  // box the payload into a temporary mixed cell on x86_64
        }
    }
}

pub(crate) fn emit_box_current_value_as_mixed(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Mixed | PhpType::Union(_) => {}
        PhpType::Int | PhpType::Bool | PhpType::Void => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x1, x0");                              // move the current scalar payload into the mixed helper argument register
                emitter.instruction("mov x2, xzr");                             // scalar mixed payloads do not use a second word
                emitter.instruction(&format!("mov x0, #{}", runtime_value_tag(ty))); // materialize the static value tag for this scalar
                emitter.instruction("bl __rt_mixed_from_value");                // box the scalar payload into a mixed cell
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, rax");                            // move the current scalar payload into the mixed helper low-word register
                emitter.instruction("xor rsi, rsi");                            // scalar mixed payloads do not use a second word
                abi::emit_load_int_immediate(emitter, "rax", runtime_value_tag(ty) as i64);
                emitter.instruction("call __rt_mixed_from_value");              // box the scalar payload into a mixed cell
            }
        },
        PhpType::Float => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("fmov x1, d0");                             // move the current float bits into the mixed helper payload register
                emitter.instruction("mov x2, xzr");                             // float payloads only use the low word
                emitter.instruction("mov x0, #2");                              // runtime tag 2 = float
                emitter.instruction("bl __rt_mixed_from_value");                // box the float payload into a mixed cell
            }
            Arch::X86_64 => {
                emitter.instruction("movq rdi, xmm0");                          // move the current float bits into the mixed helper payload register
                emitter.instruction("xor rsi, rsi");                            // float payloads only use the low word
                abi::emit_load_int_immediate(emitter, "rax", 2);
                emitter.instruction("call __rt_mixed_from_value");              // box the float payload into a mixed cell
            }
        },
        PhpType::Str => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #1");                              // runtime tag 1 = string
                emitter.instruction("bl __rt_mixed_from_value");                // persist the string payload and box it into a mixed cell
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, rax");                            // move the current string pointer into the mixed helper low-word register
                emitter.instruction("mov rsi, rdx");                            // move the current string length into the mixed helper high-word register
                abi::emit_load_int_immediate(emitter, "rax", 1);
                emitter.instruction("call __rt_mixed_from_value");              // box the string payload into a mixed cell
            }
        },
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x1, x0");                          // move the current heap pointer into the mixed helper payload register
                    emitter.instruction("mov x2, xzr");                         // heap-backed payloads only use the low word
                    emitter.instruction(&format!("mov x0, #{}", runtime_value_tag(ty))); // materialize the heap payload tag for the mixed helper
                    emitter.instruction("bl __rt_mixed_from_value");            // retain the heap child and box it into a mixed cell
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rdi, rax");                        // move the current heap pointer into the mixed helper payload register
                    emitter.instruction("xor rsi, rsi");                        // heap-backed payloads only use the low word
                    abi::emit_load_int_immediate(emitter, "rax", runtime_value_tag(ty) as i64);
                    emitter.instruction("call __rt_mixed_from_value");          // box the heap child into a mixed cell
                }
            }
        }
        PhpType::Callable | PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x1, x0");                          // move the raw pointer into the mixed helper payload register
                    emitter.instruction("mov x2, xzr");                         // raw pointers only use the low word
                    emitter.instruction("mov x0, #0");                          // treat unsupported raw pointers as integer-like payloads for now
                    emitter.instruction("bl __rt_mixed_from_value");            // box the raw pointer bits into a mixed cell
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rdi, rax");                        // move the raw pointer into the mixed helper payload register
                    emitter.instruction("xor rsi, rsi");                        // raw pointers only use the low word
                    abi::emit_load_int_immediate(emitter, "rax", 0);
                    emitter.instruction("call __rt_mixed_from_value");          // box the raw pointer bits into a mixed cell
                }
            }
        }
    }
}

pub(super) fn align16(n: usize) -> usize {
    (n + 15) & !15
}

fn load_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    abi::emit_load_int_immediate(emitter, reg, value);                          // materialize the immediate through the shared target-aware helper
}
