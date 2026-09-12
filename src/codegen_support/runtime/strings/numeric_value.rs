//! Purpose:
//! Converts bounded PHP strings into exact integer or floating-point numeric values.
//!
//! Called from:
//! - Boxed array sum and product helpers after unboxing a string entry.
//!
//! Key details:
//! - A private length-sized buffer avoids shared C-string scratch limits and reentrant aliasing.
//! - The shared PHP grammar scanner excludes libc-only spellings before conversion.
//! - Integer overflow and decimal/exponent syntax select float without rounding valid integers.

use crate::codegen_support::{abi, emit::Emitter, platform::{Arch, Platform}};

const FRAME: usize = 112;
const SOURCE: usize = 8;
const LENGTH: usize = 16;
const BUFFER: usize = 24;
const RUN: usize = 32;
const STATUS: usize = 40;
const END_INT: usize = 48;
const END_FLOAT: usize = 56;
const INTEGER: usize = 64;
const RANGE_ERROR: usize = 72;
const ERRNO: usize = 80;
const NUMBER_TAG: usize = 88;

/// Borrows C ABI pointer/length and returns tag, numeric bits and status in the Mixed-unbox registers.
/// Status zero is fully numeric, one is a numeric prefix, and two means no numeric prefix.
pub fn emit_str_numeric_value(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    let arg2 = abi::int_arg_reg_name(emitter.target, 2);
    emitter.blank();
    emitter.label_global("__rt_str_numeric_value");
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, arg0, SOURCE);
    abi::store_at_offset(emitter, arg1, LENGTH);
    abi::emit_reg_move(emitter, result, arg1);
    ins(emitter, "add x0, x0, #1", "add rax, 1");
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    abi::store_at_offset(emitter, result, BUFFER);
    abi::emit_reg_move(emitter, arg0, result);
    abi::load_at_offset(emitter, arg1, SOURCE);
    abi::load_at_offset(emitter, arg2, LENGTH);
    emitter.bl_c("memcpy");                                                     // copy exactly the PHP byte range into privately owned scratch
    terminate_and_measure(emitter);
    abi::load_at_offset(emitter, arg0, BUFFER);
    abi::emit_call_label(emitter, "__rt_php_num_scan");
    abi::store_at_offset(emitter, result, RUN);
    abi::load_at_offset(emitter, abi::secondary_scratch_reg(emitter), STATUS);
    ins(emitter, "and x10, x10, x1", "and r10, rdx");
    ins(emitter, "eor x10, x10, #1", "xor r10, 1");
    abi::store_at_offset(emitter, abi::secondary_scratch_reg(emitter), STATUS);
    ins(emitter, "ldrb w10, [x0]", "movzx r10d, BYTE PTR [rax]");
    ins(emitter, "cbz w10, __rt_str_numeric_value_none", "test r10d, r10d");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("jz __rt_str_numeric_value_none");                  // no numeric prefix contributes integer zero
    }

    // -- distinguish exact integer parsing from overflow and floating-point syntax --
    let errno_symbol = match emitter.target.platform {
        Platform::MacOS => "__error",
        Platform::Linux => "__errno_location",
        Platform::Windows => "_errno",
    };
    emitter.bl_c(errno_symbol);                                                 // access the target's thread-local errno for strtoll overflow
    abi::store_at_offset(emitter, result, ERRNO);
    ins(emitter, "str wzr, [x0]", "mov DWORD PTR [rax], 0");
    abi::load_at_offset(emitter, arg0, RUN);
    abi::emit_frame_slot_address(emitter, arg1, END_INT);
    abi::emit_load_int_immediate(emitter, arg2, 10);
    emitter.bl_c("strtoll");                                                    // retain every in-range integer bit without a double round trip
    abi::store_at_offset(emitter, result, INTEGER);
    abi::load_at_offset(emitter, abi::secondary_scratch_reg(emitter), ERRNO);
    ins(emitter, "ldr w10, [x10]", "mov r10d, DWORD PTR [r10]");
    abi::store_at_offset(emitter, abi::secondary_scratch_reg(emitter), RANGE_ERROR);
    abi::load_at_offset(emitter, arg0, RUN);
    abi::emit_frame_slot_address(emitter, arg1, END_FLOAT);
    emitter.bl_c("strtod");                                                     // parse the already clipped decimal or exponent spelling
    abi::load_at_offset(emitter, result, RANGE_ERROR);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_str_numeric_value_float");
    abi::load_at_offset(emitter, result, END_INT);
    abi::load_at_offset(emitter, abi::secondary_scratch_reg(emitter), END_FLOAT);
    ins(emitter, "cmp x0, x10", "cmp rax, r10");
    ins(emitter, "b.ne __rt_str_numeric_value_float", "jne __rt_str_numeric_value_float");
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::store_at_offset(emitter, result, NUMBER_TAG);
    abi::emit_jump(emitter, "__rt_str_numeric_value_cleanup");

    emitter.label("__rt_str_numeric_value_float");
    ins(emitter, "fmov x0, d0", "movq rax, xmm0");
    abi::store_at_offset(emitter, result, INTEGER);
    abi::emit_load_int_immediate(emitter, result, 2);
    abi::store_at_offset(emitter, result, NUMBER_TAG);
    abi::emit_jump(emitter, "__rt_str_numeric_value_cleanup");

    emitter.label("__rt_str_numeric_value_none");
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::store_at_offset(emitter, result, INTEGER);
    abi::store_at_offset(emitter, result, NUMBER_TAG);
    abi::emit_load_int_immediate(emitter, result, 2);
    abi::store_at_offset(emitter, result, STATUS);
    emitter.label("__rt_str_numeric_value_cleanup");
    abi::load_at_offset(emitter, result, BUFFER);
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    abi::load_at_offset(emitter, result, NUMBER_TAG);
    abi::load_at_offset(emitter, if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" }, INTEGER);
    abi::load_at_offset(emitter, if emitter.target.arch == Arch::AArch64 { "x2" } else { "rdx" }, STATUS);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Appends the private NUL terminator and remembers whether the PHP range contains an earlier NUL.
fn terminate_and_measure(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, scratch, LENGTH);
    ins(emitter, "strb wzr, [x0, x10]", "mov BYTE PTR [rax + r10], 0");
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), result);
    emitter.bl_c("strlen");                                                     // recognize embedded NUL bytes before the grammar scanner clips the copy
    abi::load_at_offset(emitter, scratch, LENGTH);
    ins(emitter, "cmp x0, x10", "cmp rax, r10");
    ins(emitter, "cset x0, eq", "sete al");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("movzx eax, al");                                   // keep only the bounded-length comparison result
    }
    abi::store_at_offset(emitter, result, STATUS);
}

