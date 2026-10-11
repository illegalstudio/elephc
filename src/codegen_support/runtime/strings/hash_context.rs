//! Purpose:
//! Emits the incremental HashContext runtime helpers (`__rt_hash_init`,
//! `__rt_hash_update`, `__rt_hash_final`, `__rt_hash_copy`, `__rt_hash_ctx_free`)
//! backing PHP's `hash_init`/`hash_update`/`hash_final`/`hash_copy`. Each calls
//! the elephc-crypto incremental C ABI through a fail-closed function-pointer slot.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - THESE HELPERS ARE NO LONGER THE PHP SURFACE. PHP 8's `hash_init()` returns a
//!   `HashContext` OBJECT, which elephc models in `crate::hash_prelude`; the builtins
//!   these helpers back were renamed to `internal: true` `__elephc_hash_ctx_*` and are
//!   called only from that prelude's elephc-PHP wrappers. Everything below is unchanged
//!   by that migration except the resource id (next bullet).
//! - The `elephc_crypto_init`/`_clone` handle is still boxed as a Mixed cell
//!   (tag 9, kind 2), which is what owns it, but it NO LONGER TAKES A PHP RESOURCE ID:
//!   the object holds the cell in a property, the cell never reaches user code, and PHP
//!   counts the context in the object handle space instead. `__rt_mixed_from_value`
//!   skips its bind-if-absent step for kind 2 (see `runtime::resource_ids`).
//!   `hash_update`/`hash_final`/`hash_copy` still receive the already-unboxed raw
//!   handle (the emitter uses `emit_stream_fd_arg`).
//! - `__rt_hash_final` finalizes a *clone* of the context via `elephc_crypto_final`
//!   (the original handle stays live and owned by its Mixed box) and formats the
//!   digest through the shared `__rt_digest_to_string` (hex or raw).
//! - `__rt_hash_ctx_free` is the single destructor: `__rt_mixed_free_deep` calls
//!   it when the boxed HashContext (tag 9, kind 2) leaves scope, freeing both
//!   never-finalized and already-finalized contexts exactly once.
//! - An unknown algorithm in `hash_init` throws the same catchable `\ValueError`
//!   as `hash()`.
//! - FINALIZATION IS A ONE-WAY DOOR, and each of the three helpers enforces it.
//!   PHP 8.5 answers `hash_update()`, `hash_final()` or `hash_copy()` on an
//!   already-finalized context with a catchable
//!   `TypeError: <fn>(): Argument #1 ($context) must be a valid, non-finalized HashContext`.
//!   elephc used to keep hashing the still-live handle and return a plausible but
//!   wrong digest — a silently-wrong value, not a visible failure. Each helper now
//!   asks `elephc_crypto_is_finalized` first and throws that exact TypeError,
//!   before the context is touched, so a rejected call has no side effect on the
//!   digest state. The check is a single indirect call on a path that already
//!   makes one, and it happens in the runtime helper rather than in lowering so
//!   both architectures and every call shape are covered by one implementation.
//! - THE HANDLE STAYS ALIVE ACROSS THE THROW. `final` still finalizes a *clone*
//!   and never frees, so the guard changes only what PHP sees, never ownership:
//!   `__rt_hash_ctx_free` remains the single destructor and still runs exactly
//!   once at scope exit, including for a context whose reuse was rejected.

use crate::codegen_support::abi;
use crate::codegen_support::hash_crypto;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::data::{
    HASH_COPY_FINALIZED_CTX_MSG, HASH_FINAL_FINALIZED_CTX_MSG, HASH_INIT_UNKNOWN_ALGO_MSG,
    HASH_UPDATE_FINALIZED_CTX_MSG,
};

