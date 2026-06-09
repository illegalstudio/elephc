//! Purpose:
//! Emits PHP `hash_file($algo, $filename, $binary = false)` calls. Reads the file
//! through the shared file-read runtime, then hashes the bytes through the same
//! elephc-crypto path as `hash()`, boxing the result as `string|false`.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Returns PHP `false` (a boxed Mixed cell) when the file cannot be read, matching
//!   `file_get_contents()` failure semantics; on success returns the hex (or raw,
//!   when `$binary`) digest string. An unknown algorithm throws a catchable
//!   `\ValueError` from `__rt_hash`.

use super::file_get_contents::box_file_get_contents_result;
use crate::codegen::builtins::strings::hash_crypto;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_string, coerce_to_truthiness, emit_expr};
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `hash_file($algo, $filename, $binary = false)` builtin call.
///
/// Evaluates and preserves the algorithm name and the optional `$binary` flag,
/// reads `$filename` via `__rt_file_get_contents_maybe_url`, and — on a successful
/// read — feeds the file bytes to the shared `__rt_hash` dispatcher (persisting the
/// digest so the boxed string owns its bytes). A failed read boxes PHP `false`.
/// Returns `PhpType::Mixed` (the boxed `string|false` runtime representation).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("hash_file()");
    let fail = ctx.next_label("hash_file_fail");
    let done = ctx.next_label("hash_file_box");
    emit_string_arg(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the algorithm string (evaluated first)
            emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the filename string (PHP evaluates $filename before $binary)
            emit_binary_flag(args, emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the binary flag; all three args are now evaluated in source order
            emitter.instruction("ldp x1, x2, [sp, #16]");                       // reload the filename into the reader's string registers
            abi::emit_call_label(emitter, "__rt_file_get_contents_maybe_url");   // read the file → x1=ptr, x2=len (null on failure)
            emitter.instruction(&format!("cbz x1, {}", fail));                  // a null pointer means the file could not be read → PHP false
            emitter.instruction("mov x3, x1");                                  // move the file bytes pointer into the hash data register pair
            emitter.instruction("mov x4, x2");                                  // move the file bytes length into the hash data register pair
            emitter.instruction("ldr x5, [sp]");                                // restore the binary flag into its hash argument register
            emitter.instruction("ldp x1, x2, [sp, #32]");                       // restore the algorithm string into the algorithm register pair
            emitter.instruction("add sp, sp, #48");                             // discard the preserved algorithm, filename, and binary slots
            hash_crypto::publish_elephc_crypto_function_pointers(emitter);
            abi::emit_call_label(emitter, "__rt_hash");                         // hash the file bytes → x1=ptr, x2=len of the digest string
            abi::emit_call_label(emitter, "__rt_str_persist");                  // copy the digest to owned heap so the boxed string survives buffer reuse
            emitter.instruction(&format!("b {}", done));                        // the digest string is ready to box
            emitter.label(&fail);
            emitter.instruction("add sp, sp, #48");                             // discard the preserved algorithm, filename, and binary slots
            emitter.instruction("mov x1, #0");                                  // null string pointer → boxed PHP false
            emitter.label(&done);
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the algorithm string (evaluated first)
            emit_string_arg(&args[1], emitter, ctx, data);
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the filename string (PHP evaluates $filename before $binary)
            emit_binary_flag(args, emitter, ctx, data);
            abi::emit_push_reg(emitter, "rax");                                 // preserve the binary flag; all three args are now evaluated in source order
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // reload the filename pointer into the reader's string register
            emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");               // reload the filename length into the reader's string register
            abi::emit_call_label(emitter, "__rt_file_get_contents_maybe_url");   // read the file → rax=ptr, rdx=len (null on failure)
            emitter.instruction("test rax, rax");                               // a null pointer means the file could not be read → PHP false
            emitter.instruction(&format!("jz {}", fail));                       // box false when the read failed
            emitter.instruction("mov rdi, rax");                                // move the file bytes pointer into the hash data register
            emitter.instruction("mov rsi, rdx");                                // move the file bytes length into the hash data register
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // restore the binary flag into its hash argument register
            emitter.instruction("mov rax, QWORD PTR [rsp + 32]");               // restore the algorithm string pointer
            emitter.instruction("mov rdx, QWORD PTR [rsp + 40]");               // restore the algorithm string length
            emitter.instruction("add rsp, 48");                                 // discard the preserved algorithm, filename, and binary slots
            hash_crypto::publish_elephc_crypto_function_pointers(emitter);
            abi::emit_call_label(emitter, "__rt_hash");                         // hash the file bytes → rax=ptr, rdx=len of the digest string
            abi::emit_call_label(emitter, "__rt_str_persist");                  // copy the digest to owned heap so the boxed string survives buffer reuse
            emitter.instruction(&format!("jmp {}", done));                      // the digest string is ready to box
            emitter.label(&fail);
            emitter.instruction("add rsp, 48");                                 // discard the preserved algorithm, filename, and binary slots
            emitter.instruction("xor eax, eax");                                // null string pointer → boxed PHP false
            emitter.label(&done);
        }
    }
    box_file_get_contents_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Evaluates `arg` and coerces it into the string ABI register pair (mirrors
/// `builtins::strings::args::emit_string_arg`, which is private to the strings
/// module): a Mixed value is cast through `__rt_mixed_cast_string` instead of
/// leaving a boxed cell in the result register with stale string registers.
fn emit_string_arg(arg: &Expr, emitter: &mut Emitter, ctx: &mut Context, data: &mut DataSection) {
    let ty = emit_expr(arg, emitter, ctx, data);
    coerce_to_string(emitter, ctx, data, &ty);
}

/// Materialises the optional `$binary` flag (arg index 2) as a 0/1 integer in the
/// int-result register, defaulting to `0` (PHP `false`/hex output) when omitted.
fn emit_binary_flag(args: &[Expr], emitter: &mut Emitter, ctx: &mut Context, data: &mut DataSection) {
    if args.len() > 2 {
        let ty = emit_expr(&args[2], emitter, ctx, data);
        coerce_to_truthiness(emitter, ctx, &ty);
    } else {
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0); // default $binary to false (hex output) when omitted
    }
}
