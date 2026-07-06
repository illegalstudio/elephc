//! Purpose:
//! Emits PHP `is_numeric` type predicate calls.
//! Inspects static or boxed runtime value representation and returns a PHP boolean.
//!
//! Called from:
//! - `crate::codegen_support::builtins::types::emit()`.
//!
//! Key details:
//! - Predicate behavior must match PHP sentinel, Mixed tag, and object/interface layout conventions.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits PHP `is_numeric()` for the given expression.
///
/// Dispatches on the known static type of `args[0]`:
/// - `Int` / `Float`: returns `true` immediately.
/// - `Str`: scans the string for an optional leading `-`, then digits, then an
///   optional `.` followed by more digits (at least one digit required).
/// - `Mixed` / `Union`: unboxes the runtime tag and tests it — int/float are numeric,
///   a string is run through the same scan, everything else is not numeric.
/// - All other static types: returns `false`.
///
/// # Returns
/// Always `Some(PhpType::Bool)`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_numeric()");
    let ty = emit_expr(&args[0], emitter, ctx, data);

    match ty {
        PhpType::Int | PhpType::Float => {
            // -- int and float are always numeric --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x0, #1");                          // return true for int/float types
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rax, 1");                          // return true for int/float types
                }
            }
        }
        PhpType::Str => {
            // -- scan the string operand directly (ptr/len already in the string regs) --
            emit_numeric_string_scan(emitter, ctx);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            // -- a boxed Mixed payload: unbox and dispatch on the runtime tag --
            let pass_label = ctx.next_label("isnum_mx_pass");
            let scan_label = ctx.next_label("isnum_mx_scan");
            let fail_label = ctx.next_label("isnum_mx_fail");
            let end_label = ctx.next_label("isnum_mx_end");
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // x0/rax = tag, x1/rdi = value_lo, x2/rdx = value_hi
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #0");                          // runtime tag 0 = integer
                    emitter.instruction(&format!("b.eq {}", pass_label));       // integers are numeric
                    emitter.instruction("cmp x0, #2");                          // runtime tag 2 = float
                    emitter.instruction(&format!("b.eq {}", pass_label));       // floats are numeric
                    emitter.instruction("cmp x0, #1");                          // runtime tag 1 = string
                    emitter.instruction(&format!("b.eq {}", scan_label));       // strings need the numeric-string scan
                    emitter.instruction(&format!("b {}", fail_label));          // every other payload is not numeric
                    emitter.label(&pass_label);
                    emitter.instruction("mov x0, #1");                          // return true for int/float payloads
                    emitter.instruction(&format!("b {}", end_label));           // skip the scan and failure paths
                    emitter.label(&scan_label);
                    // mixed_unbox already left the string pointer in x1 and length in x2
                    emit_numeric_string_scan(emitter, ctx);                     // result in x0
                    emitter.instruction(&format!("b {}", end_label));           // skip the failure path after scanning
                    emitter.label(&fail_label);
                    emitter.instruction("mov x0, #0");                          // return false for non-numeric payloads
                    emitter.label(&end_label);
                }
                Arch::X86_64 => {
                    emitter.instruction("cmp rax, 0");                          // runtime tag 0 = integer
                    emitter.instruction(&format!("je {}", pass_label));         // integers are numeric
                    emitter.instruction("cmp rax, 2");                          // runtime tag 2 = float
                    emitter.instruction(&format!("je {}", pass_label));         // floats are numeric
                    emitter.instruction("cmp rax, 1");                          // runtime tag 1 = string
                    emitter.instruction(&format!("je {}", scan_label));         // strings need the numeric-string scan
                    emitter.instruction(&format!("jmp {}", fail_label));        // every other payload is not numeric
                    emitter.label(&pass_label);
                    emitter.instruction("mov rax, 1");                          // return true for int/float payloads
                    emitter.instruction(&format!("jmp {}", end_label));         // skip the scan and failure paths
                    emitter.label(&scan_label);
                    emitter.instruction("mov rax, rdi");                        // move the unboxed string pointer into the scan's pointer register (length already in rdx)
                    emit_numeric_string_scan(emitter, ctx);                     // result in rax
                    emitter.instruction(&format!("jmp {}", end_label));         // skip the failure path after scanning
                    emitter.label(&fail_label);
                    emitter.instruction("mov rax, 0");                          // return false for non-numeric payloads
                    emitter.label(&end_label);
                }
            }
        }
        _ => {
            // -- all other types are not numeric --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x0, #0");                          // return false for non-numeric types
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rax, 0");                          // return false for non-numeric types
                }
            }
        }
    }

    Some(PhpType::Bool)
}

