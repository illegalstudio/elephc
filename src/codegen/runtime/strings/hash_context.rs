//! Purpose:
//! Emits the incremental HashContext runtime helpers (`__rt_hash_init`,
//! `__rt_hash_update`, `__rt_hash_final`, `__rt_hash_copy`, `__rt_hash_ctx_free`)
//! backing PHP's `hash_init`/`hash_update`/`hash_final`/`hash_copy`. Each calls
//! the elephc-crypto incremental C ABI through a fail-closed function-pointer slot.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - A `HashContext` is a resource: the `elephc_crypto_init`/`_clone` handle is
//!   boxed as a Mixed cell (tag 9, kind 2) exactly like `fopen` boxes a file
//!   descriptor. `hash_update`/`hash_final`/`hash_copy` receive the already-
//!   unboxed raw handle (the emitter uses `emit_stream_fd_arg`).
//! - `__rt_hash_final` finalizes a *clone* of the context via `elephc_crypto_final`
//!   (the original handle stays live and owned by its Mixed box) and formats the
//!   digest through the shared `__rt_digest_to_string` (hex or raw).
//! - `__rt_hash_ctx_free` is the single destructor: `__rt_mixed_free_deep` calls
//!   it when the boxed HashContext (tag 9, kind 2) leaves scope, freeing both
//!   never-finalized and already-finalized contexts exactly once.
//! - An unknown algorithm in `hash_init` throws the same catchable `\ValueError`
//!   as `hash()`.
//! - Resource model note: reusing a context after `hash_final()` — a second
//!   `hash_final()`, or a `hash_update()`/`hash_copy()` on an already-finalized
//!   handle — is memory-safe (the handle is never freed by `final`), but the
//!   result is not PHP-equivalent: PHP throws "Supplied resource is not a valid
//!   Hash Context resource", whereas elephc keeps hashing the still-live context.

use crate::codegen::abi;
use crate::codegen::builtins::hash_crypto;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime::data::HASH_INIT_UNKNOWN_ALGO_MSG;

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

            emitter.instruction("call r9");                                     // create the context, rax = handle (null on unknown algo)

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
/// (AArch64 x0=ctx, x1=data_ptr, x2=data_len; x86_64 rdi/rsi/rdx). Feeds the data
/// into the context. Out: PHP `true` (x0/rax = 1).
fn emit_hash_update(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_update (feed data into a HashContext) ---");
    emitter.label_global("__rt_hash_update");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // frame to preserve the link register across the call

            emitter.instruction("stp x29, x30, [sp]");                          // save frame pointer and return address

            emitter.instruction("mov x29, sp");                                 // set the frame pointer

            abi::emit_symbol_address(emitter, "x9", "_elephc_crypto_update_fn");
            emitter.instruction("ldr x9, [x9]");                                // load the elephc_crypto_update entry pointer

            emitter.instruction("cbz x9, __rt_hash_update_done");               // missing runtime → skip (returns true)

            emitter.instruction("blr x9");                                      // elephc_crypto_update(ctx, data_ptr, data_len)

            emitter.label("__rt_hash_update_done");
            emitter.instruction("mov x0, #1");                                  // hash_update() returns true

            emitter.instruction("ldp x29, x30, [sp]");                          // restore frame pointer and return address

            emitter.instruction("add sp, sp, #16");                             // release the frame

            emitter.instruction("ret");                                         // return true

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction("sub rsp, 16");                                 // keep the nested call 16-byte aligned

            abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_crypto_update_fn", 0); // load the elephc_crypto_update entry pointer
            emitter.instruction("test r9, r9");                                 // missing runtime → skip (returns true)

            emitter.instruction("jz __rt_hash_update_done_x86");                // jump past the call when unavailable

            emitter.instruction("call r9");                                     // elephc_crypto_update(ctx, data_ptr, data_len)

            emitter.label("__rt_hash_update_done_x86");
            emitter.instruction("mov eax, 1");                                  // hash_update() returns true

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return true

        }
    }
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

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction("sub rsp, 96");                                 // 64-byte digest buffer + saved flag (aligned)

            emitter.instruction("mov QWORD PTR [rbp - 16], r10");               // preserve the binary flag across the C calls

            emitter.instruction("mov rsi, rbp");                                // compute the digest buffer address

            emitter.instruction("sub rsi, 80");                                 // C ABI out = a 64-byte buffer within the frame

            abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_crypto_final_fn", 0); // load the elephc_crypto_final entry pointer
            emitter.instruction("test r9, r9");                                 // missing runtime → empty digest (defensive; slot published at call sites)

            emitter.instruction("jz __rt_hash_final_empty_x86");                // skip the call when the runtime is unavailable

            emitter.instruction("call r9");                                     // finalize+free the context, rax = raw digest length

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

        }
    }
}

/// `__rt_hash_copy` — in: ctx handle (AArch64 x0, x86_64 rdi). Deep-clones the
/// context and boxes the new handle as a Mixed resource. Out: Mixed in x0/rax.
fn emit_hash_copy(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_copy (clone a HashContext) ---");
    emitter.label_global("__rt_hash_copy");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // frame to preserve the link register across calls

            emitter.instruction("stp x29, x30, [sp]");                          // save frame pointer and return address

            emitter.instruction("mov x29, sp");                                 // set the frame pointer

            abi::emit_symbol_address(emitter, "x9", "_elephc_crypto_clone_fn");
            emitter.instruction("ldr x9, [x9]");                                // load the elephc_crypto_clone entry pointer

            emitter.instruction("cbz x9, __rt_hash_copy_done");                 // missing runtime → return the unboxed handle as-is

            emitter.instruction("blr x9");                                      // clone the context, x0 = new handle

            emitter.label("__rt_hash_copy_done");
            emitter.instruction("mov x1, x0");                                  // Mixed payload = the cloned context handle

            emitter.instruction("mov x2, #2");                                  // resource kind 2 = HashContext (stored in the high payload word)

            emitter.instruction("mov x0, #9");                                  // runtime tag 9 = resource

            emitter.instruction("bl __rt_mixed_from_value");                    // box the cloned handle as a PHP resource

            emitter.instruction("ldp x29, x30, [sp]");                          // restore frame pointer and return address

            emitter.instruction("add sp, sp, #16");                             // release the frame

            emitter.instruction("ret");                                         // return the boxed cloned HashContext

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction("sub rsp, 16");                                 // keep nested calls 16-byte aligned

            abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_crypto_clone_fn", 0); // load the elephc_crypto_clone entry pointer
            emitter.instruction("test r9, r9");                                 // missing runtime → return the unboxed handle as-is

            emitter.instruction("jz __rt_hash_copy_done_x86");                  // skip the clone call when unavailable

            emitter.instruction("call r9");                                     // clone the context, rax = new handle

            emitter.label("__rt_hash_copy_done_x86");
            emitter.instruction("mov rdi, rax");                                // Mixed payload = the cloned context handle

            emitter.instruction("mov esi, 2");                                  // resource kind 2 = HashContext (stored in the high payload word)

            emitter.instruction("mov eax, 9");                                  // runtime tag 9 = resource

            emitter.instruction("call __rt_mixed_from_value");                  // box the cloned handle as a PHP resource

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return the boxed cloned HashContext

        }
    }
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

            emitter.instruction("call r9");                                     // elephc_crypto_free(ctx) — frees the context

            emitter.label("__rt_hash_ctx_free_done_x86");
            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return

        }
    }
}
