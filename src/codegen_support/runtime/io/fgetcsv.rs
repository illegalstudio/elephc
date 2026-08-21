//! Purpose:
//! Emits the `__rt_fgetcsv` runtime helper assembly for CSV row parsing.
//! Supports custom separator, enclosure, and escape characters passed as a
//! packed `csv_opts` word from the EIR lowering.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - ARM64 and x86_64 variants share the same CSV state machine.
//! - `csv_opts = (esc << 16) | (enc << 8) | sep`; zero bytes select defaults
//!   (sep → ',', enc → '"', esc → 0 means RFC 4180 doubling mode).

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the `__rt_fgetcsv` runtime helper, dispatching to the target-specific variant.
pub fn emit_fgetcsv(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fgetcsv_linux_x86_64(emitter);
        emit_str_getcsv_x86_64(emitter);
        emit_csv_row_to_mixed_x86_64(emitter);
        emit_fgetcsv_row_to_mixed_x86_64(emitter);
        return;
    }
    emit_fgetcsv_aarch64(emitter);
    emit_str_getcsv_aarch64(emitter);
    emit_csv_row_to_mixed_aarch64(emitter);
    emit_fgetcsv_row_to_mixed_aarch64(emitter);
}

/// Emits `__rt_fgetcsv_row_to_mixed(x0 = eof_or_blank_or_row) -> array<mixed>|null: x0`.
///
/// `fgetcsv()` needs TWO "no row" answers where `str_getcsv()` needs one, because php-src
/// decides them in two different places. `PHP_FUNCTION(fgetcsv)` (ext/standard/file.c:1867)
/// answers `false` when `php_stream_get_line()` reports no line at all, and only then calls
/// `php_fgetcsv()`, whose own `NULL` — `first_field && bptr == line_end` at file.c:1939 — is a
/// BLANK LINE and becomes `php_bc_fgetcsv_empty_line()`, the one-element `[null]` array.
///
/// A single null pointer cannot carry both, so `__rt_fgetcsv` returns `0` for end of input and
/// the non-pointer sentinel `1` for a blank record. This helper is where the two separate again:
/// `0` passes straight through for `box_listing_or_false_result` to turn into `false`, and `1`
/// becomes the null pointer that the shared `__rt_csv_row_to_mixed` — `str_getcsv()`'s
/// substitution, unchanged — reads as "no record" and answers with `[null]`.
fn emit_fgetcsv_row_to_mixed_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fgetcsv_row_to_mixed ---");
    emitter.label_global("__rt_fgetcsv_row_to_mixed");

    emitter.instruction("cbz x0, __rt_fgetcsv_row_to_mixed_eof");               // 0 = end of input: php answers false
    emitter.instruction("cmp x0, #1");                                          // 1 = the blank-record sentinel
    emitter.instruction("b.ne __rt_fgetcsv_row_to_mixed_row");
    emitter.instruction("mov x0, #0");                                          // a blank record IS php_fgetcsv()'s NULL
    emitter.label("__rt_fgetcsv_row_to_mixed_row");
    emitter.instruction("b __rt_csv_row_to_mixed");                             // tail call: [null], or the widened row

    emitter.label("__rt_fgetcsv_row_to_mixed_eof");
    emitter.instruction("ret");                                                 // x0 is already the null pointer
}

/// x86_64 form of [`emit_fgetcsv_row_to_mixed_aarch64`]:
/// `__rt_fgetcsv_row_to_mixed(rdi = eof_or_blank_or_row) -> array<mixed>|null: rax`.
fn emit_fgetcsv_row_to_mixed_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fgetcsv_row_to_mixed ---");
    emitter.label_global("__rt_fgetcsv_row_to_mixed");

    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_fgetcsv_row_to_mixed_x_eof");                  // 0 = end of input: php answers false
    emitter.instruction("cmp rdi, 1");                                          // 1 = the blank-record sentinel
    emitter.instruction("jne __rt_fgetcsv_row_to_mixed_x_row");
    emitter.instruction("xor edi, edi");                                        // a blank record IS php_fgetcsv()'s NULL
    emitter.label("__rt_fgetcsv_row_to_mixed_x_row");
    emitter.instruction("jmp __rt_csv_row_to_mixed");                           // tail call: [null], or the widened row

    emitter.label("__rt_fgetcsv_row_to_mixed_x_eof");
    emitter.instruction("xor eax, eax");                                        // the result register carries the null pointer
    emitter.instruction("ret");
}

/// Emits `__rt_csv_row_to_mixed(x0 = row_or_null) -> array<mixed>: x0`.
///
/// php-src's `php_fgetcsv()` returns NO ARRAY for a blank record and both of its callers —
/// `PHP_FUNCTION(str_getcsv)` in `ext/standard/string.c` and `PHP_FUNCTION(fgetcsv)` in
/// `ext/standard/file.c` — replace it with `php_bc_fgetcsv_empty_line()`, a one-element array
/// holding `null`. This helper is that substitution, and it is also where the row widens from
/// string slots to boxed `Mixed` cells.
///
/// The widening is the whole point, and it is not decoration: writing the null SENTINEL into a
/// string slot is INERT, because a `array<string>` element has no way to read back as null —
/// the container was well formed and only the READS lied, so `var_dump()` kept printing
/// `string(0) ""`. A boxed cell carries runtime value tag 8 (canonical PHP null), which every
/// Mixed reader already understands.
///
/// Ownership: `__rt_array_to_mixed` TRANSFERS each owned field string into its box rather than
/// copying it, and `__rt_array_push_int` stores the fresh box pointer without retaining it, so
/// the row owns exactly one reference to every cell either way.
fn emit_csv_row_to_mixed_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: csv_row_to_mixed ---");
    emitter.label_global("__rt_csv_row_to_mixed");

    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // save fp/lr and reserve one spill slot
    emitter.instruction("add x29, sp, #0");
    emitter.instruction("cbz x0, __rt_csv_row_to_mixed_empty");                 // no record: build php's [null] instead
    emitter.instruction("mov x1, #1");                                          // parsed rows hold 16-byte string ptr/len pairs
    emitter.instruction("bl __rt_array_to_mixed");                              // transfer the owned fields into boxed Mixed cells
    emitter.instruction("b __rt_csv_row_to_mixed_return");

    // -- php_bc_fgetcsv_empty_line(): one element, holding null --
    emitter.label("__rt_csv_row_to_mixed_empty");
    emitter.instruction("mov x0, #1");                                          // capacity 1
    emitter.instruction("mov x1, #8");                                          // boxed Mixed cells are pointer-sized
    emitter.instruction("bl __rt_array_new");
    emitter.instruction("str x0, [sp, #16]");                                   // hold the row across the box allocation
    emitter.instruction("mov x0, #8");                                          // runtime value tag 8 = canonical PHP null
    emitter.instruction("mov x1, #0");                                          // canonical null has no low payload word
    emitter.instruction("mov x2, #0");                                          // canonical null has no high payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // x0 = owned boxed null
    emitter.instruction("mov x1, x0");                                          // the box transfers into the row
    emitter.instruction("ldr x0, [sp, #16]");                                   // the row again
    emitter.instruction("bl __rt_array_push_int");                              // store the cell pointer without retaining it
    emit_stamp_mixed_value_type_aarch64(emitter);

    emitter.label("__rt_csv_row_to_mixed_return");
    emitter.instruction("ldp x29, x30, [sp], #32");
    emitter.instruction("ret");
}

/// Stamps the indexed array in `x0` as holding boxed `Mixed` cells (runtime value_type 7).
///
/// `__rt_array_push_int` specializes a freshly empty array to SCALAR slots, so the tag has to be
/// written after the push, not before.
fn emit_stamp_mixed_value_type_aarch64(emitter: &mut Emitter) {
    emitter.instruction("ldr x9, [x0, #-8]");                                   // the packed array kind word
    emitter.instruction("mov x10, #0x80ff");                                    // preserve heap kind and the persistent COW flag
    emitter.instruction("and x9, x9, x10");                                     // clear the scalar value_type push_int just stamped
    emitter.instruction("mov x10, #7");                                         // runtime value_type 7 = boxed Mixed
    emitter.instruction("lsl x10, x10, #8");                                    // into the packed kind word's tag lane
    emitter.instruction("orr x9, x9, x10");
    emitter.instruction("str x9, [x0, #-8]");
}

