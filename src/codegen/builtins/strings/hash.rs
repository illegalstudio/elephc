//! Purpose:
//! Emits PHP `hash` string transformation or formatting calls.
//! Marshals string/scalar arguments into runtime helpers that allocate returned PHP strings.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.

use super::hash_crypto;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_truthiness, emit_expr};
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a PHP `hash($algo, $data, $binary = false)` call as a runtime helper invocation.
///
/// The algorithm and data strings are evaluated first, in PHP source order
/// (`$algo` then `$data`), each through `emit_string_arg` so non-string values
/// (Mixed, int, float) are coerced into the string ABI register pair, and
/// preserved on the stack while the optional
/// `$binary` flag is evaluated and coerced to a 0/1 integer. Before the
/// `__rt_hash` call the arguments are materialised in the runtime ABI registers
/// (algo ptr/len, data ptr/len, and the binary flag in AArch64 `x5` / x86_64 `r10`). The
/// `elephc_crypto_hash` entry point is published into its runtime fn-pointer slot
/// immediately before the call so only hashing programs link `-lelephc_crypto`.
///
/// # Arguments
/// - `_name`: Unused; the runtime helper handles algorithm dispatch internally.
/// - `args`: Two or three expressions — algorithm name, data string, and the
///   optional `$binary` flag (defaults to `false`/`0` when omitted).
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
/// - An unknown algorithm throws a catchable `\ValueError` from the runtime.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("hash()");
    // hash($algo, $data, $binary) — evaluate the algo string first. emit_string_arg coerces
    // each string argument via coerce_to_string, so a Mixed value (e.g. a Mixed-typed
    // function-call result) is cast through __rt_mixed_cast_string instead of leaving a
    // boxed cell in the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the algorithm string while evaluating the data string and binary flag
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the data string (PHP evaluates $data before $binary)
            emit_binary_flag(args, 2, emitter, ctx, data);
            emitter.instruction("mov x5, x0");                                  // move the 0/1 binary flag into its runtime argument register on AArch64
            emitter.instruction("ldp x3, x4, [sp], #16");                       // restore the data string into the secondary runtime argument register pair
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the algorithm string into the primary runtime argument register pair
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the algorithm string ptr/len while evaluating the data string and binary flag
            super::args::emit_string_arg(&args[1], emitter, ctx, data);
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the data string (PHP evaluates $data before $binary)
            emit_binary_flag(args, 2, emitter, ctx, data);
            emitter.instruction("mov r10, rax");                                // move the 0/1 binary flag into its runtime argument register on x86_64
            abi::emit_pop_reg_pair(emitter, "rdi", "rsi");                      // restore the data string into the secondary x86_64 runtime argument registers
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                      // restore the algorithm string ptr/len into the primary runtime argument registers
        }
    }
    hash_crypto::publish_elephc_crypto_function_pointers(emitter);
    abi::emit_call_label(emitter, "__rt_hash");                                 // call the target-aware runtime helper that hashes through elephc-crypto and returns the PHP string
    Some(PhpType::Str)
}

/// Materialises a `$binary` flag as a 0/1 integer in the int result register,
/// defaulting to `0` (PHP `false`) when `args` has no argument at `flag_index`.
///
/// Shared by `hash()` (flag at index 2) and `md5()`/`sha1()` (flag at index 1)
/// so all three honour the same truthiness coercion for their `$binary` argument.
pub(super) fn emit_binary_flag(
    args: &[Expr],
    flag_index: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if args.len() > flag_index {
        let ty = emit_expr(&args[flag_index], emitter, ctx, data);
        coerce_to_truthiness(emitter, ctx, &ty);
    } else {
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0); // default $binary to false (hex output) when omitted
    }
}
