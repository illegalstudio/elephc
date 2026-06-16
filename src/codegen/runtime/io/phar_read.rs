//! Purpose:
//! Emits the runtime `phar://` read helpers: `__rt_phar_read_entry`, which reads
//! and parses a PHAR archive at run time and materializes a named entry as a
//! readable stream, and `__rt_fopen_maybe_phar`, the `fopen` gate that
//! routes a non-literal `phar://...` read URL to it, and write-mode URLs to
//! the PHAR writer's dynamic URL open helper.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` (and the minimal x86
//!   runtime) via `crate::codegen::runtime::io`.
//! - `__rt_fopen_maybe_phar` is called by the `fopen` lowering's generic
//!   (non-literal-URL) path instead of `__rt_fopen`.
//!
//! Key details:
//! - This is the RUNTIME counterpart to the compile-time `parse_phar_entry`
//!   (src/codegen/builtins/io/phar_stream.rs). Literal `phar://` URLs still take
//!   the compile-time fast path (bytes embedded in the binary); only non-literal
//!   URLs reach here. Runtime reads support native uncompressed, gzip, and
//!   bzip2-compressed PHAR entries; runtime write opens preserve the full URL
//!   until `fclose()` so the native bridge can split archive and entry names.
//! - Reuses `__rt_file_get_contents` (reads the whole archive into a heap buffer)
//!   and tail-calls `__rt_data_stream` (writes the matched entry to an unlinked
//!   tmpfile and rewinds it) so the resulting fd behaves like any read stream.
//! - zlib/libbz2 entry points are called through runtime function-pointer slots
//!   that EIR call sites publish only for dynamic PHAR-capable reads.
//! - The archive/entry split is the `.phar/` boundary (matching the write path);
//!   a runtime archive path without `.phar/` in its name is unsupported in M2.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

