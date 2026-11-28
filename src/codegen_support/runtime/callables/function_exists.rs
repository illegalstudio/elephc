//! Purpose:
//! Emits the `__rt_function_exists_lookup` runtime helper backing a dynamic
//! `function_exists($name)` membership test.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `callables::emit_function_exists_lookup`.
//! - The emitted code is called by `codegen::lower_inst::builtins::lower_dynamic_function_exists`,
//!   which supplies the baked candidate table.
//!
//! Key details:
//! - The helper is table-driven, so its size is independent of how many function names the
//!   program declares: the names live in `.data`, not in the instruction stream at each call site.
//! - Memoir's `FunctionNameRow` owns each entry's name span and optional include-variant
//!   active-symbol projection.
//! - Comparison is case-insensitive (`__rt_strcasecmp`), matching PHP: `function_exists('STRLEN')`
//!   and `function_exists('strlen')` agree.
//! - A single leading `\` on the needle is skipped before scanning, matching PHP's
//!   `function_exists('\strlen') === true`.
//! - The needle pointer/length are re-loaded from the helper's own frame before every
//!   comparison because `__rt_strcasecmp` clobbers caller-saved registers.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use elephc_memoir::callables::layout::{self as callable_layout, FunctionNameField};

/// Emits `__rt_function_exists_lookup` for the current target.
///
/// Signature (both arches, in the platform's first four integer argument registers):
/// `int __rt_function_exists_lookup(const char *name, size_t len, const Entry *table, size_t count)`
/// returning 1 when `name` is present in `table` and 0 otherwise.
pub(crate) fn emit_function_exists_lookup(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emit_aarch64(emitter);
}

/// Emits the AArch64 `__rt_function_exists_lookup` helper.
///
/// Inputs: x0 = name pointer, x1 = name length, x2 = table pointer, x3 = entry count.
/// Output: x0 = 1 (declared) or 0 (not declared).
///
/// Frame layout (64 bytes): `[sp + 0]` name pointer, `[sp + 8]` name length,
/// `[sp + 16]` table pointer, `[sp + 24]` entry count, `[sp + 32]` scan index,
/// `[sp + 40]` current entry pointer, `[sp + 48]` saved `x29`/`x30`. Every value the scan needs
/// across a call lives in this frame, so `__rt_strcasecmp` clobbering caller-saved registers is
/// harmless.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: function_exists_lookup ---");
    emitter.label_global("__rt_function_exists_lookup");

    emitter.instruction("sub sp, sp, #64");                                     // reserve the scan frame for the needle, the table cursor, and the saved link register
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save caller frame pointer and return address around the comparison calls
    emitter.instruction("add x29, sp, #48");                                    // establish this helper's frame pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the candidate-name table pointer
    emitter.instruction("str x3, [sp, #24]");                                   // save the candidate-name entry count
    emitter.instruction("str xzr, [sp, #32]");                                  // start the scan at entry index zero
    emitter.instruction("cbz x1, __rt_function_exists_lookup_false");           // the empty string never names a function, matching PHP
    emitter.instruction("ldrb w9, [x0]");                                       // read the first needle byte to detect a fully-qualified name
    emitter.instruction("cmp w9, #92");                                         // is the needle prefixed with a namespace separator?
    emitter.instruction("b.ne __rt_function_exists_lookup_save");               // unqualified names are compared as-is
    emitter.instruction("cmp x1, #1");                                          // a lone backslash leaves an empty name behind
    emitter.instruction("b.le __rt_function_exists_lookup_false");              // reject "\" because PHP resolves it to no function
    emitter.instruction("add x0, x0, #1");                                      // skip the leading namespace separator so "\strlen" matches "strlen"
    emitter.instruction("sub x1, x1, #1");                                      // shorten the needle after dropping the separator

    emitter.label("__rt_function_exists_lookup_save");
    emitter.instruction("str x0, [sp, #0]");                                    // save the normalized needle pointer for repeated comparisons
    emitter.instruction("str x1, [sp, #8]");                                    // save the normalized needle length for repeated comparisons

    emitter.label("__rt_function_exists_lookup_loop");
    emitter.instruction("ldr x9, [sp, #32]");                                   // load the current table index
    emitter.instruction("ldr x10, [sp, #24]");                                  // load the table entry count
    emitter.instruction("cmp x9, x10");                                         // have all candidate names been compared?
    emitter.instruction("b.ge __rt_function_exists_lookup_false");              // no candidate matched, so the function does not exist
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the table base pointer
    callable_layout::emit_function_name_row_address(emitter, "x11", "x11", "x9", "x12");
    emitter.instruction("str x11, [sp, #40]");                                  // preserve the entry pointer across the comparison call
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the needle pointer as comparison left side
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the needle length as comparison left side
    callable_layout::emit_load_function_name_field(emitter, "x3", "x11", FunctionNameField::Name);
    callable_layout::emit_load_function_name_field(emitter, "x4", "x11", FunctionNameField::NameLength);
    abi::emit_call_label(emitter, "__rt_strcasecmp");                           // PHP compares function names case-insensitively
    emitter.instruction("cbnz x0, __rt_function_exists_lookup_next");           // a non-zero result means this candidate did not match
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload the entry pointer after caller-saved registers were clobbered
    callable_layout::emit_load_function_name_field(emitter, "x12", "x11", FunctionNameField::ActiveSymbol);
    emitter.instruction("cbz x12, __rt_function_exists_lookup_true");           // ordinary declarations exist as soon as the name matches
    emitter.instruction("ldr x12, [x12]");                                      // read the active implementation pointer for this variant group
    emitter.instruction("cbnz x12, __rt_function_exists_lookup_true");          // an include-loaded variant exists only once an implementation is active

    emitter.label("__rt_function_exists_lookup_next");
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the current table index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next candidate entry
    emitter.instruction("str x9, [sp, #32]");                                   // persist the incremented index
    emitter.instruction("b __rt_function_exists_lookup_loop");                  // keep scanning the candidate table

    emitter.label("__rt_function_exists_lookup_true");
    emitter.instruction("mov x0, #1");                                          // report that the function name is declared
    emitter.instruction("b __rt_function_exists_lookup_done");                  // restore this helper's frame before returning

    emitter.label("__rt_function_exists_lookup_false");
    emitter.instruction("mov x0, #0");                                          // report that no declaration carries this name

    emitter.label("__rt_function_exists_lookup_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore caller frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the scan frame
    emitter.instruction("ret");                                                 // return the boolean result in x0
}

