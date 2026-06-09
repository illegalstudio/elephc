//! Purpose:
//! Emits the PHP `hash_hmac($algo, $data, $key, $binary = false)` call, routed
//! through the elephc-crypto staticlib's `elephc_crypto_hmac` C entry point.
//! Marshals the three string arguments plus the binary flag into the registers
//! the `__rt_hash_hmac` runtime helper expects.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - The returned string pointer/length pair is an owned `_concat_buf`-backed
//!   runtime value, produced by the shared `__rt_digest_to_string` formatter.

use super::hash::emit_binary_flag;
use super::hash_crypto;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a PHP `hash_hmac($algo, $data, $key, $binary = false)` call as a
/// runtime helper invocation.
///
/// Arguments are evaluated in PHP source order — `$algo`, then `$data`, then
/// `$key`, then the optional `$binary` flag — and each intermediate string is
/// preserved on the temporary stack while later sub-expressions evaluate. The
/// three string arguments go through `emit_string_arg` so non-string values
/// (Mixed, int, float) are coerced into the string ABI register pair. They
/// are then delivered into the `__rt_hash_hmac` entry contract: on AArch64 the
/// algorithm pair in `x1`/`x2`, the data pair in `x3`/`x4`, the key pair in
/// `x5`/`x6`, and the binary flag in `x7`; on x86_64 the algorithm pair in
/// `rax`/`rdx`, the data pair in `rdi`/`rsi`, the key pair in `r10`/`r11`, and
/// the binary flag in `rcx`. The `elephc_crypto_hmac` entry point is published
/// into its runtime fn-pointer slot immediately before the call so only HMAC
/// programs link `-lelephc_crypto`.
///
/// # Arguments
/// - `_name`: Unused; the runtime helper handles algorithm dispatch internally.
/// - `args`: Three or four expressions — algorithm name, data string, key
///   string, and the optional `$binary` flag (defaults to `false`/`0`).
/// - `emitter`: Target-aware assembly emitter.
/// - `ctx`: Codegen context carrying variable layout and metadata.
/// - `data`: Data section for relocatable constants.
///
/// # Returns
/// `Some(PhpType::Str)` indicating the result is a PHP string.
///
/// # Side effects
/// - Clobbers caller-saved registers appropriate to each target's ABI.
/// - The runtime helper allocates a PHP string; caller owns the returned value.
/// - An unknown algorithm or a non-cryptographic checksum throws a catchable
///   `\ValueError` from the runtime.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("hash_hmac()");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- evaluate args in PHP source order, preserving each on the stack --
            super::args::emit_string_arg(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the algorithm string while evaluating the remaining arguments
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the data string while evaluating the remaining arguments
            super::args::emit_string_arg(&args[2], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the key string while evaluating the binary flag
            emit_binary_flag(args, 3, emitter, ctx, data);
            // -- deliver into the __rt_hash_hmac entry contract --
            emitter.instruction("mov x7, x0");                                  // binary flag → entry register x7
            emitter.instruction("ldp x5, x6, [sp], #16");                       // restore the key string into the key entry register pair
            emitter.instruction("ldp x3, x4, [sp], #16");                       // restore the data string into the data entry register pair
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the algorithm string into the algorithm entry register pair
        }
        Arch::X86_64 => {
            // -- evaluate args in PHP source order, preserving each on the stack --
            super::args::emit_string_arg(&args[0], emitter, ctx, data);
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the algorithm string while evaluating the remaining arguments
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the data string while evaluating the remaining arguments
            super::args::emit_string_arg(&args[2], emitter, ctx, data);
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the key string while evaluating the binary flag
            emit_binary_flag(args, 3, emitter, ctx, data);
            // -- deliver into the __rt_hash_hmac entry contract --
            emitter.instruction("mov rcx, rax");                                // binary flag → entry register rcx
            abi::emit_pop_reg_pair(emitter, "r10", "r11");                      // restore the key string into the key entry register pair
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the data string into the data entry register pair
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the algorithm string into the algorithm entry register pair
        }
    }
    hash_crypto::publish_elephc_crypto_function_pointers(emitter);
    abi::emit_call_label(emitter, "__rt_hash_hmac");                            // call the target-aware runtime helper that HMACs through elephc-crypto and returns the PHP string
    Some(PhpType::Str)
}