const PHAR_FLAG_GZIP: u32 = 0x0000_1000;
const PHAR_FLAG_BZIP2: u32 = 0x0000_2000;
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits `__rt_phar_read_entry` and `__rt_fopen_maybe_phar` for the active target.
pub fn emit_phar_read(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_phar_read_linux_x86_64(emitter);
        return;
    }

    // ===== __rt_phar_read_entry(x0 = url ptr, x1 = url len) -> x0 = fd =====
    emitter.blank();
    emitter.comment("--- runtime: phar_read_entry ---");
    emitter.label_global("__rt_phar_read_entry");
    // Callee-saved layout (survive the file_get_contents / data_stream calls):
    //   x19 = archive buffer ptr   x20 = archive buffer length N
    //   x23 = entry name ptr       x24 = entry name length
    // Frame: [0]x19 [8]x20 [16]x21 [24]x22 [32]x23 [40]x24 [48]x29 [56]x30.
    emitter.instruction("sub sp, sp, #64");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("stp x19, x20, [sp, #0]");                              // save callee-saved x19/x20
    emitter.instruction("stp x21, x22, [sp, #16]");                             // save callee-saved x21/x22
    emitter.instruction("stp x23, x24, [sp, #32]");                             // save callee-saved x23/x24

    // -- bridge reader for native/tar/zip PHAR containers when published --
    abi::emit_symbol_address(emitter, "x9", "_elephc_phar_extract_url_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the optional elephc-phar bridge entry pointer
    emitter.instruction("cbz x9, __rt_phar_read_asm_fallback");                 // use the assembly reader when no bridge was published
    abi::emit_symbol_address(emitter, "x2", "_phar_extract_len");
    emitter.instruction("blr x9");                                              // elephc_phar_extract_url(url_ptr, url_len, &len)
    emitter.instruction("cbz x0, __rt_phar_read_fail");                         // bridge miss means archive or entry was not readable
    abi::emit_symbol_address(emitter, "x9", "_phar_extract_len");
    emitter.instruction("ldr x1, [x9]");                                        // load the extracted entry byte length
    emitter.instruction("bl __rt_data_stream");                                 // copy bridge bytes into a readable temp stream
    emitter.instruction("b __rt_phar_read_done");                               // return the bridge-created descriptor
    emitter.label("__rt_phar_read_asm_fallback");

    // -- split the URL at ".phar/" (x0 = url ptr, x1 = url len) --
    emitter.instruction("mov x9, #7");                                          // scan index i = 7 (past "phar://")
    emitter.label("__rt_phar_read_split_loop");
    emitter.instruction("add x10, x9, #6");                                     // i + 6 (end of a candidate ".phar/")
    emitter.instruction("cmp x10, x1");                                         // does ".phar/" fit before the URL end?
    emitter.instruction("b.gt __rt_phar_read_fail");                            // no ".phar/" boundary → not selectable
    emitter.instruction("ldrb w11, [x0, x9]");                                  // url[i]
    emitter.instruction("cmp w11, #0x2e");                                      // '.'
    emitter.instruction("b.ne __rt_phar_read_split_next");                      // branch when the checked value is nonzero or different
    emitter.instruction("add x12, x9, #1");                                     // advance runtime pointer or counter
    emitter.instruction("ldrb w11, [x0, x12]");                                 // url[i+1]
    emitter.instruction("cmp w11, #0x70");                                      // 'p'
    emitter.instruction("b.ne __rt_phar_read_split_next");                      // branch when the checked value is nonzero or different
    emitter.instruction("add x12, x9, #2");                                     // advance runtime pointer or counter
    emitter.instruction("ldrb w11, [x0, x12]");                                 // url[i+2]
    emitter.instruction("cmp w11, #0x68");                                      // 'h'
    emitter.instruction("b.ne __rt_phar_read_split_next");                      // branch when the checked value is nonzero or different
    emitter.instruction("add x12, x9, #3");                                     // advance runtime pointer or counter
    emitter.instruction("ldrb w11, [x0, x12]");                                 // url[i+3]
    emitter.instruction("cmp w11, #0x61");                                      // 'a'
    emitter.instruction("b.ne __rt_phar_read_split_next");                      // branch when the checked value is nonzero or different
    emitter.instruction("add x12, x9, #4");                                     // advance runtime pointer or counter
    emitter.instruction("ldrb w11, [x0, x12]");                                 // url[i+4]
    emitter.instruction("cmp w11, #0x72");                                      // 'r'
    emitter.instruction("b.ne __rt_phar_read_split_next");                      // branch when the checked value is nonzero or different
    emitter.instruction("add x12, x9, #5");                                     // advance runtime pointer or counter
    emitter.instruction("ldrb w11, [x0, x12]");                                 // url[i+5]
    emitter.instruction("cmp w11, #0x2f");                                      // '/'
    emitter.instruction("b.eq __rt_phar_read_split_found");                     // ".phar/" matched at i
    emitter.label("__rt_phar_read_split_next");
    emitter.instruction("add x9, x9, #1");                                      // advance the scan index
    emitter.instruction("b __rt_phar_read_split_loop");                         // continue at target label
    emitter.label("__rt_phar_read_split_found");
    // entry ptr = url + i + 6 ; entry len = url_len - (i + 6)
    emitter.instruction("add x23, x0, x9");                                     // url + i
    emitter.instruction("add x23, x23, #6");                                    // entry name ptr = url + i + 6
    emitter.instruction("add x24, x9, #6");                                     // i + 6
    emitter.instruction("sub x24, x1, x24");                                    // entry name length = url_len - (i + 6)
    // archive ptr = url + 7 ; archive len = i - 2 (includes ".phar")
    emitter.instruction("add x13, x0, #7");                                     // archive path ptr = url + 7
    emitter.instruction("sub x14, x9, #2");                                     // archive path length = i - 2

    // -- read the whole archive into a heap buffer --
    emitter.instruction("mov x1, x13");                                         // file_get_contents path ptr
    emitter.instruction("mov x2, x14");                                         // file_get_contents path length
    emitter.instruction("bl __rt_file_get_contents");                           // x1 = buffer ptr, x2 = bytes read
    emitter.instruction("cbz x1, __rt_phar_read_fail");                         // archive unreadable → fail
    emitter.instruction("mov x19, x1");                                         // archive buffer ptr (callee-saved)
    emitter.instruction("mov x20, x2");                                         // archive buffer length N

    // -- scan the buffer for the "__HALT_COMPILER();" stub terminator --
    abi::emit_symbol_address(emitter, "x15", "_phar_halt_magic");
    emitter.instruction("mov x9, #0");                                          // scan index i = 0
    emitter.label("__rt_phar_read_halt_scan");
    emitter.instruction("add x10, x9, #18");                                    // i + 18 (terminator length)
    emitter.instruction("cmp x10, x20");                                        // does the terminator fit before N?
    emitter.instruction("b.gt __rt_phar_read_fail");                            // terminator not found → not a phar
    emitter.instruction("mov x11, #0");                                         // compare index j = 0
    emitter.label("__rt_phar_read_halt_cmp");
    emitter.instruction("cmp x11, #18");                                        // compared all 18 bytes?
    emitter.instruction("b.ge __rt_phar_read_halt_found");                      // terminator matched at i
    emitter.instruction("add x12, x9, x11");                                    // i + j
    emitter.instruction("ldrb w13, [x19, x12]");                                // buffer byte
    emitter.instruction("ldrb w14, [x15, x11]");                                // terminator byte
    emitter.instruction("cmp w13, w14");                                        // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_phar_read_halt_next");                       // mismatch at this position
    emitter.instruction("add x11, x11, #1");                                    // next compare byte
    emitter.instruction("b __rt_phar_read_halt_cmp");                           // continue at target label
    emitter.label("__rt_phar_read_halt_next");
    emitter.instruction("add x9, x9, #1");                                      // advance the scan index
    emitter.instruction("b __rt_phar_read_halt_scan");                          // continue at target label
    emitter.label("__rt_phar_read_halt_found");
    emitter.instruction("add x9, x9, #18");                                     // p = i + 18 (first byte past the terminator)

    // -- skip the optional "; ?>\r\n" tail bytes in order, when present --
    for ch in [0x20u32, 0x3f, 0x3e, 0x0d, 0x0a] {
        emitter.instruction("ldrb w10, [x19, x9]");                             // peek the next byte
        emitter.instruction(&format!("cmp w10, #{:#x}", ch));                   // is it the expected stub-tail byte?
        emitter.instruction(&format!("b.ne __rt_phar_read_skip_done_{:x}", ch)); // not present → stop skipping
        emitter.instruction("add x9, x9, #1");                                  // consume the stub-tail byte
        emitter.label(&format!("__rt_phar_read_skip_done_{:x}", ch));
    }
    // x9 = manifest_start
    emitter.instruction("mov x16, x9");                                         // manifest_start (kept)
    emitter.instruction("ldr w10, [x19, x16]");                                 // manifest_len
    emitter.instruction("add x10, x16, x10");                                   // manifest_start + manifest_len
    emitter.instruction("add x10, x10, #4");                                    // data_section = manifest_start + 4 + manifest_len
    emitter.instruction("add x13, x16, #4");                                    // &num_files
    emitter.instruction("ldr w11, [x19, x13]");                                 // num_files (loop counter)
    emitter.instruction("add x9, x16, #14");                                    // q = manifest_start + 14 (skip num_files/api/flags)
    emitter.instruction("ldr w13, [x19, x9]");                                  // alias_len
    emitter.instruction("add x9, x9, #4");                                      // q += 4
    emitter.instruction("add x9, x9, x13");                                     // q += alias_len
    emitter.instruction("ldr w13, [x19, x9]");                                  // manifest meta_len
    emitter.instruction("add x9, x9, #4");                                      // q += 4
    emitter.instruction("add x9, x9, x13");                                     // q += meta_len
    emitter.instruction("mov x12, #0");                                         // data_offset = 0 (sum of prior compressed sizes)

    // -- walk the entries, matching the requested name --
    emitter.label("__rt_phar_read_entry_loop");
    emitter.instruction("cbz x11, __rt_phar_read_fail");                        // no more files → entry not found
    emitter.instruction("add x13, x9, #4");                                     // q + 4
    emitter.instruction("cmp x13, x20");                                        // bounds: name_len field within N?
    emitter.instruction("b.gt __rt_phar_read_fail");                            // branch when comparison is above target
    emitter.instruction("ldr w13, [x19, x9]");                                  // name_len
    emitter.instruction("add x9, x9, #4");                                      // q now at the name bytes
    emitter.instruction("add x14, x9, x13");                                    // q + name_len
    emitter.instruction("cmp x14, x20");                                        // bounds: name bytes within N?
    emitter.instruction("b.gt __rt_phar_read_fail");                            // branch when comparison is above target
    emitter.instruction("cmp x13, x24");                                        // name_len == requested entry len?
    emitter.instruction("b.ne __rt_phar_read_entry_mismatch");                  // lengths differ → not this entry
    emitter.instruction("mov x14, #0");                                         // compare index k = 0
    emitter.label("__rt_phar_read_name_cmp");
    emitter.instruction("cmp x14, x13");                                        // compared every name byte?
    emitter.instruction("b.ge __rt_phar_read_name_match");                      // full match
    emitter.instruction("add x15, x9, x14");                                    // q + k
    emitter.instruction("ldrb w0, [x19, x15]");                                 // archive name byte
    emitter.instruction("add x15, x23, x14");                                   // entry name + k
    emitter.instruction("ldrb w1, [x15]");                                      // requested name byte
    emitter.instruction("cmp w0, w1");                                          // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_phar_read_entry_mismatch");                  // byte mismatch → not this entry
    emitter.instruction("add x14, x14, #1");                                    // next name byte
    emitter.instruction("b __rt_phar_read_name_cmp");                           // continue at target label
    emitter.label("__rt_phar_read_entry_mismatch");
    emitter.instruction("add x9, x9, x13");                                     // q += name_len (now at uncompressed)
    emitter.instruction("add x14, x9, #8");                                     // &compressed = q + 8
    emitter.instruction("ldr w15, [x19, x14]");                                 // compressed size
    emitter.instruction("add x12, x12, x15");                                   // data_offset += compressed (concatenated data blob)
    emitter.instruction("add x9, x9, #20");                                     // q += uncomp+ts+comp+crc+flags (5*4)
    emitter.instruction("ldr w13, [x19, x9]");                                  // entry meta_len
    emitter.instruction("add x9, x9, #4");                                      // q += 4
    emitter.instruction("add x9, x9, x13");                                     // q += entry meta_len
    emitter.instruction("sub x11, x11, #1");                                    // num_files--
    emitter.instruction("b __rt_phar_read_entry_loop");                         // continue at target label

    emitter.label("__rt_phar_read_name_match");
    // q at name start, x13 = name_len, x10 = data_section, x12 = data_offset.
    emitter.instruction("add x14, x9, x13");                                    // q + name_len (start of uncompressed field)
    emitter.instruction("ldr w21, [x19, x14]");                                 // uncompressed size recorded in the entry manifest
    emitter.instruction("add x15, x14, #8");                                    // &compressed = q + name_len + 8
    emitter.instruction("ldr w22, [x19, x15]");                                 // compressed/stored byte count for this entry
    emitter.instruction("add x15, x14, #16");                                   // &flags = q + name_len + 16
    emitter.instruction("ldr w24, [x19, x15]");                                 // entry flags, including gzip/bzip2 compression bits
    emitter.instruction("add x14, x10, x12");                                   // data_section + data_offset
    emitter.instruction("add x15, x14, x22");                                   // end offset after the stored entry bytes
    emitter.instruction("cmp x15, x20");                                        // bounds: stored content within N?
    emitter.instruction("b.gt __rt_phar_read_fail");                            // branch when comparison is above target
    emitter.instruction("add x23, x19, x14");                                   // content ptr = buffer + data_section + data_offset
    emitter.instruction(&format!("tst x24, #{:#x}", PHAR_FLAG_GZIP));           // is this PHAR entry gzip-compressed?
    emitter.instruction("b.ne __rt_phar_read_name_match_gzip");                 // gzip entries need raw DEFLATE inflation
    emitter.instruction(&format!("tst x24, #{:#x}", PHAR_FLAG_BZIP2));          // is this PHAR entry bzip2-compressed?
    emitter.instruction("b.ne __rt_phar_read_name_match_bzip2");                // bzip2 entries need libbz2 decompression
    emitter.instruction("mov x0, x23");                                         // uncompressed content pointer
    emitter.instruction("mov x1, x22");                                         // uncompressed content length
    emitter.instruction("b __rt_phar_read_name_match_stream");                  // stream the already-plain entry bytes
    emitter.label("__rt_phar_read_name_match_gzip");
    emitter.instruction("mov x0, x23");                                         // compressed content pointer
    emitter.instruction("mov x1, x22");                                         // compressed content length
    emitter.instruction("mov x2, x21");                                         // expected uncompressed byte length
    emitter.instruction("bl __rt_phar_inflate_raw");                            // inflate the raw-DEFLATE PHAR entry payload
    emitter.instruction("cbz x0, __rt_phar_read_fail");                         // decompression failure makes the entry unreadable
    emitter.instruction("b __rt_phar_read_name_match_stream");                  // stream the inflated entry bytes
    emitter.label("__rt_phar_read_name_match_bzip2");
    emitter.instruction("mov x0, x23");                                         // compressed content pointer
    emitter.instruction("mov x1, x22");                                         // compressed content length
    emitter.instruction("mov x2, x21");                                         // expected uncompressed byte length
    emitter.instruction("bl __rt_phar_bzip2_decompress");                       // decompress the bzip2 PHAR entry payload
    emitter.instruction("cbz x0, __rt_phar_read_fail");                         // decompression failure makes the entry unreadable
    emitter.label("__rt_phar_read_name_match_stream");
    emitter.instruction("bl __rt_data_stream");                                 // materialize a readable fd over the entry; x0 = fd
    emitter.instruction("b __rt_phar_read_done");                               // continue at target label

    emitter.label("__rt_phar_read_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 → PHP false (missing archive/entry)
    emitter.label("__rt_phar_read_done");
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore callee-saved x19/x20
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore callee-saved x21/x22
    emitter.instruction("ldp x23, x24, [sp, #32]");                             // restore callee-saved x23/x24
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the entry stream descriptor

    emit_phar_decompress_helpers_arm64(emitter);

    // ===== __rt_fopen_maybe_phar(x1=fname ptr, x2=fname len, x3=mode ptr, x4=mode len) =====
    emitter.blank();
    emitter.comment("--- runtime: fopen_maybe_phar ---");
    emitter.label_global("__rt_fopen_maybe_phar");
    emitter.instruction("cmp x2, #7");                                          // filename at least "phar://" long?
    emitter.instruction("b.lt __rt_fopen_maybe_phar_plain");                    // branch when comparison is below target
    emitter.instruction("ldrb w9, [x1, #0]");                                   // 'p'
    emitter.instruction("cmp w9, #0x70");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #1]");                                   // 'h'
    emitter.instruction("cmp w9, #0x68");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #2]");                                   // 'a'
    emitter.instruction("cmp w9, #0x61");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #3]");                                   // 'r'
    emitter.instruction("cmp w9, #0x72");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #4]");                                   // ':'
    emitter.instruction("cmp w9, #0x3a");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #5]");                                   // '/'
    emitter.instruction("cmp w9, #0x2f");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #6]");                                   // '/'
    emitter.instruction("cmp w9, #0x2f");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // branch when the checked value is nonzero or different
    emitter.instruction("cbz x4, __rt_fopen_maybe_phar_plain");                 // empty mode → not a PHAR wrapper open
    emitter.instruction("ldrb w9, [x3, #0]");                                   // mode[0]
    emitter.instruction("cmp w9, #0x72");                                       // 'r' (read)?
    emitter.instruction("b.eq __rt_fopen_maybe_phar_read");                     // read modes use the runtime PHAR reader
    emitter.instruction("cmp w9, #0x77");                                       // 'w' (write/truncate)?
    emitter.instruction("b.eq __rt_fopen_maybe_phar_write");                    // write modes use the runtime PHAR writer
    emitter.instruction("cmp w9, #0x61");                                       // 'a' (append)?
    emitter.instruction("b.eq __rt_fopen_maybe_phar_write");                    // append modes currently rewrite through the PHAR writer
    emitter.instruction("cmp w9, #0x63");                                       // 'c' (create)?
    emitter.instruction("b.eq __rt_fopen_maybe_phar_write");                    // create modes use the runtime PHAR writer
    emitter.instruction("cmp w9, #0x78");                                       // 'x' (create new)?
    emitter.instruction("b.eq __rt_fopen_maybe_phar_write");                    // exclusive create modes use the runtime PHAR writer
    emitter.instruction("b __rt_fopen_maybe_phar_plain");                       // unsupported PHAR mode falls back to generic open
    emitter.label("__rt_fopen_maybe_phar_read");
    emitter.instruction("mov x0, x1");                                          // url ptr → __rt_phar_read_entry arg0
    emitter.instruction("mov x1, x2");                                          // url len → __rt_phar_read_entry arg1
    emitter.instruction("b __rt_phar_read_entry");                              // tail-call the runtime phar reader
    emitter.label("__rt_fopen_maybe_phar_write");
    emitter.instruction("mov x0, x1");                                          // url ptr → __rt_phar_write_open_url arg0
    emitter.instruction("mov x1, x2");                                          // url len → __rt_phar_write_open_url arg1
    emitter.instruction("b __rt_phar_write_open_url");                          // tail-call the runtime PHAR writer opener
    emitter.label("__rt_fopen_maybe_phar_plain");
    emitter.instruction("b __rt_fopen");                                        // tail-call the generic open (args intact)

    // ===== __rt_file_get_contents_maybe_phar(x1=url ptr, x2=url len) -> x1=str ptr, x2=len =====
    emitter.blank();
    emitter.comment("--- runtime: file_get_contents_maybe_phar ---");
    emitter.label_global("__rt_file_get_contents_maybe_phar");
    emitter.instruction("cmp x2, #7");                                          // at least "phar://" long?
    emitter.instruction("b.lt __rt_fgc_phar_plain");                            // branch when comparison is below target
    emitter.instruction("ldrb w9, [x1, #0]");                                   // 'p'
    emitter.instruction("cmp w9, #0x70");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fgc_phar_plain");                            // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #1]");                                   // 'h'
    emitter.instruction("cmp w9, #0x68");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fgc_phar_plain");                            // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #2]");                                   // 'a'
    emitter.instruction("cmp w9, #0x61");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fgc_phar_plain");                            // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #3]");                                   // 'r'
    emitter.instruction("cmp w9, #0x72");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fgc_phar_plain");                            // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #4]");                                   // ':'
    emitter.instruction("cmp w9, #0x3a");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fgc_phar_plain");                            // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #5]");                                   // '/'
    emitter.instruction("cmp w9, #0x2f");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fgc_phar_plain");                            // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w9, [x1, #6]");                                   // '/'
    emitter.instruction("cmp w9, #0x2f");                                       // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_fgc_phar_plain");                            // branch when the checked value is nonzero or different
    // phar:// read at run time: open the entry, slurp the fd, close it.
    emitter.instruction("sub sp, sp, #48");                                     // frame: [0]=fd [8]=str ptr [16]=str len [32]=x29 [40]=x30
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("mov x0, x1");                                          // url ptr → __rt_phar_read_entry arg0
    emitter.instruction("mov x1, x2");                                          // url len → __rt_phar_read_entry arg1
    emitter.instruction("bl __rt_phar_read_entry");                             // x0 = entry stream fd (-1 on missing archive/entry)
    emitter.instruction("cmp x0, #0");                                          // did the phar read fail?
    emitter.instruction("b.lt __rt_fgc_phar_fail");                             // → boxed false
    emitter.instruction("str x0, [sp, #0]");                                    // save the fd for the close below
    emitter.instruction("bl __rt_stream_get_contents");                         // (x0=fd) → x1 = string ptr, x2 = length
    emitter.instruction("stp x1, x2, [sp, #8]");                                // save the slurped string ptr/len
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the fd
    emitter.syscall(6);                                                         // close the entry stream fd
    emitter.instruction("ldp x1, x2, [sp, #8]");                                // restore the string ptr/len as the result
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the entry contents string
    emitter.label("__rt_fgc_phar_fail");
    emitter.instruction("mov x1, #0");                                          // null string ptr → file_get_contents boxes false
    emitter.instruction("mov x2, #0");                                          // prepare AArch64 call argument
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the failure (boxed false)
    emitter.label("__rt_fgc_phar_plain");
    emitter.instruction("b __rt_file_get_contents");                            // tail-call the generic reader (args intact)
}

