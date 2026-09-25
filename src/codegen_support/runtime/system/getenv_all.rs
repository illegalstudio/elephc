//! Purpose:
//! Emits the `__rt_getenv_all` runtime helper: the whole process environment as a
//! PHP string-keyed array, which is what `getenv()` with no argument answers.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - Walks the LIVE environment, not the `envp` handed to `main`. That was the
//!   first attempt and it was wrong in a way only `putenv` shows: libc may
//!   reallocate the entry vector when a variable is added, so the startup pointer
//!   becomes a stale snapshot. Measured against `php -n`, which reports the
//!   addition — and elephc's own one-name `getenv($n)` goes through libc and saw
//!   it too, so the two halves of one builtin disagreed.
//! - Reading it live costs the platform split the startup pointer avoided:
//!   `environ` is a data symbol on Linux, and macOS answers `_NSGetEnviron()`
//!   with a pointer to it. Windows walks the Win32 process block returned by
//!   `GetEnvironmentStringsA`, which remains available across MinGW CRT
//!   variants and reflects `_putenv()` changes.
//! - Each entry is `KEY=VALUE`. The FIRST `=` separates them: a value may contain
//!   more, and splitting on the last one would rename every variable whose value
//!   holds an equals sign.
//! - An entry with no `=` at all is skipped rather than stored under itself. It
//!   is not something a shell can produce, and inventing a key for it would put a
//!   name in the array that no `getenv($name)` can ever match.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, platform::Platform};

/// Loads the live `environ` — the NULL-terminated `KEY=VALUE` vector — into `dest`.
///
/// Clobbers the call-result register on macOS, where it goes through a function
/// call, so `dest` is loaded last and callers must not hold anything volatile.
fn emit_load_environ(emitter: &mut Emitter, dest: &str) {
    match emitter.platform {
        Platform::MacOS => {
            // `_NSGetEnviron()` answers a `char ***`: the address OF the environ
            // pointer, which has to be dereferenced once more.
            emitter.bl_c("_NSGetEnviron");
            match emitter.target.arch {
                Arch::AArch64 => emitter.instruction(&format!("ldr {dest}, [x0]")), // dereference the `char ***` into the environ pointer
                Arch::X86_64 => {
                    emitter.instruction(&format!("mov {dest}, QWORD PTR [rax]")); // dereference the `char ***` into the environ pointer
                }
            }
        }
        Platform::Windows => unreachable!("Windows uses the process-block getenv_all emitter"),
        Platform::Linux => abi::emit_load_extern_symbol_to_reg(emitter, dest, "environ", 0),
    }
}

