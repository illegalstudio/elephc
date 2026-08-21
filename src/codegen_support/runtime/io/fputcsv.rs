//! Purpose:
//! Emits the `__rt_fputcsv` runtime helper assembly for writing a PHP string
//! array as a CSV row to a file descriptor. Supports custom separator,
//! enclosure, escape, and end-of-line characters passed as a packed `csv_opts`
//! word and an optional `(eol_ptr, eol_len)` pair from the EIR lowering.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - `csv_opts = (esc << 16) | (enc << 8) | sep`; zero bytes select defaults
//!   (sep → ',', enc → '"', esc → 0 means RFC 4180 doubling mode).
//! - `eol_len < 0` marks an ABSENT `$eol` and selects the default `"\n"`; `eol_len == 0` is an
//!   EMPTY `$eol`, which php writes as no terminator at all. The pointer cannot decide this —
//!   an empty string materializes with an undefined one.
//! - ARM64 and x86_64 variants mirror the same quoting and escaping logic.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the `__rt_fputcsv` runtime helper, dispatching to the target-specific variant.
pub fn emit_fputcsv(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fputcsv_linux_x86_64(emitter);
        return;
    }
    emit_fputcsv_aarch64(emitter);
}

/// ARM64 variant of `__rt_fputcsv`.
///
/// Signature: `__rt_fputcsv(fd: x0, arr: x1, csv_opts: x2, eol_ptr: x3, eol_len: x4)
/// -> bytes_written: x0`.
///
/// Writes each array element as a CSV field, quoting fields that contain the
/// separator, enclosure, escape, or whitespace characters. Internal quotes are
/// escaped by doubling (RFC 4180, `esc == 0`) or by the escape char (`esc != 0`).
/// A trailing `eol` (or `"\n"` default) is written after the last field.
fn emit_fputcsv_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fputcsv ---");
    emitter.label_global("__rt_fputcsv");

    // -- set up stack frame: 192 bytes (fd, arr, total, index, sep, enc, esc, eol_ptr, eol_len, arrlen, field_ptr, field_len, scratch, scratch2, fp, lr, escaped, elem_tag, mixed cell, owned temp, saved concat offset) --
    //    The last 48 bytes carry the non-string element machinery: the element value_type tag,
    //    a 24-byte Mixed cell built in place so one formatter serves every scalar layout, the
    //    owned cast result, and the caller's `_concat_off`. fp/lr stay at #112 so every other
    //    offset in this helper is unchanged.
    emitter.instruction("sub sp, sp, #192");                                    // allocate 192 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish new frame pointer

    // -- save inputs --
    emitter.instruction("str x0, [sp, #0]");                                    // save fd
    emitter.instruction("str x1, [sp, #8]");                                    // save array pointer
    emitter.instruction("str xzr, [sp, #16]");                                   // total bytes written = 0
    emitter.instruction("str xzr, [sp, #24]");                                   // current element index = 0

    // -- unpack csv_opts: sep = x2 & 0xFF, enc = (x2 >> 8) & 0xFF, esc = (x2 >> 16) & 0xFF --
    emitter.instruction("and w5, w2, #0xff");                                   // sep = csv_opts & 0xFF
    emitter.instruction("lsr w6, w2, #8");                                        // shift right 8 for enc
    emitter.instruction("and w6, w6, #0xff");                                   // enc = (csv_opts >> 8) & 0xFF
    emitter.instruction("lsr w7, w2, #16");                                      // shift right 16 for esc
    emitter.instruction("and w7, w7, #0xff");                                   // esc = (csv_opts >> 16) & 0xFF

    // -- unpack the element value_type the lowering stamped into csv_opts bits 24..27 --
    //    PHP casts every field to string, so the element layout — not a static string-array
    //    requirement — is what this helper needs to know.
    emitter.instruction("lsr w8, w2, #24");                                      // shift the element value_type tag down
    emitter.instruction("and x8, x8, #0xf");                                     // isolate the 4-bit element value_type tag
    emitter.instruction("str x8, [sp, #136]");                                   // save the element value_type tag

    // -- reserve the caller's concat cursor so a row's casts cannot outgrow the shared buffer --
    //    `__rt_itoa` / `__rt_ftoa` format into `_concat_buf` at `_concat_off` and advance it.
    //    Restoring the entry value on return reclaims the whole row's scratch, which keeps a
    //    long `foreach` writing numeric rows from walking off the 64 KiB arena.
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                        // load the caller's concat write offset
    emitter.instruction("str x10, [sp, #176]");                                  // save it for the epilogue to restore

    // -- apply defaults: sep==0 -> 0x2C, enc==0 -> 0x22 --
    emitter.instruction("cbnz w5, __rt_fputcsv_sep_ok");                        // if sep != 0, skip default
    emitter.instruction("mov w5, #0x2c");                                        // sep = ',' (0x2C)
    emitter.label("__rt_fputcsv_sep_ok");
    emitter.instruction("cbnz w6, __rt_fputcsv_enc_ok");                        // if enc != 0, skip default
    emitter.instruction("mov w6, #0x22");                                        // enc = '"' (0x22)
    emitter.label("__rt_fputcsv_enc_ok");

    // -- save sep/enc/esc and eol --
    emitter.instruction("str w5, [sp, #32]");                                    // save sep
    emitter.instruction("str w6, [sp, #40]");                                    // save enc
    emitter.instruction("str w7, [sp, #48]");                                    // save esc
    emitter.instruction("str x3, [sp, #56]");                                    // save eol_ptr
    emitter.instruction("str x4, [sp, #64]");                                    // save eol_len

    // -- get array length from header --
    emitter.instruction("ldr x9, [x1]");                                        // load array length from header
    emitter.instruction("str x9, [sp, #72]");                                   // save array length

    // -- main loop: iterate over array elements --
    emitter.label("__rt_fputcsv_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                    // load current index
    emitter.instruction("ldr x10, [sp, #72]");                                    // load array length
    emitter.instruction("cmp x9, x10");                                         // check if we've processed all elements
    emitter.instruction("b.hs __rt_fputcsv_eol");                                // if done, write trailing eol

    // -- write separator before 2nd+ fields --
    emitter.instruction("cbz x9, __rt_fputcsv_field");                           // skip separator for first field
    emitter.instruction("ldr x0, [sp, #0]");                                     // reload fd
    emitter.instruction("ldr w1, [sp, #32]");                                    // load sep byte
    emitter.instruction("and x1, x1, #0xff");                                     // zero-extend sep
    emitter.instruction("strb w1, [sp, #96]");                                    // store sep byte in scratch slot
    emitter.instruction("add x1, sp, #96");                                        // ptr = scratch slot
    emitter.instruction("mov x2, #1");                                            // write 1 byte (sep)
    emitter.instruction("bl __rt_fd_write");                                      // write the separator
    emitter.instruction("ldr x9, [sp, #16]");                                    // reload total bytes
    emitter.instruction("add x9, x9, x0");                                        // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                     // save updated total

    // -- load current field from array --
    emitter.label("__rt_fputcsv_field");
    emitter.instruction("ldr x9, [sp, #24]");                                    // reload current index
    emitter.instruction("ldr x10, [sp, #8]");                                    // reload array pointer
    emitter.instruction("str xzr, [sp, #168]");                                   // clear the owned cast slot; borrowed layouts release nothing
    emitter.instruction("ldr x12, [sp, #136]");                                  // reload the element value_type tag
    emitter.instruction("cmp x12, #1");                                          // is this a string array?
    emitter.instruction("b.ne __rt_fputcsv_field_nonstr");                        // only value_type 1 stores 16-byte (ptr, len) slots
    emitter.instruction("lsl x11, x9, #4");                                      // byte offset = index * 16
    emitter.instruction("add x11, x10, x11");                                    // element address = array + offset
    emitter.instruction("ldr x3, [x11, #24]");                                    // load string pointer (skip 24-byte header)
    emitter.instruction("ldr x4, [x11, #32]");                                    // load string length
    emitter.instruction("b __rt_fputcsv_field_ready");                            // the payload is already a string

    // -- every other layout stores 8-byte slots and must be cast the way PHP casts a field --
    emitter.label("__rt_fputcsv_field_nonstr");
    emitter.instruction("lsl x11, x9, #3");                                      // byte offset = index * 8
    emitter.instruction("add x11, x10, x11");                                    // element address = array + offset
    emitter.instruction("ldr x5, [x11, #24]");                                    // load the raw element payload (skip 24-byte header)
    emitter.instruction("cmp x12, #7");                                          // are elements boxed Mixed cells?
    emitter.instruction("b.eq __rt_fputcsv_field_boxed");                         // a boxed slot already points at a cell
    // A scalar array stores bare payloads. Wrapping one in a frame-local Mixed cell lets the
    // single `__rt_mixed_cast_string` formatter serve int, float, bool and null alike, and the
    // array's value_type doubles as the cell tag because both use the same numbering.
    emitter.instruction("str x12, [sp, #144]");                                  // cell tag = the array's element value_type
    emitter.instruction("str x5, [sp, #152]");                                    // cell payload low word
    emitter.instruction("str xzr, [sp, #160]");                                   // cell payload high word
    emitter.instruction("add x0, sp, #144");                                      // cast the frame-local cell
    emitter.instruction("b __rt_fputcsv_field_cast");                             // format it
    emitter.label("__rt_fputcsv_field_boxed");
    emitter.instruction("mov x0, x5");                                            // value_type 7 slots hold the cell pointer itself
    emitter.label("__rt_fputcsv_field_cast");
    emitter.instruction("bl __rt_mixed_cast_string");                             // x1 = payload pointer, x2 = payload length
    // Only the string arm allocates (through `__rt_str_persist`); int/float/bool render into the
    // shared concat scratch and null returns a null pointer. `__rt_heap_free` ignores all of
    // those by contract, so recording the result unconditionally can neither leak nor wild-free.
    emitter.instruction("str x1, [sp, #168]");                                   // record the cast result as this row's owned temporary
    emitter.instruction("mov x3, x1");                                            // field pointer
    emitter.instruction("mov x4, x2");                                            // field length
    emitter.label("__rt_fputcsv_field_ready");

    // -- check if field needs quoting (contains sep, enc, esc, or whitespace) --
    emitter.instruction("stp x3, x4, [sp, #80]");                                 // save field ptr and len (overlapping frame top is fine; we saved fp/lr at #80 but this is scratch above fp)
    emitter.instruction("mov x5, #0");                                            // needs_quote flag = 0
    emitter.instruction("mov x6, #0");                                            // scan index = 0
    emitter.label("__rt_fputcsv_scan");
    emitter.instruction("cmp x6, x4");                                            // check if scan complete
    emitter.instruction("b.hs __rt_fputcsv_scan_done");                            // if done scanning, proceed to write
    emitter.instruction("ldrb w7, [x3, x6]");                                    // load byte at current position
    emitter.instruction("ldr w8, [sp, #32]");                                     // load sep
    emitter.instruction("cmp w7, w8");                                            // byte == sep?
    emitter.instruction("b.eq __rt_fputcsv_need_q");                              // needs quoting
    emitter.instruction("ldr w8, [sp, #40]");                                     // load enc
    emitter.instruction("cmp w7, w8");                                            // byte == enc?
    emitter.instruction("b.eq __rt_fputcsv_need_q");                              // needs quoting
    emitter.instruction("ldr w8, [sp, #48]");                                     // load esc
    emitter.instruction("cbz w8, __rt_fputcsv_scan_ws");                          // esc == 0 -> skip esc check
    emitter.instruction("cmp w7, w8");                                            // byte == esc?
    emitter.instruction("b.eq __rt_fputcsv_need_q");                              // needs quoting
    emitter.label("__rt_fputcsv_scan_ws");
    emitter.instruction("cmp w7, #0x20");                                         // byte == space?
    emitter.instruction("b.eq __rt_fputcsv_need_q");                              // needs quoting
    emitter.instruction("cmp w7, #0x09");                                         // byte == tab?
    emitter.instruction("b.eq __rt_fputcsv_need_q");                              // needs quoting
    emitter.instruction("cmp w7, #0x0a");                                         // byte == newline?
    emitter.instruction("b.eq __rt_fputcsv_need_q");                              // needs quoting
    emitter.instruction("cmp w7, #0x0d");                                         // byte == carriage return?
    emitter.instruction("b.eq __rt_fputcsv_need_q");                              // needs quoting
    emitter.instruction("add x6, x6, #1");                                        // increment scan index
    emitter.instruction("b __rt_fputcsv_scan");                                    // continue scanning

    emitter.label("__rt_fputcsv_need_q");
    emitter.instruction("mov x5, #1");                                            // set needs_quote flag

    // -- write the field (quoted or unquoted) --
    emitter.label("__rt_fputcsv_scan_done");
    emitter.instruction("ldp x3, x4, [sp, #80]");                                 // reload field ptr and len
    emitter.instruction("cbz x5, __rt_fputcsv_plain");                            // if no quoting needed, write directly

    // -- write opening quote (enc) --
    emitter.instruction("ldr x0, [sp, #0]");                                       // reload fd
    emitter.instruction("ldr w1, [sp, #40]");                                      // load enc
    emitter.instruction("and x1, x1, #0xff");                                       // zero-extend enc
    emitter.instruction("strb w1, [sp, #96]");                                        // store enc byte in scratch slot
    emitter.instruction("add x1, sp, #96");                                          // ptr = scratch slot
    emitter.instruction("mov x2, #1");                                              // write 1 byte (enc)
    emitter.instruction("bl __rt_fd_write");                                        // write opening quote
    emitter.instruction("ldr x9, [sp, #16]");                                       // reload total bytes
    emitter.instruction("add x9, x9, x0");                                          // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                       // save updated total

    // -- write field contents, doubling an unescaped enclosure --
    //
    // php-src writes the enclosure TWICE for an embedded one and leaves the escape
    // character alone; it never uses the escape character to escape anything on output.
    // The one subtlety is `escaped`: an enclosure that FOLLOWS the escape character is
    // emitted verbatim, so `back\"quote` stays `back\"quote` rather than gaining a
    // doubled quote. Writing `\"` instead of `""` produced files neither PHP nor any
    // other CSV reader parses back to the value that was written.
    emitter.instruction("ldp x3, x4, [sp, #80]");                                   // reload field ptr and len
    emitter.instruction("mov x6, #0");                                              // byte index = 0
    emitter.instruction("str xzr, [sp, #128]");                                     // escaped = 0
    emitter.label("__rt_fputcsv_qloop");
    emitter.instruction("cmp x6, x4");                                              // check if all bytes written
    emitter.instruction("b.hs __rt_fputcsv_close_q");                                // if done, write closing quote
    emitter.instruction("ldrb w7, [x3, x6]");                                        // load current byte
    emitter.instruction("add x6, x6, #1");                                           // advance index
    emitter.instruction("str x6, [sp, #104]");                                        // save current index
    emitter.instruction("ldr w8, [sp, #48]");                                        // load esc
    emitter.instruction("cbz w8, __rt_fputcsv_q_chk_enc");                            // no escape character configured
    emitter.instruction("cmp w7, w8");                                                // byte == esc?
    emitter.instruction("b.ne __rt_fputcsv_q_chk_enc");                               // not the escape character
    // -- the escape character is emitted verbatim, and shields the NEXT byte --
    emitter.instruction("mov x9, #1");
    emitter.instruction("str x9, [sp, #128]");                                        // escaped = 1
    emitter.instruction("b __rt_fputcsv_qchar");                                       // write it unchanged

    emitter.label("__rt_fputcsv_q_chk_enc");
    emitter.instruction("ldr w8, [sp, #40]");                                          // load enc
    emitter.instruction("cmp w7, w8");                                                 // byte == enc?
    emitter.instruction("b.ne __rt_fputcsv_q_plain");                                  // ordinary byte
    emitter.instruction("ldr x9, [sp, #128]");                                         // was it shielded by the escape character?
    emitter.instruction("cbnz x9, __rt_fputcsv_q_plain");                              // yes: php-src does NOT double it
    // -- double it: emit one extra enclosure before the byte itself --
    emitter.instruction("ldr x0, [sp, #0]");                                           // reload fd
    emitter.instruction("ldr w1, [sp, #40]");                                          // load enc
    emitter.instruction("and x1, x1, #0xff");                                          // zero-extend enc
    emitter.instruction("strb w1, [sp, #96]");                                         // store enc in scratch
    emitter.instruction("add x1, sp, #96");                                            // ptr = scratch slot
    emitter.instruction("mov x2, #1");                                                 // write 1 byte
    emitter.instruction("bl __rt_fd_write");                                           // write the doubling enclosure
    emitter.instruction("ldr x9, [sp, #16]");                                          // reload total bytes
    emitter.instruction("add x9, x9, x0");                                             // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                          // save updated total

    emitter.label("__rt_fputcsv_q_plain");
    emitter.instruction("str xzr, [sp, #128]");                                        // escaped = 0

    emitter.label("__rt_fputcsv_qchar");
    // -- write the actual character --
    emitter.instruction("ldp x3, x4, [sp, #80]");                                     // reload field ptr and len
    emitter.instruction("ldr x6, [sp, #104]");                                         // reload byte index
    emitter.instruction("sub x9, x6, #1");                                             // index of byte to write (we advanced x6 earlier)
    emitter.instruction("add x1, x3, x9");                                             // pointer to the byte
    emitter.instruction("ldr x0, [sp, #0]");                                          // reload fd
    emitter.instruction("mov x2, #1");                                                 // write 1 byte
    emitter.instruction("bl __rt_fd_write");                                           // write this byte
    emitter.instruction("ldr x9, [sp, #16]");                                          // reload total bytes
    emitter.instruction("add x9, x9, x0");                                             // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                          // save updated total

    emitter.label("__rt_fputcsv_qloop_next");
    emitter.instruction("ldr x6, [sp, #104]");                                         // reload byte index
    emitter.instruction("ldp x3, x4, [sp, #80]");                                     // reload field ptr and len
    emitter.instruction("b __rt_fputcsv_qloop");                                       // continue writing

    // -- write closing quote (enc) --
    emitter.label("__rt_fputcsv_close_q");
    emitter.instruction("ldr x0, [sp, #0]");                                          // reload fd
    emitter.instruction("ldr w1, [sp, #40]");                                         // load enc
    emitter.instruction("and x1, x1, #0xff");                                          // zero-extend enc
    emitter.instruction("strb w1, [sp, #96]");                                         // store enc byte in scratch
    emitter.instruction("add x1, sp, #96");                                            // ptr = scratch slot
    emitter.instruction("mov x2, #1");                                                // write 1 byte (enc)
    emitter.instruction("bl __rt_fd_write");                                          // write closing quote
    emitter.instruction("ldr x9, [sp, #16]");                                         // reload total bytes
    emitter.instruction("add x9, x9, x0");                                            // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                         // save updated total
    emitter.instruction("b __rt_fputcsv_next");                                       // proceed to next field

    // -- write plain field (no quoting needed) --
    emitter.label("__rt_fputcsv_plain");
    emitter.instruction("ldr x0, [sp, #0]");                                          // reload fd
    emitter.instruction("mov x1, x3");                                                // field pointer
    emitter.instruction("mov x2, x4");                                                // field length
    emitter.instruction("bl __rt_fd_write");                                          // write the plain field
    emitter.instruction("ldr x9, [sp, #16]");                                         // reload total bytes
    emitter.instruction("add x9, x9, x0");                                            // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                         // save updated total

    // -- advance to next element --
    emitter.label("__rt_fputcsv_next");
    emitter.instruction("ldr x0, [sp, #168]");                                        // the cast result this field owned, if any
    emitter.instruction("cbz x0, __rt_fputcsv_next_advance");                          // string slots and borrowed scratch own nothing
    emitter.instruction("bl __rt_heap_free");                                          // release the persisted cast result now its bytes are written
    emitter.label("__rt_fputcsv_next_advance");
    emitter.instruction("ldr x9, [sp, #24]");                                         // reload current index
    emitter.instruction("add x9, x9, #1");                                             // increment index
    emitter.instruction("str x9, [sp, #24]");                                         // save updated index
    emitter.instruction("b __rt_fputcsv_loop");                                        // continue loop

    // -- write trailing eol (custom or default "\n") --
    emitter.label("__rt_fputcsv_eol");
    emitter.instruction("ldr x3, [sp, #56]");                                         // reload eol_ptr
    emitter.instruction("ldr x4, [sp, #64]");                                         // reload eol_len
    emitter.instruction("tbnz x4, #63, __rt_fputcsv_eol_default");                     // a NEGATIVE length is the absent argument -> "\n"
    emitter.instruction("cbz x4, __rt_fputcsv_ret");                                   // an EMPTY $eol writes no terminator at all
    // -- write custom eol --
    emitter.instruction("ldr x0, [sp, #0]");                                          // reload fd
    emitter.instruction("mov x1, x3");                                                // eol pointer
    emitter.instruction("mov x2, x4");                                                // eol length
    emitter.instruction("bl __rt_fd_write");                                          // write the eol
    emitter.instruction("ldr x9, [sp, #16]");                                         // reload total bytes
    emitter.instruction("add x9, x9, x0");                                            // add bytes written
    emitter.instruction("str x9, [sp, #16]");                                         // save final total
    emitter.instruction("b __rt_fputcsv_ret");                                        // return

    emitter.label("__rt_fputcsv_eol_default");
    emitter.instruction("ldr x0, [sp, #0]");                                          // reload fd
    emitter.adrp("x1", "__rt_fputcsv_nl_lit");                                        // load newline literal address
    emitter.add_lo12("x1", "x1", "__rt_fputcsv_nl_lit");                              // resolve exact address
    emitter.instruction("mov x2, #1");                                                // write 1 byte (newline)
    emitter.instruction("bl __rt_fd_write");                                          // write the newline
    emitter.instruction("ldr x9, [sp, #16]");                                         // reload total bytes
    emitter.instruction("add x9, x9, x0");                                            // add final bytes written
    emitter.instruction("str x9, [sp, #16]");                                         // save final total

    // -- return total bytes written --
    emitter.label("__rt_fputcsv_ret");
    // -- reclaim the row's cast scratch before returning --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [sp, #176]");                                       // the caller's concat write offset
    emitter.instruction("str x10, [x9]");                                             // hand the whole row's scratch back
    emitter.instruction("ldr x0, [sp, #16]");                                         // return total bytes written

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #112]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #192");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- literal data for newline --
    emitter.label("__rt_fputcsv_nl_lit");
    emitter.instruction(".ascii \"\\n\"");                                            // newline character literal
}