/// Emits ARM64 helpers that decompress PHAR entry payloads through published
/// zlib/libbz2 function-pointer slots instead of direct runtime references.
fn emit_phar_decompress_helpers_arm64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: phar decompress helpers ---");

    // ===== __rt_phar_inflate_raw(x0=src, x1=src_len, x2=out_len) -> x0=ptr, x1=len =====
    emitter.label("__rt_phar_inflate_raw");
    emitter.instruction("sub sp, sp, #192");                                    // reserve z_stream, saved arguments, and return state
    emitter.instruction("stp x29, x30, [sp, #176]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #176");                                   // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #112]");                                  // save the compressed source pointer
    emitter.instruction("str x1, [sp, #120]");                                  // save the compressed source length
    emitter.instruction("str x2, [sp, #128]");                                  // save the expected uncompressed length
    abi::emit_symbol_address(emitter, "x9", "_phar_zlib_inflate_init2_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the published inflateInit2_ function pointer
    emitter.instruction("cbz x9, __rt_phar_inflate_fail");                      // missing zlib publisher means compressed PHAR reads fail
    abi::emit_symbol_address(emitter, "x9", "_phar_zlib_inflate_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the published inflate function pointer
    emitter.instruction("cbz x9, __rt_phar_inflate_fail");                      // missing zlib publisher means compressed PHAR reads fail
    abi::emit_symbol_address(emitter, "x9", "_phar_zlib_inflate_end_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the published inflateEnd function pointer
    emitter.instruction("cbz x9, __rt_phar_inflate_fail");                      // missing zlib publisher means compressed PHAR reads fail

    emitter.instruction("mov x0, x2");                                          // allocation size = expected decompressed bytes
    emitter.instruction("cbnz x0, __rt_phar_inflate_alloc");                    // non-empty output can allocate its exact size
    emitter.instruction("mov x0, #1");                                          // empty entries still need a non-null buffer sentinel
    emitter.label("__rt_phar_inflate_alloc");
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the decompressed entry buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted string payload
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the decompressed buffer as an owned string
    emitter.instruction("str x0, [sp, #136]");                                  // save the destination buffer pointer

    // -- zero the z_stream before handing it to zlib --
    emitter.instruction("mov x9, #0");                                          // z_stream byte clear index
    emitter.label("__rt_phar_inflate_zero");
    emitter.instruction("cmp x9, #112");                                        // cleared the whole z_stream struct?
    emitter.instruction("b.ge __rt_phar_inflate_zeroed");                       // the z_stream is ready for initialization
    emitter.instruction("strb wzr, [sp, x9]");                                  // zero one z_stream byte
    emitter.instruction("add x9, x9, #1");                                      // advance the clear index
    emitter.instruction("b __rt_phar_inflate_zero");                            // continue clearing z_stream bytes
    emitter.label("__rt_phar_inflate_zeroed");

    // -- inflateInit2_(strm, -15, version, size): -15 selects raw DEFLATE --
    emitter.instruction("mov x0, sp");                                          // arg 0 = z_stream pointer
    emitter.instruction("mov x1, #-15");                                        // arg 1 = raw-DEFLATE window bits
    abi::emit_symbol_address(emitter, "x2", "_zlib_version");
    emitter.instruction("mov x3, #112");                                        // arg 3 = sizeof(z_stream)
    abi::emit_symbol_address(emitter, "x9", "_phar_zlib_inflate_init2_fn");
    emitter.instruction("ldr x9, [x9]");                                        // reload the published inflateInit2_ pointer
    emitter.instruction("blr x9");                                              // initialize the raw-inflate stream
    emitter.instruction("cmp x0, #0");                                          // did zlib initialize successfully?
    emitter.instruction("b.ne __rt_phar_inflate_fail");                         // failed initialization makes the entry unreadable

    // -- point the stream at the PHAR payload and fixed-size output buffer --
    emitter.instruction("ldr x9, [sp, #112]");                                  // reload compressed source pointer
    emitter.instruction("str x9, [sp, #0]");                                    // z_stream.next_in = source pointer
    emitter.instruction("ldr x9, [sp, #120]");                                  // reload compressed source length
    emitter.instruction("str w9, [sp, #8]");                                    // z_stream.avail_in = source length
    emitter.instruction("ldr x9, [sp, #136]");                                  // reload destination buffer pointer
    emitter.instruction("str x9, [sp, #24]");                                   // z_stream.next_out = destination buffer
    emitter.instruction("ldr x9, [sp, #128]");                                  // reload expected decompressed length
    emitter.instruction("str w9, [sp, #32]");                                   // z_stream.avail_out = exact output capacity

    // -- inflate the whole entry in one Z_FINISH pass, then end the stream --
    emitter.instruction("mov x0, sp");                                          // arg 0 = z_stream pointer
    emitter.instruction("mov x1, #4");                                          // arg 1 = Z_FINISH
    abi::emit_symbol_address(emitter, "x9", "_phar_zlib_inflate_fn");
    emitter.instruction("ldr x9, [x9]");                                        // reload the published inflate pointer
    emitter.instruction("blr x9");                                              // inflate the compressed PHAR entry payload
    emitter.instruction("str x0, [sp, #144]");                                  // save the zlib status code
    emitter.instruction("ldr x9, [sp, #40]");                                   // z_stream.total_out = inflated length
    emitter.instruction("str x9, [sp, #152]");                                  // save the actual inflated byte count
    emitter.instruction("mov x0, sp");                                          // arg 0 = z_stream pointer
    abi::emit_symbol_address(emitter, "x9", "_phar_zlib_inflate_end_fn");
    emitter.instruction("ldr x9, [x9]");                                        // reload the published inflateEnd pointer
    emitter.instruction("blr x9");                                              // release zlib's internal inflate state

    emitter.instruction("ldr x9, [sp, #144]");                                  // reload the inflate status code
    emitter.instruction("cmp x9, #1");                                          // did inflate reach Z_STREAM_END?
    emitter.instruction("b.ne __rt_phar_inflate_fail");                         // partial or failed inflate makes the entry unreadable
    emitter.instruction("ldr x9, [sp, #152]");                                  // reload the actual inflated byte count
    emitter.instruction("ldr x10, [sp, #128]");                                 // reload the expected uncompressed byte count
    emitter.instruction("cmp x9, x10");                                         // did zlib produce the manifest-declared size?
    emitter.instruction("b.ne __rt_phar_inflate_fail");                         // size mismatch means the archive entry is invalid
    emitter.instruction("ldr x0, [sp, #136]");                                  // return the decompressed buffer pointer
    emitter.instruction("mov x1, x9");                                          // return the decompressed buffer length
    emitter.instruction("b __rt_phar_inflate_done");                            // skip the failure result
    emitter.label("__rt_phar_inflate_fail");
    emitter.instruction("mov x0, #0");                                          // null pointer reports decompression failure
    emitter.instruction("mov x1, #0");                                          // failure has no output bytes
    emitter.label("__rt_phar_inflate_done");
    emitter.instruction("ldp x29, x30, [sp, #176]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #192");                                    // release the z_stream helper frame
    emitter.instruction("ret");                                                 // return decompressed bytes or null on failure

    // ===== __rt_phar_bzip2_decompress(x0=src, x1=src_len, x2=out_len) -> x0=ptr, x1=len =====
    emitter.label("__rt_phar_bzip2_decompress");
    emitter.instruction("sub sp, sp, #80");                                     // reserve bzip2 decompression scratch state
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the compressed source pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the compressed source length
    emitter.instruction("str x2, [sp, #16]");                                   // save the expected uncompressed length
    abi::emit_symbol_address(emitter, "x9", "_phar_bz2_decompress_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the published BZ2 decompressor pointer
    emitter.instruction("cbz x9, __rt_phar_bzip2_fail");                        // missing libbz2 publisher means compressed PHAR reads fail
    emitter.instruction("mov x0, x2");                                          // allocation size = expected decompressed bytes
    emitter.instruction("cbnz x0, __rt_phar_bzip2_alloc");                      // non-empty output can allocate its exact size
    emitter.instruction("mov x0, #1");                                          // empty entries still need a non-null buffer sentinel
    emitter.label("__rt_phar_bzip2_alloc");
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the decompressed entry buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted string payload
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the decompressed buffer as an owned string
    emitter.instruction("str x0, [sp, #24]");                                   // save the destination buffer pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload expected decompressed length
    emitter.instruction("str w9, [sp, #32]");                                   // destLen in/out value for libbz2

    emitter.instruction("ldr x0, [sp, #24]");                                   // arg 0 = destination buffer
    emitter.instruction("add x1, sp, #32");                                     // arg 1 = &destLen
    emitter.instruction("ldr x2, [sp, #0]");                                    // arg 2 = compressed source pointer
    emitter.instruction("ldr w3, [sp, #8]");                                    // arg 3 = compressed source length
    emitter.instruction("mov w4, #0");                                          // arg 4 = small = false
    emitter.instruction("mov w5, #0");                                          // arg 5 = verbosity = 0
    abi::emit_symbol_address(emitter, "x9", "_phar_bz2_decompress_fn");
    emitter.instruction("ldr x9, [x9]");                                        // reload the published BZ2 decompressor pointer
    emitter.instruction("blr x9");                                              // decompress the bzip2 PHAR entry payload
    emitter.instruction("cmp w0, #0");                                          // did libbz2 report success?
    emitter.instruction("b.ne __rt_phar_bzip2_fail");                           // non-zero status makes the entry unreadable
    emitter.instruction("ldr w9, [sp, #32]");                                   // destLen now holds the decompressed byte count
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the expected uncompressed byte count
    emitter.instruction("cmp x9, x10");                                         // did libbz2 produce the manifest-declared size?
    emitter.instruction("b.ne __rt_phar_bzip2_fail");                           // size mismatch means the archive entry is invalid
    emitter.instruction("ldr x0, [sp, #24]");                                   // return the decompressed buffer pointer
    emitter.instruction("mov x1, x9");                                          // return the decompressed buffer length
    emitter.instruction("b __rt_phar_bzip2_done");                              // skip the failure result
    emitter.label("__rt_phar_bzip2_fail");
    emitter.instruction("mov x0, #0");                                          // null pointer reports decompression failure
    emitter.instruction("mov x1, #0");                                          // failure has no output bytes
    emitter.label("__rt_phar_bzip2_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the bzip2 helper frame
    emitter.instruction("ret");                                                 // return decompressed bytes or null on failure
}

/// x86_64 Linux variant of the runtime `phar://` read helpers.
fn emit_phar_read_linux_x86_64(emitter: &mut Emitter) {
    // ===== __rt_phar_read_entry(rdi = url ptr, rsi = url len) -> rax = fd =====
    emitter.blank();
    emitter.comment("--- runtime: phar_read_entry ---");
    emitter.label_global("__rt_phar_read_entry");
    // Callee-saved layout (survive the file_get_contents / data_stream calls):
    //   r12 = archive buffer ptr   r13 = archive buffer length N
    //   r14 = entry name ptr       r15 = entry name length
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("push r12");                                            // save callee-saved r12
    emitter.instruction("push r13");                                            // save callee-saved r13
    emitter.instruction("push r14");                                            // save callee-saved r14
    emitter.instruction("push r15");                                            // save callee-saved r15
    emitter.instruction("push rbx");                                            // save callee-saved rbx (data_section)
    emitter.instruction("sub rsp, 8");                                          // realign rsp to 16 (6 pushes left it 8-off) for the nested calls

    // -- bridge reader for native/tar/zip PHAR containers when published --
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_phar_extract_url_fn", 0); // load the optional elephc-phar bridge entry pointer
    emitter.instruction("test r9, r9");                                         // was the bridge reader published?
    emitter.instruction("jz __rt_phar_read_asm_fallback_x86");                  // use the assembly reader when no bridge was published
    abi::emit_symbol_address(emitter, "rdx", "_phar_extract_len");              // pass output-length scratch to the bridge
    emitter.instruction("call r9");                                             // elephc_phar_extract_url(url_ptr, url_len, &len)
    emitter.instruction("test rax, rax");                                       // did the bridge find archive bytes?
    emitter.instruction("jz __rt_phar_read_fail_x86");                          // bridge miss means archive or entry was not readable
    emitter.instruction("mov rdi, rax");                                        // pass extracted bytes to data_stream
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_phar_extract_len", 0);       // load the extracted entry byte length
    emitter.instruction("call __rt_data_stream");                               // copy bridge bytes into a readable temp stream
    emitter.instruction("jmp __rt_phar_read_done_x86");                         // return the bridge-created descriptor
    emitter.label("__rt_phar_read_asm_fallback_x86");

    // -- split the URL at ".phar/" (rdi = url ptr, rsi = url len) --
    emitter.instruction("mov r8, 7");                                           // scan index i = 7 (past "phar://")
    emitter.label("__rt_phar_read_split_loop_x86");
    emitter.instruction("lea r9, [r8 + 6]");                                    // i + 6
    emitter.instruction("cmp r9, rsi");                                         // does ".phar/" fit before the URL end?
    emitter.instruction("jg __rt_phar_read_fail_x86");                          // no ".phar/" boundary → fail
    emitter.instruction("cmp BYTE PTR [rdi + r8], 0x2e");                       // '.'
    emitter.instruction("jne __rt_phar_read_split_next_x86");                   // branch when the checked value is nonzero or different
    emitter.instruction("lea r9, [r8 + 1]");                                    // load runtime data address
    emitter.instruction("cmp BYTE PTR [rdi + r9], 0x70");                       // 'p'
    emitter.instruction("jne __rt_phar_read_split_next_x86");                   // branch when the checked value is nonzero or different
    emitter.instruction("lea r9, [r8 + 2]");                                    // load runtime data address
    emitter.instruction("cmp BYTE PTR [rdi + r9], 0x68");                       // 'h'
    emitter.instruction("jne __rt_phar_read_split_next_x86");                   // branch when the checked value is nonzero or different
    emitter.instruction("lea r9, [r8 + 3]");                                    // load runtime data address
    emitter.instruction("cmp BYTE PTR [rdi + r9], 0x61");                       // 'a'
    emitter.instruction("jne __rt_phar_read_split_next_x86");                   // branch when the checked value is nonzero or different
    emitter.instruction("lea r9, [r8 + 4]");                                    // load runtime data address
    emitter.instruction("cmp BYTE PTR [rdi + r9], 0x72");                       // 'r'
    emitter.instruction("jne __rt_phar_read_split_next_x86");                   // branch when the checked value is nonzero or different
    emitter.instruction("lea r9, [r8 + 5]");                                    // load runtime data address
    emitter.instruction("cmp BYTE PTR [rdi + r9], 0x2f");                       // '/'
    emitter.instruction("je __rt_phar_read_split_found_x86");                   // ".phar/" matched at i
    emitter.label("__rt_phar_read_split_next_x86");
    emitter.instruction("inc r8");                                              // advance the scan index
    emitter.instruction("jmp __rt_phar_read_split_loop_x86");                   // continue at target label
    emitter.label("__rt_phar_read_split_found_x86");
    // entry ptr = url + i + 6 ; entry len = url_len - (i + 6)
    emitter.instruction("lea r14, [rdi + r8 + 6]");                             // entry name ptr = url + i + 6
    emitter.instruction("mov r15, rsi");                                        // url_len
    emitter.instruction("sub r15, r8");                                         // url_len - i
    emitter.instruction("sub r15, 6");                                          // entry name length = url_len - (i + 6)
    // archive ptr = url + 7 ; archive len = i - 2
    emitter.instruction("lea rax, [rdi + 7]");                                  // archive path ptr = url + 7
    emitter.instruction("mov rdx, r8");                                         // i
    emitter.instruction("sub rdx, 2");                                          // archive path length = i - 2

    // -- read the whole archive into a heap buffer --
    emitter.instruction("call __rt_file_get_contents");                         // rax = buffer ptr, rdx = bytes read
    emitter.instruction("test rax, rax");                                       // archive readable?
    emitter.instruction("jz __rt_phar_read_fail_x86");                          // unreadable → fail
    emitter.instruction("mov r12, rax");                                        // archive buffer ptr (callee-saved)
    emitter.instruction("mov r13, rdx");                                        // archive buffer length N

    // -- scan the buffer for the "__HALT_COMPILER();" stub terminator --
    abi::emit_symbol_address(emitter, "rbx", "_phar_halt_magic");               // terminator literal ptr
    emitter.instruction("xor r8, r8");                                          // scan index i = 0
    emitter.label("__rt_phar_read_halt_scan_x86");
    emitter.instruction("lea r9, [r8 + 18]");                                   // i + 18
    emitter.instruction("cmp r9, r13");                                         // does the terminator fit before N?
    emitter.instruction("jg __rt_phar_read_fail_x86");                          // not found → not a phar
    emitter.instruction("xor r10, r10");                                        // compare index j = 0
    emitter.label("__rt_phar_read_halt_cmp_x86");
    emitter.instruction("cmp r10, 18");                                         // compared all 18 bytes?
    emitter.instruction("jge __rt_phar_read_halt_found_x86");                   // terminator matched
    emitter.instruction("lea r9, [r8 + r10]");                                  // i + j
    emitter.instruction("movzx eax, BYTE PTR [r12 + r9]");                      // buffer byte
    emitter.instruction("movzx ecx, BYTE PTR [rbx + r10]");                     // terminator byte
    emitter.instruction("cmp al, cl");                                          // compare runtime values for the next branch
    emitter.instruction("jne __rt_phar_read_halt_next_x86");                    // mismatch
    emitter.instruction("inc r10");                                             // next compare byte
    emitter.instruction("jmp __rt_phar_read_halt_cmp_x86");                     // continue at target label
    emitter.label("__rt_phar_read_halt_next_x86");
    emitter.instruction("inc r8");                                              // advance the scan index
    emitter.instruction("jmp __rt_phar_read_halt_scan_x86");                    // continue at target label
    emitter.label("__rt_phar_read_halt_found_x86");
    emitter.instruction("add r8, 18");                                          // p = i + 18

    // -- skip the optional "; ?>\r\n" tail bytes in order, when present --
    for ch in [0x20u32, 0x3f, 0x3e, 0x0d, 0x0a] {
        emitter.instruction(&format!("cmp BYTE PTR [r12 + r8], {:#x}", ch));    // is it the expected stub-tail byte?
        emitter.instruction(&format!("jne __rt_phar_read_skip_done_{:x}_x86", ch)); // not present → stop skipping
        emitter.instruction("inc r8");                                          // consume the stub-tail byte
        emitter.label(&format!("__rt_phar_read_skip_done_{:x}_x86", ch));
    }
    // r8 = manifest_start. Register plan for the walk (no calls until data_stream):
    //   rbx = data_section (callee-saved, survives the loop), r8 = q,
    //   r11 = num_files, r10 = data_offset, r9 = name_len (per iteration),
    //   rax = bounds/compare-index scratch, rcx/rdx = byte-compare scratch.
    emitter.instruction("mov rcx, r8");                                         // manifest_start (temp)
    emitter.instruction("mov eax, DWORD PTR [r12 + rcx]");                      // manifest_len
    emitter.instruction("lea rbx, [rcx + rax + 4]");                            // data_section = manifest_start + 4 + manifest_len (rbx, survives)
    emitter.instruction("lea rax, [rcx + 4]");                                  // &num_files
    emitter.instruction("mov r11d, DWORD PTR [r12 + rax]");                     // num_files (loop counter, r11)
    emitter.instruction("lea r8, [rcx + 14]");                                  // q = manifest_start + 14
    emitter.instruction("mov eax, DWORD PTR [r12 + r8]");                       // alias_len
    emitter.instruction("add r8, 4");                                           // q += 4
    emitter.instruction("add r8, rax");                                         // q += alias_len
    emitter.instruction("mov eax, DWORD PTR [r12 + r8]");                       // manifest meta_len
    emitter.instruction("add r8, 4");                                           // q += 4
    emitter.instruction("add r8, rax");                                         // q += meta_len
    emitter.instruction("xor r10, r10");                                        // data_offset = 0 (r10)

    // -- walk the entries, matching the requested name --
    emitter.label("__rt_phar_read_entry_loop_x86");
    emitter.instruction("test r11, r11");                                       // any files left?
    emitter.instruction("jz __rt_phar_read_fail_x86");                          // entry not found
    emitter.instruction("lea rax, [r8 + 4]");                                   // q + 4
    emitter.instruction("cmp rax, r13");                                        // bounds: name_len field within N?
    emitter.instruction("jg __rt_phar_read_fail_x86");                          // branch when comparison is above target
    emitter.instruction("mov r9d, DWORD PTR [r12 + r8]");                       // name_len (r9, per iteration)
    emitter.instruction("add r8, 4");                                           // q now at the name bytes
    emitter.instruction("lea rax, [r8 + r9]");                                  // q + name_len
    emitter.instruction("cmp rax, r13");                                        // bounds: name bytes within N?
    emitter.instruction("jg __rt_phar_read_fail_x86");                          // branch when comparison is above target
    emitter.instruction("cmp r9, r15");                                         // name_len == requested entry len?
    emitter.instruction("jne __rt_phar_read_entry_mismatch_x86");               // branch when the checked value is nonzero or different
    emitter.instruction("xor rax, rax");                                        // compare index k = 0
    emitter.label("__rt_phar_read_name_cmp_x86");
    emitter.instruction("cmp rax, r9");                                         // compared every name byte?
    emitter.instruction("jge __rt_phar_read_name_match_x86");                   // full match
    emitter.instruction("lea rcx, [r8 + rax]");                                 // q + k
    emitter.instruction("movzx edx, BYTE PTR [r12 + rcx]");                     // archive name byte (dl)
    emitter.instruction("lea rcx, [r14 + rax]");                                // entry name + k
    emitter.instruction("movzx ecx, BYTE PTR [rcx]");                           // requested name byte (cl)
    emitter.instruction("cmp dl, cl");                                          // compare runtime values for the next branch
    emitter.instruction("jne __rt_phar_read_entry_mismatch_x86");               // branch when the checked value is nonzero or different
    emitter.instruction("inc rax");                                             // next name byte
    emitter.instruction("jmp __rt_phar_read_name_cmp_x86");                     // continue at target label
    emitter.label("__rt_phar_read_entry_mismatch_x86");
    emitter.instruction("add r8, r9");                                          // q += name_len (now at uncompressed)
    emitter.instruction("lea rax, [r8 + 8]");                                   // &compressed = q + 8
    emitter.instruction("mov eax, DWORD PTR [r12 + rax]");                      // compressed size
    emitter.instruction("add r10, rax");                                        // data_offset += compressed
    emitter.instruction("add r8, 20");                                          // q += uncomp+ts+comp+crc+flags
    emitter.instruction("mov eax, DWORD PTR [r12 + r8]");                       // entry meta_len
    emitter.instruction("add r8, 4");                                           // q += 4
    emitter.instruction("add r8, rax");                                         // q += entry meta_len
    emitter.instruction("dec r11");                                             // num_files--
    emitter.instruction("jmp __rt_phar_read_entry_loop_x86");                   // continue at target label

    emitter.label("__rt_phar_read_name_match_x86");
    // q at name start, r9 = name_len, rbx = data_section, r10 = data_offset.
    emitter.instruction("lea rcx, [r8 + r9]");                                  // q + name_len (uncompressed field)
    emitter.instruction("mov r11d, DWORD PTR [r12 + rcx]");                     // uncompressed size recorded in the entry manifest
    emitter.instruction("lea rax, [rcx + 8]");                                  // &compressed = q + name_len + 8
    emitter.instruction("mov eax, DWORD PTR [r12 + rax]");                      // compressed/stored byte count for this entry
    emitter.instruction("lea rdx, [rcx + 16]");                                 // &flags = q + name_len + 16
    emitter.instruction("mov r8d, DWORD PTR [r12 + rdx]");                      // entry flags, including gzip/bzip2 compression bits
    emitter.instruction("mov rcx, rbx");                                        // data_section
    emitter.instruction("add rcx, r10");                                        // + data_offset
    emitter.instruction("mov rdx, rcx");                                        // end offset = data_section + data_offset ...
    emitter.instruction("add rdx, rax");                                        // ... + stored content length
    emitter.instruction("cmp rdx, r13");                                        // bounds: content within N?
    emitter.instruction("jg __rt_phar_read_fail_x86");                          // branch when comparison is above target
    emitter.instruction("lea rdi, [r12 + rcx]");                                // content ptr = buffer + data_section + data_offset
    emitter.instruction("mov rsi, rax");                                        // stored content length
    emitter.instruction(&format!("test r8d, {:#x}", PHAR_FLAG_GZIP));           // is this PHAR entry gzip-compressed?
    emitter.instruction("jnz __rt_phar_read_name_match_gzip_x86");              // gzip entries need raw DEFLATE inflation
    emitter.instruction(&format!("test r8d, {:#x}", PHAR_FLAG_BZIP2));          // is this PHAR entry bzip2-compressed?
    emitter.instruction("jnz __rt_phar_read_name_match_bzip2_x86");             // bzip2 entries need libbz2 decompression
    emitter.instruction("jmp __rt_phar_read_name_match_stream_x86");            // stream the already-plain entry bytes
    emitter.label("__rt_phar_read_name_match_gzip_x86");
    emitter.instruction("mov rdx, r11");                                        // expected uncompressed byte length
    emitter.instruction("call __rt_phar_inflate_raw");                          // inflate the raw-DEFLATE PHAR entry payload
    emitter.instruction("test rax, rax");                                       // did decompression return a buffer?
    emitter.instruction("jz __rt_phar_read_fail_x86");                          // decompression failure makes the entry unreadable
    emitter.instruction("mov rdi, rax");                                        // pass the inflated content pointer to data_stream
    emitter.instruction("mov rsi, rdx");                                        // pass the inflated content length to data_stream
    emitter.instruction("jmp __rt_phar_read_name_match_stream_x86");            // stream the inflated entry bytes
    emitter.label("__rt_phar_read_name_match_bzip2_x86");
    emitter.instruction("mov rdx, r11");                                        // expected uncompressed byte length
    emitter.instruction("call __rt_phar_bzip2_decompress");                     // decompress the bzip2 PHAR entry payload
    emitter.instruction("test rax, rax");                                       // did decompression return a buffer?
    emitter.instruction("jz __rt_phar_read_fail_x86");                          // decompression failure makes the entry unreadable
    emitter.instruction("mov rdi, rax");                                        // pass the decompressed content pointer to data_stream
    emitter.instruction("mov rsi, rdx");                                        // pass the decompressed content length to data_stream
    emitter.label("__rt_phar_read_name_match_stream_x86");
    emitter.instruction("call __rt_data_stream");                               // materialize a readable fd; rax = fd
    emitter.instruction("jmp __rt_phar_read_done_x86");                         // continue at target label

    emitter.label("__rt_phar_read_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 → PHP false
    emitter.label("__rt_phar_read_done_x86");
    emitter.instruction("add rsp, 8");                                          // undo the alignment padding
    emitter.instruction("pop rbx");                                             // restore callee-saved rbx
    emitter.instruction("pop r15");                                             // restore callee-saved r15
    emitter.instruction("pop r14");                                             // restore callee-saved r14
    emitter.instruction("pop r13");                                             // restore callee-saved r13
    emitter.instruction("pop r12");                                             // restore callee-saved r12
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the entry stream descriptor

    emit_phar_decompress_helpers_x86_64(emitter);

    // ===== __rt_fopen_maybe_phar(rax=fname ptr, rdx=fname len, rdi=mode ptr, rsi=mode len) =====
    emitter.blank();
    emitter.comment("--- runtime: fopen_maybe_phar ---");
    emitter.label_global("__rt_fopen_maybe_phar");
    emitter.instruction("cmp rdx, 7");                                          // filename at least "phar://" long?
    emitter.instruction("jl __rt_fopen_maybe_phar_plain_x86");                  // branch when comparison is below target
    emitter.instruction("cmp BYTE PTR [rax + 0], 0x70");                        // 'p'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 1], 0x68");                        // 'h'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 2], 0x61");                        // 'a'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 3], 0x72");                        // 'r'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 4], 0x3a");                        // ':'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 5], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 6], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // branch when the checked value is nonzero or different
    emitter.instruction("test rsi, rsi");                                       // empty mode → not a PHAR wrapper open
    emitter.instruction("jz __rt_fopen_maybe_phar_plain_x86");                  // branch when the checked value is zero or equal
    emitter.instruction("cmp BYTE PTR [rdi + 0], 0x72");                        // mode[0] == 'r'?
    emitter.instruction("je __rt_fopen_maybe_phar_read_x86");                   // read modes use the runtime PHAR reader
    emitter.instruction("cmp BYTE PTR [rdi + 0], 0x77");                        // mode[0] == 'w'?
    emitter.instruction("je __rt_fopen_maybe_phar_write_x86");                  // write modes use the runtime PHAR writer
    emitter.instruction("cmp BYTE PTR [rdi + 0], 0x61");                        // mode[0] == 'a'?
    emitter.instruction("je __rt_fopen_maybe_phar_write_x86");                  // append modes currently rewrite through the PHAR writer
    emitter.instruction("cmp BYTE PTR [rdi + 0], 0x63");                        // mode[0] == 'c'?
    emitter.instruction("je __rt_fopen_maybe_phar_write_x86");                  // create modes use the runtime PHAR writer
    emitter.instruction("cmp BYTE PTR [rdi + 0], 0x78");                        // mode[0] == 'x'?
    emitter.instruction("je __rt_fopen_maybe_phar_write_x86");                  // exclusive create modes use the runtime PHAR writer
    emitter.instruction("jmp __rt_fopen_maybe_phar_plain_x86");                 // unsupported PHAR mode falls back to generic open
    emitter.label("__rt_fopen_maybe_phar_read_x86");
    emitter.instruction("mov rdi, rax");                                        // url ptr → __rt_phar_read_entry arg0
    emitter.instruction("mov rsi, rdx");                                        // url len → __rt_phar_read_entry arg1
    emitter.instruction("jmp __rt_phar_read_entry");                            // tail-call the runtime phar reader
    emitter.label("__rt_fopen_maybe_phar_write_x86");
    emitter.instruction("mov rdi, rax");                                        // url ptr → __rt_phar_write_open_url arg0
    emitter.instruction("mov rsi, rdx");                                        // url len → __rt_phar_write_open_url arg1
    emitter.instruction("jmp __rt_phar_write_open_url");                        // tail-call the runtime PHAR writer opener
    emitter.label("__rt_fopen_maybe_phar_plain_x86");
    emitter.instruction("jmp __rt_fopen");                                      // tail-call the generic open (args intact)

    // ===== __rt_file_get_contents_maybe_phar(rax=url ptr, rdx=url len) -> rax=str ptr, rdx=len =====
    emitter.blank();
    emitter.comment("--- runtime: file_get_contents_maybe_phar ---");
    emitter.label_global("__rt_file_get_contents_maybe_phar");
    emitter.instruction("cmp rdx, 7");                                          // at least "phar://" long?
    emitter.instruction("jl __rt_fgc_phar_plain_x86");                          // branch when comparison is below target
    emitter.instruction("cmp BYTE PTR [rax + 0], 0x70");                        // 'p'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");                         // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 1], 0x68");                        // 'h'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");                         // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 2], 0x61");                        // 'a'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");                         // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 3], 0x72");                        // 'r'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");                         // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 4], 0x3a");                        // ':'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");                         // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 5], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");                         // branch when the checked value is nonzero or different
    emitter.instruction("cmp BYTE PTR [rax + 6], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");                         // branch when the checked value is nonzero or different
    // phar:// read at run time: open the entry, slurp the fd, close it.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // frame: [rbp-8]=fd [rbp-16]=str ptr [rbp-24]=len
    emitter.instruction("mov rdi, rax");                                        // url ptr → __rt_phar_read_entry arg0
    emitter.instruction("mov rsi, rdx");                                        // url len → __rt_phar_read_entry arg1
    emitter.instruction("call __rt_phar_read_entry");                           // rax = entry stream fd (-1 on missing archive/entry)
    emitter.instruction("cmp rax, 0");                                          // did the phar read fail?
    emitter.instruction("jl __rt_fgc_phar_fail_x86");                           // → boxed false
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the fd for the close below
    emitter.instruction("mov rdi, rax");                                        // fd → __rt_stream_get_contents arg
    emitter.instruction("call __rt_stream_get_contents");                       // rax = string ptr, rdx = length
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the slurped string ptr
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the slurped string length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the fd
    emitter.instruction("call close");                                          // close the entry stream fd
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // restore the string ptr as the result
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // restore the string length as the result
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the entry contents string
    emitter.label("__rt_fgc_phar_fail_x86");
    emitter.instruction("xor eax, eax");                                        // null string ptr → file_get_contents boxes false
    emitter.instruction("xor edx, edx");                                        // clear register value
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure (boxed false)
    emitter.label("__rt_fgc_phar_plain_x86");
    emitter.instruction("jmp __rt_file_get_contents");                          // tail-call the generic reader (args intact)
}

/// Emits x86_64 helpers that decompress PHAR entry payloads through published
/// zlib/libbz2 function-pointer slots instead of direct runtime references.
fn emit_phar_decompress_helpers_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: phar decompress helpers ---");

    // ===== __rt_phar_inflate_raw(rdi=src, rsi=src_len, rdx=out_len) -> rax=ptr, rdx=len =====
    emitter.label("__rt_phar_inflate_raw");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 176");                                        // reserve z_stream, saved arguments, and return state
    emitter.instruction("mov QWORD PTR [rsp + 112], rdi");                      // save the compressed source pointer
    emitter.instruction("mov QWORD PTR [rsp + 120], rsi");                      // save the compressed source length
    emitter.instruction("mov QWORD PTR [rsp + 128], rdx");                      // save the expected uncompressed length
    abi::emit_load_symbol_to_reg(emitter, "r9", "_phar_zlib_inflate_init2_fn", 0); // load the published inflateInit2_ function pointer
    emitter.instruction("test r9, r9");                                         // was the zlib initializer published?
    emitter.instruction("jz __rt_phar_inflate_fail_x86");                       // missing zlib publisher means compressed PHAR reads fail
    abi::emit_load_symbol_to_reg(emitter, "r9", "_phar_zlib_inflate_fn", 0);    // load the published inflate function pointer
    emitter.instruction("test r9, r9");                                         // was the zlib inflate function published?
    emitter.instruction("jz __rt_phar_inflate_fail_x86");                       // missing zlib publisher means compressed PHAR reads fail
    abi::emit_load_symbol_to_reg(emitter, "r9", "_phar_zlib_inflate_end_fn", 0); // load the published inflateEnd function pointer
    emitter.instruction("test r9, r9");                                         // was the zlib cleanup function published?
    emitter.instruction("jz __rt_phar_inflate_fail_x86");                       // missing zlib publisher means compressed PHAR reads fail

    emitter.instruction("mov rax, rdx");                                        // allocation size = expected decompressed bytes
    emitter.instruction("test rax, rax");                                       // is the output entry empty?
    emitter.instruction("jnz __rt_phar_inflate_alloc_x86");                     // non-empty output can allocate its exact size
    emitter.instruction("mov rax, 1");                                          // empty entries still need a non-null buffer sentinel
    emitter.label("__rt_phar_inflate_alloc_x86");
    emitter.instruction("call __rt_heap_alloc");                                // allocate the decompressed entry buffer
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 1)); // materialize the owned-string heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the decompressed buffer as an owned string
    emitter.instruction("mov QWORD PTR [rsp + 136], rax");                      // save the destination buffer pointer

    // -- zero the z_stream before handing it to zlib --
    emitter.instruction("xor r9, r9");                                          // z_stream byte clear index
    emitter.label("__rt_phar_inflate_zero_x86");
    emitter.instruction("cmp r9, 112");                                         // cleared the whole z_stream struct?
    emitter.instruction("jge __rt_phar_inflate_zeroed_x86");                    // the z_stream is ready for initialization
    emitter.instruction("mov BYTE PTR [rsp + r9], 0");                          // zero one z_stream byte
    emitter.instruction("inc r9");                                              // advance the clear index
    emitter.instruction("jmp __rt_phar_inflate_zero_x86");                      // continue clearing z_stream bytes
    emitter.label("__rt_phar_inflate_zeroed_x86");

    // -- inflateInit2_(strm, -15, version, size): -15 selects raw DEFLATE --
    emitter.instruction("mov rdi, rsp");                                        // arg 0 = z_stream pointer
    emitter.instruction("mov esi, -15");                                        // arg 1 = raw-DEFLATE window bits
    abi::emit_symbol_address(emitter, "rdx", "_zlib_version");                  // arg 2 = zlib version string
    emitter.instruction("mov ecx, 112");                                        // arg 3 = sizeof(z_stream)
    abi::emit_load_symbol_to_reg(emitter, "r9", "_phar_zlib_inflate_init2_fn", 0); // reload the published inflateInit2_ pointer
    emitter.instruction("call r9");                                             // initialize the raw-inflate stream
    emitter.instruction("test eax, eax");                                       // did zlib initialize successfully?
    emitter.instruction("jnz __rt_phar_inflate_fail_x86");                      // failed initialization makes the entry unreadable

    // -- point the stream at the PHAR payload and fixed-size output buffer --
    emitter.instruction("mov r9, QWORD PTR [rsp + 112]");                       // reload compressed source pointer
    emitter.instruction("mov QWORD PTR [rsp + 0], r9");                         // z_stream.next_in = source pointer
    emitter.instruction("mov r9, QWORD PTR [rsp + 120]");                       // reload compressed source length
    emitter.instruction("mov DWORD PTR [rsp + 8], r9d");                        // z_stream.avail_in = source length
    emitter.instruction("mov r9, QWORD PTR [rsp + 136]");                       // reload destination buffer pointer
    emitter.instruction("mov QWORD PTR [rsp + 24], r9");                        // z_stream.next_out = destination buffer
    emitter.instruction("mov r9, QWORD PTR [rsp + 128]");                       // reload expected decompressed length
    emitter.instruction("mov DWORD PTR [rsp + 32], r9d");                       // z_stream.avail_out = exact output capacity

    // -- inflate the whole entry in one Z_FINISH pass, then end the stream --
    emitter.instruction("mov rdi, rsp");                                        // arg 0 = z_stream pointer
    emitter.instruction("mov esi, 4");                                          // arg 1 = Z_FINISH
    abi::emit_load_symbol_to_reg(emitter, "r9", "_phar_zlib_inflate_fn", 0);    // reload the published inflate pointer
    emitter.instruction("call r9");                                             // inflate the compressed PHAR entry payload
    emitter.instruction("mov QWORD PTR [rsp + 144], rax");                      // save the zlib status code
    emitter.instruction("mov r9, QWORD PTR [rsp + 40]");                        // z_stream.total_out = inflated length
    emitter.instruction("mov QWORD PTR [rsp + 152], r9");                       // save the actual inflated byte count
    emitter.instruction("mov rdi, rsp");                                        // arg 0 = z_stream pointer
    abi::emit_load_symbol_to_reg(emitter, "r9", "_phar_zlib_inflate_end_fn", 0); // reload the published inflateEnd pointer
    emitter.instruction("call r9");                                             // release zlib's internal inflate state

    emitter.instruction("cmp QWORD PTR [rsp + 144], 1");                        // did inflate reach Z_STREAM_END?
    emitter.instruction("jne __rt_phar_inflate_fail_x86");                      // partial or failed inflate makes the entry unreadable
    emitter.instruction("mov r9, QWORD PTR [rsp + 152]");                       // reload the actual inflated byte count
    emitter.instruction("cmp r9, QWORD PTR [rsp + 128]");                       // did zlib produce the manifest-declared size?
    emitter.instruction("jne __rt_phar_inflate_fail_x86");                      // size mismatch means the archive entry is invalid
    emitter.instruction("mov rax, QWORD PTR [rsp + 136]");                      // return the decompressed buffer pointer
    emitter.instruction("mov rdx, r9");                                         // return the decompressed buffer length
    emitter.instruction("jmp __rt_phar_inflate_done_x86");                      // skip the failure result
    emitter.label("__rt_phar_inflate_fail_x86");
    emitter.instruction("xor eax, eax");                                        // null pointer reports decompression failure
    emitter.instruction("xor edx, edx");                                        // failure has no output bytes
    emitter.label("__rt_phar_inflate_done_x86");
    emitter.instruction("add rsp, 176");                                        // release the z_stream helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return decompressed bytes or null on failure

    // ===== __rt_phar_bzip2_decompress(rdi=src, rsi=src_len, rdx=out_len) -> rax=ptr, rdx=len =====
    emitter.label("__rt_phar_bzip2_decompress");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 80");                                         // reserve bzip2 decompression scratch state
    emitter.instruction("mov QWORD PTR [rsp + 0], rdi");                        // save the compressed source pointer
    emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                        // save the compressed source length
    emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                       // save the expected uncompressed length
    abi::emit_load_symbol_to_reg(emitter, "r10", "_phar_bz2_decompress_fn", 0); // load the published BZ2 decompressor pointer
    emitter.instruction("test r10, r10");                                       // was the bzip2 decompressor published?
    emitter.instruction("jz __rt_phar_bzip2_fail_x86");                         // missing libbz2 publisher means compressed PHAR reads fail
    emitter.instruction("mov rax, rdx");                                        // allocation size = expected decompressed bytes
    emitter.instruction("test rax, rax");                                       // is the output entry empty?
    emitter.instruction("jnz __rt_phar_bzip2_alloc_x86");                       // non-empty output can allocate its exact size
    emitter.instruction("mov rax, 1");                                          // empty entries still need a non-null buffer sentinel
    emitter.label("__rt_phar_bzip2_alloc_x86");
    emitter.instruction("call __rt_heap_alloc");                                // allocate the decompressed entry buffer
    emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 1)); // materialize the owned-string heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the decompressed buffer as an owned string
    emitter.instruction("mov QWORD PTR [rsp + 24], rax");                       // save the destination buffer pointer
    emitter.instruction("mov r9, QWORD PTR [rsp + 16]");                        // reload expected decompressed length
    emitter.instruction("mov DWORD PTR [rsp + 32], r9d");                       // destLen in/out value for libbz2

    emitter.instruction("mov rdi, QWORD PTR [rsp + 24]");                       // arg 0 = destination buffer
    emitter.instruction("lea rsi, [rsp + 32]");                                 // arg 1 = &destLen
    emitter.instruction("mov rdx, QWORD PTR [rsp + 0]");                        // arg 2 = compressed source pointer
    emitter.instruction("mov ecx, DWORD PTR [rsp + 8]");                        // arg 3 = compressed source length
    emitter.instruction("xor r8d, r8d");                                        // arg 4 = small = false
    emitter.instruction("xor r9d, r9d");                                        // arg 5 = verbosity = 0
    abi::emit_load_symbol_to_reg(emitter, "r10", "_phar_bz2_decompress_fn", 0); // reload the published BZ2 decompressor pointer
    emitter.instruction("call r10");                                            // decompress the bzip2 PHAR entry payload
    emitter.instruction("test eax, eax");                                       // did libbz2 report success?
    emitter.instruction("jnz __rt_phar_bzip2_fail_x86");                        // non-zero status makes the entry unreadable
    emitter.instruction("mov r9d, DWORD PTR [rsp + 32]");                       // destLen now holds the decompressed byte count
    emitter.instruction("cmp r9, QWORD PTR [rsp + 16]");                        // did libbz2 produce the manifest-declared size?
    emitter.instruction("jne __rt_phar_bzip2_fail_x86");                        // size mismatch means the archive entry is invalid
    emitter.instruction("mov rax, QWORD PTR [rsp + 24]");                       // return the decompressed buffer pointer
    emitter.instruction("mov rdx, r9");                                         // return the decompressed buffer length
    emitter.instruction("jmp __rt_phar_bzip2_done_x86");                        // skip the failure result
    emitter.label("__rt_phar_bzip2_fail_x86");
    emitter.instruction("xor eax, eax");                                        // null pointer reports decompression failure
    emitter.instruction("xor edx, edx");                                        // failure has no output bytes
    emitter.label("__rt_phar_bzip2_done_x86");
    emitter.instruction("add rsp, 80");                                         // release the bzip2 helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return decompressed bytes or null on failure
}