/// x86_64 form of [`emit_csv_row_to_mixed_aarch64`]:
/// `__rt_csv_row_to_mixed(rdi = row_or_null) -> array<mixed>: rax`.
///
/// `__rt_mixed_from_value` takes its TAG IN RAX here, not in `rdi`: the runtime helpers do not
/// share one x86_64 convention, and reading this one as System V would box the row pointer as if
/// it were a tag.
fn emit_csv_row_to_mixed_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: csv_row_to_mixed ---");
    emitter.label_global("__rt_csv_row_to_mixed");

    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 16");                                         // one spill slot, keeping rsp 16-byte aligned
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_csv_row_to_mixed_x_empty");                    // no record: build php's [null] instead
    emitter.instruction("mov esi, 1");                                          // parsed rows hold 16-byte string ptr/len pairs
    emitter.instruction("call __rt_array_to_mixed");                            // transfer the owned fields into boxed Mixed cells
    emitter.instruction("jmp __rt_csv_row_to_mixed_x_return");

    // -- php_bc_fgetcsv_empty_line(): one element, holding null --
    emitter.label("__rt_csv_row_to_mixed_x_empty");
    emitter.instruction("mov edi, 1");                                          // capacity 1
    emitter.instruction("mov esi, 8");                                          // boxed Mixed cells are pointer-sized
    emitter.instruction("call __rt_array_new");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // hold the row across the box allocation
    emitter.instruction("mov eax, 8");                                          // runtime value tag 8 = canonical PHP null, IN RAX
    emitter.instruction("xor edi, edi");                                        // canonical null has no low payload word
    emitter.instruction("xor esi, esi");                                        // canonical null has no high payload word
    emitter.instruction("call __rt_mixed_from_value");                          // rax = owned boxed null
    emitter.instruction("mov rsi, rax");                                        // the box transfers into the row
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the row again
    emitter.instruction("call __rt_array_push_int");                            // store the cell pointer without retaining it
    // The x86_64 kind word carries the heap MARKER in its high half, and `__rt_decref_array`
    // refuses to touch an allocation whose marker does not match — it reads the word as a
    // foreign/static pointer and returns without freeing. Masking with `0x80ff` alone wiped that
    // marker, so this `[null]` array was UNFREEABLE on x86_64 while AArch64, which has no such
    // marker, stayed clean: measured, `fgetcsv()` over blank lines leaked two blocks per record
    // there and none here. `str_getcsv("")` reaches this same arm. `__rt_array_to_mixed` preserves
    // the marker exactly this way, which is why the widened rows never showed the defect.
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // the packed array kind word
    emitter.instruction("mov r11, r10");                                        // copy it so the heap marker can be preserved
    emitter.instruction("and r10, 0x80ff");                                     // preserve heap kind and the persistent COW flag
    emitter.instruction("and r11, -65536");                                     // preserve the x86_64 heap marker bits
    emitter.instruction("or r10, r11");                                         // recombine the marker with the low metadata
    emitter.instruction("or r10, 0x700");                                       // runtime value_type 7 = boxed Mixed
    emitter.instruction("mov QWORD PTR [rax - 8], r10");

    emitter.label("__rt_csv_row_to_mixed_x_return");
    emitter.instruction("leave");
    emitter.instruction("ret");
}

/// Emits `__rt_str_getcsv(x0 = csv_opts, x1 = ptr, x2 = len) -> array_ptr: x0`.
///
/// `str_getcsv()` is NOT `fgetcsv()` over one line: a newline between enclosures — and a
/// newline inside an unenclosed field — is ordinary data. Only a newline at the very END
/// is structural, and php-src strips one in two separate places, which is why the shape is
/// unusual. Measured against `php -n` 8.5.6 over 22 inputs:
///
///   1. strip ONE trailing `\r\n`, `\n` or `\r`;
///   2. if nothing is left there is NO RECORD, and the helper returns the null pointer —
///      exactly what php-src's `php_fgetcsv()` returns for a blank line. The caller
///      substitutes the one-element `[null]` array, as `php_bc_fgetcsv_empty_line()` does
///      for both `str_getcsv()` and `fgetcsv()`;
///   3. strip ONE more trailing terminator;
///   4. parse the rest with a newline as data.
///
/// Step 2 is what separates `""` from `"\n\n"` (which yields `[""]`): without it the two
/// collapse to the same answer.
///
/// The result is widened to boxed `Mixed` cells by `__rt_csv_row_to_mixed`, because the
/// `[null]` of step 2 has no representation in an `array<string>`: a string slot can hold
/// the null SENTINEL, but nothing reads that slot back as PHP null, so `var_dump()` still
/// printed `string(0) ""`. Only a boxed cell carries the null tag through to every reader.
///
/// The bytes are COPIED first, because the shared state machine unescapes in place and the
/// argument may be a read-only literal.
fn emit_str_getcsv_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_getcsv ---");
    emitter.label_global("__rt_str_getcsv");

    emitter.instruction("stp x29, x30, [sp, #-112]!");                          // same frame shape as __rt_fgetcsv
    emitter.instruction("stp x19, x20, [sp, #16]");
    emitter.instruction("stp x21, x22, [sp, #32]");
    emitter.instruction("stp x23, x24, [sp, #48]");
    emitter.instruction("stp x25, x26, [sp, #64]");
    emitter.instruction("stp x27, x28, [sp, #80]");
    emitter.instruction("add x29, sp, #0");

    // -- unpack csv_opts, which arrives in x0 here rather than in x1 --
    emitter.instruction("and w26, w0, #0xff");                                  // sep
    emitter.instruction("lsr w9, w0, #8");
    emitter.instruction("and w27, w9, #0xff");                                  // enc
    emitter.instruction("lsr w9, w0, #16");
    emitter.instruction("and w28, w9, #0xff");                                  // esc
    emitter.instruction("cbnz w26, __rt_str_getcsv_sep_done");
    emitter.instruction("mov w26, #0x2c");                                      // sep defaults to ','
    emitter.label("__rt_str_getcsv_sep_done");
    emitter.instruction("cbnz w27, __rt_str_getcsv_enc_done");
    emitter.instruction("mov w27, #0x22");                                      // enc defaults to '"'
    emitter.label("__rt_str_getcsv_enc_done");

    emitter.instruction("mov x9, #1");
    emitter.instruction("str x9, [sp, #104]");                                  // parsing a STRING: a newline is data
    emitter.instruction("str xzr, [sp, #96]");                                  // and there is no stream to continue on

    emit_strip_one_terminator_aarch64(emitter, "a");                            // step 1

    // -- step 2: nothing left is php-src's "no record at all", not an empty field --
    emitter.instruction("cbnz x2, __rt_str_getcsv_more");
    emitter.instruction("mov x20, xzr");                                        // no record: the caller substitutes [null]
    emitter.instruction("b __rt_str_getcsv_return");

    emitter.label("__rt_str_getcsv_more");
    emit_strip_one_terminator_aarch64(emitter, "b");                            // step 3

    // -- copy the bytes: the parser unescapes IN PLACE and the input may be read-only --
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // hold the source across the reservation
    emitter.instruction("mov x0, x2");
    emitter.instruction("bl __rt_concat_reserve");                              // x0 = writable destination
    emitter.instruction("ldp x1, x2, [sp], #16");                               // the source again
    emitter.instruction("mov x9, #0");                                          // copy index
    emitter.label("__rt_str_getcsv_copy");
    emitter.instruction("cmp x9, x2");
    emitter.instruction("b.hs __rt_str_getcsv_copied");
    emitter.instruction("ldrb w10, [x1, x9]");
    emitter.instruction("strb w10, [x0, x9]");
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("b __rt_str_getcsv_copy");
    emitter.label("__rt_str_getcsv_copied");
    emitter.instruction("mov x1, x0");                                          // parse over the copy
    emitter.instruction("b __rt_csv_parse_buffer");                             // join the shared state machine

    emitter.label("__rt_str_getcsv_return");
    emitter.instruction("mov x0, x20");
    emitter.instruction("ldp x19, x20, [sp, #16]");
    emitter.instruction("ldp x21, x22, [sp, #32]");
    emitter.instruction("ldp x23, x24, [sp, #48]");
    emitter.instruction("ldp x25, x26, [sp, #64]");
    emitter.instruction("ldp x27, x28, [sp, #80]");
    emitter.instruction("ldp x29, x30, [sp], #112");
    emitter.instruction("ret");
}

/// Emits one trailing-terminator strip over the `(x1, x2)` slice; `\r\n` counts as one.
fn emit_strip_one_terminator_aarch64(emitter: &mut Emitter, tag: &str) {
    let done = format!("__rt_str_getcsv_strip_{tag}_done");
    let not_nl = format!("__rt_str_getcsv_strip_{tag}_not_nl");
    emitter.instruction(&format!("cbz x2, {done}"));
    emitter.instruction("sub x9, x2, #1");
    emitter.instruction("ldrb w10, [x1, x9]");                                  // the last byte
    emitter.instruction("cmp w10, #0x0a");                                      // a line feed?
    emitter.instruction(&format!("b.ne {not_nl}"));
    emitter.instruction("sub x2, x2, #1");                                      // drop it
    emitter.instruction(&format!("cbz x2, {done}"));
    emitter.instruction("sub x9, x2, #1");
    emitter.instruction("ldrb w10, [x1, x9]");                                  // a CR before it belongs to the SAME terminator
    emitter.instruction("cmp w10, #0x0d");
    emitter.instruction(&format!("b.ne {done}"));
    emitter.instruction("sub x2, x2, #1");
    emitter.instruction(&format!("b {done}"));
    emitter.label(&not_nl);
    emitter.instruction("cmp w10, #0x0d");                                      // a lone CR is a terminator too
    emitter.instruction(&format!("b.ne {done}"));
    emitter.instruction("sub x2, x2, #1");
    emitter.label(&done);
}

