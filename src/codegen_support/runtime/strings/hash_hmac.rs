//! Purpose:
//! Emits the `__rt_hash_hmac` runtime helper that routes PHP `hash_hmac()`
//! through the elephc-crypto staticlib. Keeps PHP byte-string pointer/length
//! behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
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

use crate::codegen_support::abi;
use crate::codegen_support::hash_crypto;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Platform};
use crate::codegen_support::runtime::data::HASH_HMAC_UNKNOWN_ALGO_MSG;

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
    if emitter.target.platform == Platform::Windows && emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov r11, rax");                                    // relocate fn-ptr off the MSx64 arg registers
        emitter.instruction("sub rsp, 64");                                     // 32-byte shadow + three 8-byte stack-arg slots (16-aligned)
        emitter.instruction("mov QWORD PTR [rsp + 32], r8");                    // MSx64 5th arg = data_ptr (SysV r8) — save before the remap clobbers r8
        emitter.instruction("mov QWORD PTR [rsp + 40], r9");                    // MSx64 6th arg = data_len (SysV r9) — save before the remap clobbers r9
        emitter.instruction("mov QWORD PTR [rsp + 48], r10");                   // MSx64 7th arg = output-buffer address (already in r10)
        emitter.remap_sysv_args_to_platform_for_callback(4);                    // args 0-3 SysV→MSx64 (rcx←rdi, rdx←rsi, r8←rdx, r9←rcx), reverse order
        emitter.instruction("call r11");                                        // invoke elephc_crypto_hmac via the relocated pointer (MSx64 ABI)
        emitter.instruction("add rsp, 64");                                     // release shadow + stack-arg scratch
    } else {
        emitter.instruction("sub rsp, 16");                                     // reserve a 16-byte stack-arg slot for the 7th int arg (rsp stays 16-aligned)
        emitter.instruction("mov QWORD PTR [rsp], r10");                        // 7th arg (output buffer address) at [rsp+0]
        emitter.instruction("call rax");                                        // compute the raw HMAC into the stack buffer; rax = digest length or -1
        emitter.instruction("add rsp, 16");                                     // release the stack-arg slot
    }

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

#[cfg(test)]
mod tests {
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies the windows-x86_64 `__rt_hash_hmac` indirect call into
    /// `elephc_crypto_hmac` — a REAL native (MSx64-ABI) function reached
    /// through the `_elephc_crypto_hmac_fn` pointer slot — goes through the
    /// bespoke F46 7-int-arg native-bridge shim instead of a bare `call rax`:
    /// the fn-ptr is relocated to r11, 64 bytes are reserved (32-byte MSx64
    /// shadow space + three 8-byte slots for the 5th/6th/7th SysV int args,
    /// 16-byte aligned), the 5th/6th/7th args (r8/r9/r10: data_ptr, data_len,
    /// the digest output buffer pointer) are staged onto the MSx64 stack
    /// BEFORE the remap's `mov r8, rdx` clobbers r8, and the first 4 SysV
    /// args are remapped into rcx/rdx/r8/r9 before the call. Without this,
    /// elephc_crypto_hmac (compiled to the MSx64 ABI on windows) would read
    /// its 7 arguments from the wrong registers.
    #[test]
    fn test_windows_x86_64_hash_hmac_native_bridge_call_stages_7th_arg_before_remap() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_hash_hmac(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("mov r11, rax"), "expected the fn-ptr relocated off the MSx64 arg registers");
        assert!(asm.contains("sub rsp, 64"), "expected 32-byte shadow + three 8-byte 5th/6th/7th-arg slots");
        let stage_r8_idx = asm
            .find("mov QWORD PTR [rsp + 32], r8")
            .expect("expected the 5th SysV int arg (data_ptr) staged to the MSx64 stack slot");
        let stage_r9_idx = asm
            .find("mov QWORD PTR [rsp + 40], r9")
            .expect("expected the 6th SysV int arg (data_len) staged to the MSx64 stack slot");
        let stage_r10_idx = asm
            .find("mov QWORD PTR [rsp + 48], r10")
            .expect("expected the 7th arg (output buffer address) staged to the MSx64 stack slot");
        let remap_idx = asm
            .find("mov r8, rdx")
            .expect("expected the remap's mov r8, rdx (MSx64 arg2 <- SysV arg2)");
        assert!(stage_r8_idx < remap_idx, "the 5th-arg stack stage must precede the remap's mov r8, rdx clobber");
        assert!(stage_r9_idx < remap_idx, "the 6th-arg stack stage must precede the remap's mov r8, rdx clobber");
        assert!(stage_r10_idx < remap_idx, "the 7th-arg stack stage must precede the remap's mov r8, rdx clobber");
        assert!(asm.contains("mov rcx, rdi"), "expected MSx64 arg0 <- SysV arg0");
        assert!(asm.contains("mov rdx, rsi"), "expected MSx64 arg1 <- SysV arg1");
        assert!(asm.contains("mov r9, rcx"), "expected MSx64 arg3 <- SysV arg3");
        assert!(asm.contains("call r11"), "expected the indirect call through the relocated pointer");
        assert!(asm.contains("add rsp, 64"), "expected the shadow + stack-arg scratch released");
        assert!(!asm.contains("call rax\n"), "must not leave a bare call rax (F46: MSx64 callee reading SysV registers)");
    }

    /// Verifies linux-x86_64 emission for `__rt_hash_hmac` stays byte-identical
    /// to before the F46 native-bridge shim was introduced: the MSx64
    /// relocation and shadow-space staging are windows-x86_64-only, so
    /// linux-x86_64 must keep the plain bare `call rax` into
    /// `elephc_crypto_hmac` with the original 16-byte stack-arg slot.
    #[test]
    fn test_linux_x86_64_hash_hmac_call_stays_bare_call_rax() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_hash_hmac(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("    sub rsp, 16\n"), "expected the byte-identical 16-byte stack-arg slot reservation");
        assert!(asm.contains("    mov QWORD PTR [rsp], r10\n"), "expected the byte-identical 7th-arg stack store");
        assert!(asm.contains("    call rax\n"), "expected the byte-identical bare call rax");
        assert!(asm.contains("    add rsp, 16\n"), "expected the byte-identical stack-arg slot release");
        assert!(!asm.contains("mov r11"), "linux-x86_64 must not relocate the fn-ptr");
        assert!(!asm.contains("sub rsp, 64"), "linux-x86_64 must not reserve MSx64 shadow space");
        assert!(!asm.contains("call r11"));
    }
}
