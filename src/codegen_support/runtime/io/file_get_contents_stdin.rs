//! Reads php://stdin through the shared stream readers without owning process stdin.
//! A pure URL probe preserves the filename; the reader owns only a duplicated descriptor.

use crate::codegen_support::{abi, emit::Emitter, platform::{Arch, Platform}};
use super::file_get_contents_range::FILE_GET_CONTENTS_SEEK_MESSAGES;
use super::file_get_contents::FILE_GET_CONTENTS_FAILED_WARNING;

/// The URL probe returns its flag in x3/rdi while preserving the string pair.
/// The reader takes the file-range ABI and returns an owned string pair (null on failure).
pub fn emit_file_get_contents_stdin(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

fn emit_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_is_php_stdin");
    emitter.instruction("cmp x2, #11");                                         // a standard-input URL has exactly eleven bytes
    emitter.instruction("b.ne __rt_fgc_stdin_not");                             // leave other filenames unchanged
    emitter.instruction("ldrb w9, [x1, #0]");                                   // load one URL byte
    emitter.instruction("orr w9, w9, #0x20");                                   // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp w9, #112");                                        // compare the folded URL byte
    emitter.instruction("b.ne __rt_fgc_stdin_not");                             // leave other filenames unchanged
    emitter.instruction("ldrb w9, [x1, #1]");                                   // load one URL byte
    emitter.instruction("orr w9, w9, #0x20");                                   // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp w9, #104");                                        // compare the folded URL byte
    emitter.instruction("b.ne __rt_fgc_stdin_not");                             // leave other filenames unchanged
    emitter.instruction("ldrb w9, [x1, #2]");                                   // load one URL byte
    emitter.instruction("orr w9, w9, #0x20");                                   // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp w9, #112");                                        // compare the folded URL byte
    emitter.instruction("b.ne __rt_fgc_stdin_not");                             // leave other filenames unchanged
    emitter.instruction("ldrb w9, [x1, #3]");                                   // load one URL byte
    emitter.instruction("cmp w9, #58");                                         // compare the folded URL byte
    emitter.instruction("b.ne __rt_fgc_stdin_not");                             // leave other filenames unchanged
    emitter.instruction("ldrb w9, [x1, #4]");                                   // load one URL byte
    emitter.instruction("cmp w9, #47");                                         // compare the folded URL byte
    emitter.instruction("b.ne __rt_fgc_stdin_not");                             // leave other filenames unchanged
    emitter.instruction("ldrb w9, [x1, #5]");                                   // load one URL byte
    emitter.instruction("cmp w9, #47");                                         // compare the folded URL byte
    emitter.instruction("b.ne __rt_fgc_stdin_not");                             // leave other filenames unchanged
    emitter.instruction("ldrb w9, [x1, #6]");                                   // load one URL byte
    emitter.instruction("orr w9, w9, #0x20");                                   // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp w9, #115");                                        // compare the folded URL byte
    emitter.instruction("b.ne __rt_fgc_stdin_not");                             // leave other filenames unchanged
    emitter.instruction("ldrb w9, [x1, #7]");                                   // load one URL byte
    emitter.instruction("orr w9, w9, #0x20");                                   // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp w9, #116");                                        // compare the folded URL byte
    emitter.instruction("b.ne __rt_fgc_stdin_not");                             // leave other filenames unchanged
    emitter.instruction("ldrb w9, [x1, #8]");                                   // load one URL byte
    emitter.instruction("orr w9, w9, #0x20");                                   // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp w9, #100");                                        // compare the folded URL byte
    emitter.instruction("b.ne __rt_fgc_stdin_not");                             // leave other filenames unchanged
    emitter.instruction("ldrb w9, [x1, #9]");                                   // load one URL byte
    emitter.instruction("orr w9, w9, #0x20");                                   // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp w9, #105");                                        // compare the folded URL byte
    emitter.instruction("b.ne __rt_fgc_stdin_not");                             // leave other filenames unchanged
    emitter.instruction("ldrb w9, [x1, #10]");                                  // load one URL byte
    emitter.instruction("orr w9, w9, #0x20");                                   // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp w9, #110");                                        // compare the folded URL byte
    emitter.instruction("b.ne __rt_fgc_stdin_not");                             // leave other filenames unchanged
    emitter.instruction("mov x3, #1");                                          // recognized standard-input URL
    emitter.instruction("ret");                                                 // preserve the filename pair
    emitter.label("__rt_fgc_stdin_not");
    emitter.instruction("mov x3, #0");                                          // report a filename miss while preserving its pair
    emitter.instruction("ret");                                                 // let the ordinary file or wrapper reader handle it
    emitter.label_global("__rt_file_get_contents_stdin");
    emitter.instruction("sub sp, sp, #80");                                     // reserve fd, range, result and frame slots
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller frame
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame
    emitter.instruction("stp x3, x4, [sp]");                                    // preserve the offset and byte cap
    emitter.instruction("str x5, [sp, #16]");                                   // preserve whether a cap was supplied
    emitter.instruction("mov x0, #0");                                          // duplicate process stdin
    emitter.bl_c("dup");                                                                                  // only the duplicate is owned by this read
    emitter.instruction("cmp w0, #0");                                          // test the signed C int result
    emitter.instruction("b.lt __rt_fgc_stdin_open_failed");                     // an invalid stdin descriptor is a read failure
    emitter.instruction("sxtw x0, w0");                                         // normalize the successful descriptor
    emitter.instruction("str x0, [sp, #24]");                                   // save the owned duplicate
    emitter.instruction("ldr x1, [sp]");                                        // load the requested offset
    emitter.instruction("cbz x1, __rt_fgc_stdin_read");                         // zero preserves the current stream position
    emitter.instruction("mov x2, #0");                                          // positive offsets are absolute
    emitter.instruction("cmp x1, #0");                                          // negative offsets seek relative to the end
    emitter.instruction("mov x9, #2");                                          // SEEK_END is two
    emitter.instruction("csel x2, x9, x2, lt");                                 // select the appropriate seek origin
    emitter.bl_c("lseek");                                                                                  // let the descriptor decide whether seeking is supported
    emitter.instruction("cmn x0, #1");                                          // lseek returns minus one on failure
    emitter.instruction("b.ne __rt_fgc_stdin_read");                            // a successful seek may proceed
    emitter.bl_c(if emitter.platform == Platform::MacOS { "__error" } else { "__errno_location" }); // inspect the target thread-local errno
    emitter.instruction("ldr w9, [x0]");                                        // read the failed seek errno
    emitter.instruction("cmp w9, #29");                                         // ESPIPE identifies a non-seekable stream
    emitter.instruction("cset w9, eq");                                         // remember whether PHP needs the extra warning
    emitter.instruction("str w9, [sp, #48]");                                   // preserve the warning flag across close
    emitter.instruction("ldr x0, [sp, #24]");                                   // close the owned duplicate before diagnostics
    abi::emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("strb wzr, [x9, x0]");                                  // retire EOF state for the private duplicate
    emitter.bl_c("close");                                                     // close only the duplicated descriptor
    emitter.instruction("ldr w9, [sp, #48]");                                   // reload the non-seekable marker
    emitter.instruction("cbz w9, __rt_fgc_stdin_seek_message");                 // ordinary seek errors need only the position warning
    abi::emit_symbol_address(emitter, "x1", "_diag_fgc_unseekable_msg");
    emitter.instruction("mov x2, #62");                                         // pass the complete warning byte count
    emitter.instruction("bl __rt_diag_warning");                                // report that this descriptor cannot seek
    emitter.label("__rt_fgc_stdin_seek_message");
    emitter.instruction("ldr x0, [sp]");                                        // load the requested position
    emitter.instruction("bl __rt_itoa");                                        // format the signed seek position
    emitter.instruction("mov x3, x1");                                          // use the digits as the right concat operand
    emitter.instruction("mov x4, x2");                                          // use the digit count as its length
    abi::emit_symbol_address(emitter, "x1", "_diag_fgc_seek_prefix_msg");
    emitter.instruction(
        &format!("mov x2, #{}", FILE_GET_CONTENTS_SEEK_MESSAGES[0].1.len()),
    );                                                                          // warning prefix length
    emitter.instruction("bl __rt_concat");                                      // prepend the file_get_contents seek diagnostic
    abi::emit_symbol_address(emitter, "x3", "_diag_fgc_seek_suffix_msg");
    emitter.instruction(
        &format!("mov x4, #{}", FILE_GET_CONTENTS_SEEK_MESSAGES[1].1.len()),
    );                                                                          // warning suffix length
    emitter.instruction("bl __rt_concat");                                      // finish the seek diagnostic
    emitter.instruction("bl __rt_diag_warning");                                // report the failed position
    emitter.instruction("b __rt_fgc_stdin_false");                              // return PHP false after a failed seek
    emitter.label("__rt_fgc_stdin_read");
    emitter.instruction("ldr x0, [sp, #24]");                                   // pass the duplicated descriptor to the stream reader
    emitter.instruction("ldr x9, [sp, #16]");                                   // was a byte cap supplied
    emitter.instruction("cbz x9, __rt_fgc_stdin_unbounded");                    // an absent cap reads through EOF
    emitter.instruction("ldr x1, [sp, #8]");                                    // pass the requested maximum byte count
    emitter.instruction("bl __rt_stream_get_contents_bounded");                 // consume at most the requested bytes
    emitter.instruction("b __rt_fgc_stdin_persist");                            // both readers share owned result handling
    emitter.label("__rt_fgc_stdin_unbounded");
    emitter.instruction("bl __rt_stream_get_contents");                         // read every remaining byte, including short pipe reads
    emitter.label("__rt_fgc_stdin_persist");
    emitter.instruction("bl __rt_str_persist_owned");                           // transfer heap results or persist concat scratch
    emitter.instruction("stp x1, x2, [sp, #32]");                               // preserve the owned result while closing the duplicate
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the duplicated descriptor
    abi::emit_symbol_address(emitter, "x9", "_eof_flags");
    emitter.instruction("strb wzr, [x9, x0]");                                  // retire EOF state for the private duplicate
    emitter.bl_c("close");                                                     // close only the duplicated descriptor
    emitter.instruction("ldp x1, x2, [sp, #32]");                               // restore the owned pointer and byte count
    emitter.instruction("b __rt_fgc_stdin_return");                             // return the completed read
    emitter.label("__rt_fgc_stdin_open_failed");
    abi::emit_symbol_address(emitter, "x1", "_diag_file_get_contents_failed_msg");
    emitter.instruction(
        &format!("mov x2, #{}", FILE_GET_CONTENTS_FAILED_WARNING.len()),
    );                                                                          // existing open-failure diagnostic length
    emitter.instruction("bl __rt_diag_warning");                                // report the failed duplicate
    emitter.label("__rt_fgc_stdin_false");
    emitter.instruction("mov x1, #0");                                          // a null string pointer boxes as PHP false
    emitter.instruction("mov x2, #0");                                          // clear the unused failure length
    emitter.label("__rt_fgc_stdin_return");
    emitter.instruction("mov x3, #1");                                          // mark the stdin URL handled
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the handled result

}

