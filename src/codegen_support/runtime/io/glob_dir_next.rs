//! Purpose:
//! Emits `__rt_glob_dir_next`, one step of a `glob://` directory iterator.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_readdir`'s glob arm, and `__rt_scandir`'s.
//!
//! Key details:
//! - php's `glob://` reads the NAME, not the path the pattern matched — MEASURED on `php -n`
//!   8.5.6, `opendir("glob://g1/*.txt")` reads `a.txt` where `glob("g1/*.txt")` answers
//!   `g1/a.txt`. The directory the pattern named is the caller's already.
//! - The step is a LEAF: it makes no call, so it needs no frame, and both readers can use it from
//!   inside their own loops without spilling anything.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_path_is_glob_url(path, len) -> 1 | 0`.
///
/// Inputs: AArch64 x1/x2, x86_64 rax/rdx — the elephc string convention, so a caller that already
/// holds the path pair asks without shuffling. Output: x0 / rax.
///
/// `__rt_opendir` makes the same test INLINE, because at the point it makes it there is no frame
/// and `bl` would destroy its return address. Every caller that has a frame comes here instead.
pub fn emit_path_is_glob_url(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: does this path name the glob:// scheme ---");
    emitter.label_global("__rt_path_is_glob_url");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #0");                                  // the answer, until every byte agrees
            emitter.instruction("cmp x2, #7");                                  // "glob://" needs at least seven bytes
            emitter.instruction("b.lt __rt_pigu_no");
            for (index, byte) in b"glob://".iter().enumerate() {
                emitter.instruction(&format!("ldrb w9, [x1, #{index}]"));
                emitter.instruction(&format!("cmp w9, #{byte}"));
                emitter.instruction("b.ne __rt_pigu_no");
            }
            emitter.instruction("mov x0, #1");                                  // the scheme matched
            emitter.label("__rt_pigu_no");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("xor r11d, r11d");                              // the answer, until every byte agrees
            emitter.instruction("cmp rdx, 7");                                  // "glob://" needs at least seven bytes
            emitter.instruction("jl __rt_pigu_no_x86");
            for (index, byte) in b"glob://".iter().enumerate() {
                emitter.instruction(&format!("movzx r9d, BYTE PTR [rax + {index}]"));
                emitter.instruction(&format!("cmp r9b, {byte}"));
                emitter.instruction("jne __rt_pigu_no_x86");
            }
            emitter.instruction("mov r11d, 1");                                 // the scheme matched
            emitter.label("__rt_pigu_no_x86");
            emitter.instruction("mov rax, r11");
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_glob_dir_next(state) -> (name, len)`.
///
/// Inputs: AArch64 x0 = the glob iterator state, x86_64 rdi = the same.
/// Outputs: AArch64 x1/x2, x86_64 rsi/rdx — the entry name and its length, or a null pointer once
/// every match has been consumed. The pointer is INTO the glob vector, so a caller that keeps it
/// past `globfree` must persist it, exactly as `__rt_readdir` does.
///
/// The state's shape is `__rt_opendir_glob`'s: `[0]` the path vector, `[8]` the match count,
/// `[16]` the iteration index, `[24]` the embedded `glob_t` that `globfree` is handed.
pub fn emit_glob_dir_next(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// The AArch64 step.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: one step of a glob:// directory iterator ---");
    emitter.label_global("__rt_glob_dir_next");
    emitter.instruction("mov x10, x0");                                         // the glob iterator state
    emitter.instruction("cbz x10, __rt_gdn_end");                               // a detached owner is already exhausted
    emitter.instruction("ldr x11, [x10, #8]");                                  // the glob match count
    emitter.instruction("ldr x12, [x10, #16]");                                 // the current iteration index
    emitter.instruction("cmp x12, x11");                                        // has iteration reached the match count?
    emitter.instruction("b.hs __rt_gdn_end");                                   // every match was consumed
    emitter.instruction("ldr x13, [x10, #0]");                                  // the glob path-vector pointer
    emitter.instruction("ldr x1, [x13, x12, lsl #3]");                          // the current matched path
    emitter.instruction("add x12, x12, #1");                                    // advance the iterator
    emitter.instruction("str x12, [x10, #16]");                                 // persist the next index
    emitter.instruction("mov x2, #0");                                          // begin measuring the matched path
    emitter.instruction("mov x3, #0");                                          // one past the last '/', where the NAME starts
    emitter.label("__rt_gdn_strlen");
    emitter.instruction("ldrb w9, [x1, x2]");                                   // the next matched-path byte
    emitter.instruction("cbz w9, __rt_gdn_basename");                           // stop at the path terminator
    emitter.instruction("add x4, x2, #1");                                      // where a name would start after this byte
    emitter.instruction("cmp w9, #0x2F");                                       // '/'
    emitter.instruction("csel x3, x4, x3, eq");                                 // remember the last separator seen
    emitter.instruction("add x2, x2, #1");                                      // count one more byte
    emitter.instruction("b __rt_gdn_strlen");
    emitter.label("__rt_gdn_basename");
    emitter.instruction("add x1, x1, x3");                                      // step past the directory
    emitter.instruction("sub x2, x2, x3");                                      // and shorten the name to match
    emitter.instruction("ret");
    emitter.label("__rt_gdn_end");
    emitter.instruction("mov x1, #0");                                          // a null pointer ends the iteration
    emitter.instruction("mov x2, #0");
    emitter.instruction("ret");
}

/// The x86_64 step.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: one step of a glob:// directory iterator ---");
    emitter.label_global("__rt_glob_dir_next");
    emitter.instruction("mov r10, rdi");                                        // the glob iterator state
    emitter.instruction("test r10, r10");                                       // is the owner still attached?
    emitter.instruction("jz __rt_gdn_end_x86");                                 // detached state is already exhausted
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // the glob match count
    emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                       // the current iteration index
    emitter.instruction("cmp rdx, r11");                                        // has iteration reached the match count?
    emitter.instruction("jae __rt_gdn_end_x86");                                // every match was consumed
    emitter.instruction("mov r8, QWORD PTR [r10]");                             // the glob path-vector pointer
    emitter.instruction("mov rsi, QWORD PTR [r8 + rdx * 8]");                   // the current matched path
    emitter.instruction("add rdx, 1");                                          // advance the iterator
    emitter.instruction("mov QWORD PTR [r10 + 16], rdx");                       // persist the next index
    emitter.instruction("xor edx, edx");                                        // begin measuring the matched path
    emitter.instruction("xor ecx, ecx");                                        // one past the last '/', where the NAME starts
    emitter.label("__rt_gdn_strlen_x86");
    emitter.instruction("movzx r9d, BYTE PTR [rsi + rdx]");                     // the next matched-path byte
    emitter.instruction("test r9b, r9b");
    emitter.instruction("jz __rt_gdn_basename_x86");                            // stop at the path terminator
    emitter.instruction("cmp r9b, 0x2F");                                       // '/'
    emitter.instruction("jne __rt_gdn_next_x86");
    emitter.instruction("lea rcx, [rdx + 1]");                                  // remember the last separator seen
    emitter.label("__rt_gdn_next_x86");
    emitter.instruction("add rdx, 1");                                          // count one more byte
    emitter.instruction("jmp __rt_gdn_strlen_x86");
    emitter.label("__rt_gdn_basename_x86");
    emitter.instruction("add rsi, rcx");                                        // step past the directory
    emitter.instruction("sub rdx, rcx");                                        // and shorten the name to match
    emitter.instruction("ret");
    emitter.label("__rt_gdn_end_x86");
    emitter.instruction("xor esi, esi");                                        // a null pointer ends the iteration
    emitter.instruction("xor edx, edx");
    emitter.instruction("ret");
}