/// Emits the AArch64 "is this context spent?" probe.
///
/// Expects the context handle already in `x0` (the C ABI's first argument, which
/// is where all three helpers receive it). Branches to `finalized_label` when
/// `elephc_crypto_is_finalized` answers non-zero, and falls through to
/// `continue_label` otherwise.
///
/// A null slot falls through rather than throwing: the bridge is not linked, which
/// is the same condition under which the surrounding helper already skips its real
/// elephc-crypto call, and inventing a TypeError there would report a link-time
/// gap as a PHP-level type error.
fn emit_finalized_probe_aarch64(emitter: &mut Emitter, finalized_label: &str, continue_label: &str) {
    abi::emit_symbol_address(emitter, "x9", "_elephc_crypto_is_finalized_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the elephc_crypto_is_finalized entry pointer

    emitter.instruction(&format!("cbz x9, {}", continue_label));                // bridge not linked → no finalization state to consult

    emitter.instruction("blr x9");                                              // elephc_crypto_is_finalized(ctx), w0 = 1 when spent

    emitter.instruction(&format!("cbnz w0, {}", finalized_label));              // a spent context is PHP's TypeError case
}

/// Emits the x86_64 "is this context spent?" probe. Mirrors the AArch64 form; the
/// context handle is expected in `rdi` and the answer arrives in `eax`.
fn emit_finalized_probe_x86_64(emitter: &mut Emitter, finalized_label: &str, continue_label: &str) {
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_crypto_is_finalized_fn", 0); // load the elephc_crypto_is_finalized entry pointer
    emitter.instruction("test r9, r9");                                         // bridge not linked → no finalization state to consult

    emitter.instruction(&format!("jz {}", continue_label));                     // fall through to the normal path

    emitter.emit_native_bridge_call("r9", 1);                                    // call the Rust crypto bridge through the target native ABI

    emitter.instruction("test eax, eax");                                       // did the context already produce its digest?

    emitter.instruction(&format!("jnz {}", finalized_label));                   // a spent context is PHP's TypeError case
}

/// Emits the catchable `\TypeError` PHP raises for a spent HashContext.
fn emit_finalized_type_error(emitter: &mut Emitter, message_symbol: &str, message_len: usize) {
    hash_crypto::emit_throw_static_hash_exception(
        emitter,
        "_spl_type_error_class_id",
        message_symbol,
        message_len,
    );
}

/// Emits all four incremental HashContext runtime helpers for the target.
pub fn emit_hash_context(emitter: &mut Emitter) {
    emit_hash_init(emitter);
    emit_hash_update(emitter);
    emit_hash_final(emitter);
    emit_hash_copy(emitter);
    emit_hash_free(emitter);
}

/// `__rt_hash_init` — in: algo ptr/len (AArch64 x1/x2, x86_64 rax/rdx). Calls
/// `elephc_crypto_init`; an unknown algorithm (null handle) throws `\ValueError`;
/// otherwise boxes the handle as a Mixed resource (tag 9). Out: Mixed in x0/rax.
fn emit_hash_init(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_init (open an incremental HashContext) ---");
    emitter.label_global("__rt_hash_init");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // frame to preserve the link register across calls

            emitter.instruction("stp x29, x30, [sp]");                          // save frame pointer and return address

            emitter.instruction("mov x29, sp");                                 // set the frame pointer

            emitter.instruction("mov x0, x1");                                  // C ABI name_ptr = algorithm string pointer

            emitter.instruction("mov x1, x2");                                  // C ABI name_len = algorithm string length

            abi::emit_symbol_address(emitter, "x9", "_elephc_crypto_init_fn");
            emitter.instruction("ldr x9, [x9]");                                // load the elephc_crypto_init entry pointer

            emitter.instruction("cbz x9, __rt_hash_init_unknown");              // missing runtime → fail closed as unknown algorithm

            emitter.instruction("blr x9");                                      // create the context, x0 = handle (null on unknown algo)

            emitter.instruction("cbz x0, __rt_hash_init_unknown");              // null handle = unknown algorithm → ValueError

            emitter.instruction("mov x1, x0");                                  // Mixed payload = the context handle

            emitter.instruction("mov x2, #2");                                  // resource kind 2 = HashContext (stored in the high payload word)

            emitter.instruction("mov x0, #9");                                  // runtime tag 9 = resource

            emitter.instruction("bl __rt_mixed_from_value");                    // box the handle as a PHP resource

            emitter.instruction("ldp x29, x30, [sp]");                          // restore frame pointer and return address

            emitter.instruction("add sp, sp, #16");                             // release the frame

            emitter.instruction("ret");                                         // return the boxed HashContext resource

            emitter.label("__rt_hash_init_unknown");
            emitter.instruction("ldp x29, x30, [sp]");                          // restore frame before the non-returning throw

            emitter.instruction("add sp, sp, #16");                             // release the frame before throwing

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction("sub rsp, 16");                                 // keep nested calls 16-byte aligned

            emitter.instruction("mov rdi, rax");                                // C ABI name_ptr = algorithm string pointer

            emitter.instruction("mov rsi, rdx");                                // C ABI name_len = algorithm string length

            abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_crypto_init_fn", 0); // load the elephc_crypto_init entry pointer
            emitter.instruction("test r9, r9");                                 // missing runtime → fail closed as unknown algorithm

            emitter.instruction("jz __rt_hash_init_unknown_x86");               // jump to the ValueError throw when unavailable

            emitter.emit_native_bridge_call("r9", 2);                           // create the context, rax = handle (null on unknown algo)

            emitter.instruction("test rax, rax");                               // null handle = unknown algorithm

            emitter.instruction("jz __rt_hash_init_unknown_x86");               // → ValueError

            emitter.instruction("mov rdi, rax");                                // Mixed payload = the context handle

            emitter.instruction("mov esi, 2");                                  // resource kind 2 = HashContext (stored in the high payload word)

            emitter.instruction("mov eax, 9");                                  // runtime tag 9 = resource

            emitter.instruction("call __rt_mixed_from_value");                  // box the handle as a PHP resource

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return the boxed HashContext resource

            emitter.label("__rt_hash_init_unknown_x86");
            emitter.instruction("mov rsp, rbp");                                // release the frame before the non-returning throw

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer before throwing

        }
    }
    hash_crypto::emit_throw_unknown_algorithm_value_error(
        emitter,
        "_hash_init_unknown_algo_msg",
        HASH_INIT_UNKNOWN_ALGO_MSG.len(),
    );
}

