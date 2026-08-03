//! Purpose:
//! Emits the `__rt_ftoa` runtime helper assembly for float-to-string conversion.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Converts a double-precision float to a PHP-compatible byte string.
///
/// # Input
/// - ARM64: `d0` holds the float value
/// - x86_64: `xmm0` holds the float value (SysV variadic ABI)
///
/// # Output
/// - ARM64: `x1` = pointer to string, `x2` = length
/// - x86_64: `rax` = pointer to string, `rdx` = length
///
/// # Behavior
/// Formats the float using `snprintf` with `"%.14G"` format into the global
/// `_concat_buf` buffer at the current `_concat_off` cursor, then advances
/// `_concat_off` by the number of characters written.
///
/// # ABI Notes
/// - Apple ARM64: variadic floats are passed on the stack, not in SIMD registers
/// - Linux x86_64: delegates to `emit_ftoa_linux_x86_64`; uses SysV variadic ABI with `eax=1` to indicate one SIMD register argument
pub fn emit_ftoa(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ftoa_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: ftoa ---");
    emitter.label_global("__rt_ftoa");

    // -- set up stack frame (64 bytes) --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- get current concat_buf position --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    emitter.instruction("str x10, [sp, #32]");                                  // save original offset on stack
    emitter.instruction("str x9, [sp, #40]");                                   // save offset variable address on stack

    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x0, x11, x10");                                    // compute output buffer: concat_buf + offset
    emitter.instruction("str x0, [sp, #24]");                                   // save output buffer start on stack

    // -- call snprintf(buf, 32, "%.14G", double) --
    emitter.instruction("mov x1, #32");                                         // buffer size limit = 32 bytes
    crate::codegen_support::abi::emit_symbol_address(emitter, "x2", "_fmt_g");          // load page address of format string "%.14G"
    // -- Apple ARM64 variadic ABI: float arg goes on stack, not in SIMD reg --
    emitter.instruction("str d0, [sp]");                                        // push double onto stack for variadic call
    emitter.bl_c("snprintf");                                        // call snprintf; returns char count in x0

    emit_php_scientific_fixup_arm64(emitter);

    // -- x0 = number of chars written --
    emitter.instruction("mov x2, x0");                                          // save string length as return value

    // -- update concat_off by chars written --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload offset variable address
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload original offset
    emitter.instruction("add x10, x10, x2");                                    // new offset = original + chars written
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- set return pointer --
    emitter.instruction("ldr x1, [sp, #24]");                                   // return pointer to start of formatted string

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Rewrites C `%.14G` scientific output into PHP's scientific format, in place.
///
/// PHP's float-to-string differs from C `%G` in exactly two ways, both confined to
/// scientific notation: the mantissa always carries a fraction (`1.0E+20`, never
/// `1E+20`), and the exponent has no zero padding (`1.0E-7`, never `1E-07`). Fixed
/// notation already matches byte for byte, so a buffer without `E` is left untouched.
///
/// # Input
/// - `x0` = byte count written by `snprintf`
/// - `[sp, #24]` = buffer start (as saved by `emit_ftoa`)
///
/// # Output
/// - `x0` = adjusted byte count
///
/// # Behavior
/// Inserting `.0` grows the text by two bytes. `%.14G` never exceeds 21 bytes
/// (`-1.2345678901234E-308`), so the rewritten form stays inside the 32-byte window
/// `emit_ftoa` reserves and no bounds check is required. Clobbers `x9`-`x15`, all of
/// which `emit_ftoa` reloads from its frame afterwards.
fn emit_php_scientific_fixup_arm64(emitter: &mut Emitter) {
    emitter.comment("-- rewrite C scientific output into PHP's format --");
    emitter.instruction("ldr x9, [sp, #24]");                                   // buffer start
    emitter.instruction("mov x10, x0");                                         // current byte count
    emitter.instruction("mov x11, #0");                                         // scan cursor

    emitter.label("__rt_ftoa_find_e");
    emitter.instruction("cmp x11, x10");                                        // scanned every byte?
    emitter.instruction("b.ge __rt_ftoa_done");                                 // no exponent: fixed notation already matches PHP
    emitter.instruction("ldrb w12, [x9, x11]");                                 // load candidate byte
    emitter.instruction("cmp w12, #69");                                        // 'E'
    emitter.instruction("b.eq __rt_ftoa_have_e");                               // exponent found
    emitter.instruction("add x11, x11, #1");                                    // advance scan cursor
    emitter.instruction("b __rt_ftoa_find_e");                                  // keep scanning

    emitter.label("__rt_ftoa_have_e");
    emitter.instruction("mov x13, #0");                                         // mantissa scan cursor
    emitter.label("__rt_ftoa_find_dot");
    emitter.instruction("cmp x13, x11");                                        // reached the exponent?
    emitter.instruction("b.ge __rt_ftoa_insert_dot");                           // bare mantissa: PHP requires ".0"
    emitter.instruction("ldrb w12, [x9, x13]");                                 // load mantissa byte
    emitter.instruction("cmp w12, #46");                                        // '.'
    emitter.instruction("b.eq __rt_ftoa_exponent");                             // mantissa already has a fraction
    emitter.instruction("add x13, x13, #1");                                    // advance mantissa cursor
    emitter.instruction("b __rt_ftoa_find_dot");                                // keep scanning

    emitter.label("__rt_ftoa_insert_dot");
    emitter.instruction("mov x14, x10");                                        // copy backwards from the trailing NUL
    emitter.label("__rt_ftoa_shift_right");
    emitter.instruction("cmp x14, x11");                                        // reached the exponent?
    emitter.instruction("b.lt __rt_ftoa_write_dot");                            // gap of two bytes is open
    emitter.instruction("ldrb w12, [x9, x14]");                                 // load byte to relocate
    emitter.instruction("add x15, x14, #2");                                    // destination two bytes higher
    emitter.instruction("strb w12, [x9, x15]");                                 // store relocated byte
    emitter.instruction("sub x14, x14, #1");                                    // walk towards the mantissa
    emitter.instruction("b __rt_ftoa_shift_right");                             // keep shifting

    emitter.label("__rt_ftoa_write_dot");
    emitter.instruction("mov w12, #46");                                        // '.'
    emitter.instruction("strb w12, [x9, x11]");                                 // write the fraction point
    emitter.instruction("add x15, x11, #1");                                    // slot after the point
    emitter.instruction("mov w12, #48");                                        // '0'
    emitter.instruction("strb w12, [x9, x15]");                                 // write the mandated fraction digit
    emitter.instruction("add x10, x10, #2");                                    // text grew by two bytes
    emitter.instruction("add x11, x11, #2");                                    // exponent moved with it

    emitter.label("__rt_ftoa_exponent");
    emitter.instruction("add x13, x11, #2");                                    // first exponent digit, past 'E' and its sign
    emitter.label("__rt_ftoa_strip_zero");
    emitter.instruction("sub x14, x10, x13");                                   // exponent digits still present
    emitter.instruction("cmp x14, #1");                                         // one digit left?
    emitter.instruction("b.le __rt_ftoa_done");                                 // never strip the final digit
    emitter.instruction("ldrb w12, [x9, x13]");                                 // load leading exponent digit
    emitter.instruction("cmp w12, #48");                                        // '0'
    emitter.instruction("b.ne __rt_ftoa_done");                                 // significant digit: exponent is PHP-exact
    emitter.instruction("mov x14, x13");                                        // overwrite the padding zero
    emitter.label("__rt_ftoa_shift_left");
    emitter.instruction("add x15, x14, #1");                                    // source is the following byte
    emitter.instruction("cmp x15, x10");                                        // past the end?
    emitter.instruction("b.ge __rt_ftoa_drop");                                 // whole tail moved down
    emitter.instruction("ldrb w12, [x9, x15]");                                 // load byte to relocate
    emitter.instruction("strb w12, [x9, x14]");                                 // store one byte lower
    emitter.instruction("mov x14, x15");                                        // advance destination
    emitter.instruction("b __rt_ftoa_shift_left");                              // keep shifting

    emitter.label("__rt_ftoa_drop");
    emitter.instruction("sub x10, x10, #1");                                    // text shrank by the padding zero
    emitter.instruction("b __rt_ftoa_strip_zero");                              // a second pad zero may remain

    emitter.label("__rt_ftoa_done");
    emitter.instruction("mov x0, x10");                                         // publish the adjusted byte count
}

/// Emits the `__rt_ftoa` routine for Linux x86_64.
///
/// # Input
/// - `xmm0` holds the float value (SysV variadic ABI)
///
/// # Output
/// - `rax` = pointer to formatted string, `rdx` = length
///
/// # Behavior
/// Same as `emit_ftoa` but for the Linux x86_64 target. Uses `rbp`-based
/// frame with 32 bytes of scratch space for concat cursor and output pointer.
/// Sets `eax = 1` to signal one SIMD register argument to `snprintf`.
fn emit_ftoa_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ftoa ---");
    emitter.label_global("__rt_ftoa");

    emitter.instruction("push rbp");                                            // save the caller frame pointer before using stack locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the formatting helper
    emitter.instruction("sub rsp, 32");                                         // reserve aligned scratch space for concat offsets and the output pointer

    crate::codegen_support::abi::emit_symbol_address(emitter, "r8", "_concat_off");
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // load the current concat cursor so formatted bytes append after prior output
    emitter.instruction("mov QWORD PTR [rbp - 8], r9");                         // save the original concat cursor for the final offset update
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // save the concat cursor symbol address for the final store

    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("lea rdi, [r10 + r9]");                                 // compute the destination buffer inside the concat scratch area
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the destination pointer for the return value

    emitter.instruction("mov esi, 32");                                         // cap float formatting to the same 32-byte scratch window used on AArch64
    crate::codegen_support::abi::emit_symbol_address(emitter, "rdx", "_fmt_g");
    emitter.instruction("mov eax, 1");                                          // SysV variadic ABI: one SIMD register is live for the double argument
    emitter.instruction("call snprintf");                                       // format xmm0 using "%.14G" into the concat scratch buffer

    emit_php_scientific_fixup_x86_64(emitter);

    emitter.instruction("mov rdx, rax");                                        // return the formatted byte count in the string-length result register
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the concat cursor symbol address
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the original concat cursor
    emitter.instruction("add r9, rdx");                                         // advance the concat cursor by the number of formatted bytes
    emitter.instruction("mov QWORD PTR [r8], r9");                              // publish the updated concat cursor for subsequent string writes
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the pointer to the formatted float text

    emitter.instruction("add rsp, 32");                                         // release the local scratch area before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return pointer+length in rax/rdx
}

