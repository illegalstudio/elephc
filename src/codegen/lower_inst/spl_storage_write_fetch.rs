//! Purpose:
//! Selects trusted attached SPL storage behind an erased object or interface type.
//!
//! Called from:
//! - Typed EIR runtime-call dispatch for SplStorageHasAttachedWriteFetch.
//!
//! Key details:
//! - Only a nonzero private getter table entry admits the attached path.
//! - The probe bounds the actual class id and never calls a user offset getter.

use crate::codegen::{abi, context::FunctionContext, platform::Arch, Result};
use crate::ir::Instruction;

/// Reads the same trusted table used by runtime write fetches without boxing the receiver.
pub(super) fn lower(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ctx.load_value_to_result(super::expect_operand(inst, 0)?)?;
    let absent = ctx.next_label("spl_write_fetch_absent");
    let done = ctx.next_label("spl_write_fetch_probe_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {absent}"));              // an absent receiver has no attached storage
            ctx.emitter.instruction("ldr x9, [x0]");                            // read the immutable concrete class identity
            abi::emit_load_symbol_to_reg(ctx.emitter, "x10", "_class_iface_method_count", 0);
            ctx.emitter.instruction("cmp x9, x10");                             // negative and out-of-range identities fail the unsigned bound
            ctx.emitter.instruction(&format!("b.hs {absent}"));                 // reject before indexing the trusted table
            abi::emit_symbol_address(ctx.emitter, "x10", "_class_offsetget_forwrite_ptrs");
            ctx.emitter.instruction("ldr x9, [x10, x9, lsl #3]");               // only unchanged internal SPL getters have an entry
            ctx.emitter.instruction("cmp x9, #0");                              // distinguish a trusted selector from an ordinary user getter
            ctx.emitter.instruction("cset x0, ne");                             // return a complete boolean without executing the getter
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // an absent receiver has no attached storage
            ctx.emitter.instruction(&format!("jz {absent}"));                   // avoid dereferencing an absent receiver
            ctx.emitter.instruction("mov r9, QWORD PTR [rax]");                 // read the immutable concrete class identity
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_class_iface_method_count", 0);
            ctx.emitter.instruction("cmp r9, r10");                             // negative and out-of-range identities fail the unsigned bound
            ctx.emitter.instruction(&format!("jae {absent}"));                  // reject before indexing the trusted table
            abi::emit_symbol_address(ctx.emitter, "r10", "_class_offsetget_forwrite_ptrs");
            ctx.emitter.instruction("cmp QWORD PTR [r10 + r9 * 8], 0");         // only unchanged internal SPL getters have a trusted entry
            ctx.emitter.instruction("setne al");                                // recognize an attached selector without calling it
            ctx.emitter.instruction("movzx rax, al");                           // return the complete boolean result
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&absent);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done);
    super::store_if_result(ctx, inst)
}