/// `__rt_hash_update` — in: ctx handle + data already in C ABI registers
/// (AArch64 x0=ctx, x1=data_ptr, x2=data_len; x86_64 rdi/rsi/rdx). Rejects an
/// already-finalized context with PHP's `\TypeError`, otherwise feeds the data
/// into the context. Out: PHP `true` (x0/rax = 1).
///
/// The three incoming argument registers are spilled across the finalized probe
/// because that probe is itself a C call and takes `x0`/`rdi` as its own argument.
fn emit_hash_update(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_update (feed data into a HashContext) ---");
    emitter.label_global("__rt_hash_update");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #48");                             // frame for the link register and the spilled arguments

            emitter.instruction("stp x29, x30, [sp, #32]");                     // save frame pointer and return address

            emitter.instruction("add x29, sp, #32");                            // set the frame pointer

            emitter.instruction("stp x0, x1, [sp]");                            // spill the context handle and data pointer

            emitter.instruction("str x2, [sp, #16]");                           // spill the data length

            emit_finalized_probe_aarch64(
                emitter,
                "__rt_hash_update_finalized",
                "__rt_hash_update_live",
            );

            emitter.label("__rt_hash_update_live");
            emitter.instruction("ldp x0, x1, [sp]");                            // reload the context handle and data pointer

            emitter.instruction("ldr x2, [sp, #16]");                           // reload the data length

            abi::emit_symbol_address(emitter, "x9", "_elephc_crypto_update_fn");
            emitter.instruction("ldr x9, [x9]");                                // load the elephc_crypto_update entry pointer

            emitter.instruction("cbz x9, __rt_hash_update_done");               // missing runtime → skip (returns true)

            emitter.instruction("blr x9");                                      // elephc_crypto_update(ctx, data_ptr, data_len)

            emitter.label("__rt_hash_update_done");
            emitter.instruction("mov x0, #1");                                  // hash_update() returns true

            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore frame pointer and return address

            emitter.instruction("add sp, sp, #48");                             // release the frame

            emitter.instruction("ret");                                         // return true

            emitter.label("__rt_hash_update_finalized");
            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore frame pointer before the non-returning throw

            emitter.instruction("add sp, sp, #48");                             // release the frame before throwing

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction("sub rsp, 32");                                 // spill slots for the three arguments, 16-byte aligned

            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // spill the context handle

            emitter.instruction("mov QWORD PTR [rbp - 16], rsi");               // spill the data pointer

            emitter.instruction("mov QWORD PTR [rbp - 24], rdx");               // spill the data length

            emit_finalized_probe_x86_64(
                emitter,
                "__rt_hash_update_finalized_x86",
                "__rt_hash_update_live_x86",
            );

            emitter.label("__rt_hash_update_live_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // reload the context handle

            emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");               // reload the data pointer

            emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");               // reload the data length

            abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_crypto_update_fn", 0); // load the elephc_crypto_update entry pointer
            emitter.instruction("test r9, r9");                                 // missing runtime → skip (returns true)

            emitter.instruction("jz __rt_hash_update_done_x86");                // jump past the call when unavailable

            emitter.emit_native_bridge_call("r9", 3);                           // elephc_crypto_update(ctx, data_ptr, data_len)

            emitter.label("__rt_hash_update_done_x86");
            emitter.instruction("mov eax, 1");                                  // hash_update() returns true

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return true

            emitter.label("__rt_hash_update_finalized_x86");
            emitter.instruction("mov rsp, rbp");                                // release the frame before the non-returning throw

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer before throwing

        }
    }
    emit_finalized_type_error(
        emitter,
        "_hash_update_finalized_ctx_msg",
        HASH_UPDATE_FINALIZED_CTX_MSG.len(),
    );
}

