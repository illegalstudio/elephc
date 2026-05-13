//! Purpose:
//! Lowers reads and writes for undeclared properties on classes marked with
//! PHP 8.2 `#[\AllowDynamicProperties]`.
//!
//! Called from:
//! - `crate::codegen::stmt::assignments::properties::assign`
//! - `crate::codegen::expr::objects::access`
//!
//! Key details:
//! - Dynamic values are stored in a per-object hashtable as boxed `Mixed`
//!   cells at offset `8 + num_props * 16`.
//! - Allocation and cleanup are owned by object construction and deep-free
//!   runtime paths.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::types::PhpType;

/// Emit code that stores the current expression result (already produced by
/// the caller and pushed on the temporary stack) as a dynamic property of
/// the receiver. The receiver pointer must already sit in
/// `abi::int_result_reg(emitter)` when this function is called.
///
/// Layout assumptions:
/// - `dyn_slot_offset` is the byte offset of the hashtable slot from the
///   start of the object payload.
/// - The hashtable is already allocated (eager init at construction time).
pub(super) fn emit_dynamic_property_set(
    property: &str,
    val_ty: &PhpType,
    dyn_slot_offset: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.comment(&format!("dynamic property '{}' = ...", property));
    let object_reg = abi::symbol_scratch_reg(emitter);
    let boxed_reg = match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    };

    // Stash $this while we box the saved RHS value into a Mixed cell.
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); // save $this pointer across boxing helpers

    if *val_ty == PhpType::Void {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #8");                              // runtime tag 8 = null payload for Mixed boxing
                emitter.instruction("mov x1, xzr");                             // null mixed payloads carry no low word
                emitter.instruction("mov x2, xzr");                             // null mixed payloads carry no high word
                emitter.instruction("bl __rt_mixed_from_value");                // box null into an owned Mixed cell
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, 9223372036854775806");            // runtime null sentinel as the boxed null payload low word
                emitter.instruction("xor rsi, rsi");                            // null mixed payloads carry no high word
                emitter.instruction("mov rax, 8");                              // runtime tag 8 = null payload for Mixed boxing
                emitter.instruction("call __rt_mixed_from_value");              // box null into an owned Mixed cell
            }
        }
        abi::emit_pop_reg(emitter, object_reg); // reload $this after Mixed boxing helper
    } else {
        match val_ty {
            PhpType::Float => {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction("ldr d0, [sp, #16]");               // reload the saved float for Mixed boxing
                    }
                    Arch::X86_64 => {
                        emitter.instruction("movsd xmm0, QWORD PTR [rsp + 16]"); // reload the saved float for Mixed boxing
                    }
                }
                crate::codegen::emit_box_current_value_as_mixed(emitter, val_ty);
            }
            PhpType::Str => {
                match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction("ldp x1, x2, [sp, #16]");           // reload the saved string payload for Mixed boxing
                    }
                    Arch::X86_64 => {
                        emitter.instruction("mov rax, QWORD PTR [rsp + 16]");   // reload the saved string pointer for Mixed boxing
                        emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");   // reload the saved string length for Mixed boxing
                    }
                }
                crate::codegen::emit_box_current_value_as_mixed(emitter, val_ty);
            }
            _ => {
                abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16); // reload the saved scalar/heap value for Mixed boxing
                if !matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
                    crate::codegen::emit_box_current_value_as_mixed(emitter, val_ty);
                }
            }
        }
        abi::emit_pop_reg(emitter, object_reg); // reload $this after Mixed boxing helper
        abi::emit_release_temporary_stack(emitter, 16); // drop the saved original value after Mixed boxing
    }

    // -- now: result reg = boxed Mixed cell ptr; object_reg = $this --
    let keep_boxed_ptr = format!("mov {}, {}", boxed_reg, abi::int_result_reg(emitter));
    emitter.instruction(&keep_boxed_ptr);                                       // keep the boxed Mixed pointer in a stable scratch across hashtable setup

    let (label, key_len) = data.add_string(property.as_bytes());

    match emitter.target.arch {
        Arch::AArch64 => {
            // x0 = hash_ptr (loaded from object slot)
            // x1 = key_ptr, x2 = key_len, x3 = val_lo (mixed_ptr), x4 = val_hi (0), x5 = val_tag (7)
            emitter.instruction(&format!("ldr x0, [{}, #{}]", object_reg, dyn_slot_offset)); // load the dyn_props hashtable pointer from the receiver
            // Stash $this so we can update the slot after hash_set returns the
            // (possibly realloc'd) hashtable pointer in x0.
            abi::emit_push_reg(emitter, object_reg);                             // save $this for the post-call slot store
            abi::emit_symbol_address(emitter, "x1", &label);                     // x1 = property-name string address
            emitter.instruction(&format!("mov x2, #{}", key_len));              // x2 = property-name length
            emitter.instruction(&format!("mov x3, {}", boxed_reg));             // x3 = boxed Mixed cell pointer (value_lo)
            emitter.instruction("mov x4, xzr");                                 // x4 = value_hi (unused for Mixed)
            emitter.instruction("mov x5, #7");                                  // x5 = value tag = 7 (mixed)
            emitter.instruction("bl __rt_hash_set");                            // store entry; x0 = (possibly realloc'd) hashtable pointer
            abi::emit_pop_reg(emitter, object_reg);                              // restore $this for the dyn_props slot update
            emitter.instruction(&format!("str x0, [{}, #{}]", object_reg, dyn_slot_offset)); // write the (possibly realloc'd) hashtable pointer back into the slot
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, QWORD PTR [{} + {}]", object_reg, dyn_slot_offset)); // rdi = hashtable pointer from the receiver slot
            abi::emit_push_reg(emitter, object_reg);                             // save $this for the post-call slot store
            abi::emit_symbol_address(emitter, "rsi", &label);                    // rsi = property-name string address
            emitter.instruction(&format!("mov rdx, {}", key_len));              // rdx = property-name length
            emitter.instruction(&format!("mov rcx, {}", boxed_reg));            // rcx = boxed Mixed pointer (value_lo)
            emitter.instruction("xor r8, r8");                                  // r8  = value_hi (unused for Mixed)
            emitter.instruction("mov r9, 7");                                   // r9  = value tag = 7 (mixed)
            emitter.instruction("call __rt_hash_set");                          // store entry; rax = (possibly realloc'd) hashtable pointer
            abi::emit_pop_reg(emitter, object_reg);                              // restore $this for the dyn_props slot update
            emitter.instruction(&format!("mov QWORD PTR [{} + {}], rax", object_reg, dyn_slot_offset)); // write the (possibly realloc'd) hashtable pointer back into the slot
        }
    }
    let _ = ctx; // hash_set call inherits ABI conventions; no extra context needed yet
}