/// ARM64 variant of `__rt_fgetcsv`.
///
/// Signature: `__rt_fgetcsv(fd: x0, csv_opts: x1) -> eof_or_blank_or_row: x0`.
/// Returns `0` on EOF (PHP false), the sentinel `1` for a BLANK LINE (php's `[null]` record,
/// which is not EOF), otherwise a heap array of owned string fields.
/// `__rt_fgetcsv_row_to_mixed` is what turns those three answers into php's.
/// Supports RFC 4180 doubling mode (`esc == 0`) and escape-char mode (`esc != 0`).
fn emit_fgetcsv_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fgetcsv ---");
    emitter.label_global("__rt_fgetcsv");

    // -- prologue: save callee-saved registers + fp/lr (112-byte frame) --
    emitter.instruction("stp x29, x30, [sp, #-112]!");                          // save fp, lr; allocate 112-byte frame
    emitter.instruction("stp x19, x20, [sp, #16]");                             // save x19 (temp/len), x20 (array_ptr)
    emitter.instruction("stp x21, x22, [sp, #32]");                             // save x21 (scan_ptr), x22 (end_ptr)
    emitter.instruction("stp x23, x24, [sp, #48]");                             // save x23 (field_start), x24 (write_ptr)
    emitter.instruction("stp x25, x26, [sp, #64]");                             // save w25 (state), w26 (sep)
    emitter.instruction("stp x27, x28, [sp, #80]");                             // save w27 (enc), w28 (esc)
    emitter.instruction("add x29, sp, #0");                                    // establish frame pointer

    // -- unpack csv_opts: sep=w1&0xFF, enc=(w1>>8)&0xFF, esc=(w1>>16)&0xFF --
    emitter.instruction("and w26, w1, #0xff");                                  // sep = csv_opts & 0xFF
    emitter.instruction("lsr w2, w1, #8");                                      // shift right 8 for enc field
    emitter.instruction("and w27, w2, #0xff");                                  // enc = (csv_opts >> 8) & 0xFF
    emitter.instruction("lsr w2, w1, #16");                                     // shift right 16 for esc field
    emitter.instruction("and w28, w2, #0xff");                                  // esc = (csv_opts >> 16) & 0xFF

    // -- apply defaults: sep==0 -> ',', enc==0 -> '"' --
    emitter.instruction("cbnz w26, __rt_fgetcsv_sep_done");                     // if sep != 0, skip default
    emitter.instruction("mov w26, #0x2c");                                       // sep = ',' (0x2C)
    emitter.label("__rt_fgetcsv_sep_done");
    emitter.instruction("cbnz w27, __rt_fgetcsv_enc_done");                     // if enc != 0, skip default
    emitter.instruction("mov w27, #0x22");                                      // enc = 0x22 double-quote
    emitter.label("__rt_fgetcsv_enc_done");

    // -- read one line via __rt_fgets -> x1=ptr, x2=len --
    emitter.instruction("str x0, [sp, #96]");                                   // keep the stream handle for continuation reads
    emitter.instruction("str xzr, [sp, #104]");                                 // reading a STREAM: a bare newline ends the record
    emitter.instruction("mov x1, #0");                                          // no length bound; x1 still held csv_opts otherwise
    emitter.instruction("bl __rt_fgets");                                       // x1 = line ptr, x2 = line len

    // -- EOF check: len == 0 -> return 0 (false) --
    // The EOF exit lives past `__rt_csv_parse_buffer`, so it belongs to that helper's atom under
    // macOS dead stripping and a conditional branch cannot reach it: `.alt_entry` targets accept
    // `b`/`bl` only. Branch over an unconditional one instead.
    emitter.instruction("cbnz x2, __rt_fgetcsv_have_line");                     // a real line: parse it
    emitter.instruction("b __rt_fgetcsv_eof");                                  // len == 0 -> EOF, return 0
    emitter.label("__rt_fgetcsv_have_line");

    // -- blank-record check: a line that is nothing but its terminator has NO FIELDS --
    //
    // php-src decides this before any field is read: `php_fgetcsv()` sets `line_end` past one
    // stripped terminator (`php_fgetcsv_lookup_trailing_spaces`, file.c:1618, which drops `\r\n`,
    // `\n` or `\r` and NOTHING else despite its name) and bails with `values = NULL` on
    // `first_field && bptr == line_end` (file.c:1939). So the rule is exactly the one
    // `__rt_str_getcsv` already applies to its subject: strip ONE terminator, and an empty
    // remainder is no record. Measured on `php -n` 8.5.6, `"   \n"` and `"\t\n"` are NOT blank —
    // they are one field of whitespace — so only the terminator may be stripped.
    emitter.instruction("mov x11, x2");                                         // keep the true length; the strip mutates x2
    emit_strip_one_terminator_aarch64(emitter, "f");
    emitter.instruction("cmp x2, #0");                                          // nothing but a terminator?
    emitter.instruction("mov x2, x11");                                         // restore the full line; `mov` leaves the flags alone
    emitter.instruction("b.ne __rt_fgetcsv_not_blank");                         // a real record: parse it
    emitter.instruction("b __rt_fgetcsv_blank");                                // unconditional: the exit is in a later atom
    emitter.label("__rt_fgetcsv_not_blank");

    // -- set up scan pointers into the line buffer --
    //
    // A GLOBAL label, and reached by an explicit branch rather than fall-through: macOS
    // dead-stripping makes every `.globl` an atom of its own and localizes internal
    // labels, so a branch from `__rt_str_getcsv` into an internal label here would
    // silently resolve into a neighbouring function, and falling in from the atom above
    // is not safe either.
    emitter.instruction("b __rt_csv_parse_buffer");                             // enter the shared parser explicitly
    emitter.label_global("__rt_csv_parse_buffer");                              // `str_getcsv` joins here with its own buffer
    emitter.instruction("mov x21, x1");                                         // scan_ptr = line_ptr
    emitter.instruction("add x22, x1, x2");                                     // end_ptr = ptr + len

    // -- create result array: cap=8, elem_size=16 (ptr+len pair) --
    emitter.instruction("mov x0, #8");                                          // capacity = 8 fields
    emitter.instruction("mov x1, #16");                                         // elem_size = 16 (ptr + len pair)
    emitter.instruction("bl __rt_array_new");                                   // x0 = new array ptr
    emitter.instruction("mov x20, x0");                                         // array_ptr = result

    // -- init field tracking: field_start = write_ptr = scan_ptr, state = 0 --
    emitter.instruction("mov x23, x21");                                        // field_start = scan_ptr
    emitter.instruction("mov x24, x21");                                        // write_ptr = scan_ptr
    emitter.instruction("mov w25, #0");                                         // state = 0 (OutsideField)

    // -- main parse loop --
    emitter.label("__rt_fgetcsv_loop");
    emitter.instruction("cmp x21, x22");                                       // scan_ptr >= end_ptr?
    emitter.instruction("b.ge __rt_fgetcsv_end_line");                          // yes -> push last field, return
    emitter.instruction("ldrb w0, [x21], #1");                                  // c = *scan_ptr++; (zero-extended)

    // -- dispatch on state (w25: 0..4) --
    emitter.instruction("cmp w25, #0");                                         // state == OutsideField?
    emitter.instruction("b.eq __rt_fgetcsv_st0");                               // -> state 0 handler
    emitter.instruction("cmp w25, #1");                                         // state == InField?
    emitter.instruction("b.eq __rt_fgetcsv_st1");                               // -> state 1 handler
    emitter.instruction("cmp w25, #2");                                         // state == InQuotedField?
    emitter.instruction("b.eq __rt_fgetcsv_st2");                               // -> state 2 handler
    emitter.instruction("cmp w25, #3");                                         // state == AfterEscape?
    emitter.instruction("b.eq __rt_fgetcsv_st3");                               // -> state 3 handler
    emitter.instruction("cmp w25, #4");                                         // state == AfterCloseQuote?
    emitter.instruction("b.eq __rt_fgetcsv_st4");                               // -> state 4 handler
    emitter.instruction("b __rt_fgetcsv_end_line");                             // unknown state -> safety exit

    // -- state 0: OutsideField --
    emitter.label("__rt_fgetcsv_st0");
    emitter.instruction("cmp w0, w26");                                         // c == sep?
    emitter.instruction("b.eq __rt_fgetcsv_push_reset");                         // -> push empty field, reset
    emitter.instruction("cmp w0, w27");                                         // c == enc (opening quote)?
    emitter.instruction("b.eq __rt_fgetcsv_s0_enc");                            // -> enter quoted field
    // Whitespace in front of an opening enclosure is NOT data: php looks ahead from the start of
    // the field and, if the first byte that is neither the separator nor whitespace is the
    // enclosure, it starts the field there instead (`php_fgetcsv`, file.c). So ` "a",b` reads as
    // `a`, while ` a,b` — no enclosure ahead — keeps the space and reads as ` a`.
    //
    // The lookahead is bounded by the BUFFER, which is what makes the two callers differ for free:
    // `fgetcsv` holds one line, so it cannot reach a quote on the next one, while `str_getcsv`
    // holds the whole subject and can. Both were measured on `php -n` 8.5.6.
    emitter.instruction("cmp w0, #0x20");                                       // c == space?
    emitter.instruction("b.eq __rt_fgetcsv_s0_ws");                             // -> look for an enclosure ahead
    emitter.instruction("sub w1, w0, #0x09");                                   // 0x09..0x0d -> 0..4
    emitter.instruction("cmp w1, #4");                                          // tab, newline, vtab, formfeed, return?
    emitter.instruction("b.hi __rt_fgetcsv_s0_ws_no");                          // not whitespace: ordinary data
    emitter.label("__rt_fgetcsv_s0_ws");
    emitter.instruction("mov x10, x21");                                        // tmp = the first unconsumed byte
    emitter.label("__rt_fgetcsv_s0_ws_scan");
    emitter.instruction("cmp x10, x22");                                        // tmp >= end_ptr?
    emitter.instruction("b.ge __rt_fgetcsv_s0_ws_no");                          // ran out of buffer: no enclosure
    emitter.instruction("ldrb w1, [x10]");                                      // the byte under tmp
    emitter.instruction("cmp w1, w26");                                         // the separator ends the walk
    emitter.instruction("b.eq __rt_fgetcsv_s0_ws_no");
    emitter.instruction("cmp w1, w27");                                         // an enclosure: the field starts here
    emitter.instruction("b.eq __rt_fgetcsv_s0_ws_yes");
    emitter.instruction("cmp w1, #0x20");                                       // still whitespace?
    emitter.instruction("b.eq __rt_fgetcsv_s0_ws_next");
    emitter.instruction("sub w2, w1, #0x09");                                   // 0x09..0x0d -> 0..4
    emitter.instruction("cmp w2, #4");
    emitter.instruction("b.hi __rt_fgetcsv_s0_ws_no");                          // ordinary byte: no enclosure
    emitter.label("__rt_fgetcsv_s0_ws_next");
    emitter.instruction("add x10, x10, #1");                                    // skip this whitespace byte
    emitter.instruction("b __rt_fgetcsv_s0_ws_scan");
    emitter.label("__rt_fgetcsv_s0_ws_yes");
    emitter.instruction("add x21, x10, #1");                                    // scan_ptr = past the opening quote
    emitter.instruction("b __rt_fgetcsv_s0_enc");                               // -> enter quoted field
    emitter.label("__rt_fgetcsv_s0_ws_no");
    emitter.instruction("ldr x9, [sp, #104]");                                 // is a newline ordinary data here?
    emitter.instruction("cbnz x9, __rt_fgetcsv_st0_data");                     // `str_getcsv` keeps it as data
    emitter.instruction("cmp w0, #0x0a");                                      // c == newline (0x0A)?
    emitter.instruction("b.eq __rt_fgetcsv_push_end");                          // -> push empty field, end
    emitter.instruction("cmp w0, #0x0d");                                      // c == carriage return (0x0D)?
    emitter.instruction("b.eq __rt_fgetcsv_push_end");                          // -> push empty field, end
    emitter.label("__rt_fgetcsv_st0_data");
    emitter.instruction("mov x23, x24");                                       // field_start = write_ptr
    emitter.instruction("strb w0, [x24], #1");                                 // *write_ptr++ = c (accumulate)
    emitter.instruction("mov w25, #1");                                         // state = InField
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue loop

    emitter.label("__rt_fgetcsv_s0_enc");
    emitter.instruction("mov x23, x24");                                       // field_start = write_ptr (skip opening quote)
    emitter.instruction("mov w25, #2");                                         // state = InQuotedField
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue loop

    // -- state 1: InField (unquoted, accumulating) --
    emitter.label("__rt_fgetcsv_st1");
    emitter.instruction("cmp w0, w26");                                         // c == sep?
    emitter.instruction("b.eq __rt_fgetcsv_push_reset");                        // -> push field, reset
    emitter.instruction("ldr x9, [sp, #104]");                                  // is a newline ordinary data here?
    emitter.instruction("cbnz x9, __rt_fgetcsv_st1_data");                      // `str_getcsv` keeps it as data
    emitter.instruction("cmp w0, #0x0a");                                       // c == newline (0x0A)?
    emitter.instruction("b.eq __rt_fgetcsv_push_end");                         // -> push field, end
    emitter.instruction("cmp w0, #0x0d");                                       // c == carriage return (0x0D)?
    emitter.instruction("b.eq __rt_fgetcsv_push_end");                         // -> push field, end
    emitter.label("__rt_fgetcsv_st1_data");
    emitter.instruction("strb w0, [x24], #1");                                 // *write_ptr++ = c (accumulate)
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue loop

    // -- state 2: InQuotedField --
    emitter.label("__rt_fgetcsv_st2");
    emitter.instruction("cbz w28, __rt_fgetcsv_s2_chkenc");                     // esc == 0 -> doubling mode, skip esc check
    emitter.instruction("cmp w0, w28");                                         // c == esc?
    emitter.instruction("b.eq __rt_fgetcsv_s2_esc");                           // -> AfterEscape
    emitter.label("__rt_fgetcsv_s2_chkenc");
    emitter.instruction("cmp w0, w27");                                         // c == enc (close quote)?
    emitter.instruction("b.eq __rt_fgetcsv_s2_close");                          // -> AfterCloseQuote
    emitter.instruction("strb w0, [x24], #1");                                 // *write_ptr++ = c (accumulate)
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue loop

    emitter.label("__rt_fgetcsv_s2_esc");
    emitter.instruction("mov w25, #3");                                         // state = AfterEscape
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue loop

    emitter.label("__rt_fgetcsv_s2_close");
    emitter.instruction("mov w25, #4");                                         // state = AfterCloseQuote
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue loop

    // -- state 3: AfterEscape (esc mode only) --
    //
    // The escape byte is KEPT, whatever it precedes. php never unescapes on read: all the escape
    // character does is stop the next byte from closing the field, and both bytes land in the
    // value. `"a\"b"` reads back as `a\"b`, four bytes, on `php -n` 8.5.6 — the same string
    // `fputcsv()` wrote. Dropping it before the enclosure, as this did, silently shortened every
    // round trip through a quoted field that contained one.
    emitter.label("__rt_fgetcsv_st3");
    emitter.instruction("strb w28, [x24], #1");                                 // *write_ptr++ = esc (php keeps it)
    emitter.instruction("strb w0, [x24], #1");                                  // *write_ptr++ = c (literal)
    emitter.instruction("mov w25, #2");                                         // state = InQuotedField
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue loop

    // -- state 4: AfterCloseQuote --
    emitter.label("__rt_fgetcsv_st4");
    emitter.instruction("cmp w0, w27");                                         // c == enc (doubled quote)?
    emitter.instruction("b.eq __rt_fgetcsv_s4_dbl");                           // -> accumulate enc, back to quoted
    emitter.instruction("cmp w0, w26");                                         // c == sep?
    emitter.instruction("b.eq __rt_fgetcsv_push_reset");                        // -> push field, reset
    emitter.instruction("ldr x9, [sp, #104]");                                 // is a newline ordinary data here?
    emitter.instruction("cbnz x9, __rt_fgetcsv_st4_data");                     // `str_getcsv` keeps it as data
    emitter.instruction("cmp w0, #0x0a");                                      // c == newline (0x0A)?
    emitter.instruction("b.eq __rt_fgetcsv_push_end");                         // -> push field, end
    emitter.instruction("cmp w0, #0x0d");                                      // c == carriage return (0x0D)?
    emitter.instruction("b.eq __rt_fgetcsv_push_end");                         // -> push field, end
    emitter.label("__rt_fgetcsv_st4_data");
    // The closing enclosure is GONE, even when data follows it. php reads `"ab"cd` as `abcd`, not
    // as `ab"cd`: the quote that closed the field is consumed and everything after it is ordinary
    // data, quotes included — `"ab"c"d"` reads back as `abc"d"`. Restoring it here, as this did,
    // added a byte php never keeps.
    emitter.instruction("strb w0, [x24], #1");                                 // *write_ptr++ = c (accumulate)
    emitter.instruction("mov w25, #1");                                         // state = InField
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue loop

    emitter.label("__rt_fgetcsv_s4_dbl");
    emitter.instruction("strb w27, [x24], #1");                                 // *write_ptr++ = enc (doubled -> single quote)
    emitter.instruction("mov w25, #2");                                         // state = InQuotedField
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue loop

    // -- push field and reset for next field (separator encountered) --
    emitter.label("__rt_fgetcsv_push_reset");
    emitter.instruction("sub x19, x24, x23");                                  // x19 = len = write_ptr - field_start
    emitter.instruction("mov x1, x23");                                         // ptr = field_start (raw slice into line buf)
    emitter.instruction("mov x2, x19");                                         // len
    emitter.instruction("bl __rt_str_persist");                                 // x0 = persisted string (heap copy)
    emitter.instruction("mov x1, x0");                                         // x1 = persisted string ptr
    emitter.instruction("mov x0, x20");                                         // x0 = array_ptr
    emitter.instruction("mov x2, x19");                                         // x2 = len (restored from callee-saved x19)
    emitter.instruction("bl __rt_array_push_str");                              // x0 = array_ptr (possibly reallocated)
    emitter.instruction("mov x20, x0");                                         // update array_ptr
    emitter.instruction("mov x23, x24");                                        // field_start = write_ptr (next field)
    emitter.instruction("mov w25, #0");                                         // state = OutsideField
    emitter.instruction("b __rt_fgetcsv_loop");                                 // continue loop

    // -- push field and end (newline or end-of-buffer) --
    emitter.label("__rt_fgetcsv_push_end");
    emitter.instruction("sub x19, x24, x23");                                  // x19 = len = write_ptr - field_start
    emitter.instruction("mov x1, x23");                                         // ptr = field_start (raw slice into line buf)
    emitter.instruction("mov x2, x19");                                         // len
    emitter.instruction("bl __rt_str_persist");                                 // x0 = persisted string (heap copy)
    emitter.instruction("mov x1, x0");                                         // x1 = persisted string ptr
    emitter.instruction("mov x0, x20");                                         // x0 = array_ptr
    emitter.instruction("mov x2, x19");                                         // x2 = len (restored from callee-saved x19)
    emitter.instruction("bl __rt_array_push_str");                              // x0 = array_ptr (possibly reallocated)
    emitter.instruction("mov x20, x0");                                        // update array_ptr
    emitter.instruction("b __rt_fgetcsv_done");                                 // -> epilogue

    // -- end of line (scan_ptr reached end_ptr without trailing newline) --
    // -- end of buffer: a field still inside its enclosure CONTINUES on the next line --
    //
    // A newline between enclosures is data, so php-src keeps reading until the enclosure
    // closes. Reading one line and stopping split such a record across two rows and lost
    // the field boundary. `__rt_fgets` reserves from the shared concat scratch, so the
    // next line normally lands exactly where this one ended and the parse can simply be
    // extended; when it does not, the record ends here as before.
    emitter.label("__rt_fgetcsv_end_line");
    emitter.instruction("cmp w25, #2");                                        // still inside a quoted field?
    emitter.instruction("b.eq __rt_fgetcsv_continue_line");
    emitter.instruction("cmp w25, #3");                                        // or holding an escape inside one?
    emitter.instruction("b.ne __rt_fgetcsv_push_end");                         // no: the record really ends here
    emitter.label("__rt_fgetcsv_continue_line");
    emitter.instruction("ldr x9, [sp, #104]");                                 // parsing a STRING has no stream to read on
    emitter.instruction("cbnz x9, __rt_fgetcsv_push_end");                     // `str_getcsv` ends the record here
    emitter.instruction("ldr x0, [sp, #96]");                                  // the stream handle saved at entry
    emitter.instruction("mov x1, #0");                                         // no length bound
    emitter.instruction("bl __rt_fgets");                                      // x1 = next line ptr, x2 = its length
    emitter.instruction("cbz x2, __rt_fgetcsv_push_end");                      // EOF closes an unterminated field
    emitter.instruction("cmp x1, x22");                                        // did it land right after this buffer?
    emitter.instruction("b.ne __rt_fgetcsv_push_end");                         // not contiguous: keep the old behaviour
    emitter.instruction("add x22, x22, x2");                                   // extend the parse over the new bytes
    emitter.instruction("b __rt_fgetcsv_loop");                                // and keep going

    emitter.label("__rt_fgetcsv_push_end_from_line");
    emitter.instruction("b __rt_fgetcsv_push_end");                            // push last field, then return

    // -- done: return array_ptr in x0 --
    emitter.label("__rt_fgetcsv_done");
    emitter.instruction("mov x0, x20");                                         // x0 = array_ptr (return value)
    emitter.instruction("b __rt_fgetcsv_epilogue");                             // -> common epilogue

    // -- blank line: php_fgetcsv() reports NO RECORD, which is NOT end of input --
    // Shared for the same reason as the EOF exit below: it is reached from `__rt_fgetcsv`'s own
    // atom, across the `__rt_csv_parse_buffer` boundary.
    emitter.label_shared("__rt_fgetcsv_blank");
    emitter.instruction("mov x0, #1");                                         // sentinel 1: a blank record, not EOF's 0
    emitter.instruction("b __rt_fgetcsv_epilogue");                            // -> common epilogue

    // -- EOF: return 0 (false) --
    // Shared, not local: `__rt_fgetcsv` reaches it from its own atom, and only a real symbol keeps
    // this one alive under `-dead_strip`.
    emitter.label_shared("__rt_fgetcsv_eof");
    emitter.instruction("mov x0, #0");                                         // x0 = 0 (false / EOF)

    // -- epilogue: restore registers and return --
    emitter.label("__rt_fgetcsv_epilogue");
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore x19, x20
    emitter.instruction("ldp x21, x22, [sp, #32]");                             // restore x21, x22
    emitter.instruction("ldp x23, x24, [sp, #48]");                             // restore x23, x24
    emitter.instruction("ldp x25, x26, [sp, #64]");                             // restore x25, x26
    emitter.instruction("ldp x27, x28, [sp, #80]");                             // restore x27, x28
    emitter.instruction("ldp x29, x30, [sp], #112");                            // restore fp, lr; deallocate frame
    emitter.instruction("ret");                                                // return to caller
}

