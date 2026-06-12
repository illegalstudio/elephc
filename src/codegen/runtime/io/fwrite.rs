//! Purpose:
//! Emits the `__rt_fwrite` runtime helper, which writes a buffer to a
//! descriptor after applying any attached write-direction stream filter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//! - The `fwrite` builtin emitter.
//!
//! Key details:
//! - When a write filter is attached the payload is copied into the dedicated
//!   `_stream_filter_buf` scratch, transformed in place, and written from there
//!   so the caller's string is never mutated.
//! - A payload larger than the 64 KiB scratch is written unfiltered; v1 stream
//!   filters target the common small-write case.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

const FILTER_BUF_SIZE: i64 = 65536;

/// fwrite: write a payload to a descriptor, applying a write filter if present.
/// Input:  AArch64 x0 = fd, x1 = pointer, x2 = length
///         x86_64  rdi = fd, rsi = pointer, rdx = length
/// Output: the number of bytes written.
pub fn emit_fwrite(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fwrite_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fwrite ---");
    emitter.label_global("__rt_fwrite");

    // -- phar:// write stream synthetic fd range (0x50000000..0x50000020) --
    emitter.instruction("mov w10, #0x5000");                                    // low half of the phar-write descriptor base
    emitter.instruction("lsl w10, w10, #16");                                   // form the full 0x50000000 phar-write descriptor base
    emitter.instruction("cmp x0, x10");                                         // is the descriptor below the phar-write range?
    emitter.instruction("b.lt __rt_fwrite_not_phar");                           // below the range: use normal stream dispatch
    emitter.instruction("add x11, x10, #32");                                   // upper bound for the 32 buffered PHAR write descriptors
    emitter.instruction("cmp x0, x11");                                         // is this inside the phar-write descriptor range?
    emitter.instruction("b.lt __rt_phar_write_append");                         // append the payload to the selected phar buffer
    emitter.label("__rt_fwrite_not_phar");

    // -- user-wrapper synthetic fd path (Phase 10 step 4) --
    emitter.instruction("mov w9, #0x4000");                                     // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
    emitter.instruction("lsl w9, w9, #16");                                     // shift into bits 30..16 to form 0x40000000
    emitter.instruction("cmp x0, x9");                                          // is this a synthetic user-wrapper fd?
    emitter.instruction("b.ge __rt_user_wrapper_fwrite");                       // dispatch into the wrapper's stream_write instead of issuing a write syscall

    // Frame (48 bytes): [0]=fd [8]=pointer [16]=length [32]=x29 [40]=x30.
    emitter.instruction("sub sp, sp, #48");                                     // frame for the saved write state
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the file descriptor
    emitter.instruction("str x1, [sp, #8]");                                    // save the payload pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the payload length

    // -- look up the write filter for this descriptor --
    abi::emit_symbol_address(emitter, "x9", "_stream_write_filters");
    emitter.instruction("ldrb w3, [x9, x0]");                                   // write filter id for this descriptor
    emitter.instruction("cbz w3, __rt_fwrite_direct");                          // no filter: write the payload directly
    emitter.instruction("cmp w3, #128");                                        // user-filter id range (>= USER_FILTER_ID_BASE)?
    emitter.instruction("b.ge __rt_fwrite_user_filter");                        // dispatch through the user filter method, then write the result
    emitter.instruction("cmp w3, #4");                                          // is the zlib.deflate write filter attached?
    emitter.instruction("b.eq __rt_fwrite_zlib");                               // hand the payload to the zlib deflate helper
    emitter.instruction("cmp w3, #10");                                         // is the bzip2.compress write filter attached?
    emitter.instruction("b.eq __rt_fwrite_bz2");                                // hand the payload to the bzip2 compress helper
    emitter.instruction("cmp w3, #12");                                         // is the convert.iconv write filter attached?
    emitter.instruction("b.eq __rt_fwrite_iconv");                              // hand the payload to the iconv write helper
    emitter.instruction(&format!("mov x9, #{}", FILTER_BUF_SIZE));              // filter scratch capacity
    emitter.instruction("cmp x2, x9");                                          // is the payload larger than the scratch?
    emitter.instruction("b.gt __rt_fwrite_direct");                             // oversized payloads are written unfiltered

    // -- copy the payload into the filter scratch --
    abi::emit_symbol_address(emitter, "x4", "_stream_filter_buf");
    emitter.instruction("mov x5, #0");                                          // copy index
    emitter.label("__rt_fwrite_copy");
    emitter.instruction("cmp x5, x2");                                          // copied every byte?
    emitter.instruction("b.ge __rt_fwrite_copy_done");                          // the payload is fully copied
    emitter.instruction("ldrb w6, [x1, x5]");                                   // load a payload byte
    emitter.instruction("strb w6, [x4, x5]");                                   // store it into the filter scratch
    emitter.instruction("add x5, x5, #1");                                      // advance the copy index
    emitter.instruction("b __rt_fwrite_copy");                                  // continue copying
    emitter.label("__rt_fwrite_copy_done");

    // -- transform the scratch copy and write from it --
    emitter.instruction("mov x1, x4");                                          // filter target = the scratch buffer
    emitter.instruction("str x1, [sp, #8]");                                    // the write reads the filtered scratch
    emitter.instruction("bl __rt_apply_stream_filter");                         // transform the scratch copy in place; x2 = (possibly compacted) length
    emitter.instruction("str x2, [sp, #16]");                                   // commit the post-filter length so strip_tags / similar shrinking filters write the compacted bytes only
    emitter.instruction("b __rt_fwrite_direct");                                // continue with the standard direct write

    // -- zlib.deflate filter: deflate-compress the payload into the stream --
    emitter.label("__rt_fwrite_zlib");
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd argument for the zlib deflate helper
    emitter.instruction("ldr x1, [sp, #8]");                                    // payload pointer argument
    emitter.instruction("ldr x2, [sp, #16]");                                   // payload length argument
    abi::emit_symbol_address(emitter, "x9", "_zlib_fwrite_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the deflate fwrite helper pointer
    emitter.instruction("blr x9");                                              // deflate-compress the payload, x0 = bytes consumed
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the frame
    emitter.instruction("ret");                                                 // return the helper's bytes-consumed count

    // -- bzip2.compress filter: bzip2-compress the payload into the stream --
    emitter.label("__rt_fwrite_bz2");
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd argument for the bzip2 compress helper
    emitter.instruction("ldr x1, [sp, #8]");                                    // payload pointer argument
    emitter.instruction("ldr x2, [sp, #16]");                                   // payload length argument
    abi::emit_symbol_address(emitter, "x9", "_bz2_fwrite_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the bzip2 compress fwrite helper pointer
    emitter.instruction("blr x9");                                              // bzip2-compress the payload, x0 = bytes consumed
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the frame
    emitter.instruction("ret");                                                 // return the helper's bytes-consumed count

    // -- convert.iconv write filter: transcode the payload into the stream --
    emitter.label("__rt_fwrite_iconv");
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd argument for the iconv write helper
    emitter.instruction("ldr x1, [sp, #8]");                                    // payload pointer argument
    emitter.instruction("ldr x2, [sp, #16]");                                   // payload length argument
    abi::emit_symbol_address(emitter, "x9", "_iconv_fwrite_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the iconv write helper pointer
    emitter.instruction("blr x9");                                              // transcode the payload, x0 = bytes consumed
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the frame
    emitter.instruction("ret");                                                 // return the helper's bytes-consumed count

    // -- user filter: dispatch through filter(string), then write the result --
    emitter.label("__rt_fwrite_user_filter");
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd into the user-filter dispatcher's first arg
    emitter.instruction("ldr x1, [sp, #8]");                                    // payload ptr → second arg
    emitter.instruction("ldr x2, [sp, #16]");                                   // payload len → third arg
    emitter.instruction("mov x3, #1");                                          // direction = 1 (write)
    emitter.instruction("bl __rt_apply_user_stream_filter");                    // x1/x2 ← user filter's transformed payload
    emitter.instruction("str x1, [sp, #8]");                                    // overwrite the payload-ptr slot with the filter result
    emitter.instruction("str x2, [sp, #16]");                                   // overwrite the payload-len slot with the filter result
    emitter.instruction("b __rt_fwrite_direct");                                // fall through to the standard direct-write path

    emitter.label("__rt_fwrite_direct");
    emitter.instruction("ldr x0, [sp, #0]");                                    // file descriptor
    emitter.instruction("ldr x1, [sp, #8]");                                    // payload pointer (original or filtered)
    emitter.instruction("ldr x2, [sp, #16]");                                   // payload length
    // -- TLS dispatch: route through elephc_tls_write when fd has an
    //    attached session (Phase 11 B3). --
    abi::emit_symbol_address(emitter, "x13", "_tls_sessions");
    emitter.instruction("ldr x14, [x13, x0, lsl #3]");                          // _tls_sessions[fd] handle (0 = plain TCP)
    emitter.instruction("cbz x14, __rt_fwrite_syscall");                        // no TLS attached → write syscall
    emitter.instruction("mov x0, x14");                                         // handle as first arg
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_write_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load runtime value
    emitter.instruction("blr x9");                                              // x0 = bytes written or -1
    emitter.instruction("b __rt_fwrite_return");                                // continue at target label
    emitter.label("__rt_fwrite_syscall");
    emitter.syscall(4);
    emitter.label("__rt_fwrite_return");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the frame
    emitter.instruction("ret");                                                 // return the byte count from write
}

/// Emits the Linux x86_64 stream runtime helper for fwrite.
fn emit_fwrite_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fwrite ---");
    emitter.label_global("__rt_fwrite");

    // -- phar:// write stream synthetic fd range (0x50000000..0x50000020) --
    emitter.instruction("mov r10d, 0x50000000");                                // the phar-write synthetic descriptor base
    emitter.instruction("cmp rdi, r10");                                        // is the descriptor below the phar-write range?
    emitter.instruction("jl __rt_fwrite_not_phar_x86");                         // below the range: use normal stream dispatch
    emitter.instruction("lea r11, [r10 + 32]");                                 // upper bound for the 32 buffered PHAR write descriptors
    emitter.instruction("cmp rdi, r11");                                        // is this inside the phar-write descriptor range?
    emitter.instruction("jl __rt_phar_write_append");                           // append the payload to the selected phar buffer
    emitter.label("__rt_fwrite_not_phar_x86");

    // -- user-wrapper synthetic fd path (Phase 10 step 4) --
    emitter.instruction("mov r9d, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rdi, r9");                                         // is this a synthetic user-wrapper fd?
    emitter.instruction("jge __rt_user_wrapper_fwrite");                        // dispatch into the wrapper's stream_write instead of issuing a write syscall

    // Frame (rbp-relative): [-8]=fd [-16]=pointer [-24]=length.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // frame for the saved write state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the file descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the payload pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the payload length

    // -- look up the write filter for this descriptor --
    abi::emit_symbol_address(emitter, "r9", "_stream_write_filters");           // write-filter table base
    emitter.instruction("movzx ecx, BYTE PTR [r9 + rdi]");                      // write filter id for this descriptor
    emitter.instruction("test rcx, rcx");                                       // is a write filter attached?
    emitter.instruction("jz __rt_fwrite_direct_x86");                           // no filter: write the payload directly
    emitter.instruction("cmp rcx, 128");                                        // user-filter id range (>= USER_FILTER_ID_BASE)?
    emitter.instruction("jge __rt_fwrite_user_filter_x86");                     // dispatch through the user filter method, then write the result
    emitter.instruction("cmp rcx, 4");                                          // is the zlib.deflate write filter attached?
    emitter.instruction("je __rt_fwrite_zlib_x86");                             // hand the payload to the zlib deflate helper
    emitter.instruction("cmp rcx, 10");                                         // is the bzip2.compress write filter attached?
    emitter.instruction("je __rt_fwrite_bz2_x86");                              // hand the payload to the bzip2 compress helper
    emitter.instruction("cmp rcx, 12");                                         // is the convert.iconv write filter attached?
    emitter.instruction("je __rt_fwrite_iconv_x86");                            // hand the payload to the iconv write helper
    emitter.instruction(&format!("cmp rdx, {}", FILTER_BUF_SIZE));              // is the payload larger than the scratch?
    emitter.instruction("jg __rt_fwrite_direct_x86");                           // oversized payloads are written unfiltered

    // -- copy the payload into the filter scratch --
    abi::emit_symbol_address(emitter, "r8", "_stream_filter_buf");              // filter scratch base
    emitter.instruction("xor r9, r9");                                          // copy index
    emitter.label("__rt_fwrite_copy_x86");
    emitter.instruction("cmp r9, rdx");                                         // copied every byte?
    emitter.instruction("jge __rt_fwrite_copy_done_x86");                       // the payload is fully copied
    emitter.instruction("movzx r10d, BYTE PTR [rsi + r9]");                     // load a payload byte
    emitter.instruction("mov BYTE PTR [r8 + r9], r10b");                        // store it into the filter scratch
    emitter.instruction("inc r9");                                              // advance the copy index
    emitter.instruction("jmp __rt_fwrite_copy_x86");                            // continue copying
    emitter.label("__rt_fwrite_copy_done_x86");

    // -- transform the scratch copy and write from it --
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // the write reads the filtered scratch
    emitter.instruction("mov rax, r8");                                         // filter target = the scratch buffer
    emitter.instruction("call __rt_apply_stream_filter");                       // transform the scratch copy in place; rdx = (possibly compacted) length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // commit the post-filter length so strip_tags / similar shrinking filters write only the compacted bytes
    emitter.instruction("jmp __rt_fwrite_direct_x86");                          // continue with the standard direct write

    // -- zlib.deflate filter: deflate-compress the payload into the stream --
    emitter.label("__rt_fwrite_zlib_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd argument for the zlib deflate helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // payload pointer argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // payload length argument
    abi::emit_load_symbol_to_reg(emitter, "r9", "_zlib_fwrite_fn", 0);          // load the deflate fwrite helper pointer
    emitter.instruction("call r9");                                             // deflate-compress the payload, rax = bytes consumed
    emitter.instruction("add rsp, 32");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the helper's bytes-consumed count

    // -- bzip2.compress filter: bzip2-compress the payload into the stream --
    emitter.label("__rt_fwrite_bz2_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd argument for the bzip2 compress helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // payload pointer argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // payload length argument
    abi::emit_load_symbol_to_reg(emitter, "r9", "_bz2_fwrite_fn", 0);           // load the bzip2 compress fwrite helper pointer
    emitter.instruction("call r9");                                             // bzip2-compress the payload, rax = bytes consumed
    emitter.instruction("add rsp, 32");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the helper's bytes-consumed count

    // -- convert.iconv write filter: transcode the payload into the stream --
    emitter.label("__rt_fwrite_iconv_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd argument for the iconv write helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // payload pointer argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // payload length argument
    abi::emit_load_symbol_to_reg(emitter, "r9", "_iconv_fwrite_fn", 0);         // load the iconv write helper pointer
    emitter.instruction("call r9");                                             // transcode the payload, rax = bytes consumed
    emitter.instruction("add rsp, 32");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the helper's bytes-consumed count

    // -- user filter: dispatch through filter(string), then write the result --
    emitter.label("__rt_fwrite_user_filter_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd into the user-filter dispatcher's first arg
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // payload ptr → second arg
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // payload len → third arg
    emitter.instruction("mov ecx, 1");                                          // direction = 1 (write)
    emitter.instruction("call __rt_apply_user_stream_filter");                  // rax/rdx ← user filter's transformed payload
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // overwrite the payload-ptr slot with the filter result
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // overwrite the payload-len slot with the filter result
    emitter.instruction("jmp __rt_fwrite_direct_x86");                          // fall through to the standard direct-write path

    emitter.label("__rt_fwrite_direct_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // file descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // payload pointer (original or filtered)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // payload length
    // -- TLS dispatch (Phase 11 B3) --
    abi::emit_symbol_address(emitter, "r10", "_tls_sessions");                  // load runtime data address
    emitter.instruction("mov r11, QWORD PTR [r10 + rdi * 8]");                  // _tls_sessions[fd] handle
    emitter.instruction("test r11, r11");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_fwrite_syscall_x86");                          // plain TCP → libc write
    emitter.instruction("mov rdi, r11");                                        // handle as first arg
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_tls_write_fn", 0);     // prepare SysV call argument
    emitter.instruction("call r9");                                             // rax = bytes written or -1
    emitter.instruction("jmp __rt_fwrite_return_x86");                          // continue at target label
    emitter.label("__rt_fwrite_syscall_x86");
    emitter.instruction("call write");                                          // write the payload through libc write()
    emitter.label("__rt_fwrite_return_x86");
    emitter.instruction("add rsp, 32");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the byte count from write
}