/// `__rt_hash_final` — in: ctx handle + binary flag (AArch64 x0=ctx, x5=binary;
/// x86_64 rdi=ctx, r10=binary). Finalizes a clone of the context (the original
/// stays live and owned by its Mixed box, freed at scope exit), then formats the
/// digest as hex or raw. Out: PHP string (AArch64 x1/x2, x86_64 rax/rdx).
fn emit_hash_final(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_final (finalize a HashContext; box owns/frees it) ---");
    emitter.label_global("__rt_hash_final");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #96");                             // 64-byte digest buffer + saved flag + frame

            emitter.instruction("stp x29, x30, [sp, #80]");                     // save frame pointer and return address

            emitter.instruction("add x29, sp, #80");                            // set the frame pointer

            emitter.instruction("str x5, [sp, #72]");                           // preserve the binary flag across the C calls

            emitter.instruction("str x0, [sp, #64]");                           // preserve the context handle across the finalized probe

            emit_finalized_probe_aarch64(
                emitter,
                "__rt_hash_final_finalized",
                "__rt_hash_final_live",
            );

            emitter.label("__rt_hash_final_live");
            emitter.instruction("ldr x0, [sp, #64]");                           // reload the context handle for the C ABI first argument

            emitter.instruction("mov x1, sp");                                  // C ABI out = the 64-byte stack digest buffer

            abi::emit_symbol_address(emitter, "x9", "_elephc_crypto_final_fn");
            emitter.instruction("ldr x9, [x9]");                                // load the elephc_crypto_final entry pointer

            emitter.instruction("cbz x9, __rt_hash_final_empty");               // missing runtime → empty digest (defensive; slot published at call sites)

            emitter.instruction("blr x9");                                      // finalize+free the context, x0 = raw digest length

            emitter.instruction("b __rt_hash_final_len");                       // proceed with the real digest length

            emitter.label("__rt_hash_final_empty");
            emitter.instruction("mov x0, #0");                                  // empty digest length when the runtime is unavailable

            emitter.label("__rt_hash_final_len");
            emitter.instruction("mov x1, x0");                                  // digest length → __rt_digest_to_string length

            emitter.instruction("mov x0, sp");                                  // digest pointer = the stack buffer base

            emitter.instruction("ldr x2, [sp, #72]");                           // reload the binary flag

            emitter.instruction("bl __rt_digest_to_string");                    // format hex/raw → x1=ptr, x2=len

            emitter.instruction("ldp x29, x30, [sp, #80]");                     // restore frame pointer and return address

            emitter.instruction("add sp, sp, #96");                             // release the frame

            emitter.instruction("ret");                                         // return the digest string

            emitter.label("__rt_hash_final_finalized");
            emitter.instruction("ldp x29, x30, [sp, #80]");                     // restore frame pointer before the non-returning throw

            emitter.instruction("add sp, sp, #96");                             // release the frame before throwing

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction("sub rsp, 96");                                 // 64-byte digest buffer + saved flag (aligned)

            emitter.instruction("mov QWORD PTR [rbp - 16], r10");               // preserve the binary flag across the C calls

            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // preserve the context handle across the finalized probe

            emit_finalized_probe_x86_64(
                emitter,
                "__rt_hash_final_finalized_x86",
                "__rt_hash_final_live_x86",
            );

            emitter.label("__rt_hash_final_live_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // reload the context handle for the C ABI first argument

            emitter.instruction("mov rsi, rbp");                                // compute the digest buffer address

            emitter.instruction("sub rsi, 80");                                 // C ABI out = a 64-byte buffer within the frame

            abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_crypto_final_fn", 0); // load the elephc_crypto_final entry pointer
            emitter.instruction("test r9, r9");                                 // missing runtime → empty digest (defensive; slot published at call sites)

            emitter.instruction("jz __rt_hash_final_empty_x86");                // skip the call when the runtime is unavailable

            emitter.emit_native_bridge_call("r9", 2);                           // finalize+free the context, rax = raw digest length

            emitter.instruction("jmp __rt_hash_final_len_x86");                 // proceed with the real digest length

            emitter.label("__rt_hash_final_empty_x86");
            emitter.instruction("xor eax, eax");                                // empty digest length when the runtime is unavailable

            emitter.label("__rt_hash_final_len_x86");
            emitter.instruction("mov rsi, rax");                                // digest length → __rt_digest_to_string length

            emitter.instruction("mov rdi, rbp");                                // compute the digest buffer address again

            emitter.instruction("sub rdi, 80");                                 // digest pointer = the same 64-byte buffer

            emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");               // reload the binary flag

            emitter.instruction("call __rt_digest_to_string");                  // format hex/raw → rax=ptr, rdx=len

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return the digest string

            emitter.label("__rt_hash_final_finalized_x86");
            emitter.instruction("mov rsp, rbp");                                // release the frame before the non-returning throw

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer before throwing

        }
    }
    emit_finalized_type_error(
        emitter,
        "_hash_final_finalized_ctx_msg",
        HASH_FINAL_FINALIZED_CTX_MSG.len(),
    );
}