/// x86_64 form of [`emit_str_getcsv_aarch64`]: `__rt_str_getcsv(rdi = csv_opts,
/// rsi = ptr, rdx = len) -> array_ptr: rax`.
fn emit_str_getcsv_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_getcsv ---");
    emitter.label_global("__rt_str_getcsv");

    emitter.instruction("push rbp");                                            // same frame shape as __rt_fgetcsv
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 120");
    emitter.instruction("push rbx");
    emitter.instruction("push r12");
    emitter.instruction("push r13");
    emitter.instruction("push r14");
    emitter.instruction("push r15");

    // -- unpack csv_opts, which arrives in rdi here rather than in rsi --
    emitter.instruction("movzx eax, dil");                                      // sep
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");
    emitter.instruction("mov rax, rdi");
    emitter.instruction("shr rax, 8");
    emitter.instruction("movzx eax, al");                                       // enc
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");
    emitter.instruction("mov rax, rdi");
    emitter.instruction("shr rax, 16");
    emitter.instruction("movzx eax, al");                                       // esc
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");
    emitter.instruction("cmp QWORD PTR [rbp - 8], 0");
    emitter.instruction("jne __rt_str_getcsv_x_sep_done");
    emitter.instruction("mov QWORD PTR [rbp - 8], 0x2c");                       // sep defaults to ','
    emitter.label("__rt_str_getcsv_x_sep_done");
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");
    emitter.instruction("jne __rt_str_getcsv_x_enc_done");
    emitter.instruction("mov QWORD PTR [rbp - 16], 0x22");                      // enc defaults to '"'
    emitter.label("__rt_str_getcsv_x_enc_done");

    emitter.instruction("mov QWORD PTR [rbp - 96], 1");                         // parsing a STRING: a newline is data
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // and there is no stream to continue on

    emit_strip_one_terminator_x86_64(emitter, "a");                             // step 1

    // -- step 2: nothing left is php-src's "no record at all", not an empty field --
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jnz __rt_str_getcsv_x_more");
    emitter.instruction("xor ebx, ebx");                                        // no record: the caller substitutes [null]
    emitter.instruction("jmp __rt_str_getcsv_x_return");

    emitter.label("__rt_str_getcsv_x_more");
    emit_strip_one_terminator_x86_64(emitter, "b");                             // step 3

    // -- copy the bytes: the parser unescapes IN PLACE and the input may be read-only --
    emitter.instruction("mov QWORD PTR [rbp - 104], rsi");                      // hold the source across the reservation
    emitter.instruction("mov QWORD PTR [rbp - 112], rdx");
    emitter.instruction("mov rax, rdx");
    emitter.instruction("call __rt_concat_reserve");                            // rax = writable destination
    emitter.instruction("mov rsi, QWORD PTR [rbp - 104]");                      // the source again
    emitter.instruction("mov rdx, QWORD PTR [rbp - 112]");
    emitter.instruction("xor r8d, r8d");                                        // copy index
    emitter.label("__rt_str_getcsv_x_copy");
    emitter.instruction("cmp r8, rdx");
    emitter.instruction("jae __rt_str_getcsv_x_copied");
    emitter.instruction("movzx r9d, BYTE PTR [rsi + r8]");
    emitter.instruction("mov BYTE PTR [rax + r8], r9b");
    emitter.instruction("inc r8");
    emitter.instruction("jmp __rt_str_getcsv_x_copy");
    emitter.label("__rt_str_getcsv_x_copied");
    emitter.instruction("jmp __rt_csv_parse_buffer");                           // rax/rdx already carry the copy

    emitter.label("__rt_str_getcsv_x_return");
    emitter.instruction("mov rax, rbx");
    emitter.instruction("pop r15");
    emitter.instruction("pop r14");
    emitter.instruction("pop r13");
    emitter.instruction("pop r12");
    emitter.instruction("pop rbx");
    emitter.instruction("leave");
    emitter.instruction("ret");
}