/// Emits the x86_64 `__rt_function_exists_lookup` helper.
///
/// Inputs: rdi = name pointer, rsi = name length, rdx = table pointer, rcx = entry count.
/// Output: rax = 1 (declared) or 0 (not declared).
///
/// Frame layout (48 bytes below rbp): `[rbp - 8]` name pointer, `[rbp - 16]` name length,
/// `[rbp - 24]` table pointer, `[rbp - 32]` entry count, `[rbp - 40]` scan index,
/// `[rbp - 48]` current entry pointer. `push rbp` plus the 8-byte return address keeps rsp
/// 16-byte aligned after `sub rsp, 48`, so the nested `__rt_strcasecmp` call is ABI-correct.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: function_exists_lookup ---");
    emitter.label_global("__rt_function_exists_lookup");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before the table scan
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the needle and table cursor
    emitter.instruction("sub rsp, 48");                                         // reserve slots for needle, table pointer, count, index, and entry pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the candidate-name table pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the candidate-name entry count
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // start the scan at entry index zero
    emitter.instruction("test rsi, rsi");                                       // is the needle empty?
    emitter.instruction("je __rt_function_exists_lookup_false_x86_64");         // the empty string never names a function, matching PHP
    emitter.instruction("cmp BYTE PTR [rdi], 92");                              // is the needle prefixed with a namespace separator?
    emitter.instruction("jne __rt_function_exists_lookup_save_x86_64");         // unqualified names are compared as-is
    emitter.instruction("cmp rsi, 1");                                          // a lone backslash leaves an empty name behind
    emitter.instruction("jle __rt_function_exists_lookup_false_x86_64");        // reject "\" because PHP resolves it to no function
    emitter.instruction("add rdi, 1");                                          // skip the leading namespace separator so "\strlen" matches "strlen"
    emitter.instruction("sub rsi, 1");                                          // shorten the needle after dropping the separator

    emitter.label("__rt_function_exists_lookup_save_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the normalized needle pointer for repeated comparisons
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the normalized needle length for repeated comparisons

    emitter.label("__rt_function_exists_lookup_loop_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // load the current table index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // have all candidate names been compared?
    emitter.instruction("jge __rt_function_exists_lookup_false_x86_64");        // no candidate matched, so the function does not exist
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the table base pointer
    callable_layout::emit_function_name_row_address(emitter, "r11", "r11", "r10", "r8");
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // preserve the entry pointer across the comparison call
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the needle pointer as comparison left side
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the needle length as comparison left side
    callable_layout::emit_load_function_name_field(emitter, "rdx", "r11", FunctionNameField::Name);
    callable_layout::emit_load_function_name_field(emitter, "rcx", "r11", FunctionNameField::NameLength);
    abi::emit_call_label(emitter, "__rt_strcasecmp");                           // PHP compares function names case-insensitively
    emitter.instruction("test rax, rax");                                       // a zero result means the strings are equal
    emitter.instruction("jne __rt_function_exists_lookup_next_x86_64");         // this candidate did not match, so keep scanning
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the entry pointer after caller-saved registers were clobbered
    callable_layout::emit_load_function_name_field(emitter, "r8", "r11", FunctionNameField::ActiveSymbol);
    emitter.instruction("test r8, r8");                                         // ordinary declarations use a null active-symbol address
    emitter.instruction("je __rt_function_exists_lookup_true_x86_64");          // ordinary declarations exist as soon as the name matches
    emitter.instruction("mov r8, QWORD PTR [r8]");                              // read the active implementation pointer for this variant group
    emitter.instruction("test r8, r8");                                         // is an include-loaded implementation active?
    emitter.instruction("jne __rt_function_exists_lookup_true_x86_64");         // an include-loaded variant exists only once an implementation is active

    emitter.label("__rt_function_exists_lookup_next_x86_64");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the current table index
    emitter.instruction("add r10, 1");                                          // advance to the next candidate entry
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // persist the incremented index
    emitter.instruction("jmp __rt_function_exists_lookup_loop_x86_64");         // keep scanning the candidate table

    emitter.label("__rt_function_exists_lookup_true_x86_64");
    emitter.instruction("mov rax, 1");                                          // report that the function name is declared
    emitter.instruction("jmp __rt_function_exists_lookup_done_x86_64");         // restore this helper's frame before returning

    emitter.label("__rt_function_exists_lookup_false_x86_64");
    emitter.instruction("xor eax, eax");                                        // report that no declaration carries this name

    emitter.label("__rt_function_exists_lookup_done_x86_64");
    emitter.instruction("add rsp, 48");                                         // release the scan frame slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the boolean result in rax
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Renders the helper for one target.
    fn emit_for(arch: Arch) -> String {
        let platform = match arch {
            Arch::AArch64 => Platform::MacOS,
            Arch::X86_64 => Platform::Linux,
        };
        let mut emitter = Emitter::new(Target::new(platform, arch));
        emit_function_exists_lookup(&mut emitter);
        emitter.output()
    }

    /// `__rt_strcasecmp` clobbers caller-saved registers, so the scan may not keep the needle or
    /// the current table entry in a register across the call. This asserts the two properties the
    /// loop depends on: the needle operands are re-loaded from the frame immediately before every
    /// comparison, and the entry pointer is re-loaded from the frame after it.
    #[test]
    fn needle_and_entry_are_reloaded_around_every_comparison() {
        for (arch, needle_reloads, entry_reload) in [
            (
                Arch::AArch64,
                ["ldr x1, [sp, #0]", "ldr x2, [sp, #8]"],
                "ldr x11, [sp, #40]",
            ),
            (
                Arch::X86_64,
                [
                    "mov rdi, QWORD PTR [rbp - 8]",
                    "mov rsi, QWORD PTR [rbp - 16]",
                ],
                "mov r11, QWORD PTR [rbp - 48]",
            ),
        ] {
            let asm = emit_for(arch);
            let lines: Vec<&str> = asm.lines().map(str::trim).collect();
            let call_index = lines
                .iter()
                .position(|line| line.contains("__rt_strcasecmp"))
                .unwrap_or_else(|| panic!("{:?}: no __rt_strcasecmp call emitted", arch));
            assert_eq!(
                lines.iter().filter(|l| l.contains("__rt_strcasecmp")).count(),
                1,
                "{:?}: the table scan must issue exactly one comparison call inside the loop",
                arch
            );
            let before = &lines[..call_index];
            for reload in needle_reloads {
                assert!(
                    before.contains(&reload),
                    "{:?}: needle reload {:?} missing before the comparison call",
                    arch,
                    reload
                );
            }
            assert!(
                lines[call_index..].contains(&entry_reload),
                "{:?}: entry-pointer reload {:?} missing after the comparison call",
                arch,
                entry_reload
            );
        }
    }

    /// The helper must normalize a single leading namespace separator and reject an empty needle,
    /// matching PHP (`function_exists('\strlen')` is true, `function_exists('')` is false).
    #[test]
    fn leading_separator_and_empty_needle_are_handled() {
        let aarch64 = emit_for(Arch::AArch64);
        assert!(aarch64.contains("cmp w9, #92"), "aarch64 must test for a leading backslash");
        assert!(
            aarch64.contains("cbz x1, __rt_function_exists_lookup_false"),
            "aarch64 must reject an empty needle"
        );
        let x86_64 = emit_for(Arch::X86_64);
        assert!(
            x86_64.contains("cmp BYTE PTR [rdi], 92"),
            "x86_64 must test for a leading backslash"
        );
        assert!(
            x86_64.contains("je __rt_function_exists_lookup_false_x86_64"),
            "x86_64 must reject an empty needle"
        );
    }
}
