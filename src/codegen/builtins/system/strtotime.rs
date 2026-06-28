//! Purpose:
//! Emits PHP `strtotime` time/date builtin calls.
//! Marshals timestamp and format arguments into runtime helpers that consult wall-clock state.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Time calls are effectful/non-deterministic and must preserve PHP scalar return conventions.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_int, emit_expr};
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `strtotime(datetime[, baseTimestamp])` builtin call.
///
/// Parses a date/time string and returns a Unix timestamp (seconds since epoch). The first
/// argument (date string) is passed as a runtime string to `__rt_strtotime`. When a second
/// `baseTimestamp` argument is supplied, relative/keyword/time-only forms are resolved
/// against it instead of the current time (this also backs `DateTime::modify()`).
///
/// # Runtime ABI
/// - **AArch64**: `x1`=string ptr, `x2`=string len, `x0`=base timestamp, `x3`=has-base flag.
/// - **x86_64**: `rdi`=string ptr, `rsi`=string len, `rdx`=base timestamp, `rcx`=has-base flag.
///
/// The base argument is evaluated first and parked on the stack (mirroring `date()`), so the
/// string-argument evaluation cannot clobber it.
///
/// # Returns
/// For `strtotime`: `PhpType::Mixed` — the parsed timestamp boxed as an integer, or boxed
/// `false` on parse failure (matching PHP's `int|false`). The runtime reports failure with
/// an `i64::MIN` sentinel so `-1` stays usable as a real pre-epoch timestamp.
/// For the internal `__elephc_strtotime_raw` alias (used by the synthetic `DateTime`
/// constructor and `modify()`): `PhpType::Int` — the raw timestamp, with the failure
/// sentinel mapped to `-1` so internal callers keep plain integer storage.
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("strtotime()");

    let has_base = args.len() >= 2;

    match emitter.target.arch {
        Arch::AArch64 => {
            if has_base {
                // -- evaluate base timestamp first, then park it across the string eval --
                let base_ty = emit_expr(&args[1], emitter, ctx, data);
                coerce_to_int(emitter, &base_ty);                               // unbox a Mixed/Union base timestamp into a raw integer
                emitter.instruction("str x0, [sp, #-16]!");                     // push the base timestamp onto the stack
                emit_expr(&args[0], emitter, ctx, data);
                // x1=string ptr, x2=string len
                emitter.instruction("ldr x0, [sp], #16");                       // pop the base timestamp into x0
                emitter.instruction("mov x3, #1");                              // signal that a base timestamp was provided
            } else {
                emit_expr(&args[0], emitter, ctx, data);
                // x1=string ptr, x2=string len
                emitter.instruction("mov x3, #0");                              // no base timestamp → runtime uses the current time
            }
        }
        Arch::X86_64 => {
            if has_base {
                // -- evaluate base timestamp first, then park it across the string eval --
                let base_ty = emit_expr(&args[1], emitter, ctx, data);
                coerce_to_int(emitter, &base_ty);                               // unbox a Mixed/Union base timestamp into a raw integer
                abi::emit_push_reg(emitter, "rax");                             // save the base timestamp while the string expression is evaluated
                emit_expr(&args[0], emitter, ctx, data);
                emitter.instruction("mov rdi, rax");                            // move the input string pointer into the first SysV string-argument register
                emitter.instruction("mov rsi, rdx");                            // move the input string length into the paired SysV string-argument register
                abi::emit_pop_reg(emitter, "rdx");                              // restore the base timestamp into the base-argument register
                emitter.instruction("mov rcx, 1");                              // signal that a base timestamp was provided
            } else {
                emit_expr(&args[0], emitter, ctx, data);
                emitter.instruction("mov rdi, rax");                            // move the input string pointer into the first SysV string-argument register
                emitter.instruction("mov rsi, rdx");                            // move the input string length into the paired SysV string-argument register
                emitter.instruction("xor ecx, ecx");                            // no base timestamp → runtime uses the current time
            }
        }
    }

    // -- call runtime to parse date string and return timestamp --
    abi::emit_call_label(emitter, "__rt_strtotime");                            // parse the supported date/time string formats through the target-aware runtime helper

    if name == "__elephc_strtotime_raw" {
        // Internal alias: keep a raw integer result, mapping the failure sentinel to -1
        // (the synthetic DateTime constructor and modify() store timestamps directly).
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("movz x13, #0x8000, lsl #48");              // load the i64::MIN parse-failure sentinel
                emitter.instruction("cmp x0, x13");                             // did the parse fail?
                emitter.instruction("mov x13, #-1");                            // legacy in-object failure value
                emitter.instruction("csel x0, x13, x0, eq");                    // sentinel → -1, otherwise keep the timestamp
            }
            Arch::X86_64 => {
                emitter.instruction("movabs r10, -9223372036854775808");        // load the i64::MIN parse-failure sentinel
                emitter.instruction("cmp rax, r10");                            // did the parse fail?
                emitter.instruction("mov r10, -1");                             // legacy in-object failure value
                emitter.instruction("cmove rax, r10");                          // sentinel → -1, otherwise keep the timestamp
            }
        }
        return Some(PhpType::Int);
    }

    box_parse_result(emitter, ctx);

    Some(PhpType::Mixed)
}

