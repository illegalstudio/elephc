//! Purpose:
//! Emits PHP incremental hashing builtins `hash_init`, `hash_update`,
//! `hash_final`, `hash_copy`. A HashContext is a resource handle (Mixed tag 9),
//! produced by `hash_init`/`hash_copy` and consumed by `hash_update`/`hash_final`
//! through the elephc-crypto incremental C ABI.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Context arguments are unboxed with the shared `emit_stream_fd_arg` (tag-9
//!   resource → raw handle), the same helper `fclose` uses for streams.
//! - `hash_init` with a flags/key argument (HASH_HMAC streaming mode) is rejected
//!   by the type checker — `hash_hmac()` covers HMAC.

use super::hash::emit_binary_flag;
use super::hash_crypto;
use crate::codegen::builtins::io::stream_arg::emit_stream_fd_arg;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits `hash_init($algo)`: evaluates the algorithm name and opens an incremental
/// HashContext via `__rt_hash_init` (which throws `\ValueError` on an unknown
/// algorithm). Returns `PhpType::Mixed` (the boxed resource).
pub fn emit_init(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("hash_init()");
    // emit_string_arg coerces a Mixed algorithm argument through __rt_mixed_cast_string,
    // so the string registers never hold a stale pair when the value is a boxed cell.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    hash_crypto::publish_elephc_crypto_function_pointers(emitter);
    abi::emit_call_label(emitter, "__rt_hash_init");                            // open the HashContext (algo ptr/len already in the string registers)
    Some(PhpType::Mixed)
}

/// Emits `hash_update($ctx, $data)`: unboxes the context handle, evaluates the
/// data string, and feeds it via `__rt_hash_update`. Returns `PhpType::Bool`.
pub fn emit_update(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("hash_update()");
    emit_stream_fd_arg("hash_update", &args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the context handle while evaluating the data string
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("ldr x0, [sp], #16");                           // restore the context handle into the C ABI ctx register
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");                                 // preserve the context handle while evaluating the data string
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("mov rsi, rax");                                // C ABI data_ptr = the evaluated data string pointer
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the context handle into the C ABI ctx register
        }
    }
    hash_crypto::publish_elephc_crypto_function_pointers(emitter);
    abi::emit_call_label(emitter, "__rt_hash_update");                          // feed the data into the context (ctx/data already in C ABI registers)
    Some(PhpType::Bool)
}

/// Emits `hash_final($ctx, $binary = false)`: unboxes the context handle,
/// materialises the binary flag, and finalizes+frees the context via
/// `__rt_hash_final`. Returns `PhpType::Str`.
pub fn emit_final(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("hash_final()");
    emit_stream_fd_arg("hash_final", &args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the context handle while evaluating the binary flag
            emit_binary_flag(args, 1, emitter, ctx, data);
            emitter.instruction("mov x5, x0");                                  // move the 0/1 binary flag into its runtime argument register
            emitter.instruction("ldr x0, [sp], #16");                           // restore the context handle into the C ABI ctx register
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");                                 // preserve the context handle while evaluating the binary flag
            emit_binary_flag(args, 1, emitter, ctx, data);
            emitter.instruction("mov r10, rax");                                // move the 0/1 binary flag into its runtime argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the context handle into the C ABI ctx register
        }
    }
    hash_crypto::publish_elephc_crypto_function_pointers(emitter);
    abi::emit_call_label(emitter, "__rt_hash_final");                           // finalize+free the context and format the digest string
    Some(PhpType::Str)
}

/// Emits `hash_copy($ctx)`: unboxes the context handle and deep-clones it via
/// `__rt_hash_copy`. Returns `PhpType::Mixed` (a new boxed resource).
pub fn emit_copy(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("hash_copy()");
    emit_stream_fd_arg("hash_copy", &args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // move the context handle into the C ABI ctx register
    }
    hash_crypto::publish_elephc_crypto_function_pointers(emitter);
    abi::emit_call_label(emitter, "__rt_hash_copy");                            // clone the context (handle already in the C ABI ctx register)
    Some(PhpType::Mixed)
}
