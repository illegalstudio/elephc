//! Purpose:
//! Call-site and runtime support for routing PHP `hash()` and `hash_hmac()`
//! through the elephc-crypto staticlib. Publishes the `elephc_crypto_hash` and
//! `elephc_crypto_hmac` C entry points into their runtime function-pointer slots
//! and emits the catchable `\ValueError` thrown on an unknown algorithm name (or,
//! for `hash_hmac()`, a non-cryptographic checksum).
//!
//! Called from:
//! - `crate::codegen::builtins::strings::hash::emit()` and
//!   `crate::codegen::builtins::strings::hash_hmac::emit()` (each publishes the
//!   fn pointers immediately before its `__rt_hash`/`__rt_hash_hmac` call).
//! - `crate::codegen::runtime::strings::hash::emit_hash()` and
//!   `crate::codegen::runtime::strings::hash_hmac::emit_hash_hmac()` (emit the
//!   inline unknown-algorithm `\ValueError` throw shared between both arches).
//!
//! Key details:
//! - The fn pointers are published indirectly (mirroring the `_elephc_tls_*_fn`
//!   pattern) so only programs that actually call `hash()`/`hash_hmac()` reference
//!   the elephc-crypto entry points and therefore pull in `-lelephc_crypto` at
//!   link time.
//! - The `\ValueError` throw replicates the heap-object stamping sequence used by
//!   `crate::codegen::builtins::math::clamp`. The messages live in the fixed
//!   runtime data section as `_hash_unknown_algo_msg` / `_hash_hmac_unknown_algo_msg`,
//!   so the runtime emitter references them by symbol instead of through a
//!   per-program `DataSection`.

use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};

/// Publishes the `elephc_crypto_hash` and `elephc_crypto_hmac` C entry points
/// into their runtime function-pointer slots so `__rt_hash` and `__rt_hash_hmac`
/// can call through them.
///
/// Mirrors `publish_tls_function_pointers`: it stamps each extern symbol address
/// into its slot (`_elephc_crypto_hash_fn` / `_elephc_crypto_hmac_fn`) for both
/// supported architectures. Emitting this at the call site (rather than in the
/// shared runtime) is what makes a program reference the elephc-crypto entry
/// points, so only programs that call `hash()`/`hash_hmac()` link `-lelephc_crypto`.
pub(crate) fn publish_elephc_crypto_function_pointers(emitter: &mut Emitter) {
    const ENTRIES: &[(&str, &str)] = &[
        ("elephc_crypto_hash", "_elephc_crypto_hash_fn"),
        ("elephc_crypto_hmac", "_elephc_crypto_hmac_fn"),
        ("elephc_crypto_init", "_elephc_crypto_init_fn"),
        ("elephc_crypto_update", "_elephc_crypto_update_fn"),
        ("elephc_crypto_final", "_elephc_crypto_final_fn"),
        ("elephc_crypto_clone", "_elephc_crypto_clone_fn"),
        ("elephc_crypto_free", "_elephc_crypto_free_fn"),
    ];
    match emitter.target.arch {
        Arch::AArch64 => {
            for (c_name, slot) in ENTRIES {
                let extern_sym = emitter.target.extern_symbol(c_name);
                abi::emit_extern_symbol_address(emitter, "x9", &extern_sym);
                abi::emit_symbol_address(emitter, "x10", slot);
                emitter.instruction("str x9, [x10]");                           // publish the elephc-crypto hash entry into its runtime slot
            }
        }
        Arch::X86_64 => {
            for (c_name, slot) in ENTRIES {
                let extern_sym = emitter.target.extern_symbol(c_name);
                abi::emit_extern_symbol_address(emitter, "r9", &extern_sym);
                abi::emit_store_reg_to_symbol(emitter, "r9", slot, 0);          // publish the elephc-crypto hash entry into its runtime slot
            }
        }
    }
}