/// Emits one trailing-terminator strip over the `(rsi, rdx)` slice; `\r\n` counts as one.
fn emit_strip_one_terminator_x86_64(emitter: &mut Emitter, tag: &str) {
    let done = format!("__rt_str_getcsv_x_strip_{tag}_done");
    let not_nl = format!("__rt_str_getcsv_x_strip_{tag}_not_nl");
    emitter.instruction("test rdx, rdx");
    emitter.instruction(&format!("jz {done}"));
    emitter.instruction("movzx eax, BYTE PTR [rsi + rdx - 1]");                 // the last byte
    emitter.instruction("cmp eax, 0x0a");                                       // a line feed?
    emitter.instruction(&format!("jne {not_nl}"));
    emitter.instruction("dec rdx");                                             // drop it
    emitter.instruction("test rdx, rdx");
    emitter.instruction(&format!("jz {done}"));
    emitter.instruction("movzx eax, BYTE PTR [rsi + rdx - 1]");                 // a CR before it is the SAME terminator
    emitter.instruction("cmp eax, 0x0d");
    emitter.instruction(&format!("jne {done}"));
    emitter.instruction("dec rdx");
    emitter.instruction(&format!("jmp {done}"));
    emitter.label(&not_nl);
    emitter.instruction("cmp eax, 0x0d");                                       // a lone CR is a terminator too
    emitter.instruction(&format!("jne {done}"));
    emitter.instruction("dec rdx");
    emitter.label(&done);
}

