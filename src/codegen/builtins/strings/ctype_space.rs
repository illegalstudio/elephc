//! Purpose:
//! Emits PHP `ctype_space` character-class predicate calls.
//! Loads string bytes for runtime classification while returning PHP boolean results.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - PHP ctype semantics operate on byte strings and empty-string behavior must match the checker/runtime contract.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `ctype_space` builtin, which returns `true` iff every byte
/// in the argument string is a ASCII whitespace character (space, tab, newline,
/// carriage return, vertical tab, or form feed).
///
/// # Arguments
/// - `_name`: Unused; present for dispatcher uniformity.
/// - `args`: Must contain exactly one expression producing a PHP string (pointer in
///   `x1`/`rax`, length in `x2`/`rdx` on ARM64/x86_64 respectively).
/// - `emitter`: Target-specific instruction emission.
/// - `ctx`: Label generation and codegen context.
/// - `data`: Data section for relocations.
///
/// # Returns
/// `Some(PhpType::Bool)` always; `ctype_space` never returns `null`.
///
/// # ABI Notes
/// - ARM64: string pointer in `x1`, length in `x2`, result in `x0`.
/// - x86_64: string pointer in `rax`, length in `rdx`, result in `rax`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("ctype_space()");
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
            emitter.instruction(&format!("cbz x2, {}", fail_label));            // empty strings fail the ctype_space() contract
            emitter.instruction("mov x3, #0");                                  // x3 = current byte index inside the checked string
            emitter.label(&loop_label);
            emitter.instruction("cmp x3, x2");                                  // check whether the loop index reached the string length
            emitter.instruction(&format!("b.ge {}", pass_label));               // all bytes matched the whitespace predicate, so the string passes
            emitter.instruction("ldrb w4, [x1, x3]");                           // load the current byte from the string payload
            emitter.instruction("cmp w4, #32");                                 // test whether the current byte is an ASCII space
            emitter.instruction(&format!("b.eq {}", next_label));               // accept a space and advance to the next byte
            emitter.instruction("cmp w4, #9");                                  // test whether the current byte is a tab
            emitter.instruction(&format!("b.eq {}", next_label));               // accept a tab and advance to the next byte
            emitter.instruction("cmp w4, #10");                                 // test whether the current byte is a newline
            emitter.instruction(&format!("b.eq {}", next_label));               // accept a newline and advance to the next byte
            emitter.instruction("cmp w4, #13");                                 // test whether the current byte is a carriage return
            emitter.instruction(&format!("b.eq {}", next_label));               // accept a carriage return and advance to the next byte
            emitter.instruction("cmp w4, #11");                                 // test whether the current byte is a vertical tab
            emitter.instruction(&format!("b.eq {}", next_label));               // accept a vertical tab and advance to the next byte
            emitter.instruction("cmp w4, #12");                                 // test whether the current byte is a form feed
            emitter.instruction(&format!("b.ne {}", fail_label));               // fail immediately when the byte is outside the ASCII whitespace set
            emitter.label(&next_label);
            emitter.instruction("add x3, x3, #1");                              // advance the byte index after accepting the current whitespace character
            emitter.instruction(&format!("b {}", loop_label));                  // continue scanning until a byte fails or the string ends
            emitter.label(&fail_label);
            emitter.instruction("mov x0, #0");                                  // return false once an empty string or non-whitespace byte is observed
            emitter.instruction(&format!("b {}", end_label));                   // skip the success materialization after setting the false result
            emitter.label(&pass_label);
            emitter.instruction("mov x0, #1");                                  // return true once every byte in the string satisfied the whitespace predicate
        }
        Arch::X86_64 => {
            emitter.instruction("test rdx, rdx");                               // empty strings fail the ctype_space() contract
            emitter.instruction(&format!("je {}", fail_label));                 // jump to the false result when the checked string is empty
            emitter.instruction("xor rcx, rcx");                                // rcx = current byte index inside the checked string
            emitter.label(&loop_label);
            emitter.instruction("cmp rcx, rdx");                                // check whether the loop index reached the string length
            emitter.instruction(&format!("jge {}", pass_label));                // all bytes matched the whitespace predicate, so the string passes
            emitter.instruction("movzx r8d, BYTE PTR [rax + rcx]");             // load the current byte from the string payload into a zero-extended scratch register
            emitter.instruction("cmp r8d, 32");                                 // test whether the current byte is an ASCII space
            emitter.instruction(&format!("je {}", next_label));                 // accept a space and advance to the next byte
            emitter.instruction("cmp r8d, 9");                                  // test whether the current byte is a tab
            emitter.instruction(&format!("je {}", next_label));                 // accept a tab and advance to the next byte
            emitter.instruction("cmp r8d, 10");                                 // test whether the current byte is a newline
            emitter.instruction(&format!("je {}", next_label));                 // accept a newline and advance to the next byte
            emitter.instruction("cmp r8d, 13");                                 // test whether the current byte is a carriage return
            emitter.instruction(&format!("je {}", next_label));                 // accept a carriage return and advance to the next byte
            emitter.instruction("cmp r8d, 11");                                 // test whether the current byte is a vertical tab
            emitter.instruction(&format!("je {}", next_label));                 // accept a vertical tab and advance to the next byte
            emitter.instruction("cmp r8d, 12");                                 // test whether the current byte is a form feed
            emitter.instruction(&format!("jne {}", fail_label));                // fail immediately when the byte is outside the ASCII whitespace set
            emitter.label(&next_label);
            emitter.instruction("add rcx, 1");                                  // advance the byte index after accepting the current whitespace character
            emitter.instruction(&format!("jmp {}", loop_label));                // continue scanning until a byte fails or the string ends
            emitter.label(&fail_label);
            emitter.instruction("mov rax, 0");                                  // return false once an empty string or non-whitespace byte is observed
            emitter.instruction(&format!("jmp {}", end_label));                 // skip the success materialization after setting the false result
            emitter.label(&pass_label);
            emitter.instruction("mov rax, 1");                                  // return true once every byte in the string satisfied the whitespace predicate
        }
    }
    emitter.label(&end_label);
    Some(PhpType::Bool)
}