/// Emits a catchable `\ValueError` for the unknown-algorithm paths of `hash()`
/// and `hash_hmac()`.
///
/// `message_symbol` names a fixed runtime data string (`_hash_unknown_algo_msg`
/// for `hash()`, `_hash_hmac_unknown_algo_msg` for `hash_hmac()`) and
/// `message_len` is its byte length, so both built-ins reuse one throw path.
/// The emitted code does not return; it branches into `__rt_throw_current` after
/// publishing the exception object into `_exc_value`. Replicates `clamp`'s
/// `\ValueError` stamping sequence (heap kind 6 object word,
/// `_spl_value_error_class_id` at `[obj+0]`, message ptr/len at `[obj+8]`/`[obj+16]`,
/// code 0 at `[obj+24]`).
pub(crate) fn emit_throw_unknown_algorithm_value_error(
    emitter: &mut Emitter,
    message_symbol: &str,
    message_len: usize,
) {
    match emitter.target.arch {
        Arch::AArch64 => emit_throw_value_error_aarch64(emitter, message_symbol, message_len),
        Arch::X86_64 => emit_throw_value_error_x86_64(emitter, message_symbol, message_len),
    }
}

/// Emits the AArch64 allocation and unwinder handoff for the `hash()` `\ValueError`.
fn emit_throw_value_error_aarch64(
    emitter: &mut Emitter,
    message_symbol: &str,
    message_len: usize,
) {
    emitter.instruction("mov x0, #32");                                         // request Throwable payload storage
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the ValueError object payload
    emitter.instruction("mov x9, #6");                                          // heap kind 6 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp allocation as a runtime object
    abi::emit_symbol_address(emitter, "x9", "_spl_value_error_class_id");
    emitter.instruction("ldr x9, [x9]");                                        // load ValueError's runtime class id for this program
    emitter.instruction("str x9, [x0]");                                        // store class id at the object header
    abi::emit_symbol_address(emitter, "x9", message_symbol);
    emitter.instruction("str x9, [x0, #8]");                                    // store static ValueError message pointer
    emitter.instruction(&format!("mov x9, #{}", message_len));                  // load static ValueError message length
    emitter.instruction("str x9, [x0, #16]");                                   // store exception message length
    emitter.instruction("str xzr, [x0, #24]");                                  // exception code defaults to zero
    abi::emit_symbol_address(emitter, "x9", "_exc_value");
    emitter.instruction("str x0, [x9]");                                        // publish the active exception object
    emitter.instruction("b __rt_throw_current");                                // enter the standard exception unwinder
}

/// Emits the Linux x86_64 allocation and unwinder handoff for the `hash()` `\ValueError`.
fn emit_throw_value_error_x86_64(
    emitter: &mut Emitter,
    message_symbol: &str,
    message_len: usize,
) {
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for exception allocation
    emitter.instruction("mov rbp, rsp");                                        // establish aligned helper frame
    emitter.instruction("sub rsp, 16");                                         // keep the nested heap allocation call 16-byte aligned
    emitter.instruction("mov rax, 32");                                         // request Throwable payload storage
    emitter.instruction("call __rt_heap_alloc");                                // allocate the ValueError object payload
    emitter.instruction("mov r10, 0x4548504c00000006");                         // x86_64 heap-kind word: HE LP magic + kind 6 object
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp allocation as a runtime object
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_value_error_class_id", 0); // load ValueError's runtime class id for this program
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store class id at the object header
    abi::emit_symbol_address(emitter, "r10", message_symbol);                   // materialize static ValueError message pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store static ValueError message pointer
    emitter.instruction(&format!("mov QWORD PTR [rax + 16], {}", message_len)); // store static ValueError message length
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // exception code defaults to zero
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);             // publish the active exception object
    emitter.instruction("mov rsp, rbp");                                        // release helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emitter.instruction("jmp __rt_throw_current");                              // enter the standard exception unwinder
}
