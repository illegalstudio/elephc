//! Purpose:
//! Emits the runtime `phar://` read helpers: `__rt_phar_read_entry`, which reads
//! and parses a PHAR archive at run time and materializes a named uncompressed
//! entry as a readable stream, and `__rt_fopen_maybe_phar`, the `fopen` gate that
//! routes a non-literal `phar://...` read URL to it (and everything else to
//! `__rt_fopen`).
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
//!   URLs reach here. Milestone 2 scope: one named UNCOMPRESSED entry; gzip/bzip2
//!   entries and write-modify are out of scope.
//! - Reuses `__rt_file_get_contents` (reads the whole archive into a heap buffer)
//!   and tail-calls `__rt_data_stream` (writes the matched entry to an unlinked
//!   tmpfile and rewinds it) so the resulting fd behaves like any read stream.
//! - The archive/entry split is the `.phar/` boundary (matching the write path);
//!   a runtime archive path without `.phar/` in its name is unsupported in M2.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

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

    // -- split the URL at ".phar/" (x0 = url ptr, x1 = url len) --
    emitter.instruction("mov x9, #7");                                          // scan index i = 7 (past "phar://")
    emitter.label("__rt_phar_read_split_loop");
    emitter.instruction("add x10, x9, #6");                                     // i + 6 (end of a candidate ".phar/")
    emitter.instruction("cmp x10, x1");                                         // does ".phar/" fit before the URL end?
    emitter.instruction("b.gt __rt_phar_read_fail");                            // no ".phar/" boundary → not selectable
    emitter.instruction("ldrb w11, [x0, x9]");                                  // url[i]
    emitter.instruction("cmp w11, #0x2e");                                      // '.'
    emitter.instruction("b.ne __rt_phar_read_split_next");
    emitter.instruction("add x12, x9, #1");
    emitter.instruction("ldrb w11, [x0, x12]");                                 // url[i+1]
    emitter.instruction("cmp w11, #0x70");                                      // 'p'
    emitter.instruction("b.ne __rt_phar_read_split_next");
    emitter.instruction("add x12, x9, #2");
    emitter.instruction("ldrb w11, [x0, x12]");                                 // url[i+2]
    emitter.instruction("cmp w11, #0x68");                                      // 'h'
    emitter.instruction("b.ne __rt_phar_read_split_next");
    emitter.instruction("add x12, x9, #3");
    emitter.instruction("ldrb w11, [x0, x12]");                                 // url[i+3]
    emitter.instruction("cmp w11, #0x61");                                      // 'a'
    emitter.instruction("b.ne __rt_phar_read_split_next");
    emitter.instruction("add x12, x9, #4");
    emitter.instruction("ldrb w11, [x0, x12]");                                 // url[i+4]
    emitter.instruction("cmp w11, #0x72");                                      // 'r'
    emitter.instruction("b.ne __rt_phar_read_split_next");
    emitter.instruction("add x12, x9, #5");
    emitter.instruction("ldrb w11, [x0, x12]");                                 // url[i+5]
    emitter.instruction("cmp w11, #0x2f");                                      // '/'
    emitter.instruction("b.eq __rt_phar_read_split_found");                     // ".phar/" matched at i
    emitter.label("__rt_phar_read_split_next");
    emitter.instruction("add x9, x9, #1");                                      // advance the scan index
    emitter.instruction("b __rt_phar_read_split_loop");
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
    emitter.instruction("cmp w13, w14");
    emitter.instruction("b.ne __rt_phar_read_halt_next");                       // mismatch at this position
    emitter.instruction("add x11, x11, #1");                                    // next compare byte
    emitter.instruction("b __rt_phar_read_halt_cmp");
    emitter.label("__rt_phar_read_halt_next");
    emitter.instruction("add x9, x9, #1");                                      // advance the scan index
    emitter.instruction("b __rt_phar_read_halt_scan");
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
    emitter.instruction("b.gt __rt_phar_read_fail");
    emitter.instruction("ldr w13, [x19, x9]");                                  // name_len
    emitter.instruction("add x9, x9, #4");                                      // q now at the name bytes
    emitter.instruction("add x14, x9, x13");                                    // q + name_len
    emitter.instruction("cmp x14, x20");                                        // bounds: name bytes within N?
    emitter.instruction("b.gt __rt_phar_read_fail");
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
    emitter.instruction("cmp w0, w1");
    emitter.instruction("b.ne __rt_phar_read_entry_mismatch");                  // byte mismatch → not this entry
    emitter.instruction("add x14, x14, #1");                                    // next name byte
    emitter.instruction("b __rt_phar_read_name_cmp");
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
    emitter.instruction("b __rt_phar_read_entry_loop");

    emitter.label("__rt_phar_read_name_match");
    // q at name start, x13 = name_len, x10 = data_section, x12 = data_offset.
    emitter.instruction("add x14, x9, x13");                                    // q + name_len (start of uncompressed field)
    emitter.instruction("add x14, x14, #8");                                    // &compressed = q + name_len + 8
    emitter.instruction("ldr w15, [x19, x14]");                                 // content length = compressed size
    emitter.instruction("add x14, x10, x12");                                   // data_section + data_offset
    emitter.instruction("add x14, x14, x15");                                   // + content length (end offset)
    emitter.instruction("cmp x14, x20");                                        // bounds: content within N?
    emitter.instruction("b.gt __rt_phar_read_fail");
    emitter.instruction("add x0, x19, x10");                                    // buffer + data_section
    emitter.instruction("add x0, x0, x12");                                     // + data_offset = content ptr
    emitter.instruction("mov x1, x15");                                         // content length
    emitter.instruction("bl __rt_data_stream");                                 // materialize a readable fd over the entry; x0 = fd
    emitter.instruction("b __rt_phar_read_done");

    emitter.label("__rt_phar_read_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 → PHP false (missing archive/entry)
    emitter.label("__rt_phar_read_done");
    emitter.instruction("ldp x19, x20, [sp, #0]");                              // restore callee-saved x19/x20
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore callee-saved x21/x22
    emitter.instruction("ldp x23, x24, [sp, #32]");                             // restore callee-saved x23/x24
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the entry stream descriptor

    // ===== __rt_fopen_maybe_phar(x1=fname ptr, x2=fname len, x3=mode ptr, x4=mode len) =====
    emitter.blank();
    emitter.comment("--- runtime: fopen_maybe_phar ---");
    emitter.label_global("__rt_fopen_maybe_phar");
    emitter.instruction("cmp x2, #7");                                          // filename at least "phar://" long?
    emitter.instruction("b.lt __rt_fopen_maybe_phar_plain");
    emitter.instruction("ldrb w9, [x1, #0]");                                   // 'p'
    emitter.instruction("cmp w9, #0x70");
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");
    emitter.instruction("ldrb w9, [x1, #1]");                                   // 'h'
    emitter.instruction("cmp w9, #0x68");
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");
    emitter.instruction("ldrb w9, [x1, #2]");                                   // 'a'
    emitter.instruction("cmp w9, #0x61");
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");
    emitter.instruction("ldrb w9, [x1, #3]");                                   // 'r'
    emitter.instruction("cmp w9, #0x72");
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");
    emitter.instruction("ldrb w9, [x1, #4]");                                   // ':'
    emitter.instruction("cmp w9, #0x3a");
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");
    emitter.instruction("ldrb w9, [x1, #5]");                                   // '/'
    emitter.instruction("cmp w9, #0x2f");
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");
    emitter.instruction("ldrb w9, [x1, #6]");                                   // '/'
    emitter.instruction("cmp w9, #0x2f");
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");
    emitter.instruction("cbz x4, __rt_fopen_maybe_phar_plain");                 // empty mode → not a read open
    emitter.instruction("ldrb w9, [x3, #0]");                                   // mode[0]
    emitter.instruction("cmp w9, #0x72");                                       // 'r' (read)?
    emitter.instruction("b.ne __rt_fopen_maybe_phar_plain");                    // write/append modes stay literal-only
    emitter.instruction("mov x0, x1");                                          // url ptr → __rt_phar_read_entry arg0
    emitter.instruction("mov x1, x2");                                          // url len → __rt_phar_read_entry arg1
    emitter.instruction("b __rt_phar_read_entry");                              // tail-call the runtime phar reader
    emitter.label("__rt_fopen_maybe_phar_plain");
    emitter.instruction("b __rt_fopen");                                        // tail-call the generic open (args intact)

    // ===== __rt_file_get_contents_maybe_phar(x1=url ptr, x2=url len) -> x1=str ptr, x2=len =====
    emitter.blank();
    emitter.comment("--- runtime: file_get_contents_maybe_phar ---");
    emitter.label_global("__rt_file_get_contents_maybe_phar");
    emitter.instruction("cmp x2, #7");                                          // at least "phar://" long?
    emitter.instruction("b.lt __rt_fgc_phar_plain");
    emitter.instruction("ldrb w9, [x1, #0]");                                   // 'p'
    emitter.instruction("cmp w9, #0x70");
    emitter.instruction("b.ne __rt_fgc_phar_plain");
    emitter.instruction("ldrb w9, [x1, #1]");                                   // 'h'
    emitter.instruction("cmp w9, #0x68");
    emitter.instruction("b.ne __rt_fgc_phar_plain");
    emitter.instruction("ldrb w9, [x1, #2]");                                   // 'a'
    emitter.instruction("cmp w9, #0x61");
    emitter.instruction("b.ne __rt_fgc_phar_plain");
    emitter.instruction("ldrb w9, [x1, #3]");                                   // 'r'
    emitter.instruction("cmp w9, #0x72");
    emitter.instruction("b.ne __rt_fgc_phar_plain");
    emitter.instruction("ldrb w9, [x1, #4]");                                   // ':'
    emitter.instruction("cmp w9, #0x3a");
    emitter.instruction("b.ne __rt_fgc_phar_plain");
    emitter.instruction("ldrb w9, [x1, #5]");                                   // '/'
    emitter.instruction("cmp w9, #0x2f");
    emitter.instruction("b.ne __rt_fgc_phar_plain");
    emitter.instruction("ldrb w9, [x1, #6]");                                   // '/'
    emitter.instruction("cmp w9, #0x2f");
    emitter.instruction("b.ne __rt_fgc_phar_plain");
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
    emitter.instruction("mov x2, #0");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the failure (boxed false)
    emitter.label("__rt_fgc_phar_plain");
    emitter.instruction("b __rt_file_get_contents");                            // tail-call the generic reader (args intact)
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

    // -- split the URL at ".phar/" (rdi = url ptr, rsi = url len) --
    emitter.instruction("mov r8, 7");                                           // scan index i = 7 (past "phar://")
    emitter.label("__rt_phar_read_split_loop_x86");
    emitter.instruction("lea r9, [r8 + 6]");                                    // i + 6
    emitter.instruction("cmp r9, rsi");                                         // does ".phar/" fit before the URL end?
    emitter.instruction("jg __rt_phar_read_fail_x86");                          // no ".phar/" boundary → fail
    emitter.instruction("cmp BYTE PTR [rdi + r8], 0x2e");                       // '.'
    emitter.instruction("jne __rt_phar_read_split_next_x86");
    emitter.instruction("lea r9, [r8 + 1]");
    emitter.instruction("cmp BYTE PTR [rdi + r9], 0x70");                       // 'p'
    emitter.instruction("jne __rt_phar_read_split_next_x86");
    emitter.instruction("lea r9, [r8 + 2]");
    emitter.instruction("cmp BYTE PTR [rdi + r9], 0x68");                       // 'h'
    emitter.instruction("jne __rt_phar_read_split_next_x86");
    emitter.instruction("lea r9, [r8 + 3]");
    emitter.instruction("cmp BYTE PTR [rdi + r9], 0x61");                       // 'a'
    emitter.instruction("jne __rt_phar_read_split_next_x86");
    emitter.instruction("lea r9, [r8 + 4]");
    emitter.instruction("cmp BYTE PTR [rdi + r9], 0x72");                       // 'r'
    emitter.instruction("jne __rt_phar_read_split_next_x86");
    emitter.instruction("lea r9, [r8 + 5]");
    emitter.instruction("cmp BYTE PTR [rdi + r9], 0x2f");                       // '/'
    emitter.instruction("je __rt_phar_read_split_found_x86");                   // ".phar/" matched at i
    emitter.label("__rt_phar_read_split_next_x86");
    emitter.instruction("inc r8");                                              // advance the scan index
    emitter.instruction("jmp __rt_phar_read_split_loop_x86");
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
    emitter.instruction("lea rbx, [rip + _phar_halt_magic]");                   // terminator literal ptr
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
    emitter.instruction("cmp al, cl");
    emitter.instruction("jne __rt_phar_read_halt_next_x86");                    // mismatch
    emitter.instruction("inc r10");                                             // next compare byte
    emitter.instruction("jmp __rt_phar_read_halt_cmp_x86");
    emitter.label("__rt_phar_read_halt_next_x86");
    emitter.instruction("inc r8");                                              // advance the scan index
    emitter.instruction("jmp __rt_phar_read_halt_scan_x86");
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
    emitter.instruction("jg __rt_phar_read_fail_x86");
    emitter.instruction("mov r9d, DWORD PTR [r12 + r8]");                       // name_len (r9, per iteration)
    emitter.instruction("add r8, 4");                                           // q now at the name bytes
    emitter.instruction("lea rax, [r8 + r9]");                                  // q + name_len
    emitter.instruction("cmp rax, r13");                                        // bounds: name bytes within N?
    emitter.instruction("jg __rt_phar_read_fail_x86");
    emitter.instruction("cmp r9, r15");                                         // name_len == requested entry len?
    emitter.instruction("jne __rt_phar_read_entry_mismatch_x86");
    emitter.instruction("xor rax, rax");                                        // compare index k = 0
    emitter.label("__rt_phar_read_name_cmp_x86");
    emitter.instruction("cmp rax, r9");                                         // compared every name byte?
    emitter.instruction("jge __rt_phar_read_name_match_x86");                   // full match
    emitter.instruction("lea rcx, [r8 + rax]");                                 // q + k
    emitter.instruction("movzx edx, BYTE PTR [r12 + rcx]");                     // archive name byte (dl)
    emitter.instruction("lea rcx, [r14 + rax]");                                // entry name + k
    emitter.instruction("movzx ecx, BYTE PTR [rcx]");                           // requested name byte (cl)
    emitter.instruction("cmp dl, cl");
    emitter.instruction("jne __rt_phar_read_entry_mismatch_x86");
    emitter.instruction("inc rax");                                             // next name byte
    emitter.instruction("jmp __rt_phar_read_name_cmp_x86");
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
    emitter.instruction("jmp __rt_phar_read_entry_loop_x86");

    emitter.label("__rt_phar_read_name_match_x86");
    // q at name start, r9 = name_len, rbx = data_section, r10 = data_offset.
    emitter.instruction("lea rax, [r8 + r9]");                                  // q + name_len (uncompressed field)
    emitter.instruction("add rax, 8");                                          // &compressed = q + name_len + 8
    emitter.instruction("mov eax, DWORD PTR [r12 + rax]");                      // content length = compressed size
    emitter.instruction("mov rcx, rbx");                                        // data_section
    emitter.instruction("add rcx, r10");                                        // + data_offset
    emitter.instruction("mov rdx, rcx");                                        // end offset = data_section + data_offset ...
    emitter.instruction("add rdx, rax");                                        // ... + content length
    emitter.instruction("cmp rdx, r13");                                        // bounds: content within N?
    emitter.instruction("jg __rt_phar_read_fail_x86");
    emitter.instruction("lea rdi, [r12 + rcx]");                                // content ptr = buffer + data_section + data_offset
    emitter.instruction("mov rsi, rax");                                        // content length
    emitter.instruction("call __rt_data_stream");                               // materialize a readable fd; rax = fd
    emitter.instruction("jmp __rt_phar_read_done_x86");

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

    // ===== __rt_fopen_maybe_phar(rax=fname ptr, rdx=fname len, rdi=mode ptr, rsi=mode len) =====
    emitter.blank();
    emitter.comment("--- runtime: fopen_maybe_phar ---");
    emitter.label_global("__rt_fopen_maybe_phar");
    emitter.instruction("cmp rdx, 7");                                          // filename at least "phar://" long?
    emitter.instruction("jl __rt_fopen_maybe_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rax + 0], 0x70");                        // 'p'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rax + 1], 0x68");                        // 'h'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rax + 2], 0x61");                        // 'a'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rax + 3], 0x72");                        // 'r'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rax + 4], 0x3a");                        // ':'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rax + 5], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rax + 6], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");
    emitter.instruction("test rsi, rsi");                                       // empty mode → not a read open
    emitter.instruction("jz __rt_fopen_maybe_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rdi + 0], 0x72");                        // mode[0] == 'r'?
    emitter.instruction("jne __rt_fopen_maybe_phar_plain_x86");                 // write/append stay literal-only
    emitter.instruction("mov rdi, rax");                                        // url ptr → __rt_phar_read_entry arg0
    emitter.instruction("mov rsi, rdx");                                        // url len → __rt_phar_read_entry arg1
    emitter.instruction("jmp __rt_phar_read_entry");                            // tail-call the runtime phar reader
    emitter.label("__rt_fopen_maybe_phar_plain_x86");
    emitter.instruction("jmp __rt_fopen");                                      // tail-call the generic open (args intact)

    // ===== __rt_file_get_contents_maybe_phar(rax=url ptr, rdx=url len) -> rax=str ptr, rdx=len =====
    emitter.blank();
    emitter.comment("--- runtime: file_get_contents_maybe_phar ---");
    emitter.label_global("__rt_file_get_contents_maybe_phar");
    emitter.instruction("cmp rdx, 7");                                          // at least "phar://" long?
    emitter.instruction("jl __rt_fgc_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rax + 0], 0x70");                        // 'p'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rax + 1], 0x68");                        // 'h'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rax + 2], 0x61");                        // 'a'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rax + 3], 0x72");                        // 'r'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rax + 4], 0x3a");                        // ':'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rax + 5], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");
    emitter.instruction("cmp BYTE PTR [rax + 6], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fgc_phar_plain_x86");
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
    emitter.instruction("xor edx, edx");
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure (boxed false)
    emitter.label("__rt_fgc_phar_plain_x86");
    emitter.instruction("jmp __rt_file_get_contents");                          // tail-call the generic reader (args intact)
}