/// Rewrites C `%.14G` scientific output into PHP's scientific format, in place.
///
/// Behavioral twin of `emit_php_scientific_fixup_arm64`: the mantissa always gains a
/// fraction (`1.0E+20`, never `1E+20`) and the exponent loses its zero padding
/// (`1.0E-7`, never `1E-07`). Fixed notation already matches PHP and is left untouched.
///
/// # Input
/// - `rax` = byte count written by `snprintf`
/// - `[rbp - 24]` = buffer start (as saved by `emit_ftoa_linux_x86_64`)
///
/// # Output
/// - `rax` = adjusted byte count
///
/// # Behavior
/// Clobbers `rcx`, `rsi`, `rdi`, and `r8`-`r11`. The caller reloads `r8`/`r9` from its
/// frame afterwards, and the two bytes added by `.0` stay inside the reserved 32-byte
/// window because `%.14G` never exceeds 21 bytes.
fn emit_php_scientific_fixup_x86_64(emitter: &mut Emitter) {
    emitter.comment("-- rewrite C scientific output into PHP's format --");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // buffer start
    emitter.instruction("mov r10, rax");                                        // current byte count
    emitter.instruction("xor r11, r11");                                        // scan cursor

    emitter.label("__rt_ftoa_find_e");
    emitter.instruction("cmp r11, r10");                                        // scanned every byte?
    emitter.instruction("jge __rt_ftoa_done");                                  // no exponent: fixed notation already matches PHP
    emitter.instruction("movzx ecx, BYTE PTR [r9 + r11]");                      // load candidate byte
    emitter.instruction("cmp cl, 69");                                          // 'E'
    emitter.instruction("je __rt_ftoa_have_e");                                 // exponent found
    emitter.instruction("inc r11");                                             // advance scan cursor
    emitter.instruction("jmp __rt_ftoa_find_e");                                // keep scanning

    emitter.label("__rt_ftoa_have_e");
    emitter.instruction("xor rsi, rsi");                                        // mantissa scan cursor
    emitter.label("__rt_ftoa_find_dot");
    emitter.instruction("cmp rsi, r11");                                        // reached the exponent?
    emitter.instruction("jge __rt_ftoa_insert_dot");                            // bare mantissa: PHP requires ".0"
    emitter.instruction("movzx ecx, BYTE PTR [r9 + rsi]");                      // load mantissa byte
    emitter.instruction("cmp cl, 46");                                          // '.'
    emitter.instruction("je __rt_ftoa_exponent");                               // mantissa already has a fraction
    emitter.instruction("inc rsi");                                             // advance mantissa cursor
    emitter.instruction("jmp __rt_ftoa_find_dot");                              // keep scanning

    emitter.label("__rt_ftoa_insert_dot");
    emitter.instruction("mov rdi, r10");                                        // copy backwards from the trailing NUL
    emitter.label("__rt_ftoa_shift_right");
    emitter.instruction("cmp rdi, r11");                                        // reached the exponent?
    emitter.instruction("jl __rt_ftoa_write_dot");                              // gap of two bytes is open
    emitter.instruction("movzx ecx, BYTE PTR [r9 + rdi]");                      // load byte to relocate
    emitter.instruction("mov BYTE PTR [r9 + rdi + 2], cl");                     // store two bytes higher
    emitter.instruction("dec rdi");                                             // walk towards the mantissa
    emitter.instruction("jmp __rt_ftoa_shift_right");                           // keep shifting

    emitter.label("__rt_ftoa_write_dot");
    emitter.instruction("mov BYTE PTR [r9 + r11], 46");                         // write the fraction point
    emitter.instruction("mov BYTE PTR [r9 + r11 + 1], 48");                     // write the mandated fraction digit
    emitter.instruction("add r10, 2");                                          // text grew by two bytes
    emitter.instruction("add r11, 2");                                          // exponent moved with it

    emitter.label("__rt_ftoa_exponent");
    emitter.instruction("lea rsi, [r11 + 2]");                                  // first exponent digit, past 'E' and its sign
    emitter.label("__rt_ftoa_strip_zero");
    emitter.instruction("mov rdi, r10");                                        // exponent digits still present
    emitter.instruction("sub rdi, rsi");                                        // = length - first digit index
    emitter.instruction("cmp rdi, 1");                                          // one digit left?
    emitter.instruction("jle __rt_ftoa_done");                                  // never strip the final digit
    emitter.instruction("movzx ecx, BYTE PTR [r9 + rsi]");                      // load leading exponent digit
    emitter.instruction("cmp cl, 48");                                          // '0'
    emitter.instruction("jne __rt_ftoa_done");                                  // significant digit: exponent is PHP-exact
    emitter.instruction("mov rdi, rsi");                                        // overwrite the padding zero
    emitter.label("__rt_ftoa_shift_left");
    emitter.instruction("lea r8, [rdi + 1]");                                   // source is the following byte
    emitter.instruction("cmp r8, r10");                                         // past the end?
    emitter.instruction("jge __rt_ftoa_drop");                                  // whole tail moved down
    emitter.instruction("movzx ecx, BYTE PTR [r9 + r8]");                       // load byte to relocate
    emitter.instruction("mov BYTE PTR [r9 + rdi], cl");                         // store one byte lower
    emitter.instruction("mov rdi, r8");                                         // advance destination
    emitter.instruction("jmp __rt_ftoa_shift_left");                            // keep shifting

    emitter.label("__rt_ftoa_drop");
    emitter.instruction("dec r10");                                             // text shrank by the padding zero
    emitter.instruction("jmp __rt_ftoa_strip_zero");                            // a second pad zero may remain

    emitter.label("__rt_ftoa_done");
    emitter.instruction("mov rax, r10");                                        // publish the adjusted byte count
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies that `emit_ftoa` on Linux x86_64 uses the SysV variadic calling convention
    /// by checking that `eax` is set to 1 (one SIMD register argument) before calling `snprintf`.
    #[test]
    fn test_emit_ftoa_linux_x86_64_uses_sysv_variadic_call() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_ftoa(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_ftoa:\n"));
        assert!(asm.contains("mov eax, 1\n"));
        assert!(asm.contains("call snprintf\n"));
        assert!(asm.contains("mov rdx, rax\n"));
    }
}