/// Emits one equivalent numeric-parser instruction on the selected architecture.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 }); // preserve numeric parsing and lifetime behavior across native ABIs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Each target owns length-sized scratch and distinguishes exact integers from floating syntax.
    #[test]
    fn numeric_values_use_private_bounded_scratch_and_target_errno() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_str_numeric_value(&mut emitter);
            let asm = emitter.output();
            let allocate = asm.find("__rt_heap_alloc").unwrap();
            let scan = asm.find("__rt_php_num_scan").unwrap();
            let parse = asm.find(&target.extern_symbol("strtoll")).unwrap();
            let release = asm.find("__rt_heap_free_safe").unwrap();
            assert!(allocate < scan && scan < parse && parse < release, "{name}");
            assert!(asm.contains(&target.extern_symbol("strtod")), "{name}");
            let errno = if target.platform == Platform::MacOS { "__error" } else { "__errno_location" };
            assert!(asm.contains(&target.extern_symbol(errno)), "{name}");
            assert!(!asm.contains("__rt_cstr"), "{name}: no fixed-size shared scratch");
            assert!(asm.contains("__rt_str_numeric_value_none:"), "{name}");
            assert!(asm.contains("__rt_str_numeric_value_float:"), "{name}");
            if target.arch == Arch::X86_64 {
                assert!(!asm.contains("x10"), "{name}: no ARM address scratch");
                assert!(asm.contains("mov DWORD PTR [rax], 0"), "{name}");
            } else {
                assert!(asm.contains("str wzr, [x0]"), "{name}: errno is a 32-bit C integer");
            }
        }
    }
}