/// Emit code that loads a dynamic property from the receiver hashtable and
/// returns a `Mixed` value in the standard result registers. The receiver
/// pointer must already sit in `abi::int_result_reg(emitter)`.
pub(crate) fn emit_dynamic_property_get(
    property: &str,
    dyn_slot_offset: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("dynamic property '{}' read", property));
    let object_reg = abi::symbol_scratch_reg(emitter);
    let (label, key_len) = data.add_string(property.as_bytes());

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov {}, x0", object_reg));            // copy $this into the scratch register so we can clobber x0 for hash_get args
            emitter.instruction(&format!("ldr x0, [{}, #{}]", object_reg, dyn_slot_offset)); // x0 = hashtable pointer from the receiver slot
            abi::emit_symbol_address(emitter, "x1", &label);                     // x1 = property-name string address
            emitter.instruction(&format!("mov x2, #{}", key_len));              // x2 = property-name length
            emitter.instruction("bl __rt_hash_get");                            // x0 = found flag; x1=value_lo, x2=value_hi, x3=value_tag
            // The hash_get result is the boxed Mixed cell pointer when found,
            // or 0 when missing. PHP returns null with a notice for missing
            // dynamic property reads, so we emit a Mixed null in that case.
            let miss_label = ctx.next_label("dyn_get_miss");
            let done_label = ctx.next_label("dyn_get_done");
            emitter.instruction(&format!("cbz x0, {}", miss_label));            // if not found, jump to the null-return path
            // Found: x1 holds the Mixed cell pointer (since we stored tag=7 with value_lo=mixed_ptr).
            emitter.instruction("mov x0, x1");                                  // result = mixed cell pointer
            emitter.instruction(&format!("b {}", done_label));                  // skip the null-return path
            emitter.label(&miss_label);
            // Allocate a fresh boxed null Mixed for the missing case.
            emitter.instruction("mov x0, #8");                                  // tag 8 = null
            emitter.instruction("mov x1, xzr");                                 // value_lo unused for null
            emitter.instruction("mov x2, xzr");                                 // value_hi unused for null
            emitter.instruction("bl __rt_mixed_from_value");                    // x0 = boxed null Mixed cell pointer
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, rax", object_reg));           // copy $this into the scratch register so we can clobber rax for hash_get args
            emitter.instruction(&format!("mov rdi, QWORD PTR [{} + {}]", object_reg, dyn_slot_offset)); // rdi = hashtable pointer from the receiver slot
            abi::emit_symbol_address(emitter, "rsi", &label);                    // rsi = property-name string address
            emitter.instruction(&format!("mov rdx, {}", key_len));              // rdx = property-name length
            emitter.instruction("call __rt_hash_get");                          // rax = found flag; r12=value_lo, r13=value_hi, r14=value_tag
            let miss_label = ctx.next_label("dyn_get_miss");
            let done_label = ctx.next_label("dyn_get_done");
            emitter.instruction("test rax, rax");                               // check the hash_get found flag
            emitter.instruction(&format!("je {}", miss_label));                 // missing entries route to the null-return path
            // Found: r12 holds the Mixed cell pointer.
            emitter.instruction("mov rax, r12");                                // result = mixed cell pointer
            emitter.instruction(&format!("jmp {}", done_label));                // skip the null-return path
            emitter.label(&miss_label);
            emitter.instruction("mov rdi, 9223372036854775806");                // null sentinel low word for boxed Mixed null
            emitter.instruction("xor rsi, rsi");                                // value_hi unused for null
            emitter.instruction("mov rax, 8");                                  // tag 8 = null
            emitter.instruction("call __rt_mixed_from_value");                  // rax = boxed null Mixed cell pointer
            emitter.label(&done_label);
        }
    }

    PhpType::Mixed
}
