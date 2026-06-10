//! Purpose:
//! Emits the `__rt_hash_hmac` runtime helper that routes PHP `hash_hmac()`
//! through the elephc-crypto staticlib. Keeps PHP byte-string pointer/length
//! behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::strings`.
//!
//! Key details:
//! - `__rt_hash_hmac` calls `elephc_crypto_hmac` indirectly through the
//!   `_elephc_crypto_hmac_fn` slot (published at the call site), so the shared
//!   runtime never names elephc-crypto and non-HMAC programs do not link it.
//! - An unknown algorithm or a non-cryptographic checksum (slot null or a -1
//!   return) throws a catchable `\ValueError` through the shared clamp-style
//!   stamping sequence with the `hash_hmac()`-specific message.
//! - The raw digest is formatted into a PHP string by the shared
//!   `__rt_digest_to_string` helper (see `digest_to_string.rs`), reusing the same
//!   register contract `__rt_hash` uses.

use crate::codegen::abi;
use crate::codegen::builtins::hash_crypto;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::runtime::data::HASH_HMAC_UNKNOWN_ALGO_MSG;

/// Emits the `__rt_hash_hmac` runtime helper for the `hash_hmac()` built-in.
///
/// Input registers (the entry contract set by the emitter):
///   AArch64: x1/x2 = algorithm name ptr/len, x3/x4 = data ptr/len,
///            x5/x6 = key ptr/len, x7 = binary flag (0 = hex, non-zero = raw).
///   x86_64:  rax/rdx = algorithm name ptr/len, rdi/rsi = data ptr/len,
///            r10/r11 = key ptr/len, rcx = binary flag.
///
/// Output registers (PHP string ptr/len pair):
///   AArch64: x1 = ptr, x2 = len.
///   x86_64:  rax = ptr, rdx = len.
///
/// The C ABI orders the arguments as
/// `elephc_crypto_hmac(name,name_len,key,key_len,data,data_len,out)`, so the
/// emitter stashes every input pointer/length into scratch registers (AArch64)
/// or stack slots (x86_64) before loading the C-argument registers, keeping the
/// data-before-key entry contract clobber-free on both targets. The helper calls
/// the entry indirectly through `_elephc_crypto_hmac_fn`, throws a `\ValueError`
/// when the slot is null or the call returns -1 (unknown algo or a non-crypto
/// checksum), and otherwise formats the raw digest through `__rt_digest_to_string`.
pub fn emit_hash_hmac(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_hmac_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_hmac ---");
    emitter.label_global("__rt_hash_hmac");

    // -- set up frame: [sp,#0..64)=digest buffer, [sp,#64]=binary flag, [sp,#80]=fp/lr --
    emitter.instruction("sub sp, sp, #96");                                     // allocate 64B digest buffer + flag slot + saved fp/lr (16-byte aligned)
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set frame pointer
    emitter.instruction("str x7, [sp, #64]");                                   // save the binary flag across the clobbering C call

    // -- stash every input pair before loading the C-argument registers --
    //    The C ABI puts key before data, but the entry contract delivers data in
    //    x3/x4 and key in x5/x6, so stash all sources first to stay clobber-free.
    emitter.instruction("mov x9, x1");                                          // stash algorithm name pointer
    emitter.instruction("mov x10, x2");                                         // stash algorithm name length
    emitter.instruction("mov x11, x5");                                         // stash key pointer
    emitter.instruction("mov x12, x6");                                         // stash key length
    emitter.instruction("mov x13, x3");                                         // stash data pointer
    emitter.instruction("mov x14, x4");                                         // stash data length

    // -- marshal elephc_crypto_hmac(name,name_len,key,key_len,data,data_len,out) --
    emitter.instruction("mov x0, x9");                                          // C arg0 = algorithm name pointer
    emitter.instruction("mov x1, x10");                                         // C arg1 = algorithm name length
    emitter.instruction("mov x2, x11");                                         // C arg2 = key pointer
    emitter.instruction("mov x3, x12");                                         // C arg3 = key length
    emitter.instruction("mov x4, x13");                                         // C arg4 = data pointer
    emitter.instruction("mov x5, x14");                                         // C arg5 = data length
    emitter.instruction("add x6, sp, #0");                                      // C arg6 = stack-backed 64-byte raw-digest output buffer

    // -- call elephc_crypto_hmac indirectly through the published slot --
    abi::emit_symbol_address(emitter, "x9", "_elephc_crypto_hmac_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the published elephc_crypto_hmac function pointer
    emitter.instruction("cbz x9, __rt_hash_hmac_unknown");                      // null slot means the program never linked elephc-crypto → unknown algo
    abi::emit_call_reg(emitter, "x9");                                          // compute the raw HMAC into the stack buffer; x0 = digest length or -1

    // -- handle an unknown algorithm / non-crypto checksum (-1) before formatting --
    emitter.instruction("cmp x0, #0");                                          // did elephc_crypto_hmac reject the algorithm name?
    emitter.instruction("b.lt __rt_hash_hmac_unknown");                         // a negative length means the algorithm is unknown or a non-crypto checksum

    // -- format the raw digest into a PHP string --
    emitter.instruction("mov x1, x0");                                          // digest length argument for the shared formatter
    emitter.instruction("add x0, sp, #0");                                      // raw digest pointer argument for the shared formatter
    emitter.instruction("ldr x2, [sp, #64]");                                   // reload the binary flag for the shared formatter
    emitter.instruction("bl __rt_digest_to_string");                            // turn (ptr,len,flag) into a _concat_buf string in x1/x2
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the helper frame
    emitter.instruction("ret");                                                 // return the PHP string ptr/len in x1/x2

    // -- unknown algorithm / non-crypto checksum: throw a catchable \ValueError --
    emitter.label("__rt_hash_hmac_unknown");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address before throwing
    emitter.instruction("add sp, sp, #96");                                     // deallocate the helper frame before throwing
    hash_crypto::emit_throw_unknown_algorithm_value_error(
        emitter,
        "_hash_hmac_unknown_algo_msg",
        HASH_HMAC_UNKNOWN_ALGO_MSG.len(),
    );
}

/// Emits the x86_64 Linux variant of the `__rt_hash_hmac` runtime helper.
///
/// See [`emit_hash_hmac`] for the register contract. The entry contract delivers
/// algo in rax/rdx, data in rdi/rsi, key in r10/r11, and the binary flag in rcx.
/// Every input is stashed into rbp-relative slots before the marshal so the C
/// ABI's key-before-data order stays clobber-free, the 7th argument (the output
/// buffer) is passed on the stack at `[rsp]` with a 16-byte-aligned call, and the
/// raw digest is then formatted through `__rt_digest_to_string`. Preserves rbp.
fn emit_hash_hmac_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_hmac ---");
    emitter.label_global("__rt_hash_hmac");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the digest buffer and stashed inputs
    emitter.instruction("sub rsp, 128");                                        // reserve digest buffer + flag + stashed input slots (16-byte aligned)

    // -- stash every input before loading the C-argument registers --
    //    The C ABI puts key before data, but the entry contract delivers data in
    //    rdi/rsi and key in r10/r11, so stash all sources first to stay clobber-free.
    emitter.instruction("mov QWORD PTR [rbp - 72], rcx");                       // save the binary flag across the clobbering C call
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // stash algorithm name pointer
    emitter.instruction("mov QWORD PTR [rbp - 88], rdx");                       // stash algorithm name length
    emitter.instruction("mov QWORD PTR [rbp - 96], r10");                       // stash key pointer
    emitter.instruction("mov QWORD PTR [rbp - 104], r11");                      // stash key length
    emitter.instruction("mov QWORD PTR [rbp - 112], rdi");                      // stash data pointer
    emitter.instruction("mov QWORD PTR [rbp - 120], rsi");                      // stash data length

    // -- marshal elephc_crypto_hmac(name,name_len,key,key_len,data,data_len,out) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // C arg0 = algorithm name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 88]");                       // C arg1 = algorithm name length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 96]");                       // C arg2 = key pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 104]");                      // C arg3 = key length
    emitter.instruction("mov r8, QWORD PTR [rbp - 112]");                       // C arg4 = data pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 120]");                       // C arg5 = data length

    // -- call elephc_crypto_hmac indirectly through the published slot --
    abi::emit_load_symbol_to_reg(emitter, "rax", "_elephc_crypto_hmac_fn", 0);  // load the published elephc_crypto_hmac function pointer
    emitter.instruction("test rax, rax");                                       // a null slot means the program never linked elephc-crypto → unknown algo
    emitter.instruction("jz __rt_hash_hmac_unknown_linux_x86_64");              // throw the unknown-algorithm ValueError when the slot is null
    emitter.instruction("lea r10, [rbp - 64]");                                 // address of the stack-backed 64-byte raw-digest output buffer
    emitter.instruction("sub rsp, 16");                                         // reserve a 16-byte stack-arg slot for the 7th int arg (rsp stays 16-aligned)
    emitter.instruction("mov QWORD PTR [rsp], r10");                            // 7th arg (output buffer address) at [rsp+0]
    emitter.instruction("call rax");                                            // compute the raw HMAC into the stack buffer; rax = digest length or -1
    emitter.instruction("add rsp, 16");                                         // release the stack-arg slot

    // -- handle an unknown algorithm / non-crypto checksum (-1) before formatting --
    emitter.instruction("test rax, rax");                                       // did elephc_crypto_hmac reject the algorithm name?
    emitter.instruction("js __rt_hash_hmac_unknown_linux_x86_64");              // a negative length means the algorithm is unknown or a non-crypto checksum

    // -- format the raw digest into a PHP string --
    emitter.instruction("mov rsi, rax");                                        // digest length argument for the shared formatter
    emitter.instruction("lea rdi, [rbp - 64]");                                 // raw digest pointer argument for the shared formatter
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // reload the binary flag for the shared formatter
    emitter.instruction("call __rt_digest_to_string");                          // turn (ptr,len,flag) into a _concat_buf string in rax/rdx
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the PHP string ptr/len in rax/rdx

    // -- unknown algorithm / non-crypto checksum: throw a catchable \ValueError --
    emitter.label("__rt_hash_hmac_unknown_linux_x86_64");
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before throwing
    hash_crypto::emit_throw_unknown_algorithm_value_error(
        emitter,
        "_hash_hmac_unknown_algo_msg",
        HASH_HMAC_UNKNOWN_ALGO_MSG.len(),
    );
}