fn emit_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_is_php_stdin");
    emitter.instruction("cmp rdx, 11");                                         // a standard-input URL has exactly eleven bytes
    emitter.instruction("jne __rt_fgc_stdin_not_x86");                          // leave other filenames unchanged
    emitter.instruction("mov r8b, BYTE PTR [rax + 0]");                         // load one URL byte
    emitter.instruction("or r8b, 0x20");                                        // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp r8b, 112");                                        // compare the folded URL byte
    emitter.instruction("jne __rt_fgc_stdin_not_x86");                          // leave other filenames unchanged
    emitter.instruction("mov r8b, BYTE PTR [rax + 1]");                         // load one URL byte
    emitter.instruction("or r8b, 0x20");                                        // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp r8b, 104");                                        // compare the folded URL byte
    emitter.instruction("jne __rt_fgc_stdin_not_x86");                          // leave other filenames unchanged
    emitter.instruction("mov r8b, BYTE PTR [rax + 2]");                         // load one URL byte
    emitter.instruction("or r8b, 0x20");                                        // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp r8b, 112");                                        // compare the folded URL byte
    emitter.instruction("jne __rt_fgc_stdin_not_x86");                          // leave other filenames unchanged
    emitter.instruction("mov r8b, BYTE PTR [rax + 3]");                         // load one URL byte
    emitter.instruction("cmp r8b, 58");                                         // compare the folded URL byte
    emitter.instruction("jne __rt_fgc_stdin_not_x86");                          // leave other filenames unchanged
    emitter.instruction("mov r8b, BYTE PTR [rax + 4]");                         // load one URL byte
    emitter.instruction("cmp r8b, 47");                                         // compare the folded URL byte
    emitter.instruction("jne __rt_fgc_stdin_not_x86");                          // leave other filenames unchanged
    emitter.instruction("mov r8b, BYTE PTR [rax + 5]");                         // load one URL byte
    emitter.instruction("cmp r8b, 47");                                         // compare the folded URL byte
    emitter.instruction("jne __rt_fgc_stdin_not_x86");                          // leave other filenames unchanged
    emitter.instruction("mov r8b, BYTE PTR [rax + 6]");                         // load one URL byte
    emitter.instruction("or r8b, 0x20");                                        // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp r8b, 115");                                        // compare the folded URL byte
    emitter.instruction("jne __rt_fgc_stdin_not_x86");                          // leave other filenames unchanged
    emitter.instruction("mov r8b, BYTE PTR [rax + 7]");                         // load one URL byte
    emitter.instruction("or r8b, 0x20");                                        // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp r8b, 116");                                        // compare the folded URL byte
    emitter.instruction("jne __rt_fgc_stdin_not_x86");                          // leave other filenames unchanged
    emitter.instruction("mov r8b, BYTE PTR [rax + 8]");                         // load one URL byte
    emitter.instruction("or r8b, 0x20");                                        // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp r8b, 100");                                        // compare the folded URL byte
    emitter.instruction("jne __rt_fgc_stdin_not_x86");                          // leave other filenames unchanged
    emitter.instruction("mov r8b, BYTE PTR [rax + 9]");                         // load one URL byte
    emitter.instruction("or r8b, 0x20");                                        // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp r8b, 105");                                        // compare the folded URL byte
    emitter.instruction("jne __rt_fgc_stdin_not_x86");                          // leave other filenames unchanged
    emitter.instruction("mov r8b, BYTE PTR [rax + 10]");                        // load one URL byte
    emitter.instruction("or r8b, 0x20");                                        // PHP stream names are ASCII case insensitive
    emitter.instruction("cmp r8b, 110");                                        // compare the folded URL byte
    emitter.instruction("jne __rt_fgc_stdin_not_x86");                          // leave other filenames unchanged
    emitter.instruction("mov edi, 1");                                          // recognized standard-input URL
    emitter.instruction("ret");                                                 // preserve the filename pair
    emitter.label("__rt_fgc_stdin_not_x86");
    emitter.instruction("xor edi, edi");                                        // report a filename miss while preserving its pair
    emitter.instruction("ret");                                                 // let the ordinary reader handle this filename
    emitter.label_global("__rt_file_get_contents_stdin");
    emitter.instruction("push rbp");                                            // preserve the caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 64");                                         // reserve aligned fd, range and result slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the requested offset
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested byte cap
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save whether a cap was supplied
    emitter.instruction("xor edi, edi");                                        // duplicate process stdin
    emitter.instruction("call dup");                                            // only the duplicate is owned by this read
    emitter.instruction("test eax, eax");                                       // test the signed C int result
    emitter.instruction("js __rt_fgc_stdin_open_failed_x86");                   // an invalid stdin descriptor is a read failure
    emitter.instruction("movsxd rax, eax");                                     // normalize the successful descriptor
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the owned duplicate
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // load the requested offset
    emitter.instruction("test rsi, rsi");                                       // zero preserves the current stream position
    emitter.instruction("jz __rt_fgc_stdin_read_x86");                          // skip seeking for a zero offset
    emitter.instruction("mov rdi, rax");                                        // pass the duplicated descriptor
    emitter.instruction("xor edx, edx");                                        // positive offsets are absolute
    emitter.instruction("test rsi, rsi");                                       // negative offsets seek relative to the end
    emitter.instruction("jns __rt_fgc_stdin_seek_x86");                         // keep SEEK_SET for positive offsets
    emitter.instruction("mov edx, 2");                                          // use SEEK_END for a negative offset
    emitter.label("__rt_fgc_stdin_seek_x86");
    emitter.instruction("call lseek");                                          // let the descriptor decide whether seeking is supported
    emitter.instruction("cmp rax, -1");                                         // lseek returns minus one on failure
    emitter.instruction("jne __rt_fgc_stdin_read_x86");                         // a successful seek may proceed
    emitter.instruction("call __errno_location");                               // inspect the target thread-local errno
    emitter.instruction("cmp DWORD PTR [rax], 29");                             // ESPIPE identifies a non-seekable stream
    emitter.instruction("sete al");                                             // remember whether PHP needs the extra warning
    emitter.instruction("movzx eax, al");                                       // normalize the warning marker
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve the marker across close
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // close the owned duplicate before diagnostics
    abi::emit_symbol_address(emitter, "r10", "_eof_flags");
    emitter.instruction("mov BYTE PTR [r10 + rdi], 0");                         // retire EOF state for the private duplicate
    emitter.instruction("call close");                                          // close only the duplicated descriptor
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                         // does PHP need the non-seekable diagnostic
    emitter.instruction("je __rt_fgc_stdin_seek_message_x86");                  // ordinary seek errors need only the position warning
    abi::emit_symbol_address(emitter, "rdi", "_diag_fgc_unseekable_msg");
    emitter.instruction("mov esi, 62");                                         // complete warning byte count
    emitter.instruction("call __rt_diag_warning");                              // report that this descriptor cannot seek
    emitter.label("__rt_fgc_stdin_seek_message_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // load the requested position
    emitter.instruction("call __rt_itoa");                                      // format the signed seek position
    emitter.instruction("mov rcx, rax");                                        // use the digits as the right concat operand
    emitter.instruction("mov r8, rdx");                                         // use their byte count as its length
    abi::emit_symbol_address(emitter, "rax", "_diag_fgc_seek_prefix_msg");
    emitter.instruction(
        &format!("mov rdx, {}", FILE_GET_CONTENTS_SEEK_MESSAGES[0].1.len()),
    );                                                                          // warning prefix length
    emitter.instruction("call __rt_concat");                                    // prepend the failed-seek diagnostic
    abi::emit_symbol_address(emitter, "rcx", "_diag_fgc_seek_suffix_msg");
    emitter.instruction(
        &format!("mov r8, {}", FILE_GET_CONTENTS_SEEK_MESSAGES[1].1.len()),
    );                                                                          // warning suffix length
    emitter.instruction("call __rt_concat");                                    // finish the seek diagnostic
    emitter.instruction("mov rdi, rax");                                        // pass the complete warning pointer
    emitter.instruction("mov rsi, rdx");                                        // pass its byte count
    emitter.instruction("call __rt_diag_warning");                              // report the failed position
    emitter.instruction("jmp __rt_fgc_stdin_false_x86");                        // return PHP false after a failed seek
    emitter.label("__rt_fgc_stdin_read_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the duplicated descriptor
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // was a byte cap supplied
    emitter.instruction("je __rt_fgc_stdin_unbounded_x86");                     // an absent cap reads through EOF
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the requested maximum byte count
    emitter.instruction("call __rt_stream_get_contents_bounded");               // consume at most the requested bytes
    emitter.instruction("jmp __rt_fgc_stdin_persist_x86");                      // both readers share owned result handling
    emitter.label("__rt_fgc_stdin_unbounded_x86");
    emitter.instruction("call __rt_stream_get_contents");                       // read every remaining byte, including short pipe reads
    emitter.label("__rt_fgc_stdin_persist_x86");
    emitter.instruction("call __rt_str_persist_owned");                         // transfer heap results or persist concat scratch
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the owned result pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // preserve its byte count across close
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the duplicated descriptor
    abi::emit_symbol_address(emitter, "r10", "_eof_flags");
    emitter.instruction("mov BYTE PTR [r10 + rdi], 0");                         // retire EOF state for the private duplicate
    emitter.instruction("call close");                                          // close only the duplicated descriptor
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // restore the owned result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // restore the returned byte count
    emitter.instruction("jmp __rt_fgc_stdin_return_x86");                       // return the completed read
    emitter.label("__rt_fgc_stdin_open_failed_x86");
    abi::emit_symbol_address(emitter, "rdi", "_diag_file_get_contents_failed_msg");
    emitter.instruction(
        &format!("mov esi, {}", FILE_GET_CONTENTS_FAILED_WARNING.len()),
    );                                                                          // existing open-failure diagnostic length
    emitter.instruction("call __rt_diag_warning");                              // report the failed duplicate
    emitter.label("__rt_fgc_stdin_false_x86");
    emitter.instruction("xor eax, eax");                                        // a null string pointer boxes as PHP false
    emitter.instruction("xor edx, edx");                                        // clear the unused failure length
    emitter.label("__rt_fgc_stdin_return_x86");
    emitter.instruction("mov edi, 1");                                          // mark the stdin URL handled
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame
    emitter.instruction("ret");                                                 // return the handled result

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Each supported ABI must duplicate stdin, preserve owned results and support byte caps.
    #[test]
    fn stdin_reader_is_emitted_for_every_supported_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_file_get_contents_stdin(&mut emitter);
            let asm = emitter.output();
            for symbol in ["__rt_is_php_stdin", "__rt_file_get_contents_stdin", "__rt_stream_get_contents_bounded", "__rt_str_persist_owned"] {
                assert!(asm.contains(symbol), "{name}: missing {symbol}");
            }
            assert!(!asm.contains("__rt_fclose"), "{name}: no nonexistent close helper");
            if name == "linux-x86_64" {
                assert!(asm.contains("test eax, eax"), "{name}: dup returns a signed C int");
                assert!(asm.contains("call __errno_location"), "{name}: target errno accessor");
            } else {
                assert!(asm.contains("cmp w0, #0"), "{name}: dup returns a signed C int");
                let errno = if name.starts_with("linux") { "__errno_location" } else { "__error" };
                assert!(asm.contains(errno), "{name}: target errno accessor");
            }
        }
    }
}