/// x86_64 Linux variant of `__rt_fputcsv`.
///
/// Signature: `__rt_fputcsv(fd: rdi, arr: rsi, csv_opts: rdx, eol_ptr: rcx,
/// eol_len: r8) -> bytes_written: rax`. Mirrors the ARM64 quoting/escaping
/// logic using the System V ABI and a rbp-relative frame.
fn emit_fputcsv_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fputcsv ---");
    emitter.label_global("__rt_fputcsv");

    // -- prologue: 176-byte frame with rbp --
    //    [rbp - 120] carries the `escaped` flag the enclosure-doubling loop needs; the last 48
    //    bytes carry the non-string element machinery (element value_type tag, a 24-byte Mixed
    //    cell built in place, the owned cast result, the caller's `_concat_off`). Every other
    //    offset is unchanged.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 176");                                         // reserve aligned stack space for writer state

    // -- save inputs --
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                         // preserve the destination file descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                        // preserve the source string-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                          // total written bytes start at zero
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                          // current field index starts at zero
    // -- save eol_ptr (rcx) and eol_len (r8) BEFORE unpacking csv_opts (which clobbers rcx) --
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                        // preserve eol_ptr before rcx is clobbered
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                         // preserve eol_len (r8 not clobbered by unpack)

    // -- unpack csv_opts from rdx: sep=dl, enc=(rdx>>8)&0xFF, esc=(rdx>>16)&0xFF --
    emitter.instruction("movzx ecx, dl");                                       // sep = csv_opts & 0xFF
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                         // save sep
    emitter.instruction("shr rdx, 8");                                          // shift right 8 for enc
    emitter.instruction("movzx ecx, dl");                                       // enc = (csv_opts >> 8) & 0xFF
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                         // save enc
    emitter.instruction("shr rdx, 8");                                          // shift right 8 more for esc
    emitter.instruction("movzx ecx, dl");                                       // esc = (csv_opts >> 16) & 0xFF
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                         // save esc

    // -- unpack the element value_type the lowering stamped into csv_opts bits 24..27 --
    //    PHP casts every field to string, so the element layout — not a static string-array
    //    requirement — is what this helper needs to know.
    emitter.instruction("shr rdx, 8");                                          // shift the element value_type tag down
    emitter.instruction("and rdx, 0xf");                                        // isolate the 4-bit element value_type tag
    emitter.instruction("mov QWORD PTR [rbp - 128], rdx");                       // save the element value_type tag

    // -- reserve the caller's concat cursor so a row's casts cannot outgrow the shared buffer --
    //    `__rt_itoa` / `__rt_ftoa` format into `_concat_buf` at `_concat_off` and advance it.
    //    Restoring the entry value on return reclaims the whole row's scratch, which keeps a
    //    long `foreach` writing numeric rows from walking off the 64 KiB arena.
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r11, QWORD PTR [r10]");                             // load the caller's concat write offset
    emitter.instruction("mov QWORD PTR [rbp - 168], r11");                       // save it for the epilogue to restore

    // -- apply defaults: sep==0 -> 0x2C, enc==0 -> 0x22 --
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                           // sep == 0?
    emitter.instruction("jne __rt_fputcsv_x_sep_ok");                            // if not, skip default
    emitter.instruction("mov QWORD PTR [rbp - 40], 0x2c");                        // sep = ',' (0x2C)
    emitter.label("__rt_fputcsv_x_sep_ok");
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                          // enc == 0?
    emitter.instruction("jne __rt_fputcsv_x_enc_ok");                             // if not, skip default
    emitter.instruction("mov QWORD PTR [rbp - 48], 0x22");                       // enc = '"' (0x22)
    emitter.label("__rt_fputcsv_x_enc_ok");

    // -- get array length from header --
    emitter.instruction("mov r10, QWORD PTR [rsi]");                             // load array length before entering the loop
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                        // preserve the source array length

    // -- main loop: iterate over array elements --
    emitter.label("__rt_fputcsv_x_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                         // reload the current field index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 80]");                         // have we already emitted every field?
    emitter.instruction("jae __rt_fputcsv_x_eol");                                // write the trailing eol once every field has been emitted

    // -- write separator before 2nd+ fields --
    emitter.instruction("test r10, r10");                                         // is the current field index zero?
    emitter.instruction("jz __rt_fputcsv_x_field");                              // skip the separator before the first field
    emitter.instruction("mov edi, DWORD PTR [rbp - 8]");                          // pass the destination fd for the separator
    emitter.instruction("lea rsi, [rbp - 40]");                                  // ptr = address of sep byte on stack
    emitter.instruction("mov edx, 1");                                           // write exactly one separator byte
    emitter.instruction("call __rt_fd_write");                                   // emit the separator through __rt_fd_write()
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                         // accumulate the separator byte count

    // -- load current field from array --
    emitter.label("__rt_fputcsv_x_field");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                         // reload the current field index
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                         // reload the source string-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 160], 0");                          // clear the owned cast slot; borrowed layouts release nothing
    emitter.instruction("mov rdx, QWORD PTR [rbp - 128]");                        // reload the element value_type tag
    emitter.instruction("cmp rdx, 1");                                           // is this a string array?
    emitter.instruction("jne __rt_fputcsv_x_field_nonstr");                       // only value_type 1 stores 16-byte (ptr, len) slots
    emitter.instruction("mov rcx, r10");                                         // copy the field index before scaling
    emitter.instruction("shl rcx, 4");                                           // convert the field index into the byte offset
    emitter.instruction("lea rcx, [r11 + rcx + 24]");                            // compute the current string-slot address
    emitter.instruction("mov r8, QWORD PTR [rcx]");                              // load the current field string pointer
    emitter.instruction("mov r9, QWORD PTR [rcx + 8]");                           // load the current field string length
    emitter.instruction("jmp __rt_fputcsv_x_field_ready");                        // the payload is already a string

    // -- every other layout stores 8-byte slots and must be cast the way PHP casts a field --
    emitter.label("__rt_fputcsv_x_field_nonstr");
    emitter.instruction("mov rcx, r10");                                         // copy the field index before scaling
    emitter.instruction("shl rcx, 3");                                           // convert the field index into the byte offset
    emitter.instruction("lea rcx, [r11 + rcx + 24]");                            // compute the current 8-byte slot address
    // The cast takes its cell pointer in `rax`, NOT in the SysV first-argument register: the
    // runtime's own helpers are split, `__rt_fd_write` above takes rdi/rsi/rdx while
    // `__rt_mixed_cast_string` — like `__rt_heap_free` below — reads `rax`, because it opens by
    // tail-calling `__rt_mixed_unbox`, whose input register that is. Staging in `rdi` handed the
    // unboxer whatever the previous `__rt_fd_write` had returned, which is a byte count, and it
    // dereferenced it. AArch64 has one convention for both, so the same code was correct there.
    emitter.instruction("mov rax, QWORD PTR [rcx]");                             // load the raw element payload
    emitter.instruction("cmp rdx, 7");                                           // are elements boxed Mixed cells?
    emitter.instruction("je __rt_fputcsv_x_field_cast");                          // a boxed slot already holds the cell pointer
    // A scalar array stores bare payloads. Wrapping one in a frame-local Mixed cell lets the
    // single `__rt_mixed_cast_string` formatter serve int, float, bool and null alike, and the
    // array's value_type doubles as the cell tag because both use the same numbering.
    // A Mixed cell is read at ASCENDING addresses — tag at the base, payload at +8, high word at
    // +16 — and an rbp-relative frame grows DOWNWARD, so the tag has to take the lowest address
    // of the three. Writing them in frame order instead put the payload and high word BELOW the
    // base, and the reader then took the two slots above it: the element value_type and the
    // enclosure-doubling flag. AArch64 addresses the same cell from `sp` with rising offsets,
    // which is why only one architecture was wrong.
    emitter.instruction("mov QWORD PTR [rbp - 152], rdx");                        // cell tag = the array's element value_type
    emitter.instruction("mov QWORD PTR [rbp - 144], rax");                        // cell payload low word
    emitter.instruction("mov QWORD PTR [rbp - 136], 0");                          // cell payload high word
    emitter.instruction("lea rax, [rbp - 152]");                                  // cast the frame-local cell
    emitter.label("__rt_fputcsv_x_field_cast");
    emitter.instruction("call __rt_mixed_cast_string");                           // rax = payload pointer, rdx = payload length
    // Only the string arm allocates (through `__rt_str_persist`); int/float/bool render into the
    // shared concat scratch and null returns a null pointer. `__rt_heap_free` ignores all of
    // those by contract, so recording the result unconditionally can neither leak nor wild-free.
    emitter.instruction("mov QWORD PTR [rbp - 160], rax");                        // record the cast result as this row's owned temporary
    emitter.instruction("mov r8, rax");                                          // field pointer
    emitter.instruction("mov r9, rdx");                                          // field length
    emitter.label("__rt_fputcsv_x_field_ready");
    emitter.instruction("mov QWORD PTR [rbp - 88], r8");                         // preserve the current field pointer
    emitter.instruction("mov QWORD PTR [rbp - 96], r9");                         // preserve the current field length
    emitter.instruction("mov QWORD PTR [rbp - 104], 0");                         // needs_quote starts false
    emitter.instruction("xor ecx, ecx");                                         // start scanning from byte index zero

    // -- scan field for quote-triggering bytes --
    emitter.label("__rt_fputcsv_x_scan");
    emitter.instruction("cmp rcx, r9");                                          // have we scanned every byte?
    emitter.instruction("jae __rt_fputcsv_x_scan_done");                         // proceed to field emission once scan completes
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                        // load the current field byte
    emitter.instruction("cmp dl, BYTE PTR [rbp - 40]");                           // byte == sep?
    emitter.instruction("je __rt_fputcsv_x_need_q");                             // quote the field when it contains the separator
    emitter.instruction("cmp dl, BYTE PTR [rbp - 48]");                          // byte == enc?
    emitter.instruction("je __rt_fputcsv_x_need_q");                             // quote the field when it contains the enclosure
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                          // esc == 0?
    emitter.instruction("jz __rt_fputcsv_x_scan_ws");                            // skip esc check if esc is disabled
    emitter.instruction("cmp dl, BYTE PTR [rbp - 56]");                          // byte == esc?
    emitter.instruction("je __rt_fputcsv_x_need_q");                             // quote the field when it contains the escape
    emitter.label("__rt_fputcsv_x_scan_ws");
    emitter.instruction("cmp dl, 0x20");                                         // byte == space?
    emitter.instruction("je __rt_fputcsv_x_need_q");                             // quote the field when it contains whitespace
    emitter.instruction("cmp dl, 0x09");                                         // byte == tab?
    emitter.instruction("je __rt_fputcsv_x_need_q");                             // quote the field when it contains a tab
    emitter.instruction("cmp dl, 0x0a");                                         // byte == newline?
    emitter.instruction("je __rt_fputcsv_x_need_q");                             // quote the field when it contains a newline
    emitter.instruction("cmp dl, 0x0d");                                         // byte == carriage return?
    emitter.instruction("je __rt_fputcsv_x_need_q");                             // quote the field when it contains a carriage return
    emitter.instruction("add rcx, 1");                                           // advance to the next field byte
    emitter.instruction("jmp __rt_fputcsv_x_scan");                              // continue scanning

    emitter.label("__rt_fputcsv_x_need_q");
    emitter.instruction("mov QWORD PTR [rbp - 104], 1");                         // remember that the current field must be quoted

    // -- write the field (quoted or unquoted) --
    emitter.label("__rt_fputcsv_x_scan_done");
    emitter.instruction("cmp QWORD PTR [rbp - 104], 0");                         // does the current field require quoting?
    emitter.instruction("je __rt_fputcsv_x_plain");                              // write the field directly when no quoting needed

    // -- write opening quote (enc) --
    emitter.instruction("mov edi, DWORD PTR [rbp - 8]");                          // pass the destination fd for the opening quote
    emitter.instruction("lea rsi, [rbp - 48]");                                  // ptr = address of enc byte on stack
    emitter.instruction("mov edx, 1");                                          // write exactly one opening quote byte
    emitter.instruction("call __rt_fd_write");                                   // emit the opening quote
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                        // accumulate the opening-quote byte count
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                         // current byte index inside the quoted field

    // -- write field contents, doubling an unescaped enclosure --
    //
    // Mirrors the AArch64 loop: php-src doubles the enclosure and never emits the escape
    // character as an escape, and an enclosure that FOLLOWS the escape character is left
    // alone.
    emitter.instruction("mov QWORD PTR [rbp - 120], 0");                         // escaped = 0
    emitter.label("__rt_fputcsv_x_qloop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 112]");                       // reload the current byte index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 96]");                         // have we emitted every byte from the field?
    emitter.instruction("jae __rt_fputcsv_x_close_q");                           // write the closing quote once all bytes emitted
    emitter.instruction("mov r8, QWORD PTR [rbp - 88]");                         // reload the current field string pointer
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                        // load the current field byte
    emitter.instruction("add QWORD PTR [rbp - 112], 1");                         // advance the byte index
    emitter.instruction("cmp QWORD PTR [rbp - 56], 0");                          // is an escape character configured?
    emitter.instruction("jz __rt_fputcsv_x_chk_enc");                            // none: it cannot shield anything
    emitter.instruction("cmp dl, BYTE PTR [rbp - 56]");                          // byte == esc?
    emitter.instruction("jne __rt_fputcsv_x_chk_enc");                           // not the escape character
    // -- the escape character is emitted verbatim, and shields the NEXT byte --
    emitter.instruction("mov QWORD PTR [rbp - 120], 1");                         // escaped = 1
    emitter.instruction("jmp __rt_fputcsv_x_qchar");                             // write it unchanged

    emitter.label("__rt_fputcsv_x_chk_enc");
    emitter.instruction("cmp dl, BYTE PTR [rbp - 48]");                          // is the byte the enclosure?
    emitter.instruction("jne __rt_fputcsv_x_q_plain");                           // ordinary byte
    emitter.instruction("cmp QWORD PTR [rbp - 120], 0");                         // was it shielded by the escape character?
    emitter.instruction("jne __rt_fputcsv_x_q_plain");                           // yes: php-src does NOT double it
    // -- double it: emit one extra enclosure before the byte itself --
    emitter.instruction("mov edi, DWORD PTR [rbp - 8]");                         // pass the destination fd
    emitter.instruction("lea rsi, [rbp - 48]");                                  // ptr = address of enc byte
    emitter.instruction("mov edx, 1");                                          // write one enc
    emitter.instruction("call __rt_fd_write");                                  // emit the doubling enclosure
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                        // accumulate

    emitter.label("__rt_fputcsv_x_q_plain");
    emitter.instruction("mov QWORD PTR [rbp - 120], 0");                         // escaped = 0

    emitter.label("__rt_fputcsv_x_qchar");
    // -- write the actual character --
    emitter.instruction("mov edi, DWORD PTR [rbp - 8]");                        // pass the destination fd
    emitter.instruction("mov r8, QWORD PTR [rbp - 88]");                        // reload the current field string pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 112]");                      // reload the byte index (already advanced)
    emitter.instruction("sub rcx, 1");                                        // index of byte to write
    emitter.instruction("lea rsi, [r8 + rcx]");                                // pointer to the byte
    emitter.instruction("mov edx, 1");                                         // write exactly one byte
    emitter.instruction("call __rt_fd_write");                                 // emit the byte
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                      // accumulate
    emitter.instruction("jmp __rt_fputcsv_x_qloop");                           // continue the quoted field loop

    // -- write closing quote (enc) --
    emitter.label("__rt_fputcsv_x_close_q");
    emitter.instruction("mov edi, DWORD PTR [rbp - 8]");                       // pass the destination fd
    emitter.instruction("lea rsi, [rbp - 48]");                                // ptr = address of enc byte
    emitter.instruction("mov edx, 1");                                         // write one closing quote
    emitter.instruction("call __rt_fd_write");                                // emit the closing quote
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                      // accumulate
    emitter.instruction("jmp __rt_fputcsv_x_next");                            // advance to the next field

    // -- write plain field (no quoting needed) --
    emitter.label("__rt_fputcsv_x_plain");
    emitter.instruction("mov edi, DWORD PTR [rbp - 8]");                       // pass the destination fd
    emitter.instruction("mov rsi, QWORD PTR [rbp - 88]");                       // pass the field pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 96]");                      // pass the field length
    emitter.instruction("call __rt_fd_write");                                 // emit the plain field
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                      // accumulate

    // -- advance to next element --
    emitter.label("__rt_fputcsv_x_next");
    emitter.instruction("mov rax, QWORD PTR [rbp - 160]");                     // the cast result this field owned, if any
    emitter.instruction("test rax, rax");                                      // string slots and borrowed scratch own nothing
    emitter.instruction("jz __rt_fputcsv_x_next_advance");                     // skip the release for a borrowed payload
    emitter.instruction("call __rt_heap_free");                                // release the persisted cast result now its bytes are written
    emitter.label("__rt_fputcsv_x_next_advance");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                        // advance the field index
    emitter.instruction("jmp __rt_fputcsv_x_loop");                           // continue emitting the remaining fields

    // -- write trailing eol (custom or default "\n") --
    emitter.label("__rt_fputcsv_x_eol");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                      // reload eol_ptr
    emitter.instruction("mov rcx, QWORD PTR [rbp - 72]");                      // reload eol_len
    emitter.instruction("test rcx, rcx");                                      // classify the eol length
    emitter.instruction("js __rt_fputcsv_x_eol_default");                      // a NEGATIVE length is the absent argument -> "\n"
    emitter.instruction("jz __rt_fputcsv_x_ret");                              // an EMPTY $eol writes no terminator at all
    // -- write custom eol --
    emitter.instruction("mov edi, DWORD PTR [rbp - 8]");                      // pass the destination fd
    emitter.instruction("mov rsi, rax");                                      // pass eol pointer
    emitter.instruction("mov rdx, rcx");                                      // pass eol length
    emitter.instruction("call __rt_fd_write");                                // emit the eol
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                      // accumulate
    emitter.instruction("jmp __rt_fputcsv_x_ret");                            // return

    emitter.label("__rt_fputcsv_x_eol_default");
    emitter.instruction("mov edi, DWORD PTR [rbp - 8]");                      // pass the destination fd
    emitter.instruction("lea rsi, [rip + __rt_fputcsv_nl_lit]");               // pass the newline literal address
    emitter.instruction("mov edx, 1");                                        // write exactly one trailing newline byte
    emitter.instruction("call __rt_fd_write");                                // emit the trailing newline
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                      // accumulate the trailing newline byte count

    // -- return total bytes written --
    emitter.label("__rt_fputcsv_x_ret");
    // -- reclaim the row's cast scratch before returning --
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r11, QWORD PTR [rbp - 168]");                     // the caller's concat write offset
    emitter.instruction("mov QWORD PTR [r10], r11");                           // hand the whole row's scratch back
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                      // return the total written byte count
    emitter.instruction("leave");                                             // restore rbp + rsp
    emitter.instruction("ret");                                               // return to caller

    // -- literal data for newline --
    emitter.label("__rt_fputcsv_nl_lit");
    emitter.instruction(".ascii \"\\n\"");                                    // trailing newline character literal
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// The frame-local Mixed cell must be written at ASCENDING addresses from the pointer the
    /// formatter receives, on BOTH architectures.
    ///
    /// This is the one invariant a same-architecture test suite cannot catch. A Mixed cell is
    /// read as tag@0, payload@+8, high@+16. AArch64 addresses it from `sp` with rising offsets,
    /// so writing the fields in frame order is automatically correct there; an x86_64 `rbp`
    /// frame grows DOWNWARD, so the same line order puts the payload and high word BELOW the
    /// base and the formatter reads the two unrelated slots above it instead. The whole CSV cast
    /// suite was green on aarch64 and segfaulted on x86_64 for exactly this reason, so the
    /// layout is pinned on the EMITTED assembly rather than on a run.
    #[test]
    fn test_fputcsv_x86_64_builds_the_mixed_cell_at_ascending_addresses() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_fputcsv(&mut emitter);
        let asm = emitter.output();
        let base = asm
            .find("lea rax, [rbp - 152]")
            .expect("the cell pointer must be the LOWEST of the three slots");
        for (offset, slot) in [(0usize, "rbp - 152"), (8, "rbp - 144"), (16, "rbp - 136")] {
            assert!(
                asm.contains(&format!("QWORD PTR [{slot}]")),
                "cell field at +{offset} must live at [{slot}]"
            );
        }
        // The tag is the field the formatter reads first, so it must occupy the base itself.
        let tag_write = asm
            .find("mov QWORD PTR [rbp - 152], rdx")
            .expect("the cell tag must be written at the base address");
        assert!(tag_write < base, "the cell must be filled before it is passed");
    }

    /// The cast must receive its cell pointer in the register the formatter actually READS.
    ///
    /// The runtime's helpers do not share one calling convention on x86_64: `__rt_fd_write` takes
    /// rdi/rsi/rdx like any SysV function, while `__rt_mixed_cast_string` and `__rt_heap_free`
    /// read `rax` — the first opens by falling into `__rt_mixed_unbox`, whose input register that
    /// is. Staging the cell in `rdi` therefore compiled, linked and ran, and handed the unboxer
    /// whatever the previous `__rt_fd_write` had left in `rax`: a byte count, dereferenced as a
    /// pointer. Every non-string CSV layout segfaulted on x86_64 and every one passed on aarch64,
    /// where one convention serves both helpers.
    ///
    /// `implode` is the reference caller — it has always staged this cast in `rax`.
    #[test]
    fn test_the_cast_argument_reaches_the_register_the_formatter_reads() {
        for (arch, label, stage) in [
            (Arch::AArch64, "__rt_fputcsv_field_cast:", "add x0, sp, #144"),
            (Arch::X86_64, "__rt_fputcsv_x_field_cast:", "lea rax, [rbp - 152]"),
        ] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_fputcsv(&mut emitter);
            let asm = emitter.output();
            let staged = asm
                .find(stage)
                .unwrap_or_else(|| panic!("{arch:?}: the cell pointer must be staged for the cast"));
            let at = asm
                .find(label)
                .unwrap_or_else(|| panic!("{arch:?}: the cast must be labelled"));
            assert!(
                staged < at,
                "{arch:?}: the pointer must be in place before the cast is entered"
            );
            // The scalar arm falls through into the label; the boxed arm jumps to it. Both must
            // therefore leave the pointer in the SAME register, which is the half that was wrong.
            let boxed = asm
                .find(match arch {
                    Arch::AArch64 => "mov x0, x5",
                    Arch::X86_64 => "mov rax, QWORD PTR [rcx]",
                })
                .unwrap_or_else(|| panic!("{arch:?}: the boxed arm must load the cell pointer"));
            assert!(
                boxed < at,
                "{arch:?}: the boxed arm must load into the same register the scalar arm uses"
            );
        }
    }

    /// The AArch64 half writes the same cell upward from `sp`, and its pointer is the LOWEST
    /// offset of the three for the same reason.
    #[test]
    fn test_fputcsv_aarch64_builds_the_mixed_cell_at_ascending_addresses() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::AArch64));
        emit_fputcsv(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("str x12, [sp, #144]"), "cell tag at the base");
        assert!(asm.contains("str x5, [sp, #152]"), "cell payload at +8");
        assert!(asm.contains("str xzr, [sp, #160]"), "cell high word at +16");
        assert!(asm.contains("add x0, sp, #144"), "the cell pointer is the base");
    }

    /// Both architectures must hand the row's formatting scratch back before returning.
    ///
    /// `__rt_itoa` formats into the shared 64 KiB concat arena and advances its cursor, so a
    /// long numeric loop would walk off the arena — silently — if the writer kept the ground it
    /// used. The failure this pins is a memory overrun, not a wrong field, which is why it is
    /// pinned on the emitted code rather than left to a functional test.
    #[test]
    fn test_fputcsv_restores_the_callers_concat_cursor() {
        // The symbol is NAMED a different number of times per architecture — AArch64 needs an
        // `adrp`/`add` pair per address, x86_64 a single `lea` — so the count is stated per
        // target rather than shared, which is the honest form of "twice: once in, once out".
        for (arch, mentions) in [(Arch::AArch64, 4), (Arch::X86_64, 2)] {
            let mut emitter = Emitter::new(Target::new(Platform::Linux, arch));
            emit_fputcsv(&mut emitter);
            let asm = emitter.output();
            assert_eq!(
                asm.matches("_concat_off").count(),
                mentions,
                "{arch:?} must read the cursor on entry and write it back on return"
            );
        }
    }
}
