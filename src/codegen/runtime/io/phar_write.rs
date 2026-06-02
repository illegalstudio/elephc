//! Purpose:
//! Emits the phar-write runtime: `__rt_phar_write_open`, `__rt_phar_write_append`,
//! and `__rt_phar_write_finalize`. Together they implement Milestone-1 writing of
//! a single uncompressed `phar://` entry by buffering the archive in memory and
//! flushing it to disk on `fclose()`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` (and the minimal x86
//!   runtime) via `crate::codegen::runtime::io`.
//! - `__rt_phar_write_open` from the `fopen("phar://...","w")` emitter.
//! - `__rt_phar_write_append` from `__rt_fwrite` when the descriptor is the phar-
//!   write synthetic fd `0x50000000`.
//! - `__rt_phar_write_finalize` from the `fclose` emitter for that same fd.
//!
//! Key details:
//! - State lives in the fixed `.bss` globals `_phar_write_out` (the archive buffer,
//!   template prefix followed by the entry content), `_phar_write_len` (bytes used),
//!   `_phar_write_tpl_len` (template prefix length), and `_phar_write_path_ptr` /
//!   `_phar_write_path_len` (the on-disk archive path the emitter records).
//! - The template prefix carries a one-file manifest whose uncompressed-size,
//!   compressed-size, and crc32 fields are zero placeholders; finalize patches them
//!   in place (little-endian u32) before writing, then reuses `__rt_crc32` and
//!   `__rt_file_put_contents`. Milestone-1: one phar-write stream at a time,
//!   content bounded by the buffer size, no signature.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits the phar-write runtime routines (open/append/finalize) for the active
/// target. On AArch64 they are emitted inline here; on x86_64 the work is
/// delegated to [`emit_phar_write_linux_x86_64`].
pub fn emit_phar_write(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_phar_write_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: phar_write open ---");
    emitter.label_global("__rt_phar_write_open");
    // __rt_phar_write_open(x0 = template ptr, x1 = template len): copy the
    // template prefix into the archive buffer and seed the length counters.
    abi::emit_symbol_address(emitter, "x9", "_phar_write_out");
    emitter.instruction("mov x10, #0");                                         // copy index = 0
    emitter.label("__rt_phar_write_open_loop");
    emitter.instruction("cmp x10, x1");                                         // copied every template byte?
    emitter.instruction("b.ge __rt_phar_write_open_done");                      // template fully copied into the buffer
    emitter.instruction("ldrb w11, [x0, x10]");                                 // load a template byte
    emitter.instruction("strb w11, [x9, x10]");                                 // store it into _phar_write_out
    emitter.instruction("add x10, x10, #1");                                    // advance the copy index
    emitter.instruction("b __rt_phar_write_open_loop");                         // continue copying the template
    emitter.label("__rt_phar_write_open_done");
    abi::emit_symbol_address(emitter, "x12", "_phar_write_len");
    emitter.instruction("str x1, [x12]");                                       // buffer length starts at the template length
    abi::emit_symbol_address(emitter, "x12", "_phar_write_tpl_len");
    emitter.instruction("str x1, [x12]");                                       // record the template length for finalize
    emitter.instruction("ret");                                                 // return to the fopen caller
    emitter.blank();
    emitter.comment("--- runtime: phar_write append ---");
    emitter.label_global("__rt_phar_write_append");
    // __rt_phar_write_append(x1 = payload ptr, x2 = payload len; x0 = fd, ignored):
    // append the payload to the buffer and return the byte count, like write().
    abi::emit_symbol_address(emitter, "x9", "_phar_write_len");
    emitter.instruction("ldr x10, [x9]");                                       // current buffer length (template + prior writes)
    abi::emit_symbol_address(emitter, "x11", "_phar_write_out");
    emitter.instruction("add x11, x11, x10");                                   // append destination = buffer base + current length
    emitter.instruction("mov x12, #0");                                         // copy index = 0
    emitter.label("__rt_phar_write_append_loop");
    emitter.instruction("cmp x12, x2");                                         // appended every payload byte?
    emitter.instruction("b.ge __rt_phar_write_append_done");                    // payload fully appended
    emitter.instruction("ldrb w13, [x1, x12]");                                 // load a payload byte
    emitter.instruction("strb w13, [x11, x12]");                                // store it into the phar-write buffer
    emitter.instruction("add x12, x12, #1");                                    // advance the copy index
    emitter.instruction("b __rt_phar_write_append_loop");                       // continue appending the payload
    emitter.label("__rt_phar_write_append_done");
    emitter.instruction("add x10, x10, x2");                                    // grow the buffer length by the payload size
    emitter.instruction("str x10, [x9]");                                       // commit the new buffer length
    emitter.instruction("mov x0, x2");                                          // fwrite() returns the number of bytes written
    emitter.instruction("ret");                                                 // return to the fwrite caller
    emitter.blank();
    emitter.comment("--- runtime: phar_write finalize ---");
    emitter.label_global("__rt_phar_write_finalize");
    // __rt_phar_write_finalize(): patch the manifest size/crc fields, then flush the
    // buffered archive to its on-disk path. Returns 1 (fclose success).
    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate a 16-byte frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    // -- compute the content length and the entry anchor --
    abi::emit_symbol_address(emitter, "x9", "_phar_write_len");
    emitter.instruction("ldr x9, [x9]");                                        // total buffer length (template + content)
    abi::emit_symbol_address(emitter, "x10", "_phar_write_tpl_len");
    emitter.instruction("ldr x10, [x10]");                                      // template prefix length
    emitter.instruction("sub x11, x9, x10");                                    // content length = total - template
    abi::emit_symbol_address(emitter, "x12", "_phar_write_out");
    emitter.instruction("add x13, x12, x10");                                   // entry anchor = buffer base + template length
    // -- patch the manifest size fields (little-endian u32) --
    emitter.instruction("str w11, [x13, #-24]");                                // uncompressed size = content length
    emitter.instruction("str w11, [x13, #-16]");                                // compressed size = content length (stored uncompressed)
    // -- checksum the entry content --
    emitter.instruction("mov x1, x13");                                         // crc32 input pointer = entry content
    emitter.instruction("mov x2, x11");                                         // crc32 input length = content length
    emitter.instruction("bl __rt_crc32");                                       // x0 = CRC-32 of the entry content
    emitter.instruction("str w0, [x13, #-12]");                                 // patch the manifest crc32 field
    // -- append the SHA1 signature trailer: raw-sha1(20) ++ LE32(0x0002) ++ "GBMB".
    //    PHP hashes the whole archive (stub+manifest+data) up to the trailer, which
    //    is exactly _phar_write_out[0.._phar_write_len] at this point. --
    abi::emit_symbol_address(emitter, "x10", "_phar_write_out");
    abi::emit_symbol_address(emitter, "x9", "_phar_write_len");
    emitter.instruction("ldr x11, [x9]");                                       // length so far (everything before the signature)
    emitter.instruction("mov x0, x10");                                         // CC_SHA1 data = archive buffer base
    emitter.instruction("mov w1, w11");                                         // CC_SHA1 length (CC_LONG) = current archive length
    emitter.instruction("add x2, x10, x11");                                    // CC_SHA1 md = buffer + length (write 20 raw bytes past the data)
    emitter.bl_c("CC_SHA1");                                                    // compute the raw 20-byte SHA1 digest in place
    abi::emit_symbol_address(emitter, "x10", "_phar_write_out");
    abi::emit_symbol_address(emitter, "x9", "_phar_write_len");
    emitter.instruction("ldr x11, [x9]");                                       // reload length (CC_SHA1 clobbered caller-saved regs)
    emitter.instruction("add x12, x10, x11");                                   // trailer base = buffer + length (raw digest occupies +0..+20)
    emitter.instruction("mov w13, #2");                                         // signature type 0x0002 = Phar::SHA1
    emitter.instruction("str w13, [x12, #20]");                                 // little-endian signature type after the 20 digest bytes
    emitter.instruction("mov w13, #0x47");                                      // 'G' of the "GBMB" phar magic
    emitter.instruction("strb w13, [x12, #24]");                                // magic byte 0
    emitter.instruction("mov w13, #0x42");                                      // 'B'
    emitter.instruction("strb w13, [x12, #25]");                                // magic byte 1
    emitter.instruction("mov w13, #0x4d");                                      // 'M'
    emitter.instruction("strb w13, [x12, #26]");                                // magic byte 2
    emitter.instruction("mov w13, #0x42");                                      // 'B'
    emitter.instruction("strb w13, [x12, #27]");                                // magic byte 3
    emitter.instruction("add x11, x11, #28");                                   // grow the archive length by the 28-byte signature trailer
    emitter.instruction("str x11, [x9]");                                       // commit the signed archive length
    // -- write the finished archive to disk --
    abi::emit_symbol_address(emitter, "x1", "_phar_write_path_ptr");
    emitter.instruction("ldr x1, [x1]");                                        // archive path pointer (file_put_contents fname ptr)
    abi::emit_symbol_address(emitter, "x2", "_phar_write_path_len");
    emitter.instruction("ldr x2, [x2]");                                        // archive path length (file_put_contents fname len)
    abi::emit_symbol_address(emitter, "x3", "_phar_write_out");
    emitter.instruction("mov x3, x3");                                          // archive data pointer (file_put_contents data ptr)
    abi::emit_symbol_address(emitter, "x4", "_phar_write_len");
    emitter.instruction("ldr x4, [x4]");                                        // archive byte count (file_put_contents data len)
    emitter.instruction("bl __rt_file_put_contents");                           // write the assembled phar archive to disk
    // -- return true and restore the frame --
    emitter.instruction("mov x0, #1");                                          // fclose() returns true after a successful finalize
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");                                                 // return to the fclose caller
}

