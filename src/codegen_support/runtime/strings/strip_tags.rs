//! Purpose:
//! Emits `__rt_strip_tags` for PHP 8.5 `strip_tags()`, including the optional
//! allow-list, for every supported target.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Ports php-src `php_strip_tags_ex(..., allow_tag_spaces=0)` and `php_tag_find`.
//! - Input is a subject pointer/length plus an allow-list pointer/length. A zero
//!   allow length means "strip every tag". Output is a fresh string in the
//!   target string-result registers, reserved through `__rt_concat_reserve`.
//! - HTML comments and PHP tags are always stripped and cannot be allow-listed.
//! - Oversized output reservations stay transient until the helper adopts them into
//!   independently owned string storage before returning.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::CONCAT_TEMP_HEAP_KIND;

mod aarch64;
mod x86_64;

/// Emits `__rt_strip_tags` for the active target.
pub fn emit_strip_tags(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_strip_tags_linux_x86_64(emitter);
        return;
    }
    aarch64::emit_strip_tags_aarch64(emitter);
}

/// Marks only heap-backed reservations as temporary so final persistence can adopt them.
pub(super) fn emit_mark_reserved_result_temporary(emitter: &mut Emitter) {
    let ready = "__rt_strip_tags_reservation_ready";
    // -- retain scratch semantics, but transfer oversized reservations to their consumer --
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_concat_buf");
            emitter.instruction("sub x10, x0, x9");                             // classify the reservation by its unsigned offset from concat scratch
            abi::emit_load_int_immediate(emitter, "x11", emitter.concat_capacity as i64);
            emitter.instruction("cmp x10, x11");                                // scratch reservations do not own a heap header
            emitter.instruction(&format!("b.lo {}", ready));                    // leave scratch storage untagged for ordinary persistence
            abi::emit_load_int_immediate(emitter, "x9", CONCAT_TEMP_HEAP_KIND as i64);
            emitter.instruction("str x9, [x0, #-8]");                           // let string persistence adopt this sole heap reservation instead of copying it
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", "_concat_buf");
            emitter.instruction("mov r11, rax");                                // preserve the output pointer while classifying its reservation
            emitter.instruction("sub r11, r10");                                // an unsigned out-of-window offset identifies owned heap fallback storage
            crate::codegen_support::abi::emit_load_int_immediate(emitter, "r9", emitter.concat_capacity as i64); // load the shared scratch capacity
            emitter.instruction("cmp r11, r9");                                 // scratch reservations do not own a heap header
            emitter.instruction(&format!("jb {}", ready));                      // leave scratch storage untagged for ordinary persistence
            abi::emit_load_int_immediate(emitter, "r10",
                crate::codegen_support::sentinels::x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND) as i64);
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // let string persistence adopt the sole heap reservation with its required kind marker
        }
    }
    emitter.label(ready);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Emits the strip-tags scanner for one target and returns the assembly text.
    fn strip_tags_asm(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_strip_tags(&mut emitter);
        emitter.output()
    }

    /// The no-allow-list fast path copies text with a sixteen-byte block and skips
    /// ordinary tags without entering the full state machine.
    #[test]
    fn strip_tags_no_allow_list_uses_the_fast_path() {
        let arm = strip_tags_asm(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("__rt_strip_tags_fast_entry:"), "{arm}");
        assert!(arm.contains("str q0, [x23], #16"), "{arm}");

        let x64 = strip_tags_asm(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x64.contains("__rt_strip_tags_fast_entry_x86:"), "{x64}");
        assert!(x64.contains("movdqu XMMWORD PTR [rbx], xmm0"), "{x64}");
    }

    /// The hot output cursor stays in a callee-saved register instead of being
    /// reloaded and stored around every retained byte.
    #[test]
    fn strip_tags_output_cursor_stays_in_a_register() {
        let arm = strip_tags_asm(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm.contains("strb w22, [x23], #1"), "{arm}");
        assert!(arm.contains("add x23, x23, x2"), "{arm}");

        let x64 = strip_tags_asm(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x64.contains("mov BYTE PTR [rbx], r15b"), "{x64}");
        assert!(x64.contains("add rbx, rcx"), "{x64}");
    }

    /// Every target adopts the reservation into independently owned storage before returning.
    #[test]
    fn strip_tags_output_is_independent_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_strip_tags(&mut emitter);
            let assembly = emitter.output();
            let (reserve, mark, save, publish, persist) = if target.arch == Arch::AArch64 {
                (
                    "bl __rt_concat_reserve",
                    "str x9, [x0, #-8]",
                    "str x0, [x29, #-40]",
                    "bl __rt_concat_publish",
                    "bl __rt_str_persist",
                )
            } else {
                (
                    "call __rt_concat_reserve",
                    "mov QWORD PTR [rax - 8], r10",
                    "mov QWORD PTR [rbp - 40], rax",
                    "call __rt_concat_publish",
                    "call __rt_str_persist",
                )
            };
            assert!(assembly.find(reserve).unwrap() < assembly.find(mark).unwrap(), "{name}");
            assert!(assembly.find(mark).unwrap() < assembly.find(save).unwrap(), "{name}");
            assert!(assembly.find(publish).unwrap() < assembly.find(persist).unwrap(), "{name}");
            assert!(assembly.contains("__rt_strip_tags_reservation_ready"), "{name}: guard scratch pointers");
        }
    }
}
