//! Purpose:
//! Emits the `__rt_file`, `__rt_file_get_contents` runtime helper assembly for file.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the `__rt_file` runtime helper: reads a file and splits it into an array of lines.
///
/// Each line includes its trailing newline character (`\n`) except for the last line if the file
/// does not end with a newline. Returns a pointer to a runtime array of strings.
///
/// PHP's `$flags` bitmask is applied while the lines are produced, so no line is ever allocated
/// and then trimmed or discarded: `FILE_IGNORE_NEW_LINES` (2) drops a trailing `\n` plus a
/// preceding `\r` before the line is pushed, and `FILE_SKIP_EMPTY_LINES` (4) then suppresses a
/// line that is left empty. php-src evaluates the two in exactly that order, which is why
/// `FILE_SKIP_EMPTY_LINES` alone keeps a bare `"\n"` line (its length is still 1).
/// `FILE_USE_INCLUDE_PATH` (1) is accepted and has no effect: elephc resolves includes at compile
/// time and has no run-time include path, which matches PHP's own default empty `include_path`.
///
/// Stack frame (ARM64, 64 bytes):
/// - sp+#0..#7:   file data pointer and length (saved across calls)
/// - sp+#8..#15:  scratch
/// - sp+#16..#23: result array pointer (preserved across `__rt_array_push_str` calls)
/// - sp+#24..#31: saved scan cursor when calling `__rt_array_push_str`
/// - sp+#32..#39: the `$flags` bitmask (saved across every call)
/// - sp+#40..#47: scratch
/// - sp+#48..#63: saved x29/x30
///
/// Input: x0 = `$flags`, x1 = filename pointer, x2 = filename length.
///
/// On x86_64 Linux, delegates to `emit_file_linux_x86_64` which follows the System V AMD64 ABI.
pub fn emit_file(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_file_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: file ---");
    emitter.label_global("__rt_file");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer
    emitter.instruction("str x0, [sp, #32]");                                   // save the PHP $flags bitmask across every helper call

    // -- read entire file contents --
    emitter.instruction("bl __rt_file_get_contents");                           // read file, x1=ptr, x2=len
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save file data ptr and len on stack
    emitter.instruction("cbz x1, __rt_file_failed");                            // a null payload is a FAILED read, which PHP reports as false

    // -- create a new string array (capacity = 256 lines) --
    emitter.instruction("mov x0, #256");                                        // initial capacity of 256 elements
    emitter.instruction("mov x1, #16");                                         // element size = 16 bytes (ptr + len)
    emitter.instruction("bl __rt_array_new");                                   // create array, x0=array pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save array pointer on stack

    // -- scan file data for newlines and push each line --
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload file data ptr and total len
    emitter.instruction("mov x3, x1");                                          // x3 = current line start pointer
    emitter.instruction("add x4, x1, x2");                                      // x4 = pointer past end of data
    emitter.instruction("mov x5, #0");                                          // x5 = current line length counter

    emitter.label("__rt_file_scan");
    emitter.instruction("cmp x3, x4");                                          // check if we've reached end of data
    emitter.instruction("b.hs __rt_file_last");                                 // if at or past end, handle last line

    // -- check current byte --
    emitter.instruction("ldrb w6, [x3]");                                       // load current byte
    emitter.instruction("add x3, x3, #1");                                      // advance scan pointer
    emitter.instruction("add x5, x5, #1");                                      // increment line length
    emitter.instruction("cmp w6, #0x0A");                                       // compare with newline
    emitter.instruction("b.ne __rt_file_scan");                                 // if not newline, continue scanning

    // -- found newline: apply the PHP $flags bitmask, then push this line to array --
    emitter.instruction("sub x7, x3, x5");                                      // line start = current pos - line length
    emit_file_line_flags_aarch64(emitter, "scan");
    emitter.instruction("str x3, [sp, #24]");                                   // save scan pointer (push_str clobbers x3)
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload array pointer
    emitter.instruction("mov x1, x7");                                          // line start pointer
    emitter.instruction("mov x2, x5");                                          // line length after flag trimming
    emitter.instruction("bl __rt_array_push_str");                              // push line to array (x0 = possibly new array)
    emitter.instruction("str x0, [sp, #16]");                                   // update array pointer after possible growth
    emitter.instruction("ldr x3, [sp, #24]");                                   // restore scan pointer
    emitter.label("__rt_file_scan_skip");
    emitter.instruction("mov x5, #0");                                          // reset line length for next line

    // -- reload scan state and continue --
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload original data ptr and len
    emitter.instruction("add x4, x1, x2");                                      // recompute end pointer
    emitter.instruction("b __rt_file_scan");                                    // continue scanning

    // -- handle last line (no trailing newline) --
    emitter.label("__rt_file_last");
    emitter.instruction("cbz x5, __rt_file_ret");                               // if last line is empty, skip it
    emitter.instruction("sub x7, x3, x5");                                      // line start = current pos - line length
    emit_file_line_flags_aarch64(emitter, "last");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload array pointer
    emitter.instruction("mov x1, x7");                                          // line start pointer
    emitter.instruction("mov x2, x5");                                          // line length after flag trimming
    emitter.instruction("bl __rt_array_push_str");                              // push last line to array
    emitter.instruction("str x0, [sp, #16]");                                   // update array pointer after possible growth
    emitter.label("__rt_file_last_skip");

    // -- return array pointer --
    emitter.label("__rt_file_ret");
    emitter.instruction("ldr x0, [sp, #16]");                                   // return array pointer
    emitter.instruction("b __rt_file_epilogue");                                // skip the failure result on the success path

    // -- a read that failed answers null, which the lowering boxes as PHP's false --
    emitter.label("__rt_file_failed");
    emitter.instruction("mov x0, #0");                                          // null result distinguishes a failed read from an EMPTY file

    // -- restore frame and return --
    emitter.label("__rt_file_epilogue");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the ARM64 `$flags` handling applied to one complete `file()` line before it is pushed.
///
/// On entry `x7` is the line start pointer and `x5` its length including any terminator; on exit
/// `x5` is the length PHP would store. `FILE_IGNORE_NEW_LINES` (bit 1) removes a trailing `\n` and
/// then a trailing `\r`, so a CRLF file yields the same lines as an LF one. `FILE_SKIP_EMPTY_LINES`
/// (bit 2) is evaluated afterwards and branches to `__rt_file_<site>_skip`, suppressing the push
/// entirely; the ordering is what makes `FILE_SKIP_EMPTY_LINES` alone keep a bare `"\n"` line, the
/// same as php-src.
///
/// `site` names the caller so the mid-loop and trailing-line copies get distinct local labels.
fn emit_file_line_flags_aarch64(emitter: &mut Emitter, site: &str) {
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the PHP $flags bitmask
    emitter.instruction("tst x9, #2");                                          // FILE_IGNORE_NEW_LINES requested?
    emitter.instruction(&format!("b.eq __rt_file_{}_keep_eol", site));          // keep the terminator when the flag is clear
    emitter.instruction(&format!("cbz x5, __rt_file_{}_keep_eol", site));       // an already-empty line has no terminator to drop
    emitter.instruction("sub x10, x5, #1");                                     // index of the line's last byte
    emitter.instruction("ldrb w11, [x7, x10]");                                 // load the line's last byte
    emitter.instruction("cmp w11, #0x0A");                                      // is the line terminated by a line feed?
    emitter.instruction(&format!("b.ne __rt_file_{}_keep_eol", site));          // nothing to trim without a line feed
    emitter.instruction("mov x5, x10");                                         // drop the trailing line feed
    emitter.instruction(&format!("cbz x5, __rt_file_{}_keep_eol", site));       // a lone line feed leaves an empty line
    emitter.instruction("sub x10, x5, #1");                                     // index of the byte before the dropped line feed
    emitter.instruction("ldrb w11, [x7, x10]");                                 // load that byte to detect a CRLF terminator
    emitter.instruction("cmp w11, #0x0D");                                      // is the terminator a carriage return + line feed pair?
    emitter.instruction(&format!("b.ne __rt_file_{}_keep_eol", site));          // a bare line feed needs no further trimming
    emitter.instruction("mov x5, x10");                                         // drop the carriage return of a CRLF terminator
    emitter.label(&format!("__rt_file_{}_keep_eol", site));
    emitter.instruction("tst x9, #4");                                          // FILE_SKIP_EMPTY_LINES requested?
    emitter.instruction(&format!("b.eq __rt_file_{}_emit", site));              // keep every line when the flag is clear
    emitter.instruction(&format!("cbz x5, __rt_file_{}_skip", site));           // suppress a line left empty by the trimming above
    emitter.label(&format!("__rt_file_{}_emit", site));
}

/// Emits the x86_64 Linux variant of `__rt_file` using the System V AMD64 ABI.
///
/// Follows the same semantics as the ARM64 version: reads a file via `__rt_file_get_contents`,
/// splits on newlines, and returns a runtime array of strings. Each line includes its trailing
/// `\n` except the final line if the file has no trailing newline.
///
/// Stack frame (64 bytes, aligned to 16):
/// - rbp-8:   owned file payload pointer (preserved across array operations)
/// - rbp-16:  owned file payload length (preserved across array operations)
/// - rbp-24:  result array pointer (updated after each `__rt_array_push_str` call)
/// - rbp-32:  scan cursor spill (preserved across `__rt_array_push_str`)
/// - rbp-40:  the `$flags` bitmask (preserved across every call)
/// Caller-saved registers r8–r11 and rcx hold the scan state.
///
/// Input: rdi = `$flags`, plus the filename in the shared x86_64 elephc string registers.
fn emit_file_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: file ---");
    emitter.label_global("__rt_file");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while file() uses scan state and array spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the file payload, scan cursors, and result array pointer
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for the file payload, line scan cursors, and result array pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save the PHP $flags bitmask across every helper call

    emitter.instruction("call __rt_file_get_contents");                         // read the full file payload into an owned elephc string before splitting it into lines
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the owned file payload pointer across the later array allocation and line pushes
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the owned file payload length across the later array allocation and scan loop
    emitter.instruction("test rax, rax");                                       // a null payload is a FAILED read, which PHP reports as false
    emitter.instruction("jz __rt_file_failed");                                 // answer null rather than the empty array an EMPTY file produces

    emitter.instruction("mov rdi, 256");                                        // request an initial array capacity of 256 line slots for the line-splitting helper
    emitter.instruction("mov rsi, 16");                                         // request 16-byte elements so each slot can hold a string pointer and string length pair
    emitter.instruction("call __rt_array_new");                                 // allocate the result array that will collect the split file lines
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the result array pointer across the line-scan loop and possible growth helpers

    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // load the owned file payload pointer into the active scan cursor register
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // preserve the current line start pointer separately from the active scan cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // load the full file payload length before computing the end-of-buffer pointer
    emitter.instruction("lea r11, [r8 + r10]");                                 // compute the pointer one byte past the end of the owned file payload
    emitter.instruction("xor rcx, rcx");                                        // start the current line length counter at zero before scanning the file payload

    emitter.label("__rt_file_scan");
    emitter.instruction("cmp r8, r11");                                         // stop the scan loop once the active cursor reaches the end of the file payload
    emitter.instruction("jae __rt_file_last");                                  // finish with the final partial line once the end of the file payload is reached
    emitter.instruction("mov dl, BYTE PTR [r8]");                               // load the current file payload byte before deciding whether a line terminator was reached
    emitter.instruction("add r8, 1");                                           // advance the active scan cursor after consuming one source byte from the file payload
    emitter.instruction("add rcx, 1");                                          // extend the current line length after consuming one source byte from the file payload
    emitter.instruction("cmp dl, 0x0A");                                        // test whether the consumed byte is a line-feed terminator
    emitter.instruction("jne __rt_file_scan");                                  // continue scanning the current line until a terminating line-feed is found

    emit_file_line_flags_x86_64(emitter, "scan");
    emitter.instruction("mov QWORD PTR [rbp - 32], r8");                        // preserve the active scan cursor because array_push_str() is free to clobber caller-saved registers
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the result array pointer into the x86_64 append-helper receiver register
    emitter.instruction("mov rsi, r9");                                         // pass the current line start pointer as the string payload argument to array_push_str()
    emitter.instruction("mov rdx, rcx");                                        // pass the completed line length after flag trimming to array_push_str()
    emitter.instruction("call __rt_array_push_str");                            // append the completed line slice as an owned string in the result array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the updated array pointer after array_push_str() handles possible growth
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // restore the active scan cursor after the append helper clobbers caller-saved registers
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the full file payload length before rebuilding the end-of-buffer pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the owned file payload base pointer before rebuilding the end-of-buffer pointer
    emitter.instruction("add r11, r10");                                        // rebuild the pointer one byte past the end of the owned file payload after the helper call
    emitter.label("__rt_file_scan_skip");
    emitter.instruction("mov r9, r8");                                          // start the next line at the scan cursor immediately after the consumed newline
    emitter.instruction("xor rcx, rcx");                                        // reset the current line length counter before scanning the next line
    emitter.instruction("jmp __rt_file_scan");                                  // continue scanning the remaining bytes in the file payload for more newline terminators

    emitter.label("__rt_file_last");
    emitter.instruction("test rcx, rcx");                                       // detect whether the file ended with a partial line that still needs to be appended
    emitter.instruction("jz __rt_file_cleanup");                                // skip the final push when the file already ended exactly on a newline boundary
    emit_file_line_flags_x86_64(emitter, "last");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the result array pointer into the x86_64 append-helper receiver register
    emitter.instruction("mov rsi, r9");                                         // pass the trailing line start pointer as the string payload argument to array_push_str()
    emitter.instruction("mov rdx, rcx");                                        // pass the trailing line length after flag trimming to array_push_str()
    emitter.instruction("call __rt_array_push_str");                            // append the trailing partial line as an owned string in the result array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the updated array pointer after appending the trailing partial line
    emitter.label("__rt_file_last_skip");

    emitter.label("__rt_file_cleanup");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the result array pointer in the canonical x86_64 integer result register
    emitter.instruction("jmp __rt_file_epilogue");                              // skip the failure result on the success path

    // -- a read that failed answers null, which the lowering boxes as PHP's false --
    emitter.label("__rt_file_failed");
    emitter.instruction("xor eax, eax");                                        // null result distinguishes a failed read from an EMPTY file

    emitter.label("__rt_file_epilogue");
    emitter.instruction("add rsp, 64");                                         // release the temporary file payload and scan-state spill slots used by file()
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the line array
    emitter.instruction("ret");                                                 // return the array of file lines to the caller
}

