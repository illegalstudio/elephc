//! Purpose:
//! Emits PHP `sha1($string, $binary = false)` calls.
//! Routes the data string and the optional `$binary` flag into the `__rt_sha1`
//! runtime helper, which hashes through the elephc-crypto staticlib.
//!
//! Called from:
//! - `crate::codegen_support::builtins::strings::emit()`.
//!
//! Key details:
//! - The data string is evaluated first (PHP source order) and preserved on the
//!   stack while the `$binary` flag is evaluated, then both are materialised in
//!   the `__rt_sha1` register contract (data ptr/len + flag in AArch64 x5 /
//!   x86_64 r10).
//! - Returned string pointer/length pairs are owned runtime values when the
//!   helper allocates.

use super::hash::emit_binary_flag;
use super::hash_crypto;
use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Lowers the PHP `sha1($string, $binary = false)` call.
///
/// Evaluates `args[0]` into the string ABI register pair, preserves it while the
/// optional `$binary` flag (`args[1]`, default `false`) is coerced to a 0/1
/// integer, materialises the flag in AArch64 `x5` / x86_64 `r10`, publishes the
/// elephc-crypto function pointer, and calls `__rt_sha1`. Returns `PhpType::Str`.
///
/// The returned string pointer/length pair is an owned runtime value; the caller
/// owns it.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("sha1()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the data string while evaluating the binary flag
            emit_binary_flag(args, 1, emitter, ctx, data);
            emitter.instruction("mov x5, x0");                                  // move the 0/1 binary flag into the runtime argument register on AArch64
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the data string after evaluating the binary flag
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the data string ptr/len while evaluating the binary flag on x86_64
            emit_binary_flag(args, 1, emitter, ctx, data);
            emitter.instruction("mov r10, rax");                                // move the 0/1 binary flag into the runtime argument register on x86_64
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the data string ptr/len after evaluating the binary flag
        }
    }
    hash_crypto::publish_elephc_crypto_function_pointers(emitter);
    abi::emit_call_label(emitter, "__rt_sha1");                                 // call the target-aware runtime helper that hashes through elephc-crypto and returns the PHP string
    Some(PhpType::Str)
}