/// `__rt_hash_copy` — in: ctx handle (AArch64 x0, x86_64 rdi). Rejects an
/// already-finalized context with PHP's `\TypeError`, otherwise deep-clones the
/// context and boxes the new handle as a Mixed resource. Out: Mixed in x0/rax.
fn emit_hash_copy(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_copy (clone a HashContext) ---");
    emitter.label_global("__rt_hash_copy");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // frame for the link register and the spilled context handle

            emitter.instruction("stp x29, x30, [sp, #16]");                     // save frame pointer and return address

            emitter.instruction("add x29, sp, #16");                            // set the frame pointer

            emitter.instruction("str x0, [sp]");                                // spill the context handle across the finalized probe

            emit_finalized_probe_aarch64(
                emitter,
                "__rt_hash_copy_finalized",
                "__rt_hash_copy_live",
            );

            emitter.label("__rt_hash_copy_live");
            emitter.instruction("ldr x0, [sp]");                                // reload the context handle for the C ABI first argument

            abi::emit_symbol_address(emitter, "x9", "_elephc_crypto_clone_fn");
            emitter.instruction("ldr x9, [x9]");                                // load the elephc_crypto_clone entry pointer

            emitter.instruction("cbz x9, __rt_hash_copy_done");                 // missing runtime → return the unboxed handle as-is

            emitter.instruction("blr x9");                                      // clone the context, x0 = new handle

            emitter.label("__rt_hash_copy_done");
            emitter.instruction("mov x1, x0");                                  // Mixed payload = the cloned context handle

            emitter.instruction("mov x2, #2");                                  // resource kind 2 = HashContext (stored in the high payload word)

            emitter.instruction("mov x0, #9");                                  // runtime tag 9 = resource

            emitter.instruction("bl __rt_mixed_from_value");                    // box the cloned handle as a PHP resource

            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address

            emitter.instruction("add sp, sp, #32");                             // release the frame

            emitter.instruction("ret");                                         // return the boxed cloned HashContext

            emitter.label("__rt_hash_copy_finalized");
            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer before the non-returning throw

            emitter.instruction("add sp, sp, #32");                             // release the frame before throwing

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction("sub rsp, 16");                                 // keep nested calls 16-byte aligned

            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // spill the context handle across the finalized probe

            emit_finalized_probe_x86_64(
                emitter,
                "__rt_hash_copy_finalized_x86",
                "__rt_hash_copy_live_x86",
            );

            emitter.label("__rt_hash_copy_live_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // reload the context handle for the C ABI first argument

            abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_crypto_clone_fn", 0); // load the elephc_crypto_clone entry pointer
            emitter.instruction("test r9, r9");                                 // missing runtime → return the unboxed handle as-is

            emitter.instruction("jz __rt_hash_copy_done_x86");                  // skip the clone call when unavailable

            emitter.emit_native_bridge_call("r9", 1);                           // clone the context, rax = new handle

            emitter.label("__rt_hash_copy_done_x86");
            emitter.instruction("mov rdi, rax");                                // Mixed payload = the cloned context handle

            emitter.instruction("mov esi, 2");                                  // resource kind 2 = HashContext (stored in the high payload word)

            emitter.instruction("mov eax, 9");                                  // runtime tag 9 = resource

            emitter.instruction("call __rt_mixed_from_value");                  // box the cloned handle as a PHP resource

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return the boxed cloned HashContext

            emitter.label("__rt_hash_copy_finalized_x86");
            emitter.instruction("mov rsp, rbp");                                // release the frame before the non-returning throw

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer before throwing

        }
    }
    emit_finalized_type_error(
        emitter,
        "_hash_copy_finalized_ctx_msg",
        HASH_COPY_FINALIZED_CTX_MSG.len(),
    );
}