/// Emits the x86_64 `$flags` handling applied to one complete `file()` line before it is pushed.
///
/// Mirrors [`emit_file_line_flags_aarch64`] instruction for instruction: on entry `r9` is the line
/// start pointer and `rcx` its length including any terminator, and on exit `rcx` is the length
/// PHP would store. `FILE_IGNORE_NEW_LINES` (bit 1) drops a trailing `\n` and then a trailing `\r`;
/// `FILE_SKIP_EMPTY_LINES` (bit 2) is evaluated afterwards and jumps to `__rt_file_<site>_skip`.
///
/// `site` names the caller so the mid-loop and trailing-line copies get distinct local labels.
fn emit_file_line_flags_x86_64(emitter: &mut Emitter, site: &str) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the PHP $flags bitmask
    emitter.instruction("test rax, 2");                                         // FILE_IGNORE_NEW_LINES requested?
    emitter.instruction(&format!("je __rt_file_{}_keep_eol", site));            // keep the terminator when the flag is clear
    emitter.instruction("test rcx, rcx");                                       // does the line have any bytes to trim?
    emitter.instruction(&format!("jz __rt_file_{}_keep_eol", site));            // an already-empty line has no terminator to drop
    emitter.instruction("mov rsi, rcx");                                        // copy the line length before computing the last byte index
    emitter.instruction("sub rsi, 1");                                          // index of the line's last byte
    emitter.instruction("mov dl, BYTE PTR [r9 + rsi]");                         // load the line's last byte
    emitter.instruction("cmp dl, 0x0A");                                        // is the line terminated by a line feed?
    emitter.instruction(&format!("jne __rt_file_{}_keep_eol", site));           // nothing to trim without a line feed
    emitter.instruction("mov rcx, rsi");                                        // drop the trailing line feed
    emitter.instruction("test rcx, rcx");                                       // did dropping the line feed leave an empty line?
    emitter.instruction(&format!("jz __rt_file_{}_keep_eol", site));            // a lone line feed leaves an empty line
    emitter.instruction("mov rsi, rcx");                                        // copy the trimmed length before probing the previous byte
    emitter.instruction("sub rsi, 1");                                          // index of the byte before the dropped line feed
    emitter.instruction("mov dl, BYTE PTR [r9 + rsi]");                         // load that byte to detect a CRLF terminator
    emitter.instruction("cmp dl, 0x0D");                                        // is the terminator a carriage return + line feed pair?
    emitter.instruction(&format!("jne __rt_file_{}_keep_eol", site));           // a bare line feed needs no further trimming
    emitter.instruction("mov rcx, rsi");                                        // drop the carriage return of a CRLF terminator
    emitter.label(&format!("__rt_file_{}_keep_eol", site));
    emitter.instruction("test rax, 4");                                         // FILE_SKIP_EMPTY_LINES requested?
    emitter.instruction(&format!("je __rt_file_{}_emit", site));                // keep every line when the flag is clear
    emitter.instruction("test rcx, rcx");                                       // is the line empty after the trimming above?
    emitter.instruction(&format!("jz __rt_file_{}_skip", site));                // suppress a line left empty by the trimming above
    emitter.label(&format!("__rt_file_{}_emit", site));
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::emit_file;

    /// `file()` must report a FAILED read as null, on BOTH architectures.
    ///
    /// The helper returned the line array unconditionally, so a missing path and an EMPTY
    /// file both answered the same empty array and `file()` could not have a `false` arm at
    /// all. The distinction exists one level down: `__rt_file_get_contents` answers a null
    /// payload pointer on failure, while an empty file goes through `__rt_heap_alloc`, which
    /// rounds a zero-byte request up to 8 and hands back a real pointer.
    ///
    /// Pinned on the emitted assembly of each target rather than by running the program,
    /// because a behavioural test only ever exercises the host's emitter — the same blind
    /// spot that let `scandir()` segfault on one architecture for as long as it did.
    #[test]
    fn file_reports_a_failed_read_as_null_on_every_target() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(Target::new(platform, arch));
            emit_file(&mut emitter);
            let asm = emitter.output();
            let read_at = asm
                .find("__rt_file_get_contents")
                .expect("file() reads the whole file first");
            let scan_at = asm
                .find("__rt_file_scan:")
                .expect("file() scans the payload for line terminators");
            let guard = &asm[read_at..scan_at];
            let branches_away = guard.contains("cbz x1, __rt_file_failed")
                || guard.contains("jz __rt_file_failed");
            assert!(
                branches_away,
                "{arch:?}: a null payload must skip the scan, or a failed read answers an \
                 empty array instead of false:\n{guard}"
            );
            assert!(
                asm.contains("__rt_file_failed:"),
                "{arch:?}: the failure path must be emitted"
            );
        }
    }
}
