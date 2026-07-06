//! Purpose:
//! Emits PHP `ctype_alpha` character-class predicate calls.
//! Loads string bytes for runtime classification while returning PHP boolean results.
//!
//! Called from:
//! - `crate::codegen_support::builtins::strings::emit()`.
//!
//! Key details:
//! - PHP ctype semantics operate on byte strings and empty-string behavior must match the checker/runtime contract.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `ctype_alpha` builtin.
///
/// `ctype_alpha($string)` returns `true` if every byte in `$string` is an
/// ASCII letter (A-Z or a-z), and `false` otherwise. Empty strings return
/// `false` per the PHP/Checker runtime contract.
///
/// # Arguments
/// * `_name` - Unused builtin name (dispatch already performed).
/// * `args` - Must contain exactly one expression producing a string in the
///   platform string registers (x1/x2 on AArch64, rax/rdx on X86_64).
/// * `emitter` - Assembly emitter for the target architecture.
/// * `ctx` - Codegen context providing label generation and metadata.
/// * `data` - Data section for embedded literals if needed.
///
/// # Returns
/// Always returns `Some(PhpType::Bool)`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ctype_alpha()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    let loop_label = ctx.next_label("ctype_loop");
    let next_label = ctx.next_label("ctype_next");
    let fail_label = ctx.next_label("ctype_fail");
    let pass_label = ctx.next_label("ctype_pass");
    let end_label = ctx.next_label("ctype_end");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- return false for empty string --
            emitter.instruction(&format!("cbz x2, {}", fail_label));            // empty strings fail the ctype_alpha() contract
            emitter.instruction("mov x3, #0");                                  // x3 = current byte index inside the checked string
            emitter.label(&loop_label);
            emitter.instruction("cmp x3, x2");                                  // check whether the loop index reached the string length
            emitter.instruction(&format!("b.ge {}", pass_label));               // all bytes matched the alpha predicate, so the string passes
            emitter.instruction("ldrb w4, [x1, x3]");                           // load the current byte from the string payload
            emitter.instruction("sub w5, w4, #65");                             // normalize the byte against 'A' for the upper-case range check
            emitter.instruction("cmp w5, #25");                                 // test whether the normalized byte falls inside A-Z
            emitter.instruction(&format!("b.ls {}", next_label));               // accept upper-case ASCII letters and advance to the next byte
            emitter.instruction("sub w5, w4, #97");                             // normalize the byte against 'a' for the lower-case range check
            emitter.instruction("cmp w5, #25");                                 // test whether the normalized byte falls inside a-z
            emitter.instruction(&format!("b.hi {}", fail_label));               // fail immediately when the byte is outside both ASCII alpha ranges
            emitter.label(&next_label);
            emitter.instruction("add x3, x3, #1");                              // advance the byte index after accepting the current alpha character
            emitter.instruction(&format!("b {}", loop_label));                  // continue scanning until a byte fails or the string ends
            emitter.label(&fail_label);
            emitter.instruction("mov x0, #0");                                  // return false once an empty string or non-alpha byte is observed
            emitter.instruction(&format!("b {}", end_label));                   // skip the success materialization after setting the false result
            emitter.label(&pass_label);
            emitter.instruction("mov x0, #1");                                  // return true once every byte in the string satisfied the alpha predicate
        }
        Arch::X86_64 => {
            // -- return false for empty string --
            emitter.instruction("test rdx, rdx");                               // empty strings fail the ctype_alpha() contract
            emitter.instruction(&format!("je {}", fail_label));                 // jump to the false result when the checked string is empty
            emitter.instruction("xor rcx, rcx");                                // rcx = current byte index inside the checked string
            emitter.label(&loop_label);
            emitter.instruction("cmp rcx, rdx");                                // check whether the loop index reached the string length
            emitter.instruction(&format!("jge {}", pass_label));                // all bytes matched the alpha predicate, so the string passes
            emitter.instruction("movzx r8d, BYTE PTR [rax + rcx]");             // load the current byte from the string payload into a zero-extended scratch register
            emitter.instruction("mov r9d, r8d");                                // copy the current byte so the upper-case range check can normalize it in place
            emitter.instruction("sub r9d, 65");                                 // normalize the byte against 'A' for the upper-case range check
            emitter.instruction("cmp r9d, 25");                                 // test whether the normalized byte falls inside A-Z
            emitter.instruction(&format!("jbe {}", next_label));                // accept upper-case ASCII letters and advance to the next byte
            emitter.instruction("mov r9d, r8d");                                // restore the current byte before attempting the lower-case range check
            emitter.instruction("sub r9d, 97");                                 // normalize the byte against 'a' for the lower-case range check
            emitter.instruction("cmp r9d, 25");                                 // test whether the normalized byte falls inside a-z
            emitter.instruction(&format!("ja {}", fail_label));                 // fail immediately when the byte is outside both ASCII alpha ranges
            emitter.label(&next_label);
            emitter.instruction("add rcx, 1");                                  // advance the byte index after accepting the current alpha character
            emitter.instruction(&format!("jmp {}", loop_label));                // continue scanning until a byte fails or the string ends
            emitter.label(&fail_label);
            emitter.instruction("mov rax, 0");                                  // return false once an empty string or non-alpha byte is observed
            emitter.instruction(&format!("jmp {}", end_label));                 // skip the success materialization after setting the false result
            emitter.label(&pass_label);
            emitter.instruction("mov rax, 1");                                  // return true once every byte in the string satisfied the alpha predicate
        }
    }
    emitter.label(&end_label);
    Some(PhpType::Bool)
}