/// Emits the numeric-string scan: optional leading `-`, digits, an optional `.` with more
/// digits, requiring at least one digit. Expects the string pointer/length in the canonical
/// string registers (ARM64: `x1`/`x2`; x86_64: `rax`/`rdx`) and returns 1/0 in `x0`/`rax`.
/// Shared by the `Str` path and the Mixed string-tag path.
fn emit_numeric_string_scan(emitter: &mut Emitter, ctx: &mut Context) {
    let loop_label = ctx.next_label("isnum_loop");
    let dot_label = ctx.next_label("isnum_dot");
    let frac_loop = ctx.next_label("isnum_frac");
    let fail_label = ctx.next_label("isnum_fail");
    let pass_label = ctx.next_label("isnum_pass");
    let end_label = ctx.next_label("isnum_end");

    match emitter.target.arch {
        Arch::AArch64 => {
            // -- return false for empty string --
            emitter.instruction(&format!("cbz x2, {}", fail_label));            // empty string is not numeric
            emitter.instruction("mov x3, #0");                                  // x3 = loop index
            emitter.instruction("mov x5, #0");                                  // x5 = digit count

            // -- check for optional leading minus sign --
            emitter.instruction("ldrb w4, [x1]");                               // load first byte
            emitter.instruction("cmp w4, #45");                                 // check if '-'
            emitter.instruction(&format!("b.ne {}", loop_label));               // not minus, start digit loop
            emitter.instruction("add x3, x3, #1");                              // skip the minus sign
            emitter.instruction("cmp x3, x2");                                  // check if string is just "-"
            emitter.instruction(&format!("b.ge {}", fail_label));               // just "-" is not numeric

            // -- scan integer part: digits before optional dot --
            emitter.label(&loop_label);
            emitter.instruction("cmp x3, x2");                                  // check if index reached length
            emitter.instruction(&format!("b.ge {}", pass_label));               // end of string, check if we had digits
            emitter.instruction("ldrb w4, [x1, x3]");                           // load byte at index
            emitter.instruction("cmp w4, #46");                                 // check if '.'
            emitter.instruction(&format!("b.eq {}", dot_label));                // found dot, switch to fractional part
            emitter.instruction("sub w6, w4, #48");                             // w6 = byte - '0'
            emitter.instruction("cmp w6, #9");                                  // check if in range 0-9
            emitter.instruction(&format!("b.hi {}", fail_label));               // not a digit, fail
            emitter.instruction("add x5, x5, #1");                              // increment digit count
            emitter.instruction("add x3, x3, #1");                              // increment index
            emitter.instruction(&format!("b {}", loop_label));                  // continue loop

            // -- found a dot, scan fractional digits --
            emitter.label(&dot_label);
            emitter.instruction("add x3, x3, #1");                              // skip the dot
            emitter.label(&frac_loop);
            emitter.instruction("cmp x3, x2");                                  // check if index reached length
            emitter.instruction(&format!("b.ge {}", pass_label));               // end of string after dot
            emitter.instruction("ldrb w4, [x1, x3]");                           // load byte at index
            emitter.instruction("sub w6, w4, #48");                             // w6 = byte - '0'
            emitter.instruction("cmp w6, #9");                                  // check if in range 0-9
            emitter.instruction(&format!("b.hi {}", fail_label));               // not a digit after dot, fail
            emitter.instruction("add x5, x5, #1");                              // increment digit count
            emitter.instruction("add x3, x3, #1");                              // increment index
            emitter.instruction(&format!("b {}", frac_loop));                   // continue fractional loop

            // -- must have at least one digit to be numeric --
            emitter.label(&pass_label);
            emitter.instruction("cmp x5, #0");                                  // check if we found any digits
            emitter.instruction(&format!("b.eq {}", fail_label));               // no digits found, not numeric
            emitter.instruction("mov x0, #1");                                  // return true
            emitter.instruction(&format!("b {}", end_label));                   // jump to end

            emitter.label(&fail_label);
            emitter.instruction("mov x0, #0");                                  // return false

            emitter.label(&end_label);
        }
        Arch::X86_64 => {
            // -- return false for empty string --
            emitter.instruction("test rdx, rdx");                               // empty string is not numeric
            emitter.instruction(&format!("je {}", fail_label));                 // branch to failure when the string length is zero
            emitter.instruction("mov rcx, 0");                                  // rcx = loop index
            emitter.instruction("mov r8, 0");                                   // r8 = digit count

            // -- check for optional leading minus sign --
            emitter.instruction("movzx r9d, BYTE PTR [rax]");                   // load the first byte of the string
            emitter.instruction("cmp r9d, 45");                                 // check whether the string starts with '-'
            emitter.instruction(&format!("jne {}", loop_label));                // skip the sign handling when the first byte is not '-'
            emitter.instruction("add rcx, 1");                                  // skip the minus sign
            emitter.instruction("cmp rcx, rdx");                                // check if the string was just "-"
            emitter.instruction(&format!("jae {}", fail_label));                // just "-" is not numeric

            // -- scan integer part: digits before optional dot --
            emitter.label(&loop_label);
            emitter.instruction("cmp rcx, rdx");                                // check if the scan index reached the string length
            emitter.instruction(&format!("jae {}", pass_label));                // end of string, check whether we saw any digits
            emitter.instruction("movzx r9d, BYTE PTR [rax + rcx]");             // load the current byte
            emitter.instruction("cmp r9d, 46");                                 // check whether the current byte is '.'
            emitter.instruction(&format!("je {}", dot_label));                  // switch to fractional scanning when a dot is found
            emitter.instruction("sub r9d, 48");                                 // normalize the byte into a candidate digit value
            emitter.instruction("cmp r9d, 9");                                  // check whether the normalized digit is in the range 0-9
            emitter.instruction(&format!("ja {}", fail_label));                 // any other byte makes the string non-numeric
            emitter.instruction("add r8, 1");                                   // record that we consumed one more digit
            emitter.instruction("add rcx, 1");                                  // advance to the next byte
            emitter.instruction(&format!("jmp {}", loop_label));                // continue scanning the integer part

            // -- found a dot, scan fractional digits --
            emitter.label(&dot_label);
            emitter.instruction("add rcx, 1");                                  // skip the dot itself
            emitter.label(&frac_loop);
            emitter.instruction("cmp rcx, rdx");                                // check if the fractional scan reached the end of the string
            emitter.instruction(&format!("jae {}", pass_label));                // end of string after the dot still needs at least one digit overall
            emitter.instruction("movzx r9d, BYTE PTR [rax + rcx]");             // load the current fractional byte
            emitter.instruction("sub r9d, 48");                                 // normalize the byte into a candidate digit value
            emitter.instruction("cmp r9d, 9");                                  // check whether the normalized digit is in the range 0-9
            emitter.instruction(&format!("ja {}", fail_label));                 // any non-digit after the dot makes the string non-numeric
            emitter.instruction("add r8, 1");                                   // record that we consumed one more digit
            emitter.instruction("add rcx, 1");                                  // advance to the next byte
            emitter.instruction(&format!("jmp {}", frac_loop));                 // continue scanning the fractional part

            // -- must have at least one digit to be numeric --
            emitter.label(&pass_label);
            emitter.instruction("test r8, r8");                                 // check whether any digits were consumed in either scan phase
            emitter.instruction(&format!("je {}", fail_label));                 // reject strings like "." or "-."
            emitter.instruction("mov rax, 1");                                  // return true for a numeric-looking string
            emitter.instruction(&format!("jmp {}", end_label));                 // skip the failure path after choosing the true result

            emitter.label(&fail_label);
            emitter.instruction("mov rax, 0");                                  // return false for a non-numeric string

            emitter.label(&end_label);
        }
    }
}