/// `__rt_hash_ctx_free` — in: ctx handle (AArch64 x0, x86_64 rdi). Frees an
/// unfinalized HashContext via `elephc_crypto_free` through the indirect slot.
/// Called by `__rt_mixed_free_deep` when a Mixed(tag=9, kind=2) cell is released.
fn emit_hash_free(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_ctx_free (free an unfinalized HashContext) ---");
    emitter.label_global("__rt_hash_ctx_free");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // frame to preserve the link register across the call

            emitter.instruction("stp x29, x30, [sp]");                          // save frame pointer and return address

            emitter.instruction("mov x29, sp");                                 // set the frame pointer

            emitter.instruction("cbz x0, __rt_hash_ctx_free_done");             // skip null handles (already finalized or never initialized)

            abi::emit_symbol_address(emitter, "x9", "_elephc_crypto_free_fn");
            emitter.instruction("ldr x9, [x9]");                                // load the elephc_crypto_free entry pointer

            emitter.instruction("cbz x9, __rt_hash_ctx_free_done");             // missing runtime → skip (defensive; slot published at call sites)

            emitter.instruction("blr x9");                                      // elephc_crypto_free(ctx) — frees the context

            emitter.label("__rt_hash_ctx_free_done");
            emitter.instruction("ldp x29, x30, [sp]");                          // restore frame pointer and return address

            emitter.instruction("add sp, sp, #16");                             // release the frame

            emitter.instruction("ret");                                         // return

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction("sub rsp, 16");                                 // keep the nested call 16-byte aligned

            emitter.instruction("test rdi, rdi");                               // skip null handles (already finalized or never initialized)

            emitter.instruction("jz __rt_hash_ctx_free_done_x86");              // jump to the return path when the handle is null

            abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_crypto_free_fn", 0); // load the elephc_crypto_free entry pointer
            emitter.instruction("test r9, r9");                                 // missing runtime → skip (defensive; slot published at call sites)

            emitter.instruction("jz __rt_hash_ctx_free_done_x86");              // jump to the return path when unavailable

            emitter.emit_native_bridge_call("r9", 1);                           // elephc_crypto_free(ctx) — frees the context

            emitter.label("__rt_hash_ctx_free_done_x86");
            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return

        }
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies the windows-x86_64 `__rt_hash_init` indirect call into
    /// `elephc_crypto_init` — a REAL native (MSx64-ABI) function reached
    /// through the `_elephc_crypto_init_fn` pointer slot — goes through the
    /// F46 native-bridge shim instead of a bare `call r9`: the fn-ptr is
    /// relocated to r11, the 32-byte MSx64 shadow space is reserved, the two
    /// SysV args (rdi=name ptr, rsi=name len) are remapped into rcx/rdx, and
    /// the call goes through r11. Without this, elephc_crypto_init (compiled
    /// to the MSx64 ABI on windows) would read its arguments from the wrong
    /// registers — the root cause of the F46 hash_*/md5_*/sha1_* cluster.
    #[test]
    fn test_windows_x86_64_hash_init_native_bridge_call_remaps_two_args() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_hash_context(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("mov r11, r9"), "expected the fn-ptr relocated off the MSx64 arg registers");
        assert!(asm.contains("sub rsp, 32"), "expected the 32-byte MSx64 shadow space reserved");
        assert!(asm.contains("mov rcx, rdi"), "expected MSx64 arg0 <- SysV arg0 (algorithm name ptr)");
        assert!(asm.contains("mov rdx, rsi"), "expected MSx64 arg1 <- SysV arg1 (algorithm name len)");
        assert!(asm.contains("call r11"), "expected the indirect call through the relocated pointer");
        assert!(asm.contains("add rsp, 32"), "expected the shadow-space scratch released");
    }

    /// Verifies linux-x86_64 emission keeps direct SysV indirect calls into
    /// elephc-crypto. There are eight calls now: init/free each make one,
    /// while update/final/copy each make a finalized-state probe followed by
    /// their operation call. The Windows-only adapter must not appear.
    #[test]
    fn test_linux_x86_64_hash_context_calls_stay_bare_call_r9() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_hash_context(&mut emitter);
        let asm = emitter.output();

        assert_eq!(asm.matches("call r9").count(), 8, "expected all 8 HashContext call sites to stay bare call r9");
        assert!(!asm.contains("mov r11"), "linux-x86_64 must not relocate any fn-ptr");
        assert!(!asm.contains("call r11"));
        // `__rt_hash_update` legitimately uses a 32-byte local frame for its
        // three spilled arguments; only the Windows callback adapter's extra
        // shadow-space sequence is forbidden here, and it is identified by
        // the relocation/call-r11 assertions above.
    }
}
