//! Purpose:
//! Emits the `__rt_hash` runtime helper that routes PHP `hash()` through the
//! elephc-crypto staticlib. Keeps PHP byte-string pointer/length behavior and
//! target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - `__rt_hash` calls `elephc_crypto_hash` indirectly through the
//!   `_elephc_crypto_hash_fn` slot (published at the call site), so the shared
//!   runtime never names elephc-crypto and non-hashing programs do not link it.
//! - An unknown algorithm (slot null or a -1 return) throws a catchable
//!   `\ValueError` through the shared clamp-style stamping sequence.
//! - The raw digest is formatted into a PHP string by the shared
//!   `__rt_digest_to_string` helper (see `digest_to_string.rs`), which `md5()`
//!   and `sha1()` reuse through the same register contract.

use crate::codegen_support::abi;
use crate::codegen_support::hash_crypto;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::data::HASH_UNKNOWN_ALGO_MSG;

/// Emits the `__rt_hash` runtime helper for the `hash()` built-in.
///
/// Input registers:
///   AArch64: x1/x2 = algorithm name ptr/len, x3/x4 = data ptr/len,
///            x5 = binary flag (0 = hex output, non-zero = raw bytes).
///   x86_64:  rax/rdx = algorithm name ptr/len, rdi/rsi = data ptr/len,
///            r10 = binary flag.
///
/// Output registers (PHP string ptr/len pair):
///   AArch64: x1 = ptr, x2 = len.
///   x86_64:  rax = ptr, rdx = len.
///
/// Marshals the C ABI for `elephc_crypto_hash(name,name_len,data,data_len,out)`,
/// calls it indirectly through `_elephc_crypto_hash_fn`, throws a `\ValueError`
/// when the slot is null or the call returns -1, and otherwise formats the raw
/// digest through `__rt_digest_to_string`. Saves and restores fp/lr (rbp).
pub fn emit_hash(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash ---");
    emitter.label_global("__rt_hash");

    // -- set up frame: [sp,#0..64)=digest buffer, [sp,#64]=binary flag, [sp,#80]=fp/lr --
    emitter.instruction("sub sp, sp, #96");                                     // allocate 64B digest buffer + flag slot + saved fp/lr (16-byte aligned)
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set frame pointer
    emitter.instruction("str x5, [sp, #64]");                                   // save the binary flag across the clobbering C call

    // -- marshal the C ABI for elephc_crypto_hash(name,name_len,data,data_len,out) --
    emitter.instruction("mov x6, x1");                                          // stash algorithm name pointer before the argument shuffle
    emitter.instruction("mov x7, x2");                                          // stash algorithm name length before the argument shuffle
    emitter.instruction("mov x0, x6");                                          // C arg0 = algorithm name pointer
    emitter.instruction("mov x1, x7");                                          // C arg1 = algorithm name length
    emitter.instruction("mov x2, x3");                                          // C arg2 = data pointer
    emitter.instruction("mov x3, x4");                                          // C arg3 = data length
    emitter.instruction("add x4, sp, #0");                                      // C arg4 = stack-backed 64-byte raw-digest output buffer

    // -- call elephc_crypto_hash indirectly through the published slot --
    abi::emit_symbol_address(emitter, "x9", "_elephc_crypto_hash_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the published elephc_crypto_hash function pointer
    emitter.instruction("cbz x9, __rt_hash_unknown");                           // null slot means the program never linked elephc-crypto → unknown algo
    abi::emit_call_reg(emitter, "x9");                                          // compute the raw digest into the stack buffer; x0 = digest length or -1

    // -- handle an unknown algorithm (-1) before formatting --
    emitter.instruction("cmp x0, #0");                                          // did elephc_crypto_hash reject the algorithm name?
    emitter.instruction("b.lt __rt_hash_unknown");                              // a negative length means the algorithm is unknown

    // -- format the raw digest into a PHP string --
    emitter.instruction("mov x1, x0");                                          // digest length argument for the shared formatter
    emitter.instruction("add x0, sp, #0");                                      // raw digest pointer argument for the shared formatter
    emitter.instruction("ldr x2, [sp, #64]");                                   // reload the binary flag for the shared formatter
    emitter.instruction("bl __rt_digest_to_string");                            // turn (ptr,len,flag) into a _concat_buf string in x1/x2
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the helper frame
    emitter.instruction("ret");                                                 // return the PHP string ptr/len in x1/x2

    // -- unknown algorithm: throw a catchable \ValueError --
    emitter.label("__rt_hash_unknown");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address before throwing
    emitter.instruction("add sp, sp, #96");                                     // deallocate the helper frame before throwing
    hash_crypto::emit_throw_unknown_algorithm_value_error(
        emitter,
        "_hash_unknown_algo_msg",
        HASH_UNKNOWN_ALGO_MSG.len(),
    );
}

/// Emits the x86_64 Linux variant of the `__rt_hash` runtime helper.
///
/// See [`emit_hash`] for the register contract. Receives the binary flag in r10
/// (the 5th C argument register r8 is reserved for the output buffer), saves it
/// to the stack across the C call, calls `elephc_crypto_hash` indirectly, throws
/// a `\ValueError` on a null slot or -1 return, and otherwise formats the raw
/// digest through `__rt_digest_to_string`. Preserves rbp.
fn emit_hash_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash ---");
    emitter.label_global("__rt_hash");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving the digest scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the digest buffer and saved flag
    emitter.instruction("sub rsp, 96");                                         // reserve a 64-byte raw-digest buffer plus saved-flag scratch (16-byte aligned)
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // save the binary flag across the clobbering C call

    // -- marshal the C ABI for elephc_crypto_hash(name,name_len,data,data_len,out) --
    emitter.instruction("mov r8, rdi");                                         // stash the data pointer before rdi is overwritten by the algorithm name
    emitter.instruction("mov r9, rsi");                                         // stash the data length before rsi is overwritten by the algorithm name length
    emitter.instruction("mov rdi, rax");                                        // C arg0 = algorithm name pointer
    emitter.instruction("mov rsi, rdx");                                        // C arg1 = algorithm name length
    emitter.instruction("mov rdx, r8");                                         // C arg2 = data pointer
    emitter.instruction("mov rcx, r9");                                         // C arg3 = data length
    emitter.instruction("lea r8, [rbp - 64]");                                  // C arg4 = stack-backed 64-byte raw-digest output buffer

    // -- call elephc_crypto_hash indirectly through the published slot --
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_crypto_hash_fn", 0);   // load the published elephc_crypto_hash function pointer
    emitter.instruction("test r9, r9");                                         // a null slot means the program never linked elephc-crypto → unknown algo
    emitter.instruction("jz __rt_hash_unknown_linux_x86_64");                   // throw the unknown-algorithm ValueError when the slot is null
    emitter.emit_native_bridge_call("r9", 5);                                   // compute the raw digest into the stack buffer; rax = digest length or -1

    // -- handle an unknown algorithm (-1) before formatting --
    emitter.instruction("test rax, rax");                                       // did elephc_crypto_hash reject the algorithm name?
    emitter.instruction("js __rt_hash_unknown_linux_x86_64");                   // a negative length means the algorithm is unknown

    // -- format the raw digest into a PHP string --
    emitter.instruction("mov rsi, rax");                                        // digest length argument for the shared formatter
    emitter.instruction("lea rdi, [rbp - 64]");                                 // raw digest pointer argument for the shared formatter
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // reload the binary flag for the shared formatter
    emitter.instruction("call __rt_digest_to_string");                          // turn (ptr,len,flag) into a _concat_buf string in rax/rdx
    emitter.instruction("add rsp, 96");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the PHP string ptr/len in rax/rdx

    // -- unknown algorithm: throw a catchable \ValueError --
    emitter.label("__rt_hash_unknown_linux_x86_64");
    emitter.instruction("add rsp, 96");                                         // release the helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before throwing
    hash_crypto::emit_throw_unknown_algorithm_value_error(
        emitter,
        "_hash_unknown_algo_msg",
        HASH_UNKNOWN_ALGO_MSG.len(),
    );
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies the windows-x86_64 `__rt_hash` indirect call into
    /// `elephc_crypto_hash` — a REAL native (MSx64-ABI) function reached
    /// through the `_elephc_crypto_hash_fn` pointer slot — goes through the
    /// F46 native-bridge shim instead of a bare `call r9`: the fn-ptr is
    /// relocated to r11, the 5-int-arg call reserves 48 bytes (32-byte MSx64
    /// shadow space + one 8-byte slot for the 5th SysV arg, 16-byte aligned),
    /// the 5th arg (r8, the digest output buffer pointer) is staged onto the
    /// MSx64 stack BEFORE the remap's `mov r8, rdx` clobbers r8, and the
    /// first 4 SysV args are remapped into rcx/rdx/r8/r9 before the call.
    /// Without this, elephc_crypto_hash (compiled to the MSx64 ABI on
    /// windows) would read its 5 arguments from the wrong registers.
    #[test]
    fn test_windows_x86_64_hash_native_bridge_call_stages_5th_arg_before_remap() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_hash(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("mov r11, r9"), "expected the fn-ptr relocated off the MSx64 arg registers");
        assert!(asm.contains("sub rsp, 48"), "expected 32-byte shadow + one 8-byte 5th-arg slot");
        let stage_idx = asm
            .find("mov QWORD PTR [rsp + 32], r8")
            .expect("expected the 5th SysV int arg staged to the MSx64 stack slot");
        let remap_idx = asm
            .find("mov r8, rdx")
            .expect("expected the remap's mov r8, rdx (MSx64 arg2 <- SysV arg2)");
        assert!(stage_idx < remap_idx, "the 5th-arg stack stage must precede the remap's mov r8, rdx clobber");
        assert!(asm.contains("mov rcx, rdi"), "expected MSx64 arg0 <- SysV arg0");
        assert!(asm.contains("mov rdx, rsi"), "expected MSx64 arg1 <- SysV arg1");
        assert!(asm.contains("mov r9, rcx"), "expected MSx64 arg3 <- SysV arg3");
        assert!(asm.contains("call r11"), "expected the indirect call through the relocated pointer");
        assert!(asm.contains("add rsp, 48"), "expected the shadow + stack-arg scratch released");
        assert!(!asm.contains("call r9\n"), "must not leave a bare call r9 (F46: MSx64 callee reading SysV registers)");
    }

    /// Verifies linux-x86_64 emission for `__rt_hash` stays byte-identical to
    /// before the F46 native-bridge shim was introduced: the MSx64 relocation
    /// and shadow-space staging are windows-x86_64-only, so linux-x86_64 must
    /// keep the plain bare `call r9` into `elephc_crypto_hash`.
    #[test]
    fn test_linux_x86_64_hash_call_stays_bare_call_r9() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_hash(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("    call r9\n"), "expected the byte-identical bare call r9");
        assert!(!asm.contains("mov r11"), "linux-x86_64 must not relocate the fn-ptr");
        assert!(!asm.contains("sub rsp, 48"), "linux-x86_64 must not reserve MSx64 shadow space");
        assert!(!asm.contains("call r11"));
    }
}