/// Emits `__rt_getenv_all` for the current target.
///
/// Input:  none.
/// Output: `x0`/`rax` = a PHP hash of every environment variable, string-keyed.
///
/// `__rt_hash_set` persists the key itself; the value is persisted here, because
/// it points into the environment block the program does not own.
pub fn emit_getenv_all(emitter: &mut Emitter) {
    if emitter.platform == Platform::Windows {
        emit_getenv_all_windows_x86_64(emitter);
        return;
    }
    if emitter.target.arch == Arch::X86_64 {
        emit_getenv_all_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: getenv_all ---");
    emitter.label_global("__rt_getenv_all");

    // Stack: [sp,#0] saved x19/x20, [sp,#16] saved x21/x22, [sp,#32] x29/x30.
    emitter.instruction("sub sp, sp, #48");                                     // reserve callee-saved slots and the frame record
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save the frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish this helper's frame pointer
    emitter.instruction("stp x19, x20, [sp, #0]");                              // preserve the entry cursor and the hash across calls
    emitter.instruction("stp x21, x22, [sp, #16]");                             // preserve the key length and the entry pointer

    emit_load_environ(emitter, "x19");                                          // x19 = the live entry vector
    emitter.instruction("mov x0, #128");                                        // capacity: a shell environment is ~60 entries and the table grows past 75% load, so 128 clears it without a rebuild
    emitter.instruction("mov x1, #7");                                          // value type 7 = mixed, the shape a boxed assoc array is read as
    abi::emit_call_label(emitter, "__rt_hash_new");                             // allocate the destination hash
    emitter.instruction("mov x20, x0");                                         // x20 = hash, updated after every insert

    emitter.label("__rt_getenv_all_next");
    emitter.instruction("cbz x19, __rt_getenv_all_done");                       // a null vector means no environment at all
    emitter.instruction("ldr x22, [x19]");                                      // x22 = the current "KEY=VALUE" entry
    emitter.instruction("cbz x22, __rt_getenv_all_done");                       // the vector ends at a null entry

    // -- find the FIRST '=', which is where the name ends --
    emitter.instruction("mov x21, #0");                                         // x21 = offset of the scan cursor
    emitter.label("__rt_getenv_all_scan");
    emitter.instruction("ldrb w9, [x22, x21]");                                 // load the next byte of the entry
    emitter.instruction("cbz w9, __rt_getenv_all_skip");                        // no '=' before the terminator: not a variable
    emitter.instruction("cmp w9, #61");                                         // 61 = '='
    emitter.instruction("b.eq __rt_getenv_all_split");                          // the name ends here
    emitter.instruction("add x21, x21, #1");                                    // keep scanning
    emitter.instruction("b __rt_getenv_all_scan");                              // continue scanning the current environment entry

    // -- measure the value, which runs from just past the '=' to the terminator --
    emitter.label("__rt_getenv_all_split");
    emitter.instruction("add x10, x22, x21");                                   // x10 = address of the '='
    emitter.instruction("add x10, x10, #1");                                    // x10 = start of the value
    emitter.instruction("mov x11, #0");                                         // x11 = value length
    emitter.label("__rt_getenv_all_vlen");
    emitter.instruction("ldrb w9, [x10, x11]");                                 // load the next byte of the value
    emitter.instruction("cbz w9, __rt_getenv_all_store");                       // the value ends at the terminator
    emitter.instruction("add x11, x11, #1");                                    // keep measuring
    emitter.instruction("b __rt_getenv_all_vlen");                              // continue measuring the environment value

    // -- copy the value out of the environment block before the hash owns it --
    emitter.label("__rt_getenv_all_store");
    emitter.instruction("mov x1, x10");                                         // str_persist source pointer
    emitter.instruction("mov x2, x11");                                         // str_persist source length
    abi::emit_call_label(emitter, "__rt_str_persist");                          // x1/x2 = owned heap copy of the value
    emitter.instruction("mov x3, x1");                                          // value_lo = the owned payload pointer
    emitter.instruction("mov x4, x2");                                          // value_hi = its length
    emitter.instruction("mov x5, #1");                                          // value_tag 1 = string
    emitter.instruction("mov x0, x20");                                         // hash to insert into
    emitter.instruction("mov x1, x22");                                         // key_lo = the entry, whose name is its prefix
    emitter.instruction("mov x2, x21");                                         // key_hi = the name length, stopping at the '='
    abi::emit_call_label(emitter, "__rt_hash_set");                             // hash_set persists the key itself
    emitter.instruction("mov x20, x0");                                         // the table may have been reallocated

    emitter.label("__rt_getenv_all_skip");
    emitter.instruction("add x19, x19, #8");                                    // advance to the next entry pointer
    emitter.instruction("b __rt_getenv_all_next");                              // advance through the environment vector

    emitter.label("__rt_getenv_all_done");
    emitter.instruction("mov x0, x20");                                         // return the populated hash
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore the cursor and hash registers
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore the length and entry registers
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the frame
    emitter.instruction("ret");                                                 // return the populated environment hash
}

/// Emits the Windows implementation using the process environment block.
///
/// MinGW distributions do not agree on whether the legacy CRT environment
/// accessor is present in `libmsvcrt.a`.  `GetEnvironmentStringsA` is the
/// stable Win32 API and observes the same process environment mutations as
/// `_putenv`; its contiguous `KEY=VALUE\0...\0\0` block is walked directly.
fn emit_getenv_all_windows_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: getenv_all (Windows process block) ---");
    emitter.label_global("__rt_getenv_all");

    // rbx = current entry, r15 = original block, r12 = result hash,
    // r13 = current entry, r14 = key length. Six callee-saved pushes plus
    // eight bytes keep both SysV and MSx64 calls aligned.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the frame base
    emitter.instruction("push rbx");                                            // preserve the environment cursor
    emitter.instruction("push r12");                                            // preserve the result hash register
    emitter.instruction("push r13");                                            // preserve the scratch callee-saved register
    emitter.instruction("push r14");                                            // preserve the key-length register
    emitter.instruction("push r15");                                            // preserve the environment block pointer
    emitter.instruction("sub rsp, 8");                                          // complete the call-alignment padding

    emitter.instruction("sub rsp, 32");                                         // reserve the Windows shadow space
    emitter.instruction("call GetEnvironmentStringsA");                         // acquire the process environment block
    emitter.instruction("add rsp, 32");                                         // release the Windows shadow space
    emitter.instruction("mov r15, rax");                                        // retain the block for final release
    emitter.instruction("mov rbx, rax");                                        // begin walking at the first entry
    emitter.instruction("mov rdi, 128");                                        // seed the result hash capacity
    emitter.instruction("mov rsi, 7");                                          // select the environment hash policy
    abi::emit_call_label(emitter, "__rt_hash_new");                         // allocate the result hash
    emitter.instruction("mov r12, rax");                                        // retain the new hash pointer

    emitter.label("__rt_getenv_all_windows_next");
    emitter.instruction("test rbx, rbx");                                       // handle an unavailable environment block
    emitter.instruction("jz __rt_getenv_all_windows_done");                     // finish with the empty result hash
    emitter.instruction("cmp BYTE PTR [rbx], 0");                               // detect the double-NUL terminator
    emitter.instruction("je __rt_getenv_all_windows_done");                     // stop at the end of the block

    emitter.instruction("xor r14, r14");                                        // reset the key-length accumulator
    emitter.label("__rt_getenv_all_windows_scan");
    emitter.instruction("mov cl, BYTE PTR [rbx + r14]");                        // read the next entry byte
    emitter.instruction("test cl, cl");                                         // check for an empty entry
    emitter.instruction("jz __rt_getenv_all_windows_skip");                     // skip malformed or empty entries
    emitter.instruction("cmp cl, 61");                                          // search for the key/value separator
    emitter.instruction("je __rt_getenv_all_windows_split");                    // begin value scanning after '='
    emitter.instruction("add r14, 1");                                          // extend the key length
    emitter.instruction("jmp __rt_getenv_all_windows_scan");                    // continue scanning the key

    emitter.label("__rt_getenv_all_windows_split");
    emitter.instruction("lea r10, [rbx + r14 + 1]");                            // point at the value bytes
    emitter.instruction("xor r11, r11");                                        // reset the value-length accumulator
    emitter.label("__rt_getenv_all_windows_vlen");
    emitter.instruction("mov cl, BYTE PTR [r10 + r11]");                        // read the next value byte
    emitter.instruction("test cl, cl");                                         // check for the entry terminator
    emitter.instruction("jz __rt_getenv_all_windows_store");                    // finish measuring the value
    emitter.instruction("add r11, 1");                                          // extend the value length
    emitter.instruction("jmp __rt_getenv_all_windows_vlen");                    // continue scanning the value

    emitter.label("__rt_getenv_all_windows_store");
    emitter.instruction("mov rax, r10");                                        // pass the value pointer to persistence
    emitter.instruction("mov rdx, r11");                                        // pass the measured value length
    abi::emit_call_label(emitter, "__rt_str_persist");                         // copy the value out of the process block
    emitter.instruction("mov rcx, rax");                                        // provide the persisted value pointer
    emitter.instruction("mov r8, rdx");                                         // provide the persisted value length
    emitter.instruction("mov r9, 1");                                           // mark the inserted value as owned
    emitter.instruction("mov rdi, r12");                                        // select the result hash
    emitter.instruction("mov rsi, rbx");                                        // provide the entry key pointer
    emitter.instruction("mov rdx, r14");                                        // provide the key length
    abi::emit_call_label(emitter, "__rt_hash_set");                            // insert the key/value pair
    emitter.instruction("mov r12, rax");                                        // retain the possibly reallocated hash

    emitter.label("__rt_getenv_all_windows_skip");
    emitter.label("__rt_getenv_all_windows_advance");
    emitter.instruction("inc rbx");                                             // advance through the current entry
    emitter.instruction("cmp BYTE PTR [rbx], 0");                               // locate the entry terminator
    emitter.instruction("jne __rt_getenv_all_windows_advance");                 // continue to the next byte
    emitter.instruction("inc rbx");                                             // skip the entry terminator
    emitter.instruction("jmp __rt_getenv_all_windows_next");                    // process the next environment entry

    emitter.label("__rt_getenv_all_windows_done");
    emitter.instruction("test r15, r15");                                       // check whether a block needs releasing
    emitter.instruction("jz __rt_getenv_all_windows_return");                   // skip release for a null block
    emitter.instruction("mov rdi, r15");                                        // pass the original block pointer
    emitter.instruction("sub rsp, 32");                                         // reserve the Windows shadow space
    emitter.instruction("mov rcx, rdi");                                        // pass the block in the Windows ABI register
    emitter.instruction("call FreeEnvironmentStringsA");                        // release the process environment block
    emitter.instruction("add rsp, 32");                                         // release the Windows shadow space
    emitter.label("__rt_getenv_all_windows_return");
    emitter.instruction("mov rax, r12");                                        // return the populated hash
    emitter.instruction("add rsp, 8");                                          // remove the alignment padding
    emitter.instruction("pop r15");                                             // restore the environment block register
    emitter.instruction("pop r14");                                             // restore the key-length register
    emitter.instruction("pop r13");                                             // restore the scratch callee-saved register
    emitter.instruction("pop r12");                                             // restore the result hash register
    emitter.instruction("pop rbx");                                             // restore the environment cursor
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
/// x86_64 Linux variant of [`emit_getenv_all`]. Same walk, SysV registers.
///
/// The callee-saved trio is `rbx` (envp cursor), `r12` (hash) and `r13` (entry),
/// because every iteration calls three helpers and caller-saved registers do not
/// survive them.
fn emit_getenv_all_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: getenv_all ---");
    emitter.label_global("__rt_getenv_all");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish this helper's frame base
    emitter.instruction("push rbx");                                            // preserve the envp cursor across nested calls
    emitter.instruction("push r12");                                            // preserve the destination hash
    emitter.instruction("push r13");                                            // preserve the current entry pointer
    emitter.instruction("push r14");                                            // preserve the name length
    // No padding: rsp is 16-byte aligned here already. A call leaves it at 8 mod
    // 16 on entry, and the five pushes above bring it back to 0 — so subtracting
    // another 8 would MISALIGN every nested call rather than align it, which is
    // what the comment used to claim it did.

    emit_load_environ(emitter, "rbx");                                          // rbx = the live entry vector
    emitter.instruction("mov rdi, 128");                                        // capacity: a shell environment is ~60 entries and the table grows past 75% load, so 128 clears it without a rebuild
    emitter.instruction("mov rsi, 7");                                          // value type 7 = mixed, the shape a boxed assoc array is read as
    abi::emit_call_label(emitter, "__rt_hash_new");                             // allocate the destination hash
    emitter.instruction("mov r12, rax");                                        // r12 = hash

    emitter.label("__rt_getenv_all_next");
    emitter.instruction("test rbx, rbx");                                       // a null vector means no environment at all
    emitter.instruction("jz __rt_getenv_all_done");                             // a null vector means no environment at all
    emitter.instruction("mov r13, QWORD PTR [rbx]");                            // r13 = the current "KEY=VALUE" entry
    emitter.instruction("test r13, r13");                                       // the vector ends at a null entry
    emitter.instruction("jz __rt_getenv_all_done");                             // the vector ends at a null entry

    // -- find the FIRST '=', which is where the name ends --
    emitter.instruction("xor r14, r14");                                        // r14 = scan cursor and, at the end, the name length
    emitter.label("__rt_getenv_all_scan");
    emitter.instruction("mov cl, BYTE PTR [r13 + r14]");                        // load the next byte of the entry
    emitter.instruction("test cl, cl");                                         // no '=' before the terminator: not a variable
    emitter.instruction("jz __rt_getenv_all_skip");                             // skip an entry without an equals separator
    emitter.instruction("cmp cl, 61");                                          // 61 = '='
    emitter.instruction("je __rt_getenv_all_split");                            // the name ends here
    emitter.instruction("add r14, 1");                                          // keep scanning
    emitter.instruction("jmp __rt_getenv_all_scan");                            // continue scanning the current environment entry

    // -- measure the value, which runs from just past the '=' to the terminator --
    emitter.label("__rt_getenv_all_split");
    emitter.instruction("lea r10, [r13 + r14 + 1]");                            // r10 = start of the value, one past the '='
    emitter.instruction("xor r11, r11");                                        // r11 = value length
    emitter.label("__rt_getenv_all_vlen");
    emitter.instruction("mov cl, BYTE PTR [r10 + r11]");                        // load the next byte of the value
    emitter.instruction("test cl, cl");                                         // the value ends at the terminator
    emitter.instruction("jz __rt_getenv_all_store");                            // the value ends at the terminator
    emitter.instruction("add r11, 1");                                          // keep measuring
    emitter.instruction("jmp __rt_getenv_all_vlen");                            // continue measuring the environment value

    // -- copy the value out of the environment block before the hash owns it --
    emitter.label("__rt_getenv_all_store");
    emitter.instruction("mov rax, r10");                                        // str_persist source pointer
    emitter.instruction("mov rdx, r11");                                        // str_persist source length
    abi::emit_call_label(emitter, "__rt_str_persist");                          // rax/rdx = owned heap copy of the value
    // rdi=hash, rsi=key_ptr, rdx=key_len, rcx=value_lo, r8=value_hi, r9=value_tag.
    // Loaded in that order and from rax/rdx last-first, because rdx carries the
    // persisted length in AND the key length out.
    emitter.instruction("mov rcx, rax");                                        // value_lo = the owned payload pointer
    emitter.instruction("mov r8, rdx");                                         // value_hi = its length
    emitter.instruction("mov r9, 1");                                           // value_tag 1 = string
    emitter.instruction("mov rdi, r12");                                        // hash to insert into
    emitter.instruction("mov rsi, r13");                                        // key_ptr = the entry, whose name is its prefix
    emitter.instruction("mov rdx, r14");                                        // key_len = the name length, stopping at the '='
    abi::emit_call_label(emitter, "__rt_hash_set");                             // hash_set persists the key itself
    emitter.instruction("mov r12, rax");                                        // the table may have been reallocated

    emitter.label("__rt_getenv_all_skip");
    emitter.instruction("add rbx, 8");                                          // advance to the next entry pointer
    emitter.instruction("jmp __rt_getenv_all_next");                            // advance through the environment vector

    emitter.label("__rt_getenv_all_done");
    emitter.instruction("mov rax, r12");                                        // return the populated hash
    emitter.instruction("pop r14");                                             // restore the name-length register
    emitter.instruction("pop r13");                                             // restore the entry register
    emitter.instruction("pop r12");                                             // restore the hash register
    emitter.instruction("pop rbx");                                             // restore the envp cursor
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Windows reads the live Win32 process environment block that `_putenv()` mutates.
    #[test]
    fn windows_getenv_all_uses_the_process_environment_block() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_getenv_all(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("call GetEnvironmentStringsA"), "{asm}");
        assert!(asm.contains("call FreeEnvironmentStringsA"), "{asm}");
        assert!(!asm.contains("__p__environ"), "{asm}");
    }
}