/// x86_64 Linux variant of `__rt_fgetcsv` using the System V ABI.
///
/// Signature: `__rt_fgetcsv(fd: rdi, csv_opts: rsi) -> eof_or_blank_or_row: rax`, with the same
/// `0` / `1` / row answers as the ARM64 form.
/// Mirrors the ARM64 state machine; spills parser state to a rbp-relative frame.
fn emit_fgetcsv_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fgetcsv ---");
    emitter.label_global("__rt_fgetcsv");

    // -- prologue: 96-byte frame with rbp --
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish frame base
    // 104, not 96: the five callee-saved pushes below move rsp by an odd multiple of 8, so
    // a 96-byte frame leaves it at 8 mod 16 and every SysV call made from here — anything
    // touching SSE — faults. Eight more bytes restore the 16-byte alignment the ABI wants.
    emitter.instruction("sub rsp, 120");                                        // reserve parser state, keeping rsp 16-byte aligned after the pushes
    emitter.instruction("push rbx");                                            // save rbx (callee-saved, used for array_ptr)
    emitter.instruction("push r12");                                            // save r12 (scan_ptr)
    emitter.instruction("push r13");                                            // save r13 (end_ptr)
    emitter.instruction("push r14");                                            // save r14 (field_start)
    emitter.instruction("push r15");                                            // save r15 (write_ptr)

    // -- unpack csv_opts from rsi: sep=rsi&0xFF, enc=(rsi>>8)&0xFF, esc=(rsi>>16)&0xFF --
    emitter.instruction("movzx edx, sil");                                     // sep = csv_opts & 0xFF
    emitter.instruction("mov [rbp - 8], rdx");                                  // save sep at [rbp-8]
    emitter.instruction("shr rsi, 8");                                          // shift right 8 for enc
    emitter.instruction("movzx edx, sil");                                      // enc = (csv_opts >> 8) & 0xFF
    emitter.instruction("mov [rbp - 16], rdx");                                 // save enc at [rbp-16]
    emitter.instruction("shr rsi, 8");                                          // shift right 8 more for esc
    emitter.instruction("movzx edx, sil");                                      // esc = (csv_opts >> 16) & 0xFF
    emitter.instruction("mov [rbp - 24], rdx");                                 // save esc at [rbp-24]

    // -- apply defaults: sep==0 -> ',', enc==0 -> '"' --
    emitter.instruction("cmp QWORD PTR [rbp - 8], 0");                          // sep == 0?
    emitter.instruction("jne __rt_fgetcsv_x_sep_done");                         // if not, skip default
    emitter.instruction("mov QWORD PTR [rbp - 8], 0x2c");                       // sep = ',' (0x2C)
    emitter.label("__rt_fgetcsv_x_sep_done");
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // enc == 0?
    emitter.instruction("jne __rt_fgetcsv_x_enc_done");                          // if not, skip default
    emitter.instruction("mov QWORD PTR [rbp - 16], 0x22");                      // enc = '"' (0x22)
    emitter.label("__rt_fgetcsv_x_enc_done");

    // -- read one line via __rt_fgets -> rax=ptr, rdx=len --
    emitter.instruction("mov QWORD PTR [rbp - 88], rdi");                       // keep the stream handle for continuation reads
    emitter.instruction("mov QWORD PTR [rbp - 96], 0");                         // reading a STREAM: a bare newline ends the record
    emitter.instruction("xor esi, esi");                                        // no length bound; rsi still held csv_opts otherwise
    emitter.instruction("call __rt_fgets");                                    // rax = line ptr, rdx = line len

    // -- EOF check: len == 0 -> return 0 (false) --
    emitter.instruction("test rdx, rdx");                                       // len == 0?
    emitter.instruction("jz __rt_fgetcsv_x_eof");                               // -> EOF, return 0

    // -- blank-record check: a line that is nothing but its terminator has NO FIELDS --
    // Mirrors the AArch64 path; see there for the php-src rule. The strip helper reads its
    // slice from `(rsi, rdx)` and CLOBBERS rax, which is holding the line pointer here.
    emitter.instruction("mov r9, rax");                                         // save the line pointer across the strip
    emitter.instruction("mov r8, rdx");                                         // and the true length
    emitter.instruction("mov rsi, rax");                                        // the strip reads its slice from rsi/rdx
    emit_strip_one_terminator_x86_64(emitter, "f");
    emitter.instruction("test rdx, rdx");                                       // nothing but a terminator?
    emitter.instruction("mov rax, r9");                                         // restore the line pointer; `mov` leaves the flags alone
    emitter.instruction("mov rdx, r8");                                         // and the full length, for the parser
    emitter.instruction("jz __rt_fgetcsv_x_blank");                             // -> blank record, return the sentinel

    // -- set up scan pointers --
    //
    // A GLOBAL label entered by an explicit jump: `__rt_str_getcsv` shares this parser,
    // and jumping into another function's internal label is not safe once macOS
    // dead-stripping localizes them.
    emitter.instruction("jmp __rt_csv_parse_buffer");                           // enter the shared parser explicitly
    emitter.label_global("__rt_csv_parse_buffer");                              // `str_getcsv` joins here with its own buffer
    emitter.instruction("mov r12, rax");                                        // scan_ptr = line_ptr
    emitter.instruction("mov r13, rax");                                        // save line_ptr for end_ptr calc
    emitter.instruction("add r13, rdx");                                        // end_ptr = ptr + len

    // -- create result array: cap=8, elem_size=16 --
    emitter.instruction("mov edi, 8");                                          // capacity = 8 fields
    emitter.instruction("mov esi, 16");                                         // elem_size = 16 (ptr + len pair)
    emitter.instruction("call __rt_array_new");                                  // rax = new array ptr
    emitter.instruction("mov rbx, rax");                                        // array_ptr = result

    // -- init field tracking: field_start = write_ptr = scan_ptr, state = 0 --
    emitter.instruction("mov r14, r12");                                        // field_start = scan_ptr
    emitter.instruction("mov r15, r12");                                        // write_ptr = scan_ptr
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // state = 0 (OutsideField)

    // -- main parse loop --
    emitter.label("__rt_fgetcsv_x_loop");
    emitter.instruction("cmp r12, r13");                                        // scan_ptr >= end_ptr?
    emitter.instruction("jae __rt_fgetcsv_x_end_line");                          // yes -> push last field, return
    emitter.instruction("movzx eax, BYTE PTR [r12]");                            // c = *scan_ptr (zero-extended)
    emitter.instruction("add r12, 1");                                          // scan_ptr++

    // -- dispatch on state ([rbp-32]: 0..4) --
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // load state
    emitter.instruction("cmp rcx, 0");                                          // state == OutsideField?
    emitter.instruction("je __rt_fgetcsv_x_st0");                               // -> state 0 handler
    emitter.instruction("cmp rcx, 1");                                          // state == InField?
    emitter.instruction("je __rt_fgetcsv_x_st1");                               // -> state 1 handler
    emitter.instruction("cmp rcx, 2");                                          // state == InQuotedField?
    emitter.instruction("je __rt_fgetcsv_x_st2");                               // -> state 2 handler
    emitter.instruction("cmp rcx, 3");                                          // state == AfterEscape?
    emitter.instruction("je __rt_fgetcsv_x_st3");                               // -> state 3 handler
    emitter.instruction("cmp rcx, 4");                                          // state == AfterCloseQuote?
    emitter.instruction("je __rt_fgetcsv_x_st4");                               // -> state 4 handler
    emitter.instruction("jmp __rt_fgetcsv_x_end_line");                         // unknown state -> safety exit

    // -- state 0: OutsideField --
    emitter.label("__rt_fgetcsv_x_st0");
    emitter.instruction("movzx ecx, BYTE PTR [rbp - 8]");                        // load sep
    emitter.instruction("cmp al, cl");                                          // c == sep?
    emitter.instruction("je __rt_fgetcsv_x_push_reset");                        // -> push empty field, reset
    emitter.instruction("movzx ecx, BYTE PTR [rbp - 16]");                      // load enc
    emitter.instruction("cmp al, cl");                                          // c == enc (opening quote)?
    emitter.instruction("je __rt_fgetcsv_x_s0_enc");                            // -> enter quoted field
    // See the AArch64 counterpart: whitespace in front of an opening enclosure is skipped, and the
    // walk is bounded by the buffer, which is what separates `fgetcsv` from `str_getcsv`.
    emitter.instruction("cmp al, 0x20");                                        // c == space?
    emitter.instruction("je __rt_fgetcsv_x_s0_ws");                             // -> look for an enclosure ahead
    emitter.instruction("mov edx, eax");
    emitter.instruction("sub edx, 0x09");                                       // 0x09..0x0d -> 0..4
    emitter.instruction("cmp edx, 4");                                          // tab, newline, vtab, formfeed, return?
    emitter.instruction("ja __rt_fgetcsv_x_s0_ws_no");                          // not whitespace: ordinary data
    emitter.label("__rt_fgetcsv_x_s0_ws");
    emitter.instruction("mov r10, r12");                                        // tmp = the first unconsumed byte
    emitter.label("__rt_fgetcsv_x_s0_ws_scan");
    emitter.instruction("cmp r10, r13");                                        // tmp >= end_ptr?
    emitter.instruction("jae __rt_fgetcsv_x_s0_ws_no");                         // ran out of buffer: no enclosure
    emitter.instruction("movzx r11d, BYTE PTR [r10]");                          // the byte under tmp
    emitter.instruction("movzx ecx, BYTE PTR [rbp - 8]");                       // load sep
    emitter.instruction("cmp r11b, cl");                                        // the separator ends the walk
    emitter.instruction("je __rt_fgetcsv_x_s0_ws_no");
    emitter.instruction("movzx ecx, BYTE PTR [rbp - 16]");                      // load enc
    emitter.instruction("cmp r11b, cl");                                        // an enclosure: the field starts here
    emitter.instruction("je __rt_fgetcsv_x_s0_ws_yes");
    emitter.instruction("cmp r11d, 0x20");                                      // still whitespace?
    emitter.instruction("je __rt_fgetcsv_x_s0_ws_next");
    emitter.instruction("mov edx, r11d");
    emitter.instruction("sub edx, 0x09");                                       // 0x09..0x0d -> 0..4
    emitter.instruction("cmp edx, 4");
    emitter.instruction("ja __rt_fgetcsv_x_s0_ws_no");                          // ordinary byte: no enclosure
    emitter.label("__rt_fgetcsv_x_s0_ws_next");
    emitter.instruction("add r10, 1");                                          // skip this whitespace byte
    emitter.instruction("jmp __rt_fgetcsv_x_s0_ws_scan");
    emitter.label("__rt_fgetcsv_x_s0_ws_yes");
    emitter.instruction("lea r12, [r10 + 1]");                                  // scan_ptr = past the opening quote
    emitter.instruction("jmp __rt_fgetcsv_x_s0_enc");                           // -> enter quoted field
    emitter.label("__rt_fgetcsv_x_s0_ws_no");
    emitter.instruction("cmp QWORD PTR [rbp - 96], 0");                         // is a newline ordinary data here?
    emitter.instruction("jne __rt_fgetcsv_x_st0_data");                            // `str_getcsv` keeps it as data
    emitter.instruction("cmp al, 0x0a");                                        // c == newline (0x0A)?
    emitter.instruction("je __rt_fgetcsv_x_push_end");                         // -> push empty field, end
    emitter.instruction("cmp al, 0x0d");                                        // c == carriage return (0x0D)?
    emitter.instruction("je __rt_fgetcsv_x_push_end");                         // -> push empty field, end
    emitter.label("__rt_fgetcsv_x_st0_data");
    emitter.instruction("mov r14, r15");                                        // field_start = write_ptr
    emitter.instruction("mov BYTE PTR [r15], al");                              // *write_ptr = c
    emitter.instruction("add r15, 1");                                          // write_ptr++
    emitter.instruction("mov QWORD PTR [rbp - 32], 1");                        // state = InField
    emitter.instruction("jmp __rt_fgetcsv_x_loop");                             // continue loop

    emitter.label("__rt_fgetcsv_x_s0_enc");
    emitter.instruction("mov r14, r15");                                        // field_start = write_ptr (skip opening quote)
    emitter.instruction("mov QWORD PTR [rbp - 32], 2");                        // state = InQuotedField
    emitter.instruction("jmp __rt_fgetcsv_x_loop");                             // continue loop

    // -- state 1: InField (unquoted, accumulating) --
    emitter.label("__rt_fgetcsv_x_st1");
    emitter.instruction("movzx ecx, BYTE PTR [rbp - 8]");                        // load sep
    emitter.instruction("cmp al, cl");                                          // c == sep?
    emitter.instruction("je __rt_fgetcsv_x_push_reset");                        // -> push field, reset
    emitter.instruction("cmp QWORD PTR [rbp - 96], 0");                         // is a newline ordinary data here?
    emitter.instruction("jne __rt_fgetcsv_x_st1_data");                            // `str_getcsv` keeps it as data
    emitter.instruction("cmp al, 0x0a");                                        // c == newline (0x0A)?
    emitter.instruction("je __rt_fgetcsv_x_push_end");                         // -> push field, end
    emitter.instruction("cmp al, 0x0d");                                        // c == carriage return (0x0D)?
    emitter.instruction("je __rt_fgetcsv_x_push_end");                         // -> push field, end
    emitter.label("__rt_fgetcsv_x_st1_data");
    emitter.instruction("mov BYTE PTR [r15], al");                              // *write_ptr = c
    emitter.instruction("add r15, 1");                                          // write_ptr++
    emitter.instruction("jmp __rt_fgetcsv_x_loop");                             // continue loop

    // -- state 2: InQuotedField --
    emitter.label("__rt_fgetcsv_x_st2");
    emitter.instruction("movzx ecx, BYTE PTR [rbp - 24]");                      // load esc
    emitter.instruction("test cl, cl");                                         // esc == 0?
    emitter.instruction("jz __rt_fgetcsv_x_s2_chkenc");                          // -> doubling mode, skip esc check
    emitter.instruction("cmp al, cl");                                          // c == esc?
    emitter.instruction("je __rt_fgetcsv_x_s2_esc");                            // -> AfterEscape
    emitter.label("__rt_fgetcsv_x_s2_chkenc");
    emitter.instruction("movzx ecx, BYTE PTR [rbp - 16]");                      // load enc
    emitter.instruction("cmp al, cl");                                          // c == enc (close quote)?
    emitter.instruction("je __rt_fgetcsv_x_s2_close");                          // -> AfterCloseQuote
    emitter.instruction("mov BYTE PTR [r15], al");                              // *write_ptr = c
    emitter.instruction("add r15, 1");                                          // write_ptr++
    emitter.instruction("jmp __rt_fgetcsv_x_loop");                             // continue loop

    emitter.label("__rt_fgetcsv_x_s2_esc");
    emitter.instruction("mov QWORD PTR [rbp - 32], 3");                        // state = AfterEscape
    emitter.instruction("jmp __rt_fgetcsv_x_loop");                             // continue loop

    emitter.label("__rt_fgetcsv_x_s2_close");
    emitter.instruction("mov QWORD PTR [rbp - 32], 4");                        // state = AfterCloseQuote
    emitter.instruction("jmp __rt_fgetcsv_x_loop");                             // continue loop

    // -- state 3: AfterEscape (esc mode only) --
    //
    // See the AArch64 counterpart: php KEEPS the escape byte whatever it precedes.
    emitter.label("__rt_fgetcsv_x_st3");
    emitter.instruction("movzx ecx, BYTE PTR [rbp - 24]");                      // load esc
    emitter.instruction("mov BYTE PTR [r15], cl");                              // *write_ptr = esc (php keeps it)
    emitter.instruction("add r15, 1");                                          // write_ptr++
    emitter.instruction("mov BYTE PTR [r15], al");                              // *write_ptr = c (literal)
    emitter.instruction("add r15, 1");                                          // write_ptr++
    emitter.instruction("mov QWORD PTR [rbp - 32], 2");                        // state = InQuotedField
    emitter.instruction("jmp __rt_fgetcsv_x_loop");                             // continue loop

    // -- state 4: AfterCloseQuote --
    emitter.label("__rt_fgetcsv_x_st4");
    emitter.instruction("movzx ecx, BYTE PTR [rbp - 16]");                      // load enc
    emitter.instruction("cmp al, cl");                                          // c == enc (doubled quote)?
    emitter.instruction("je __rt_fgetcsv_x_s4_dbl");                           // -> accumulate enc, back to quoted
    emitter.instruction("movzx ecx, BYTE PTR [rbp - 8]");                        // load sep
    emitter.instruction("cmp al, cl");                                          // c == sep?
    emitter.instruction("je __rt_fgetcsv_x_push_reset");                        // -> push field, reset
    emitter.instruction("cmp QWORD PTR [rbp - 96], 0");                         // is a newline ordinary data here?
    emitter.instruction("jne __rt_fgetcsv_x_st4_data");                            // `str_getcsv` keeps it as data
    emitter.instruction("cmp al, 0x0a");                                        // c == newline (0x0A)?
    emitter.instruction("je __rt_fgetcsv_x_push_end");                         // -> push field, end
    emitter.instruction("cmp al, 0x0d");                                        // c == carriage return (0x0D)?
    emitter.instruction("je __rt_fgetcsv_x_push_end");                         // -> push field, end
    emitter.label("__rt_fgetcsv_x_st4_data");
    // See the AArch64 counterpart: the closing enclosure is consumed, never written back.
    emitter.instruction("mov BYTE PTR [r15], al");                              // *write_ptr = c
    emitter.instruction("add r15, 1");                                          // write_ptr++
    emitter.instruction("mov QWORD PTR [rbp - 32], 1");                        // state = InField
    emitter.instruction("jmp __rt_fgetcsv_x_loop");                             // continue loop

    emitter.label("__rt_fgetcsv_x_s4_dbl");
    emitter.instruction("movzx ecx, BYTE PTR [rbp - 16]");                      // load enc
    emitter.instruction("mov BYTE PTR [r15], cl");                              // *write_ptr = enc (doubled -> single)
    emitter.instruction("add r15, 1");                                          // write_ptr++
    emitter.instruction("mov QWORD PTR [rbp - 32], 2");                        // state = InQuotedField
    emitter.instruction("jmp __rt_fgetcsv_x_loop");                             // continue loop

    // -- push field and reset for next field (separator encountered) --
    emitter.label("__rt_fgetcsv_x_push_reset");
    emitter.instruction("mov rax, r15");                                        // rax = write_ptr
    emitter.instruction("sub rax, r14");                                        // rax = len = write_ptr - field_start
    emitter.instruction("mov [rbp - 40], rax");                                 // save len at [rbp-40]
    // __rt_str_persist takes the string in rax/rdx, not rsi/rdx: this passed the pointer
    // in rsi and left rax holding the LENGTH, so the helper dereferenced the length as an
    // address — 1 for a one-byte field — and every fgetcsv() segfaulted on x86_64.
    emitter.instruction("mov rax, r14");                                        // ptr = field_start
    emitter.instruction("mov rdx, [rbp - 40]");                                 // len
    emitter.instruction("call __rt_str_persist");                               // rax = persisted string (heap copy)
    emitter.instruction("mov rsi, rax");                                        // rsi = persisted string ptr
    emitter.instruction("mov rdi, rbx");                                        // rdi = array_ptr
    emitter.instruction("mov rdx, [rbp - 40]");                                 // rdx = len
    emitter.instruction("call __rt_array_push_str");                            // rax = array_ptr (possibly reallocated)
    emitter.instruction("mov rbx, rax");                                        // update array_ptr
    emitter.instruction("mov r14, r15");                                        // field_start = write_ptr (next field)
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                        // state = OutsideField
    emitter.instruction("jmp __rt_fgetcsv_x_loop");                             // continue loop

    // -- push field and end (newline or end-of-buffer) --
    emitter.label("__rt_fgetcsv_x_push_end");
    emitter.instruction("mov rax, r15");                                        // rax = write_ptr
    emitter.instruction("sub rax, r14");                                        // rax = len = write_ptr - field_start
    emitter.instruction("mov [rbp - 40], rax");                                 // save len at [rbp-40]
    // __rt_str_persist takes the string in rax/rdx, not rsi/rdx: this passed the pointer
    // in rsi and left rax holding the LENGTH, so the helper dereferenced the length as an
    // address — 1 for a one-byte field — and every fgetcsv() segfaulted on x86_64.
    emitter.instruction("mov rax, r14");                                        // ptr = field_start
    emitter.instruction("mov rdx, [rbp - 40]");                                 // len
    emitter.instruction("call __rt_str_persist");                               // rax = persisted string (heap copy)
    emitter.instruction("mov rsi, rax");                                        // rsi = persisted string ptr
    emitter.instruction("mov rdi, rbx");                                        // rdi = array_ptr
    emitter.instruction("mov rdx, [rbp - 40]");                                 // rdx = len
    emitter.instruction("call __rt_array_push_str");                            // rax = array_ptr (possibly reallocated)
    emitter.instruction("mov rbx, rax");                                        // update array_ptr
    emitter.instruction("jmp __rt_fgetcsv_x_done");                             // -> epilogue

    // -- end of buffer: a field still inside its enclosure CONTINUES on the next line --
    //
    // Mirrors the AArch64 path: a newline between enclosures is data, so the record only
    // ends once the enclosure closes. `__rt_fgets` reserves from the shared concat
    // scratch, so the next line normally lands exactly where this one ended; when it does
    // not, the record ends here as before.
    emitter.label("__rt_fgetcsv_x_end_line");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 2");                         // still inside a quoted field?
    emitter.instruction("je __rt_fgetcsv_x_continue_line");
    emitter.instruction("cmp QWORD PTR [rbp - 32], 3");                         // or holding an escape inside one?
    emitter.instruction("jne __rt_fgetcsv_x_push_end");                         // no: the record really ends here
    emitter.label("__rt_fgetcsv_x_continue_line");
    emitter.instruction("cmp QWORD PTR [rbp - 96], 0");                         // parsing a STRING has no stream to read on
    emitter.instruction("jne __rt_fgetcsv_x_push_end");                         // `str_getcsv` ends the record here
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // the stream handle saved at entry
    emitter.instruction("xor esi, esi");                                        // no length bound
    emitter.instruction("call __rt_fgets");                                    // rax = next line ptr, rdx = its length
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_fgetcsv_x_push_end");                          // EOF closes an unterminated field
    emitter.instruction("cmp rax, r13");                                        // did it land right after this buffer?
    emitter.instruction("jne __rt_fgetcsv_x_push_end");                         // not contiguous: keep the old behaviour
    emitter.instruction("add r13, rdx");                                        // extend the parse over the new bytes
    emitter.instruction("jmp __rt_fgetcsv_x_loop");                             // and keep going

    // -- done: return array_ptr in rax --
    emitter.label("__rt_fgetcsv_x_done");
    emitter.instruction("mov rax, rbx");                                        // rax = array_ptr (return value)
    emitter.instruction("jmp __rt_fgetcsv_x_epilogue");                         // -> common epilogue

    // -- blank line: php_fgetcsv() reports NO RECORD, which is NOT end of input --
    emitter.label("__rt_fgetcsv_x_blank");
    emitter.instruction("mov eax, 1");                                          // sentinel 1: a blank record, not EOF's 0
    emitter.instruction("jmp __rt_fgetcsv_x_epilogue");                         // -> common epilogue

    // -- EOF: return 0 (false) --
    emitter.label("__rt_fgetcsv_x_eof");
    emitter.instruction("xor eax, eax");                                        // rax = 0 (false / EOF)

    // -- epilogue: restore registers and return --
    emitter.label("__rt_fgetcsv_x_epilogue");
    emitter.instruction("pop r15");                                             // restore r15
    emitter.instruction("pop r14");                                             // restore r14
    emitter.instruction("pop r13");                                             // restore r13
    emitter.instruction("pop r12");                                             // restore r12
    emitter.instruction("pop rbx");                                             // restore rbx
    emitter.instruction("leave");                                               // restore rbp + rsp
    emitter.instruction("ret");                                                 // return to caller
}