/// Box a raw `strtotime` result as a `Mixed` value.
///
/// Reads the raw timestamp from `x0` (ARM64) or `rax` (x86_64). The `i64::MIN`
/// parse-failure sentinel boxes as boolean `false` (`tag = 3`); every other value —
/// including `-1`, a valid pre-epoch timestamp — boxes as an integer (`tag = 0`),
/// preserving PHP's `strtotime(...) === false` contract.
///
/// Uses `ctx.next_label` to generate local branch labels unique to this invocation.
fn box_parse_result(emitter: &mut Emitter, ctx: &mut Context) {
    let ok_label = ctx.next_label("strtotime_ok");
    let end_label = ctx.next_label("strtotime_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("movz x13, #0x8000, lsl #48");                  // load the i64::MIN parse-failure sentinel
            emitter.instruction("cmp x0, x13");                                 // distinguish a parsed timestamp from the failure sentinel
            emitter.instruction(&format!("b.ne {}", ok_label));                 // box a parsed timestamp as an integer result
            emitter.instruction("mov x1, #0");                                  // false payload = 0 for the mixed bool box
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false for a failed parse
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false so -1 remains a valid timestamp value
            emitter.instruction(&format!("b {}", end_label));                   // skip the integer boxing path after the failure result
            emitter.label(&ok_label);
            emitter.instruction("mov x1, x0");                                  // move the parsed timestamp into the mixed helper payload register
            emitter.instruction("mov x2, #0");                                  // integer mixed payloads do not use a high word
            emitter.instruction("mov x0, #0");                                  // runtime tag 0 = int for parsed timestamps
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the parsed timestamp as mixed
            emitter.label(&end_label);
        }
        Arch::X86_64 => {
            emitter.instruction("movabs r10, -9223372036854775808");            // load the i64::MIN parse-failure sentinel
            emitter.instruction("cmp rax, r10");                                // distinguish a parsed timestamp from the failure sentinel
            emitter.instruction(&format!("jne {}", ok_label));                  // box a parsed timestamp as an integer result
            emitter.instruction("xor edi, edi");                                // false payload = 0 for the mixed bool box
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false for a failed parse
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false so -1 remains a valid timestamp value
            emitter.instruction(&format!("jmp {}", end_label));                 // skip the integer boxing path after the failure result
            emitter.label(&ok_label);
            emitter.instruction("mov rdi, rax");                                // move the parsed timestamp into the mixed helper payload register
            emitter.instruction("xor esi, esi");                                // integer mixed payloads do not use a high word
            emitter.instruction("xor eax, eax");                                // runtime tag 0 = int for parsed timestamps
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the parsed timestamp as mixed
            emitter.label(&end_label);
        }
    }
}
