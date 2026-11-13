//! Purpose:
//! Exposes native array casts and the shared debug recursion stack to Magician.
//!
//! Called from:
//! - Eval value bridge emission and RuntimeValueOps native projection hooks.
//!
//! Key details:
//! - The guard borrows an object cell and preserves the caller's previous depth.
//! - Internal object cast adapters run through the interpreter before the generic cast.

use super::*;

/// Emits C wrappers with the same native semantics on every supported target.
pub(super) fn emit(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_cast_array");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rax, rdi");                                    // adapt the C cell argument to the Mixed helper ABI
    }
    abi::emit_jump(emitter, "__rt_mixed_cast_array");

    label_c_global(emitter, "__elephc_eval_debug_guard");
    abi::emit_frame_prologue(emitter, 48);
    for (index, offset) in [(0, 8), (1, 16), (2, 24)] {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    let result = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, result, 16);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #2");                                  // restoration only needs the saved depth
            emitter.instruction("b.eq __rt_eval_debug_guard_restore");          // avoid dereferencing the absent restore receiver
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 2");                                  // restoration only needs the saved depth
            emitter.instruction("je __rt_eval_debug_guard_restore");            // avoid dereferencing the absent restore receiver
        }
    }
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_mixed_deref");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("ldr x0, [x0, #8]"),               // borrow the concrete native object pointer
        Arch::X86_64 => emitter.instruction("mov rdi, QWORD PTR [rax + 8]"),    // borrow the concrete native object pointer
    }
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_eval_debug_guard_find");
    abi::emit_load_symbol_to_reg(emitter, result, "_vd_seen_n", 0);
    abi::store_at_offset(emitter, result, 32);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::emit_call_label(emitter, "__rt_vd_seen_push");
    abi::load_at_offset(emitter, result, 32);
    abi::emit_jump(emitter, "__rt_eval_debug_guard_done");
    emitter.label("__rt_eval_debug_guard_find");
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::emit_call_label(emitter, "__rt_vd_seen_find");
    abi::emit_jump(emitter, "__rt_eval_debug_guard_done");
    emitter.label("__rt_eval_debug_guard_restore");
    abi::load_at_offset(emitter, result, 24);
    abi::emit_store_reg_to_symbol(emitter, result, "_vd_seen_n", 0);
    emitter.label("__rt_eval_debug_guard_done");
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}
