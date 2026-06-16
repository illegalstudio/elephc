//! Purpose:
//! Emits the phar-write runtime: `__rt_phar_write_open`, `__rt_phar_write_append`,
//! and `__rt_phar_write_finalize`. Together they buffer one `phar://` payload
//! per open write stream and flush it on `fclose()`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` (and the minimal x86
//!   runtime) via `crate::codegen::runtime::io`.
//! - `__rt_phar_write_open` from the `fopen("phar://...","w")` emitter.
//! - `__rt_phar_write_append` from `__rt_fwrite` when the descriptor is in the
//!   phar-write synthetic range `0x50000000..0x50000020`.
//! - `__rt_phar_write_finalize` from the `fclose` emitter for that same range.
//! - `__rt_file_put_contents_maybe_phar` from non-literal `file_put_contents()`
//!   lowering when a runtime string may be a `phar://` URL.
//! - `__rt_phar_write_open_url` from `__rt_fopen_maybe_phar` when a non-literal
//!   `phar://` URL is opened in a write-capable mode.
//!
//! Key details:
//! - When the `elephc-phar` stream bridge pointers are published, buffered state
//!   lives in Rust-owned per-descriptor slots and multiple PHAR write streams can
//!   stay open concurrently.
//! - The fixed `.bss` globals remain as a fallback for the older single-entry
//!   assembly writer when the bridge is not linked.
//! - Runtime-built `file_put_contents()` and write-mode `fopen()` URLs use a
//!   separate bridge entry that receives the complete `phar://archive/entry`
//!   string and performs the split in Rust.
//! - Current limits: stored ZIP writes only, no explicit compression-control
//!   APIs, and no private-key signing variants.

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
    abi::emit_symbol_address(emitter, "x9", "_elephc_phar_stream_open_entry_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the optional buffered PHAR stream opener
    emitter.instruction("cbz x9, __rt_phar_write_open_asm");                    // no bridge published: use the assembly fallback
    emitter.instruction("sub sp, sp, #16");                                     // allocate a bridge call frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the bridge call frame
    abi::emit_symbol_address(emitter, "x0", "_phar_write_path_ptr");
    emitter.instruction("ldr x0, [x0]");                                        // bridge arg 0 = archive path pointer
    abi::emit_symbol_address(emitter, "x1", "_phar_write_path_len");
    emitter.instruction("ldr x1, [x1]");                                        // bridge arg 1 = archive path length
    abi::emit_symbol_address(emitter, "x2", "_phar_write_entry_ptr");
    emitter.instruction("ldr x2, [x2]");                                        // bridge arg 2 = entry name pointer
    abi::emit_symbol_address(emitter, "x3", "_phar_write_entry_len");
    emitter.instruction("ldr x3, [x3]");                                        // bridge arg 3 = entry name length
    emitter.instruction("blr x9");                                              // allocate a buffered PHAR write stream descriptor
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the bridge call frame
    emitter.instruction("ret");                                                 // return the synthetic descriptor or failure sentinel
    emitter.label("__rt_phar_write_open_asm");
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
    abi::emit_symbol_address(emitter, "x12", "_phar_write_url_len");
    emitter.instruction("str xzr, [x12]");                                      // mark this stream as a literal archive/entry write
    emitter.instruction("ret");                                                 // return to the fopen caller
    emitter.blank();
    emitter.comment("--- runtime: phar_write open_url ---");
    emitter.label_global("__rt_phar_write_open_url");
    // __rt_phar_write_open_url(x0 = full phar:// URL ptr, x1 = URL len): persist
    // the URL for fclose(), then start an empty payload buffer.
    abi::emit_symbol_address(emitter, "x9", "_elephc_phar_stream_open_url_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the optional buffered PHAR URL stream opener
    emitter.instruction("cbz x9, __rt_phar_write_open_url_asm");                // no bridge published: use the assembly fallback
    emitter.instruction("sub sp, sp, #16");                                     // allocate a bridge call frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the bridge call frame
    emitter.instruction("blr x9");                                              // allocate a buffered runtime-URL PHAR write stream
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the bridge call frame
    emitter.instruction("ret");                                                 // return the synthetic descriptor or failure sentinel
    emitter.label("__rt_phar_write_open_url_asm");
    emitter.instruction("sub sp, sp, #16");                                     // allocate a 16-byte frame for the persist call
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("mov x2, x1");                                          // str_persist arg length = URL length
    emitter.instruction("mov x1, x0");                                          // str_persist arg pointer = URL pointer
    emitter.instruction("bl __rt_str_persist");                                 // copy the runtime-built URL into persistent heap storage
    abi::emit_symbol_address(emitter, "x9", "_phar_write_url_ptr");
    emitter.instruction("str x1, [x9]");                                        // record the persistent phar:// URL pointer for finalize
    abi::emit_symbol_address(emitter, "x9", "_phar_write_url_len");
    emitter.instruction("str x2, [x9]");                                        // record the persistent phar:// URL length for finalize
    abi::emit_symbol_address(emitter, "x9", "_phar_write_len");
    emitter.instruction("str xzr, [x9]");                                       // dynamic stream payload starts at buffer offset zero
    abi::emit_symbol_address(emitter, "x9", "_phar_write_tpl_len");
    emitter.instruction("str xzr, [x9]");                                       // no manifest template precedes a dynamic stream payload
    emitter.instruction("mov w0, #0x5000");                                     // low half of the phar-write descriptor 0x50000000
    emitter.instruction("lsl w0, w0, #16");                                     // form the phar-write synthetic descriptor
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the synthetic stream descriptor
    emitter.blank();
    emitter.comment("--- runtime: phar_write append ---");
    emitter.label_global("__rt_phar_write_append");
    // __rt_phar_write_append(x0 = fd, x1 = payload ptr, x2 = payload len):
    // append the payload to the selected buffer and return the byte count.
    abi::emit_symbol_address(emitter, "x9", "_elephc_phar_stream_append_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the optional buffered PHAR stream appender
    emitter.instruction("cbz x9, __rt_phar_write_append_asm");                  // no bridge published: append to the assembly fallback buffer
    emitter.instruction("sub sp, sp, #16");                                     // allocate a bridge call frame
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the bridge call frame
    emitter.instruction("blr x9");                                              // append to the selected buffered PHAR stream
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the bridge call frame
    emitter.instruction("ret");                                                 // return bytes consumed or the failure sentinel
    emitter.label("__rt_phar_write_append_asm");
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
    abi::emit_symbol_address(emitter, "x9", "_elephc_phar_stream_finalize_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the optional buffered PHAR stream finalizer
    emitter.instruction("cbz x9, __rt_phar_write_finalize_asm_state");          // no bridge published: use the assembly fallback state
    emitter.instruction("blr x9");                                              // flush and release the selected buffered PHAR stream
    emitter.instruction("b __rt_phar_write_finalize_return");                   // restore the frame and return the bridge result
    emitter.label("__rt_phar_write_finalize_asm_state");
    // -- compute the content length and the entry anchor --
    abi::emit_symbol_address(emitter, "x9", "_phar_write_len");
    emitter.instruction("ldr x9, [x9]");                                        // total buffer length (template + content)
    abi::emit_symbol_address(emitter, "x10", "_phar_write_tpl_len");
    emitter.instruction("ldr x10, [x10]");                                      // template prefix length
    emitter.instruction("sub x11, x9, x10");                                    // content length = total - template
    abi::emit_symbol_address(emitter, "x12", "_phar_write_out");
    emitter.instruction("add x13, x12, x10");                                   // entry anchor = buffer base + template length
    // -- runtime-built fopen("phar://...","w") stores the full URL for the bridge --
    abi::emit_symbol_address(emitter, "x14", "_phar_write_url_len");
    emitter.instruction("ldr x15, [x14]");                                      // load full phar:// URL length for dynamic write streams
    emitter.instruction("cbz x15, __rt_phar_write_finalize_entry_bridge");      // zero URL length means literal archive/entry slots are active
    abi::emit_symbol_address(emitter, "x14", "_elephc_phar_put_url_fn");
    emitter.instruction("ldr x14, [x14]");                                      // load the optional native PHAR URL writer bridge pointer
    emitter.instruction("cbz x14, __rt_phar_write_finalize_bridge_fail");       // dynamic phar:// writes require the URL bridge
    abi::emit_symbol_address(emitter, "x0", "_phar_write_url_ptr");
    emitter.instruction("ldr x0, [x0]");                                        // bridge arg 0 = full phar:// URL pointer
    emitter.instruction("mov x1, x15");                                         // bridge arg 1 = full phar:// URL length
    emitter.instruction("mov x2, x13");                                         // bridge arg 2 = buffered entry payload pointer
    emitter.instruction("mov x3, x11");                                         // bridge arg 3 = buffered entry payload length
    emitter.instruction("blr x14");                                             // insert/update the entry through the Rust PHAR URL bridge
    emitter.instruction("cmn x0, #1");                                          // did the bridge report usize::MAX failure?
    emitter.instruction("b.eq __rt_phar_write_finalize_bridge_fail");           // bridge failure makes fclose() false
    emitter.instruction("mov x0, #1");                                          // fclose() returns true after a successful bridge write
    emitter.instruction("b __rt_phar_write_finalize_return");                   // skip the literal archive/entry bridge
    emitter.label("__rt_phar_write_finalize_entry_bridge");
    // -- prefer the elephc-phar bridge so existing native entries are preserved --
    abi::emit_symbol_address(emitter, "x14", "_elephc_phar_put_entry_fn");
    emitter.instruction("ldr x14, [x14]");                                      // load the optional native PHAR writer bridge pointer
    emitter.instruction("cbz x14, __rt_phar_write_finalize_asm");               // no bridge published → use the single-entry assembly writer
    abi::emit_symbol_address(emitter, "x0", "_phar_write_path_ptr");
    emitter.instruction("ldr x0, [x0]");                                        // bridge arg 0 = archive path pointer
    abi::emit_symbol_address(emitter, "x1", "_phar_write_path_len");
    emitter.instruction("ldr x1, [x1]");                                        // bridge arg 1 = archive path length
    abi::emit_symbol_address(emitter, "x2", "_phar_write_entry_ptr");
    emitter.instruction("ldr x2, [x2]");                                        // bridge arg 2 = entry name pointer
    abi::emit_symbol_address(emitter, "x3", "_phar_write_entry_len");
    emitter.instruction("ldr x3, [x3]");                                        // bridge arg 3 = entry name length
    emitter.instruction("mov x4, x13");                                         // bridge arg 4 = buffered entry payload pointer
    emitter.instruction("mov x5, x11");                                         // bridge arg 5 = buffered entry payload length
    emitter.instruction("blr x14");                                             // insert/update the entry through the Rust PHAR bridge
    emitter.instruction("cmn x0, #1");                                          // did the bridge report usize::MAX failure?
    emitter.instruction("b.eq __rt_phar_write_finalize_bridge_fail");           // bridge failure makes fclose() false
    emitter.instruction("mov x0, #1");                                          // fclose() returns true after a successful bridge write
    emitter.instruction("b __rt_phar_write_finalize_return");                   // skip the single-entry assembly writer
    emitter.label("__rt_phar_write_finalize_bridge_fail");
    emitter.instruction("mov x0, #0");                                          // fclose() returns false when the bridge rejects the write
    emitter.instruction("b __rt_phar_write_finalize_return");                   // restore the frame and return
    emitter.label("__rt_phar_write_finalize_asm");
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
    abi::emit_symbol_address(emitter, "x0", "_sha1_algo_name");
    emitter.instruction("mov x1, #4");                                          // elephc_crypto_hash name length = strlen("sha1")
    emitter.instruction("mov x2, x10");                                         // elephc_crypto_hash data = archive buffer base
    emitter.instruction("mov x3, x11");                                         // elephc_crypto_hash data length = current archive length
    emitter.instruction("add x4, x10, x11");                                    // elephc_crypto_hash out = buffer + length (raw 20 bytes)
    abi::emit_symbol_address(emitter, "x9", "_elephc_crypto_hash_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the elephc-crypto hash entry pointer
    emitter.instruction("blr x9");                                              // compute the raw 20-byte SHA1 digest in place
    abi::emit_symbol_address(emitter, "x10", "_phar_write_out");
    abi::emit_symbol_address(emitter, "x9", "_phar_write_len");
    emitter.instruction("ldr x11, [x9]");                                       // reload length (the hash call clobbered caller-saved regs)
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
    emitter.label("__rt_phar_write_finalize_return");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");                                                 // return to the fclose caller

    emit_file_put_contents_maybe_phar_aarch64(emitter);
}

/// Emits the AArch64 dynamic `file_put_contents()` gate for runtime phar:// URLs.
fn emit_file_put_contents_maybe_phar_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: file_put_contents_maybe_phar ---");
    emitter.label_global("__rt_file_put_contents_maybe_phar");
    emitter.instruction("cmp x2, #7");                                          // filename at least "phar://" long?
    emitter.instruction("b.lt __rt_fpc_maybe_phar_plain");                      // too short: use ordinary filesystem writes
    emitter.instruction("ldrb w9, [x1, #0]");                                   // filename byte 0
    emitter.instruction("cmp w9, #0x70");                                       // 'p'
    emitter.instruction("b.ne __rt_fpc_maybe_phar_plain");                      // not phar://: use ordinary filesystem writes
    emitter.instruction("ldrb w9, [x1, #1]");                                   // filename byte 1
    emitter.instruction("cmp w9, #0x68");                                       // 'h'
    emitter.instruction("b.ne __rt_fpc_maybe_phar_plain");                      // not phar://: use ordinary filesystem writes
    emitter.instruction("ldrb w9, [x1, #2]");                                   // filename byte 2
    emitter.instruction("cmp w9, #0x61");                                       // 'a'
    emitter.instruction("b.ne __rt_fpc_maybe_phar_plain");                      // not phar://: use ordinary filesystem writes
    emitter.instruction("ldrb w9, [x1, #3]");                                   // filename byte 3
    emitter.instruction("cmp w9, #0x72");                                       // 'r'
    emitter.instruction("b.ne __rt_fpc_maybe_phar_plain");                      // not phar://: use ordinary filesystem writes
    emitter.instruction("ldrb w9, [x1, #4]");                                   // filename byte 4
    emitter.instruction("cmp w9, #0x3a");                                       // ':'
    emitter.instruction("b.ne __rt_fpc_maybe_phar_plain");                      // not phar://: use ordinary filesystem writes
    emitter.instruction("ldrb w9, [x1, #5]");                                   // filename byte 5
    emitter.instruction("cmp w9, #0x2f");                                       // '/'
    emitter.instruction("b.ne __rt_fpc_maybe_phar_plain");                      // not phar://: use ordinary filesystem writes
    emitter.instruction("ldrb w9, [x1, #6]");                                   // filename byte 6
    emitter.instruction("cmp w9, #0x2f");                                       // '/'
    emitter.instruction("b.ne __rt_fpc_maybe_phar_plain");                      // not phar://: use ordinary filesystem writes
    abi::emit_symbol_address(emitter, "x9", "_elephc_phar_put_url_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the optional PHAR URL writer bridge pointer
    emitter.instruction("cbz x9, __rt_fpc_maybe_phar_fail");                    // phar:// without a bridge cannot be written
    emitter.instruction("sub sp, sp, #16");                                     // allocate a call frame for the Rust bridge
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("mov x0, x1");                                          // bridge arg 0 = full phar:// URL pointer
    emitter.instruction("mov x1, x2");                                          // bridge arg 1 = full phar:// URL length
    emitter.instruction("mov x2, x3");                                          // bridge arg 2 = payload pointer
    emitter.instruction("mov x3, x4");                                          // bridge arg 3 = payload length
    emitter.instruction("blr x9");                                              // insert/update the entry through the Rust PHAR bridge
    emitter.instruction("cmn x0, #1");                                          // did the bridge report usize::MAX failure?
    emitter.instruction("b.ne __rt_fpc_maybe_phar_return");                     // successful bridge write already returned the byte count
    emitter.instruction("mov x0, #-1");                                         // failure follows file_put_contents' negative result convention
    emitter.label("__rt_fpc_maybe_phar_return");
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the bridge call frame
    emitter.instruction("ret");                                                 // return the write result to the caller
    emitter.label("__rt_fpc_maybe_phar_fail");
    emitter.instruction("mov x0, #-1");                                         // report failure for phar:// when the bridge is unavailable
    emitter.instruction("ret");                                                 // return the failure result
    emitter.label("__rt_fpc_maybe_phar_plain");
    emitter.instruction("b __rt_file_put_contents");                            // tail-call the ordinary filesystem writer
}

/// Emits the x86_64 Linux variant of the phar-write runtime routines.
fn emit_phar_write_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: phar_write open ---");
    emitter.label_global("__rt_phar_write_open");
    // __rt_phar_write_open(rdi = template ptr, rsi = template len).
    abi::emit_load_symbol_to_reg(emitter, "r10", "_elephc_phar_stream_open_entry_fn", 0); // load the optional buffered PHAR stream opener
    emitter.instruction("test r10, r10");                                       // was the stream opener bridge published?
    emitter.instruction("jz __rt_phar_write_open_asm_x86");                     // no bridge published: use the assembly fallback
    emitter.instruction("push rbp");                                            // align the stack and preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the bridge call frame
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_phar_write_path_ptr", 0);    // bridge arg 0 = archive path pointer
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_phar_write_path_len", 0);    // bridge arg 1 = archive path length
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_phar_write_entry_ptr", 0);   // bridge arg 2 = entry name pointer
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_phar_write_entry_len", 0);   // bridge arg 3 = entry name length
    emitter.instruction("call r10");                                            // allocate a buffered PHAR write stream descriptor
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the synthetic descriptor or failure sentinel
    emitter.label("__rt_phar_write_open_asm_x86");
    abi::emit_symbol_address(emitter, "r8", "_phar_write_out");                 // phar-write buffer base
    emitter.instruction("xor r9, r9");                                          // copy index = 0
    emitter.label("__rt_phar_write_open_loop_x86");
    emitter.instruction("cmp r9, rsi");                                         // copied every template byte?
    emitter.instruction("jge __rt_phar_write_open_done_x86");                   // template fully copied
    emitter.instruction("mov r10b, BYTE PTR [rdi + r9]");                       // load a template byte
    emitter.instruction("mov BYTE PTR [r8 + r9], r10b");                        // store it into the buffer
    emitter.instruction("inc r9");                                              // advance the copy index
    emitter.instruction("jmp __rt_phar_write_open_loop_x86");                   // continue copying
    emitter.label("__rt_phar_write_open_done_x86");
    abi::emit_symbol_address(emitter, "r8", "_phar_write_len");                 // buffer length slot
    emitter.instruction("mov QWORD PTR [r8], rsi");                             // length starts at the template length
    abi::emit_symbol_address(emitter, "r8", "_phar_write_tpl_len");             // template length slot
    emitter.instruction("mov QWORD PTR [r8], rsi");                             // record the template length for finalize
    abi::emit_symbol_address(emitter, "r8", "_phar_write_url_len");             // dynamic URL length slot
    emitter.instruction("mov QWORD PTR [r8], 0");                               // mark this stream as a literal archive/entry write
    emitter.instruction("ret");                                                 // return to the fopen caller
    emitter.blank();
    emitter.comment("--- runtime: phar_write open_url ---");
    emitter.label_global("__rt_phar_write_open_url");
    // __rt_phar_write_open_url(rdi = full phar:// URL ptr, rsi = URL len).
    abi::emit_load_symbol_to_reg(emitter, "r10", "_elephc_phar_stream_open_url_fn", 0); // load the optional buffered PHAR URL stream opener
    emitter.instruction("test r10, r10");                                       // was the URL stream opener bridge published?
    emitter.instruction("jz __rt_phar_write_open_url_asm_x86");                 // no bridge published: use the assembly fallback
    emitter.instruction("push rbp");                                            // align the stack and preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the bridge call frame
    emitter.instruction("call r10");                                            // allocate a buffered runtime-URL PHAR write stream
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the synthetic descriptor or failure sentinel
    emitter.label("__rt_phar_write_open_url_asm_x86");
    emitter.instruction("push rbp");                                            // align the stack and preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("mov rax, rdi");                                        // str_persist arg pointer = URL pointer
    emitter.instruction("mov rdx, rsi");                                        // str_persist arg length = URL length
    emitter.instruction("call __rt_str_persist");                               // copy the runtime-built URL into persistent heap storage
    abi::emit_store_reg_to_symbol(emitter, "rax", "_phar_write_url_ptr", 0);    // record the persistent phar:// URL pointer for finalize
    abi::emit_store_reg_to_symbol(emitter, "rdx", "_phar_write_url_len", 0);    // record the persistent phar:// URL length for finalize
    abi::emit_store_imm_to_symbol(emitter, "_phar_write_len", 0, 0);            // dynamic stream payload starts at buffer offset zero
    abi::emit_store_imm_to_symbol(emitter, "_phar_write_tpl_len", 0, 0);        // no manifest template precedes a dynamic stream payload
    emitter.instruction("mov eax, 0x50000000");                                 // return the phar-write synthetic descriptor
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the fopen caller
    emitter.blank();
    emitter.comment("--- runtime: phar_write append ---");
    emitter.label_global("__rt_phar_write_append");
    // __rt_phar_write_append(rdi = fd, rsi = payload ptr, rdx = payload len).
    abi::emit_load_symbol_to_reg(emitter, "r10", "_elephc_phar_stream_append_fn", 0); // load the optional buffered PHAR stream appender
    emitter.instruction("test r10, r10");                                       // was the stream appender bridge published?
    emitter.instruction("jz __rt_phar_write_append_asm_x86");                   // no bridge published: append to the assembly fallback buffer
    emitter.instruction("push rbp");                                            // align the stack and preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the bridge call frame
    emitter.instruction("call r10");                                            // append to the selected buffered PHAR stream
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return bytes consumed or the failure sentinel
    emitter.label("__rt_phar_write_append_asm_x86");
    abi::emit_symbol_address(emitter, "r8", "_phar_write_len");                 // buffer length slot
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // current buffer length
    abi::emit_symbol_address(emitter, "r10", "_phar_write_out");                // buffer base
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
    abi::emit_load_symbol_to_reg(emitter, "r10", "_elephc_phar_stream_finalize_fn", 0); // load the optional buffered PHAR stream finalizer
    emitter.instruction("test r10, r10");                                       // was the stream finalizer bridge published?
    emitter.instruction("jz __rt_phar_write_finalize_asm_state_x86");           // no bridge published: use the assembly fallback state
    emitter.instruction("call r10");                                            // flush and release the selected buffered PHAR stream
    emitter.instruction("jmp __rt_phar_write_finalize_return_x86");             // restore the frame and return the bridge result
    emitter.label("__rt_phar_write_finalize_asm_state_x86");
    // -- compute the content length and the entry anchor --
    abi::emit_symbol_address(emitter, "r8", "_phar_write_len");                 // buffer length slot
    emitter.instruction("mov r9, QWORD PTR [r8]");                              // total buffer length (template + content)
    abi::emit_symbol_address(emitter, "r8", "_phar_write_tpl_len");             // template length slot
    emitter.instruction("mov r10, QWORD PTR [r8]");                             // template prefix length
    emitter.instruction("mov r11, r9");                                         // content length = total ...
    emitter.instruction("sub r11, r10");                                        // ... minus the template length
    abi::emit_symbol_address(emitter, "rcx", "_phar_write_out");                // buffer base
    emitter.instruction("add rcx, r10");                                        // entry anchor = base + template length
    // -- runtime-built fopen("phar://...","w") stores the full URL for the bridge --
    abi::emit_load_symbol_to_reg(emitter, "rax", "_phar_write_url_len", 0);     // load full phar:// URL length for dynamic write streams
    emitter.instruction("test rax, rax");                                       // is this a dynamic URL write stream?
    emitter.instruction("jz __rt_phar_write_finalize_entry_bridge_x86");        // zero URL length means literal archive/entry slots are active
    abi::emit_load_symbol_to_reg(emitter, "r10", "_elephc_phar_put_url_fn", 0); // load the optional native PHAR URL writer bridge pointer
    emitter.instruction("test r10, r10");                                       // was the URL writer bridge published?
    emitter.instruction("jz __rt_phar_write_finalize_bridge_fail_x86");         // dynamic phar:// writes require the URL bridge
    emitter.instruction("mov r8, rcx");                                         // bridge arg 2 = buffered entry payload pointer
    emitter.instruction("mov r9, r11");                                         // bridge arg 3 = buffered entry payload length
    emitter.instruction("mov rsi, rax");                                        // bridge arg 1 = full phar:// URL length
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_phar_write_url_ptr", 0);     // bridge arg 0 = full phar:// URL pointer
    emitter.instruction("mov rdx, r8");                                         // bridge arg 2 = buffered entry payload pointer
    emitter.instruction("mov rcx, r9");                                         // bridge arg 3 = buffered entry payload length
    emitter.instruction("call r10");                                            // insert/update the entry through the Rust PHAR URL bridge
    emitter.instruction("cmp rax, -1");                                         // did the bridge report usize::MAX failure?
    emitter.instruction("je __rt_phar_write_finalize_bridge_fail_x86");         // bridge failure makes fclose() false
    emitter.instruction("mov eax, 1");                                          // fclose() returns true after a successful bridge write
    emitter.instruction("jmp __rt_phar_write_finalize_return_x86");             // skip the literal archive/entry bridge
    emitter.label("__rt_phar_write_finalize_entry_bridge_x86");
    // -- prefer the elephc-phar bridge so existing native entries are preserved --
    abi::emit_load_symbol_to_reg(emitter, "r10", "_elephc_phar_put_entry_fn", 0); // load the optional native PHAR writer bridge pointer
    emitter.instruction("test r10, r10");                                       // was the writer bridge published?
    emitter.instruction("jz __rt_phar_write_finalize_asm_x86");                 // no bridge published → use the single-entry assembly writer
    emitter.instruction("mov r8, rcx");                                         // bridge arg 4 = buffered entry payload pointer
    emitter.instruction("mov r9, r11");                                         // bridge arg 5 = buffered entry payload length
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_phar_write_path_ptr", 0);    // bridge arg 0 = archive path pointer
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_phar_write_path_len", 0);    // bridge arg 1 = archive path length
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_phar_write_entry_ptr", 0);   // bridge arg 2 = entry name pointer
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_phar_write_entry_len", 0);   // bridge arg 3 = entry name length
    emitter.instruction("call r10");                                            // insert/update the entry through the Rust PHAR bridge
    emitter.instruction("cmp rax, -1");                                         // did the bridge report usize::MAX failure?
    emitter.instruction("je __rt_phar_write_finalize_bridge_fail_x86");         // bridge failure makes fclose() false
    emitter.instruction("mov eax, 1");                                          // fclose() returns true after a successful bridge write
    emitter.instruction("jmp __rt_phar_write_finalize_return_x86");             // skip the single-entry assembly writer
    emitter.label("__rt_phar_write_finalize_bridge_fail_x86");
    emitter.instruction("xor eax, eax");                                        // fclose() returns false when the bridge rejects the write
    emitter.instruction("jmp __rt_phar_write_finalize_return_x86");             // restore the frame and return
    emitter.label("__rt_phar_write_finalize_asm_x86");
    // -- patch the manifest size fields (little-endian u32) --
    emitter.instruction("mov DWORD PTR [rcx - 24], r11d");                      // uncompressed size = content length
    emitter.instruction("mov DWORD PTR [rcx - 16], r11d");                      // compressed size = content length (stored uncompressed)
    // -- checksum the entry content --
    emitter.instruction("mov rax, rcx");                                        // crc32 input pointer = entry content
    emitter.instruction("mov edx, r11d");                                       // crc32 input length = content length
    emitter.instruction("call __rt_crc32");                                     // rax = CRC-32 of the entry content
    emitter.instruction("mov DWORD PTR [rbp - 8], eax");                        // stash the crc across the address reloads
    abi::emit_symbol_address(emitter, "rcx", "_phar_write_out");                // buffer base
    abi::emit_symbol_address(emitter, "r8", "_phar_write_tpl_len");             // template length slot
    emitter.instruction("add rcx, QWORD PTR [r8]");                             // recompute the entry anchor
    emitter.instruction("mov eax, DWORD PTR [rbp - 8]");                        // reload the crc
    emitter.instruction("mov DWORD PTR [rcx - 12], eax");                       // patch the manifest crc32 field
    // -- append the SHA1 signature trailer: raw-sha1(20) ++ LE32(0x0002) ++ "GBMB".
    //    PHP hashes stub+manifest+data up to the trailer = _phar_write_out[0.._phar_write_len]. --
    abi::emit_symbol_address(emitter, "r8", "_phar_write_len");                 // buffer length slot
    emitter.instruction("mov rcx, QWORD PTR [r8]");                             // elephc_crypto_hash data length = current archive length
    abi::emit_symbol_address(emitter, "rdi", "_sha1_algo_name");                // elephc_crypto_hash name = "sha1"
    emitter.instruction("mov esi, 4");                                          // elephc_crypto_hash name length = strlen("sha1")
    abi::emit_symbol_address(emitter, "rdx", "_phar_write_out");                // elephc_crypto_hash data = archive buffer base
    abi::emit_symbol_address(emitter, "r8", "_phar_write_out");                 // elephc_crypto_hash out base = archive buffer base ...
    emitter.instruction("add r8, rcx");                                         // ... + length (raw 20 bytes past the data)
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_crypto_hash_fn", 0);   // load the elephc-crypto hash entry pointer
    emitter.instruction("call r9");                                             // compute the raw 20-byte SHA1 digest in place
    abi::emit_symbol_address(emitter, "r8", "_phar_write_len");                 // buffer length slot
    emitter.instruction("mov rcx, QWORD PTR [r8]");                             // reload length (the hash call clobbered caller-saved regs)
    abi::emit_symbol_address(emitter, "r9", "_phar_write_out");                 // buffer base
    emitter.instruction("add r9, rcx");                                         // trailer base = buffer + length (raw digest occupies +0..+20)
    emitter.instruction("mov DWORD PTR [r9 + 20], 2");                          // little-endian signature type 0x0002 = Phar::SHA1
    emitter.instruction("mov BYTE PTR [r9 + 24], 0x47");                        // 'G' of the "GBMB" phar magic
    emitter.instruction("mov BYTE PTR [r9 + 25], 0x42");                        // 'B'
    emitter.instruction("mov BYTE PTR [r9 + 26], 0x4d");                        // 'M'
    emitter.instruction("mov BYTE PTR [r9 + 27], 0x42");                        // 'B'
    emitter.instruction("add rcx, 28");                                         // grow the archive length by the 28-byte signature trailer
    emitter.instruction("mov QWORD PTR [r8], rcx");                             // commit the signed archive length
    // -- write the finished archive to disk --
    abi::emit_symbol_address(emitter, "r8", "_phar_write_path_ptr");            // archive path pointer slot
    emitter.instruction("mov rax, QWORD PTR [r8]");                             // archive path pointer (file_put_contents fname ptr)
    abi::emit_symbol_address(emitter, "r8", "_phar_write_path_len");            // archive path length slot
    emitter.instruction("mov rdx, QWORD PTR [r8]");                             // archive path length (file_put_contents fname len)
    abi::emit_symbol_address(emitter, "rdi", "_phar_write_out");                // archive data pointer (file_put_contents data ptr)
    abi::emit_symbol_address(emitter, "r8", "_phar_write_len");                 // buffer length slot
    emitter.instruction("mov rsi, QWORD PTR [r8]");                             // archive byte count (file_put_contents data len)
    emitter.instruction("call __rt_file_put_contents");                         // write the assembled phar archive to disk
    // -- return true and restore the frame --
    emitter.instruction("mov eax, 1");                                          // fclose() returns true after a successful finalize
    emitter.label("__rt_phar_write_finalize_return_x86");
    emitter.instruction("add rsp, 16");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the fclose caller

    emit_file_put_contents_maybe_phar_linux_x86_64(emitter);
}

/// Emits the x86_64 dynamic `file_put_contents()` gate for runtime phar:// URLs.
fn emit_file_put_contents_maybe_phar_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: file_put_contents_maybe_phar ---");
    emitter.label_global("__rt_file_put_contents_maybe_phar");
    emitter.instruction("cmp rdx, 7");                                          // filename at least "phar://" long?
    emitter.instruction("jl __rt_fpc_maybe_phar_plain_x86");                    // too short: use ordinary filesystem writes
    emitter.instruction("cmp BYTE PTR [rax + 0], 0x70");                        // 'p'
    emitter.instruction("jne __rt_fpc_maybe_phar_plain_x86");                   // not phar://: use ordinary filesystem writes
    emitter.instruction("cmp BYTE PTR [rax + 1], 0x68");                        // 'h'
    emitter.instruction("jne __rt_fpc_maybe_phar_plain_x86");                   // not phar://: use ordinary filesystem writes
    emitter.instruction("cmp BYTE PTR [rax + 2], 0x61");                        // 'a'
    emitter.instruction("jne __rt_fpc_maybe_phar_plain_x86");                   // not phar://: use ordinary filesystem writes
    emitter.instruction("cmp BYTE PTR [rax + 3], 0x72");                        // 'r'
    emitter.instruction("jne __rt_fpc_maybe_phar_plain_x86");                   // not phar://: use ordinary filesystem writes
    emitter.instruction("cmp BYTE PTR [rax + 4], 0x3a");                        // ':'
    emitter.instruction("jne __rt_fpc_maybe_phar_plain_x86");                   // not phar://: use ordinary filesystem writes
    emitter.instruction("cmp BYTE PTR [rax + 5], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fpc_maybe_phar_plain_x86");                   // not phar://: use ordinary filesystem writes
    emitter.instruction("cmp BYTE PTR [rax + 6], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fpc_maybe_phar_plain_x86");                   // not phar://: use ordinary filesystem writes
    abi::emit_load_symbol_to_reg(emitter, "r10", "_elephc_phar_put_url_fn", 0); // load the optional PHAR URL writer bridge pointer
    emitter.instruction("test r10, r10");                                       // was the writer bridge published?
    emitter.instruction("jz __rt_fpc_maybe_phar_fail_x86");                     // phar:// without a bridge cannot be written
    emitter.instruction("push rbp");                                            // align the stack and preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("mov r8, rdi");                                         // preserve payload pointer while arranging bridge args
    emitter.instruction("mov r9, rsi");                                         // preserve payload length while arranging bridge args
    emitter.instruction("mov rdi, rax");                                        // bridge arg 0 = full phar:// URL pointer
    emitter.instruction("mov rsi, rdx");                                        // bridge arg 1 = full phar:// URL length
    emitter.instruction("mov rdx, r8");                                         // bridge arg 2 = payload pointer
    emitter.instruction("mov rcx, r9");                                         // bridge arg 3 = payload length
    emitter.instruction("call r10");                                            // insert/update the entry through the Rust PHAR bridge
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the bridge byte count or -1 failure
    emitter.label("__rt_fpc_maybe_phar_fail_x86");
    emitter.instruction("mov rax, -1");                                         // report failure for phar:// when the bridge is unavailable
    emitter.instruction("ret");                                                 // return the failure result
    emitter.label("__rt_fpc_maybe_phar_plain_x86");
    emitter.instruction("jmp __rt_file_put_contents");                          // tail-call the ordinary filesystem writer
}