/// Emits the x86_64 Linux variant of the phar-write runtime routines.
fn emit_phar_write_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: phar_write open ---");
    emitter.label_global("__rt_phar_write_open");
    // __rt_phar_write_open(rdi = template ptr, rsi = template len).
    emitter.instruction("lea r8, [rip + _phar_write_out]");                     // phar-write buffer base
    emitter.instruction("xor r9, r9");                                          // copy index = 0
    emitter.label("__rt_phar_write_open_loop_x86");
    emitter.instruction("cmp r9, rsi");                                         // copied every template byte?
    emitter.instruction("jge __rt_phar_write_open_done_x86");                   // template fully copied
    emitter.instruction("mov r10b, BYTE PTR [rdi + r9]");                       // load a template byte
    emitter.instruction("mov BYTE PTR [r8 + r9], r10b");                        // store it into the buffer
    emitter.instruction("inc r9");                                              // advance the copy index
    emitter.instruction("jmp __rt_phar_write_open_loop_x86");                   // continue copying
    emitter.label("__rt_phar_write_open_done_x86");
    emitter.instruction("lea r8, [rip + _phar_write_len]");                     // buffer length slot
    emitter.instruction("mov QWORD PTR [r8], rsi");                             // length starts at the template length
    emitter.instruction("lea r8, [rip + _phar_write_tpl_len]");                 // template length slot
    emitter.instruction("mov QWORD PTR [r8], rsi");                             // record the template length for finalize
    emitter.instruction("ret");                                                 // return to the fopen caller
    emitter.blank();
    emitter.comment("--- runtime: phar_write append ---");
    emitter.label_global("__rt_phar_write_append");
    // __rt_phar_write_append(rsi = payload ptr, rdx = payload len; rdi = fd, ignored).
    emitter.instruction("lea r8, [rip + _phar_write_len]");                     // buffer length slot
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // current buffer length
    emitter.instruction("lea r10, [rip + _phar_write_out]");                    // buffer base
    emitter.instruction("add r10, r9");                                         // append destination = base + current length
    emitter.instruction("xor r11, r11");                                        // copy index = 0
    emitter.label("__rt_phar_write_append_loop_x86");
    emitter.instruction("cmp r11, rdx");                                        // appended every payload byte?
    emitter.instruction("jge __rt_phar_write_append_done_x86");                 // payload fully appended
    emitter.instruction("mov cl, BYTE PTR [rsi + r11]");                        // load a payload byte
    emitter.instruction("mov BYTE PTR [r10 + r11], cl");                        // store it into the buffer
    emitter.instruction("inc r11");                                             // advance the copy index
    emitter.instruction("jmp __rt_phar_write_append_loop_x86");                 // continue appending
    emitter.label("__rt_phar_write_append_done_x86");
    emitter.instruction("add r9, rdx");                                         // grow the buffer length by the payload size
    emitter.instruction("mov QWORD PTR [r8], r9");                              // commit the new buffer length
    emitter.instruction("mov rax, rdx");                                        // fwrite() returns the number of bytes written
    emitter.instruction("ret");                                                 // return to the fwrite caller
    emitter.blank();
    emitter.comment("--- runtime: phar_write finalize ---");
    emitter.label_global("__rt_phar_write_finalize");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve a small aligned frame for the crc stash
    // -- compute the content length and the entry anchor --
    emitter.instruction("lea r8, [rip + _phar_write_len]");                     // buffer length slot
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // total buffer length (template + content)
    emitter.instruction("lea r8, [rip + _phar_write_tpl_len]");                 // template length slot
    emitter.instruction("mov r10, QWORD PTR [r8]");                             // template prefix length
    emitter.instruction("mov r11, r9");                                         // content length = total ...
    emitter.instruction("sub r11, r10");                                        // ... minus the template length
    emitter.instruction("lea rcx, [rip + _phar_write_out]");                    // buffer base
    emitter.instruction("add rcx, r10");                                        // entry anchor = base + template length
    // -- patch the manifest size fields (little-endian u32) --
    emitter.instruction("mov DWORD PTR [rcx - 24], r11d");                      // uncompressed size = content length
    emitter.instruction("mov DWORD PTR [rcx - 16], r11d");                      // compressed size = content length (stored uncompressed)
    // -- checksum the entry content --
    emitter.instruction("mov rax, rcx");                                        // crc32 input pointer = entry content
    emitter.instruction("mov edx, r11d");                                       // crc32 input length = content length
    emitter.instruction("call __rt_crc32");                                     // rax = CRC-32 of the entry content
    emitter.instruction("mov DWORD PTR [rbp - 8], eax");                        // stash the crc across the address reloads
    emitter.instruction("lea rcx, [rip + _phar_write_out]");                    // buffer base
    emitter.instruction("lea r8, [rip + _phar_write_tpl_len]");                 // template length slot
    emitter.instruction("add rcx, QWORD PTR [r8]");                             // recompute the entry anchor
    emitter.instruction("mov eax, DWORD PTR [rbp - 8]");                        // reload the crc
    emitter.instruction("mov DWORD PTR [rcx - 12], eax");                       // patch the manifest crc32 field
    // -- append the SHA1 signature trailer: raw-sha1(20) ++ LE32(0x0002) ++ "GBMB".
    //    PHP hashes stub+manifest+data up to the trailer = _phar_write_out[0.._phar_write_len]. --
    emitter.instruction("lea rdi, [rip + _phar_write_out]");                    // CC_SHA1 data = archive buffer base
    emitter.instruction("lea r8, [rip + _phar_write_len]");                     // buffer length slot
    emitter.instruction("mov rcx, QWORD PTR [r8]");                             // current archive length (everything before the signature)
    emitter.instruction("mov esi, ecx");                                        // CC_SHA1 length (CC_LONG) = low 32 bits of the length
    emitter.instruction("lea rdx, [rip + _phar_write_out]");                    // CC_SHA1 md base = buffer base ...
    emitter.instruction("add rdx, rcx");                                        // ... + length (write 20 raw bytes past the data)
    emitter.bl_c("CC_SHA1");                                                    // compute the raw 20-byte SHA1 digest in place
    emitter.instruction("lea r8, [rip + _phar_write_len]");                     // buffer length slot
    emitter.instruction("mov rcx, QWORD PTR [r8]");                             // reload length (CC_SHA1 clobbered caller-saved regs)
    emitter.instruction("lea r9, [rip + _phar_write_out]");                     // buffer base
    emitter.instruction("add r9, rcx");                                         // trailer base = buffer + length (raw digest occupies +0..+20)
    emitter.instruction("mov DWORD PTR [r9 + 20], 2");                          // little-endian signature type 0x0002 = Phar::SHA1
    emitter.instruction("mov BYTE PTR [r9 + 24], 0x47");                        // 'G' of the "GBMB" phar magic
    emitter.instruction("mov BYTE PTR [r9 + 25], 0x42");                        // 'B'
    emitter.instruction("mov BYTE PTR [r9 + 26], 0x4d");                        // 'M'
    emitter.instruction("mov BYTE PTR [r9 + 27], 0x42");                        // 'B'
    emitter.instruction("add rcx, 28");                                         // grow the archive length by the 28-byte signature trailer
    emitter.instruction("mov QWORD PTR [r8], rcx");                             // commit the signed archive length
    // -- write the finished archive to disk --
    emitter.instruction("lea r8, [rip + _phar_write_path_ptr]");                // archive path pointer slot
    emitter.instruction("mov rax, QWORD PTR [r8]");                             // archive path pointer (file_put_contents fname ptr)
    emitter.instruction("lea r8, [rip + _phar_write_path_len]");                // archive path length slot
    emitter.instruction("mov rdx, QWORD PTR [r8]");                             // archive path length (file_put_contents fname len)
    emitter.instruction("lea rdi, [rip + _phar_write_out]");                    // archive data pointer (file_put_contents data ptr)
    emitter.instruction("lea r8, [rip + _phar_write_len]");                     // buffer length slot
    emitter.instruction("mov rsi, QWORD PTR [r8]");                             // archive byte count (file_put_contents data len)
    emitter.instruction("call __rt_file_put_contents");                         // write the assembled phar archive to disk
    // -- return true and restore the frame --
    emitter.instruction("mov eax, 1");                                          // fclose() returns true after a successful finalize
    emitter.instruction("add rsp, 16");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the fclose caller
}
