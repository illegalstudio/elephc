//! Purpose:
//! Emits `__rt_mb_strimwidth`, the runtime helper for PHP's `mb_strimwidth()`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//! - `crate::codegen::lower_inst::builtins::strings::lower_mb_strimwidth()`.
//!
//! Key details:
//! - UTF-8 (omitted/`null`/`UTF-8`/`UTF8`) trims by PHP 8.5 East Asian display width.
//! - `8bit`/`binary`/`7bit` treat every byte as width 1; unknown encodings throw `ValueError`.
//! - `$start` is a character offset; a too-wide input is cut and the trim marker is appended
//!   so the result's width equals the requested budget (or the marker alone when it is wider).

use crate::codegen_support::{
    abi,
    emit::Emitter,
    platform::Arch,
    runtime::{
        arrays::value_error,
        data::{
            MB_STRIMWIDTH_START_RANGE_MSG, MB_STRIMWIDTH_UNKNOWN_ENCODING_MSG,
            MB_STRIMWIDTH_WIDTH_RANGE_MSG,
        },
    },
};

/// Maximum explicit encoding-name length copied into the runtime's stack buffer.
const MAX_ENCODING_NAME_LEN: usize = 63;

/// Emits `__rt_mb_strimwidth` for the active target.
pub fn emit_mb_strimwidth(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mb_strimwidth_x86_64(emitter);
    } else {
        emit_mb_strimwidth_aarch64(emitter);
    }
}

/// Emits the AArch64 implementation for macOS and Linux.
fn emit_mb_strimwidth_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_strimwidth (display-width trim) ---");
    emitter.label_global("__rt_mb_strimwidth");

    // Frame: [sp+0]=x19/x20 ... [sp+80]=x29/x30. Slots sit above the saved pair.
    // [sp+0] str ptr, [sp+8] str len, [sp+16] start, [sp+24] width,
    // [sp+32] marker ptr, [sp+40] marker len, [sp+48] enc ptr, [sp+56] enc len,
    // [sp+64] byte_mode, [sp+72] start_byte, [sp+80] result start, [sp+88] scratch,
    // [sp+96] encoding name, [sp+160] x19-x28 / x29 / x30.
    emitter.instruction("sub sp, sp, #256");                                    // reserve argument spills, encoding name, and callee-saved regs
    emitter.instruction("stp x29, x30, [sp, #240]");                            // save the caller frame and return address
    emitter.instruction("add x29, sp, #240");                                   // establish the helper frame pointer
    emitter.instruction("stp x19, x20, [sp, #160]");                            // preserve callee-saved x19/x20
    emitter.instruction("stp x21, x22, [sp, #176]");                            // preserve callee-saved x21/x22
    emitter.instruction("stp x23, x24, [sp, #192]");                            // preserve callee-saved x23/x24
    emitter.instruction("stp x25, x26, [sp, #208]");                            // preserve callee-saved x25/x26
    emitter.instruction("stp x27, x28, [sp, #224]");                            // preserve callee-saved x27/x28
    emitter.instruction("str x1, [sp, #0]");                                    // spill the source string pointer
    emitter.instruction("str x2, [sp, #8]");                                    // spill the source string length
    emitter.instruction("str x3, [sp, #16]");                                   // spill the signed start offset
    emitter.instruction("str x4, [sp, #24]");                                   // spill the signed display-width budget
    emitter.instruction("str x5, [sp, #32]");                                   // spill the trim-marker pointer
    emitter.instruction("str x6, [sp, #40]");                                   // spill the trim-marker length
    emitter.instruction("str x7, [sp, #48]");                                   // spill the optional encoding pointer
    emitter.instruction("str x0, [sp, #56]");                                   // spill the optional encoding length
    emitter.instruction("str xzr, [sp, #64]");                                  // default to the UTF-8 display-width scanner

    emitter.instruction("cbz x7, __rt_mb_strimwidth_count");                    // omitted/null encoding uses UTF-8
    emitter.instruction(&format!("cmp x0, #{}", MAX_ENCODING_NAME_LEN));        // does the encoding name fit the stack C-string buffer?
    emitter.instruction("b.hi __rt_mb_strimwidth_unknown_encoding");            // reject names longer than every supported alias
    emitter.instruction("add x9, sp, #96");                                     // destination is the 64-byte encoding-name buffer
    emitter.instruction("mov x10, #0");                                         // copied-byte index starts at zero
    emitter.label("__rt_mb_strimwidth_encoding_copy");
    emitter.instruction("cmp x10, x0");                                         // copied the whole explicit encoding name?
    emitter.instruction("b.hs __rt_mb_strimwidth_encoding_copied");             // terminate the C string once every byte is copied
    emitter.instruction("ldrb w11, [x7, x10]");                                 // load one encoding-name byte from the PHP string
    emitter.instruction("strb w11, [x9, x10]");                                 // append the byte to the stack C string
    emitter.instruction("add x10, x10, #1");                                    // advance the encoding-name byte index
    emitter.instruction("b __rt_mb_strimwidth_encoding_copy");                  // continue copying the remaining encoding-name bytes
    emitter.label("__rt_mb_strimwidth_encoding_copied");
    emitter.instruction("strb wzr, [x9, x0]");                                  // NUL-terminate the explicit encoding name
    emitter.instruction("add x0, sp, #96");                                     // first strcasecmp argument is the copied encoding name
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_utf8_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with UTF-8
    emitter.instruction("cbz x0, __rt_mb_strimwidth_count");                    // UTF-8 uses the display-width scanner
    emitter.instruction("add x0, sp, #96");                                     // reload the copied encoding name
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_utf8_alias");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with UTF8
    emitter.instruction("cbz x0, __rt_mb_strimwidth_count");                    // the UTF8 alias uses the same scanner
    emitter.instruction("add x0, sp, #96");                                     // reload the copied encoding name
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_8bit_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with 8bit
    emitter.instruction("cbz x0, __rt_mb_strimwidth_byte_mode");                // 8bit treats every byte as width 1
    emitter.instruction("add x0, sp, #96");                                     // reload the copied encoding name
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_binary_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with binary
    emitter.instruction("cbz x0, __rt_mb_strimwidth_byte_mode");                // binary is PHP's alias for 8bit
    emitter.instruction("add x0, sp, #96");                                     // reload the copied encoding name
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_7bit_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with 7bit
    emitter.instruction("cbz x0, __rt_mb_strimwidth_byte_mode");                // 7bit preserves one-character-per-byte width
    emitter.instruction("b __rt_mb_strimwidth_unknown_encoding");               // any other name is an unknown encoding

    emitter.label("__rt_mb_strimwidth_byte_mode");
    emitter.instruction("mov x9, #1");                                          // mark the scanner as byte-width mode
    emitter.instruction("str x9, [sp, #64]");                                   // persist the byte-width flag

    emitter.label("__rt_mb_strimwidth_count");
    emitter.instruction("ldr x19, [sp, #0]");                                   // x19 = source pointer
    emitter.instruction("ldr x20, [sp, #8]");                                   // x20 = source length
    emitter.instruction("ldr x21, [sp, #64]");                                  // x21 = byte-mode flag
    emitter.instruction("mov x0, x19");                                         // count characters in the whole source
    emitter.instruction("mov x1, x20");                                         // pass the source length
    emitter.instruction("mov x2, x21");                                         // pass the width mode
    emitter.instruction("bl __rt_mb_strimwidth_count_chars");                   // x0 = character count
    emitter.instruction("mov x22, x0");                                         // x22 = character count
    emitter.instruction("ldr x9, [sp, #16]");                                   // load the signed start offset
    emitter.instruction("cbz x9, __rt_mb_strimwidth_start_ok");                 // start 0 never needs a range check
    emitter.instruction("cmp x9, #0");                                          // is start negative?
    emitter.instruction("b.ge __rt_mb_strimwidth_start_nonneg");                // a non-negative start is already an origin offset
    emitter.instruction("add x9, x9, x22");                                     // negative start counts from the end
    emitter.label("__rt_mb_strimwidth_start_nonneg");
    emitter.instruction("cmp x9, #0");                                          // did a negative start underflow the string?
    emitter.instruction("b.lt __rt_mb_strimwidth_start_error");                 // start below zero is out of range
    emitter.instruction("cmp x9, x22");                                         // is start past the last character?
    emitter.instruction("b.gt __rt_mb_strimwidth_start_error");                 // start > char_count is out of range
    emitter.instruction("str x9, [sp, #16]");                                   // persist the resolved non-negative start
    emitter.label("__rt_mb_strimwidth_start_ok");
    emitter.instruction("ldr x9, [sp, #24]");                                   // load the signed width budget
    emitter.instruction("cmp x9, #0");                                          // is width negative?
    emitter.instruction("b.ge __rt_mb_strimwidth_width_ok");                    // a non-negative width is already a remaining budget
    emitter.instruction("mov x0, x19");                                         // measure the whole string's display width
    emitter.instruction("mov x1, x20");                                         // pass the source length
    emitter.instruction("mov x2, x21");                                         // pass the width mode
    emitter.instruction("bl __rt_mb_strimwidth_strwidth");                      // x0 = total display width
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the negative width
    emitter.instruction("add x9, x9, x0");                                      // PHP adds the whole-string width first
    emitter.instruction("ldr x10, [sp, #16]");                                  // load the resolved start
    emitter.instruction("cbz x10, __rt_mb_strimwidth_neg_width_checked");       // start 0 has no prefix width to subtract
    emitter.instruction("mov x0, x19");                                         // skip the prefix so we can measure it
    emitter.instruction("mov x1, x20");                                         // pass the source length
    emitter.instruction("mov x2, x10");                                         // skip this many characters
    emitter.instruction("mov x3, x21");                                         // pass the width mode
    emitter.instruction("bl __rt_mb_strimwidth_skip");                          // x0 = prefix byte length
    emitter.instruction("mov x1, x0");                                          // measure only the skipped prefix
    emitter.instruction("mov x0, x19");                                         // prefix starts at the source pointer
    emitter.instruction("mov x2, x21");                                         // pass the width mode
    emitter.instruction("bl __rt_mb_strimwidth_strwidth");                      // x0 = prefix display width
    emitter.instruction("sub x9, x9, x0");                                      // subtract the skipped prefix width
    emitter.label("__rt_mb_strimwidth_neg_width_checked");
    emitter.instruction("cmp x9, #0");                                          // did the adjusted width underflow?
    emitter.instruction("b.lt __rt_mb_strimwidth_width_error");                 // a still-negative width is out of range
    emitter.instruction("str x9, [sp, #24]");                                   // persist the resolved non-negative width
    emitter.label("__rt_mb_strimwidth_width_ok");

    emitter.instruction("ldr x0, [sp, #0]");                                    // skip `start` characters to find the kept suffix
    emitter.instruction("ldr x1, [sp, #8]");                                    // pass the source length
    emitter.instruction("ldr x2, [sp, #16]");                                   // pass the resolved start
    emitter.instruction("ldr x3, [sp, #64]");                                   // pass the width mode
    emitter.instruction("bl __rt_mb_strimwidth_skip");                          // x0 = start byte offset
    emitter.instruction("str x0, [sp, #72]");                                   // persist the suffix origin
    emitter.instruction("ldr x19, [sp, #0]");                                   // reload the source pointer
    emitter.instruction("add x19, x19, x0");                                    // x19 = suffix pointer
    emitter.instruction("ldr x20, [sp, #8]");                                   // reload the source length
    emitter.instruction("sub x20, x20, x0");                                    // x20 = suffix byte length
    emitter.instruction("mov x0, x19");                                         // measure the suffix display width
    emitter.instruction("mov x1, x20");                                         // pass the suffix length
    emitter.instruction("ldr x2, [sp, #64]");                                   // pass the width mode
    emitter.instruction("bl __rt_mb_strimwidth_strwidth");                      // x0 = suffix display width
    emitter.instruction("ldr x9, [sp, #24]");                                   // load the resolved width budget
    emitter.instruction("cmp x0, x9");                                          // does the suffix already fit?
    emitter.instruction("b.ls __rt_mb_strimwidth_copy_suffix");                 // yes → return the suffix unchanged
    emitter.instruction("ldr x0, [sp, #32]");                                   // measure the trim marker's display width
    emitter.instruction("ldr x1, [sp, #40]");                                   // pass the marker length
    emitter.instruction("ldr x2, [sp, #64]");                                   // pass the width mode
    emitter.instruction("bl __rt_mb_strimwidth_strwidth");                      // x0 = marker display width
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the requested width
    emitter.instruction("cmp x9, x0");                                          // is the budget no larger than the marker?
    emitter.instruction("b.ls __rt_mb_strimwidth_copy_marker");                 // yes → PHP returns the marker alone
    emitter.instruction("sub x2, x9, x0");                                      // remaining budget for source characters
    emitter.instruction("mov x0, x19");                                         // take a prefix of the suffix
    emitter.instruction("mov x1, x20");                                         // pass the suffix length
    emitter.instruction("ldr x3, [sp, #64]");                                   // pass the width mode
    emitter.instruction("bl __rt_mb_strimwidth_take");                          // x0 = kept source byte count
    emitter.instruction("mov x21, x0");                                         // x21 = kept source bytes
    emitter.instruction("ldr x9, [sp, #40]");                                   // load the marker length
    emitter.instruction("add x0, x21, x9");                                     // reserve source prefix plus marker
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage
    emitter.instruction("str x0, [sp, #80]");                                   // persist the result start pointer
    emitter.instruction("mov x1, x19");                                         // memcpy source is the suffix
    emitter.instruction("mov x2, x21");                                         // memcpy length is the kept prefix
    emitter.bl_c("memmove"); // copy the kept source prefix into the reservation
    emitter.instruction("ldr x0, [sp, #80]");                                   // reload the result start
    emitter.instruction("add x0, x0, x21");                                     // destination for the trim marker
    emitter.instruction("ldr x1, [sp, #32]");                                   // memcpy source is the marker
    emitter.instruction("ldr x2, [sp, #40]");                                   // memcpy length is the marker length
    emitter.bl_c("memmove"); // append the trim marker
    emitter.instruction("ldr x1, [sp, #80]");                                   // result pointer is the reservation start
    emitter.instruction("ldr x2, [sp, #40]");                                   // start with the marker length
    emitter.instruction("add x2, x2, x21");                                     // add the kept source bytes
    emitter.instruction("bl __rt_concat_publish");                              // publish the concat-scratch write offset
    emitter.instruction("b __rt_mb_strimwidth_return");                         // restore callee-saved regs and return

    emitter.label("__rt_mb_strimwidth_copy_suffix");
    emitter.instruction("mov x0, x20");                                         // reserve exactly the suffix bytes
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage
    emitter.instruction("str x0, [sp, #80]");                                   // persist the result start pointer
    emitter.instruction("mov x1, x19");                                         // memcpy source is the suffix
    emitter.instruction("mov x2, x20");                                         // memcpy length is the suffix length
    emitter.bl_c("memmove"); // copy the untrimmed suffix
    emitter.instruction("ldr x1, [sp, #80]");                                   // result pointer is the reservation start
    emitter.instruction("mov x2, x20");                                         // result length is the suffix length
    emitter.instruction("bl __rt_concat_publish");                              // publish the concat-scratch write offset
    emitter.instruction("b __rt_mb_strimwidth_return");                         // restore callee-saved regs and return

    emitter.label("__rt_mb_strimwidth_copy_marker");
    emitter.instruction("ldr x0, [sp, #40]");                                   // reserve exactly the marker bytes
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage
    emitter.instruction("str x0, [sp, #80]");                                   // persist the result start pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // memcpy source is the marker
    emitter.instruction("ldr x2, [sp, #40]");                                   // memcpy length is the marker length
    emitter.bl_c("memmove"); // copy the marker alone
    emitter.instruction("ldr x1, [sp, #80]");                                   // result pointer is the reservation start
    emitter.instruction("ldr x2, [sp, #40]");                                   // result length is the marker length
    emitter.instruction("bl __rt_concat_publish");                              // publish the concat-scratch write offset

    emitter.label("__rt_mb_strimwidth_return");
    emitter.instruction("ldp x19, x20, [sp, #160]");                            // restore callee-saved x19/x20
    emitter.instruction("ldp x21, x22, [sp, #176]");                            // restore callee-saved x21/x22
    emitter.instruction("ldp x23, x24, [sp, #192]");                            // restore callee-saved x23/x24
    emitter.instruction("ldp x25, x26, [sp, #208]");                            // restore callee-saved x25/x26
    emitter.instruction("ldp x27, x28, [sp, #224]");                            // restore callee-saved x27/x28
    emitter.instruction("ldp x29, x30, [sp, #240]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #256");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return the trimmed string pointer/length

    emitter.label("__rt_mb_strimwidth_unknown_encoding");
    emitter.instruction("ldp x19, x20, [sp, #160]");                            // restore callee-saved regs before throwing
    emitter.instruction("ldp x21, x22, [sp, #176]");                            // restore callee-saved x21/x22
    emitter.instruction("ldp x23, x24, [sp, #192]");                            // restore callee-saved x23/x24
    emitter.instruction("ldp x25, x26, [sp, #208]");                            // restore callee-saved x25/x26
    emitter.instruction("ldp x27, x28, [sp, #224]");                            // restore callee-saved x27/x28
    emitter.instruction("ldp x29, x30, [sp, #240]");                            // restore the caller frame before throwing
    emitter.instruction("add sp, sp, #256");                                    // release the helper frame before unwinding
    value_error::emit_throw_value_error_aarch64(
        emitter,
        "_mb_strimwidth_unknown_encoding_msg",
        MB_STRIMWIDTH_UNKNOWN_ENCODING_MSG.len(),
    );

    emitter.label("__rt_mb_strimwidth_start_error");
    emitter.instruction("ldp x19, x20, [sp, #160]");                            // restore callee-saved regs before throwing
    emitter.instruction("ldp x21, x22, [sp, #176]");                            // restore callee-saved x21/x22
    emitter.instruction("ldp x23, x24, [sp, #192]");                            // restore callee-saved x23/x24
    emitter.instruction("ldp x25, x26, [sp, #208]");                            // restore callee-saved x25/x26
    emitter.instruction("ldp x27, x28, [sp, #224]");                            // restore callee-saved x27/x28
    emitter.instruction("ldp x29, x30, [sp, #240]");                            // restore the caller frame before throwing
    emitter.instruction("add sp, sp, #256");                                    // release the helper frame before unwinding
    value_error::emit_throw_value_error_aarch64(
        emitter,
        "_mb_strimwidth_start_range_msg",
        MB_STRIMWIDTH_START_RANGE_MSG.len(),
    );

    emitter.label("__rt_mb_strimwidth_width_error");
    emitter.instruction("ldp x19, x20, [sp, #160]");                            // restore callee-saved regs before throwing
    emitter.instruction("ldp x21, x22, [sp, #176]");                            // restore callee-saved x21/x22
    emitter.instruction("ldp x23, x24, [sp, #192]");                            // restore callee-saved x23/x24
    emitter.instruction("ldp x25, x26, [sp, #208]");                            // restore callee-saved x25/x26
    emitter.instruction("ldp x27, x28, [sp, #224]");                            // restore callee-saved x27/x28
    emitter.instruction("ldp x29, x30, [sp, #240]");                            // restore the caller frame before throwing
    emitter.instruction("add sp, sp, #256");                                    // release the helper frame before unwinding
    value_error::emit_throw_value_error_aarch64(
        emitter,
        "_mb_strimwidth_width_range_msg",
        MB_STRIMWIDTH_WIDTH_RANGE_MSG.len(),
    );

    emit_mb_strimwidth_walkers_aarch64(emitter);
}

/// Emits AArch64 character-walk helpers shared by the trim helper.
fn emit_mb_strimwidth_walkers_aarch64(emitter: &mut Emitter) {
    // count_chars(x0=ptr, x1=len, x2=byte_mode) -> x0=count
    emitter.label_shared("__rt_mb_strimwidth_count_chars");
    emitter.instruction("cbnz x2, __rt_mb_strimwidth_count_bytes");             // byte mode returns the raw length
    emitter.instruction("mov x3, x0");                                          // x3 = scan pointer
    emitter.instruction("add x4, x0, x1");                                      // x4 = one-past-end pointer
    emitter.instruction("mov x0, #0");                                          // character count starts at zero
    emitter.label("__rt_mb_strimwidth_count_loop");
    emitter.instruction("cmp x3, x4");                                          // scanned every byte?
    emitter.instruction("b.hs __rt_mb_strimwidth_count_done");                  // return the accumulated count
    emitter.instruction("mov x5, x3");                                          // next-char input is the current pointer
    emitter.instruction("mov x6, x4");                                          // next-char end is the one-past-end pointer
    emitter.instruction("stp x0, x30, [sp, #-16]!");                            // preserve the count and return address
    emitter.instruction("bl __rt_mb_strimwidth_next");                          // x5 advances past one character
    emitter.instruction("ldp x0, x30, [sp], #16");                              // restore the count and return address
    emitter.instruction("mov x3, x5");                                          // continue after the consumed character
    emitter.instruction("add x0, x0, #1");                                      // count one more character
    emitter.instruction("b __rt_mb_strimwidth_count_loop");                     // keep scanning
    emitter.label("__rt_mb_strimwidth_count_bytes");
    emitter.instruction("mov x0, x1");                                          // byte encodings count one character per byte
    emitter.label("__rt_mb_strimwidth_count_done");
    emitter.instruction("ret");                                                 // return the character count

    // strwidth(x0=ptr, x1=len, x2=byte_mode) -> x0=width
    emitter.label_shared("__rt_mb_strimwidth_strwidth");
    emitter.instruction("cbnz x2, __rt_mb_strimwidth_strwidth_bytes");          // byte mode returns the raw length
    emitter.instruction("mov x3, x0");                                          // x3 = scan pointer
    emitter.instruction("add x4, x0, x1");                                      // x4 = one-past-end pointer
    emitter.instruction("mov x0, #0");                                          // display width starts at zero
    emitter.label("__rt_mb_strimwidth_strwidth_loop");
    emitter.instruction("cmp x3, x4");                                          // scanned every byte?
    emitter.instruction("b.hs __rt_mb_strimwidth_strwidth_done");               // return the accumulated width
    emitter.instruction("mov x5, x3");                                          // next-char input is the current pointer
    emitter.instruction("mov x6, x4");                                          // next-char end is the one-past-end pointer
    emitter.instruction("stp x0, x30, [sp, #-16]!");                            // preserve the width and return address
    emitter.instruction("bl __rt_mb_strimwidth_next");                          // x7 = codepoint, x5 advances
    emitter.instruction("stp x4, x5, [sp, #-16]!");                             // preserve the end pointer and advanced cursor
    emitter.instruction("mov x0, x7");                                          // look up the consumed character's width
    emitter.instruction("bl __rt_mb_strimwidth_char_width");                    // x0 = 1 or 2
    emitter.instruction("mov x8, x0");                                          // stash the character width
    emitter.instruction("ldp x4, x5, [sp], #16");                               // restore the end pointer and advanced cursor
    emitter.instruction("ldp x0, x30, [sp], #16");                              // restore the total width and return address
    emitter.instruction("add x0, x0, x8");                                      // accumulate the character width
    emitter.instruction("mov x3, x5");                                          // continue after the consumed character
    emitter.instruction("b __rt_mb_strimwidth_strwidth_loop");                  // keep scanning
    emitter.label("__rt_mb_strimwidth_strwidth_bytes");
    emitter.instruction("mov x0, x1");                                          // byte encodings have width equal to length
    emitter.label("__rt_mb_strimwidth_strwidth_done");
    emitter.instruction("ret");                                                 // return the display width

    // skip(x0=ptr, x1=len, x2=count, x3=byte_mode) -> x0=byte offset
    emitter.label_shared("__rt_mb_strimwidth_skip");
    emitter.instruction("cbnz x3, __rt_mb_strimwidth_skip_bytes");              // byte mode skips `count` bytes
    emitter.instruction("mov x5, x0");                                          // x5 = scan pointer
    emitter.instruction("add x6, x0, x1");                                      // x6 = one-past-end pointer
    emitter.instruction("mov x4, x0");                                          // remember the origin for the returned offset
    emitter.instruction("mov x8, x2");                                          // remaining characters to skip
    emitter.label("__rt_mb_strimwidth_skip_loop");
    emitter.instruction("cbz x8, __rt_mb_strimwidth_skip_done");                // finished skipping the requested count
    emitter.instruction("cmp x5, x6");                                          // reached the end of the string?
    emitter.instruction("b.hs __rt_mb_strimwidth_skip_done");                   // cannot skip past the last character
    emitter.instruction("stp x4, x8, [sp, #-16]!");                             // preserve origin and remaining count
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the return address
    emitter.instruction("bl __rt_mb_strimwidth_next");                          // advance one character
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the return address
    emitter.instruction("ldp x4, x8, [sp], #16");                               // restore origin and remaining count
    emitter.instruction("sub x8, x8, #1");                                      // one fewer character remains
    emitter.instruction("b __rt_mb_strimwidth_skip_loop");                      // keep skipping
    emitter.label("__rt_mb_strimwidth_skip_done");
    emitter.instruction("sub x0, x5, x4");                                      // return the consumed byte count
    emitter.instruction("ret");                                                 // return the skip offset
    emitter.label("__rt_mb_strimwidth_skip_bytes");
    emitter.instruction("cmp x2, x1");                                          // would the skip pass the last byte?
    emitter.instruction("csel x0, x2, x1, lo");                                 // clamp the byte skip to the string length
    emitter.instruction("ret");                                                 // return the clamped byte offset

    // take(x0=ptr, x1=len, x2=budget, x3=byte_mode) -> x0=byte count
    emitter.label_shared("__rt_mb_strimwidth_take");
    emitter.instruction("cbnz x3, __rt_mb_strimwidth_take_bytes");              // byte mode takes `budget` bytes
    emitter.instruction("mov x5, x0");                                          // x5 = scan pointer
    emitter.instruction("add x6, x0, x1");                                      // x6 = one-past-end pointer
    emitter.instruction("mov x4, x0");                                          // remember the origin for the returned length
    emitter.instruction("mov x8, x2");                                          // remaining display-width budget
    emitter.label("__rt_mb_strimwidth_take_loop");
    emitter.instruction("cmp x5, x6");                                          // reached the end of the string?
    emitter.instruction("b.hs __rt_mb_strimwidth_take_done");                   // take the whole suffix
    emitter.instruction("stp x4, x8, [sp, #-16]!");                             // preserve origin and remaining budget
    emitter.instruction("stp x6, x30, [sp, #-16]!");                            // preserve the end pointer and return address
    emitter.instruction("mov x9, x5");                                          // remember the cursor before this character
    emitter.instruction("bl __rt_mb_strimwidth_next");                          // x7 = codepoint, x5 tentatively advanced
    emitter.instruction("stp x9, x5, [sp, #-16]!");                             // preserve the pre/post-decode cursors
    emitter.instruction("mov x0, x7");                                          // look up the candidate character width
    emitter.instruction("bl __rt_mb_strimwidth_char_width");                    // x0 = 1 or 2
    emitter.instruction("ldp x9, x5, [sp], #16");                               // restore the pre/post-decode cursors
    emitter.instruction("ldp x6, x30, [sp], #16");                              // restore the end pointer and return address
    emitter.instruction("ldp x4, x8, [sp], #16");                               // restore origin and remaining budget
    emitter.instruction("cmp x8, x0");                                          // does the character still fit the budget?
    emitter.instruction("b.lo __rt_mb_strimwidth_take_reject");                 // no → leave this character out
    emitter.instruction("sub x8, x8, x0");                                      // consume the character's display width
    emitter.instruction("b __rt_mb_strimwidth_take_loop");                      // keep taking
    emitter.label("__rt_mb_strimwidth_take_reject");
    emitter.instruction("mov x5, x9");                                          // rewind to exclude the overflowing character
    emitter.label("__rt_mb_strimwidth_take_done");
    emitter.instruction("sub x0, x5, x4");                                      // return the kept byte count
    emitter.instruction("ret");                                                 // return the taken prefix length
    emitter.label("__rt_mb_strimwidth_take_bytes");
    emitter.instruction("cmp x2, x1");                                          // would the take pass the last byte?
    emitter.instruction("csel x0, x2, x1, lo");                                 // clamp the byte take to the string length
    emitter.instruction("ret");                                                 // return the clamped byte count

    // next(x5=ptr, x6=end) -> x5=advanced ptr, x7=codepoint
    emitter.label_shared("__rt_mb_strimwidth_next");
    emitter.instruction("ldrb w9, [x5]");                                       // load the next possible UTF-8 leading byte
    emitter.instruction("cmp w9, #0x80");                                       // ASCII bytes are complete one-byte characters
    emitter.instruction("b.lo __rt_mb_strimwidth_next_ascii");                  // consume one ASCII byte
    emitter.instruction("cmp w9, #0xc2");                                       // C0/C1 and continuation bytes are malformed leaders
    emitter.instruction("b.lo __rt_mb_strimwidth_next_invalid");                // substitute one malformed byte
    emitter.instruction("cmp w9, #0xe0");                                       // C2-DF introduce two-byte sequences
    emitter.instruction("b.lo __rt_mb_strimwidth_next_two");                    // validate a two-byte character
    emitter.instruction("cmp w9, #0xf0");                                       // E0-EF introduce three-byte sequences
    emitter.instruction("b.lo __rt_mb_strimwidth_next_three");                  // validate a three-byte character
    emitter.instruction("cmp w9, #0xf5");                                       // F0-F4 introduce Unicode-range four-byte sequences
    emitter.instruction("b.lo __rt_mb_strimwidth_next_four");                   // validate a four-byte character
    emitter.instruction("b __rt_mb_strimwidth_next_invalid");                   // F5-FF cannot begin valid UTF-8
    emitter.label("__rt_mb_strimwidth_next_ascii");
    emitter.instruction("uxtw x7, w9");                                         // the ASCII byte is the code point
    emitter.instruction("add x5, x5, #1");                                      // consume the one-byte character
    emitter.instruction("ret");                                                 // return the ASCII character
    emitter.label("__rt_mb_strimwidth_next_invalid");
    emitter.instruction("mov x7, #0xffffffff");                                 // malformed bytes have no Unicode scalar
    emitter.instruction("add x5, x5, #1");                                      // substitute one malformed byte
    emitter.instruction("ret");                                                 // return the substitution character
    emitter.label("__rt_mb_strimwidth_next_two");
    emitter.instruction("add x10, x5, #2");                                     // two-byte sequences need one continuation
    emitter.instruction("cmp x10, x6");                                         // is the sequence truncated?
    emitter.instruction("b.hi __rt_mb_strimwidth_next_trunc");                  // group the truncated prefix as one character
    emitter.instruction("ldrb w10, [x5, #1]");                                  // load the continuation byte
    emitter.instruction("and w11, w10, #0xc0");                                 // isolate the continuation-byte prefix
    emitter.instruction("cmp w11, #0x80");                                      // does the second byte have the required 10xxxxxx shape?
    emitter.instruction("b.ne __rt_mb_strimwidth_next_invalid");                // malformed continuation leaves the leader substituted
    emitter.instruction("and w9, w9, #0x1f");                                   // keep the two-byte payload bits
    emitter.instruction("lsl w9, w9, #6");                                      // shift the leader payload into place
    emitter.instruction("and w10, w10, #0x3f");                                 // keep the continuation payload bits
    emitter.instruction("orr w7, w9, w10");                                     // assemble the two-byte code point
    emitter.instruction("add x5, x5, #2");                                      // consume the complete two-byte character
    emitter.instruction("ret");                                                 // return the two-byte character
    emitter.label("__rt_mb_strimwidth_next_three");
    emitter.instruction("add x10, x5, #3");                                     // three-byte sequences need two continuations
    emitter.instruction("cmp x10, x6");                                         // is the sequence truncated?
    emitter.instruction("b.hi __rt_mb_strimwidth_next_trunc");                  // group the truncated prefix as one character
    emitter.instruction("ldrb w10, [x5, #1]");                                  // load the first continuation byte
    emitter.instruction("ldrb w11, [x5, #2]");                                  // load the second continuation byte
    emitter.instruction("and w12, w10, #0xc0");                                 // isolate the first continuation prefix
    emitter.instruction("cmp w12, #0x80");                                      // is the first continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strimwidth_next_invalid");                // malformed continuation substitutes only the leader
    emitter.instruction("and w12, w11, #0xc0");                                 // isolate the second continuation prefix
    emitter.instruction("cmp w12, #0x80");                                      // is the second continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strimwidth_next_invalid");                // malformed final byte substitutes only the leader
    emitter.instruction("and w9, w9, #0x0f");                                   // keep the three-byte leader payload
    emitter.instruction("lsl w9, w9, #12");                                     // shift the leader payload into place
    emitter.instruction("and w10, w10, #0x3f");                                 // keep the first continuation payload
    emitter.instruction("lsl w10, w10, #6");                                    // shift the first continuation into place
    emitter.instruction("and w11, w11, #0x3f");                                 // keep the second continuation payload
    emitter.instruction("orr w9, w9, w10");                                     // merge leader and first continuation
    emitter.instruction("orr w7, w9, w11");                                     // assemble the three-byte code point
    emitter.instruction("add x5, x5, #3");                                      // consume the complete three-byte character
    emitter.instruction("ret");                                                 // return the three-byte character
    emitter.label("__rt_mb_strimwidth_next_four");
    emitter.instruction("add x10, x5, #4");                                     // four-byte sequences need three continuations
    emitter.instruction("cmp x10, x6");                                         // is the sequence truncated?
    emitter.instruction("b.hi __rt_mb_strimwidth_next_trunc");                  // group the truncated prefix as one character
    emitter.instruction("ldrb w10, [x5, #1]");                                  // load the first continuation byte
    emitter.instruction("ldrb w11, [x5, #2]");                                  // load the second continuation byte
    emitter.instruction("ldrb w12, [x5, #3]");                                  // load the third continuation byte
    emitter.instruction("and w13, w10, #0xc0");                                 // isolate the first continuation prefix
    emitter.instruction("cmp w13, #0x80");                                      // is the first continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strimwidth_next_invalid");                // malformed continuation substitutes only the leader
    emitter.instruction("and w13, w11, #0xc0");                                 // isolate the second continuation prefix
    emitter.instruction("cmp w13, #0x80");                                      // is the second continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strimwidth_next_invalid");                // malformed middle byte substitutes only the leader
    emitter.instruction("and w13, w12, #0xc0");                                 // isolate the third continuation prefix
    emitter.instruction("cmp w13, #0x80");                                      // is the third continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strimwidth_next_invalid");                // malformed final byte substitutes only the leader
    emitter.instruction("and w9, w9, #0x07");                                   // keep the four-byte leader payload
    emitter.instruction("lsl w9, w9, #18");                                     // shift the leader payload into place
    emitter.instruction("and w10, w10, #0x3f");                                 // keep the first continuation payload
    emitter.instruction("lsl w10, w10, #12");                                   // shift the first continuation into place
    emitter.instruction("and w11, w11, #0x3f");                                 // keep the second continuation payload
    emitter.instruction("lsl w11, w11, #6");                                    // shift the second continuation into place
    emitter.instruction("and w12, w12, #0x3f");                                 // keep the third continuation payload
    emitter.instruction("orr w9, w9, w10");                                     // merge leader and first continuation
    emitter.instruction("orr w9, w9, w11");                                     // merge the second continuation
    emitter.instruction("orr w7, w9, w12");                                     // assemble the four-byte code point
    emitter.instruction("add x5, x5, #4");                                      // consume the complete four-byte character
    emitter.instruction("ret");                                                 // return the four-byte character
    emitter.label("__rt_mb_strimwidth_next_trunc");
    emitter.instruction("mov x7, #0xffffffff");                                 // a truncated prefix is one substitution character
    emitter.instruction("mov x5, x6");                                          // consume the remaining suffix bytes
    emitter.instruction("ret");                                                 // return the truncated-prefix character

    // char_width(x0=codepoint) -> x0=1 or 2
    emitter.label_shared("__rt_mb_strimwidth_char_width");
    abi::emit_load_int_immediate(emitter, "x1", 0x1100);
    emitter.instruction("cmp x0, x1");                                          // code points below U+1100 are never fullwidth
    emitter.instruction("b.lo __rt_mb_strimwidth_width_one");                   // return width 1 for the fast path
    abi::emit_symbol_address(emitter, "x2", "_mb_eaw_table");
    abi::emit_symbol_address(emitter, "x3", "_mb_eaw_table_count");
    emitter.instruction("ldr x3, [x3]");                                        // load the number of inclusive width-2 ranges
    emitter.instruction("mov x4, #0");                                          // binary-search low index starts at zero
    emitter.label("__rt_mb_strimwidth_width_search");
    emitter.instruction("cmp x4, x3");                                          // has the search range emptied?
    emitter.instruction("b.hs __rt_mb_strimwidth_width_one");                   // no range contained the code point
    emitter.instruction("add x5, x4, x3");                                      // probe = (lo + hi)
    emitter.instruction("lsr x5, x5, #1");                                      // probe = (lo + hi) / 2
    emitter.instruction("add x6, x2, x5, lsl #3");                              // address the probe range (two 32-bit words)
    emitter.instruction("ldr w7, [x6]");                                        // load the inclusive range begin
    emitter.instruction("ldr w8, [x6, #4]");                                    // load the inclusive range end
    emitter.instruction("cmp w0, w7");                                          // is the code point before this range?
    emitter.instruction("b.lo __rt_mb_strimwidth_width_left");                  // search the lower half
    emitter.instruction("cmp w0, w8");                                          // is the code point after this range?
    emitter.instruction("b.hi __rt_mb_strimwidth_width_right");                 // search the upper half
    emitter.instruction("mov x0, #2");                                          // a containing range means display width 2
    emitter.instruction("ret");                                                 // return the fullwidth result
    emitter.label("__rt_mb_strimwidth_width_left");
    emitter.instruction("mov x3, x5");                                          // hi = probe
    emitter.instruction("b __rt_mb_strimwidth_width_search");                   // continue the binary search
    emitter.label("__rt_mb_strimwidth_width_right");
    emitter.instruction("add x4, x5, #1");                                      // lo = probe + 1
    emitter.instruction("b __rt_mb_strimwidth_width_search");                   // continue the binary search
    emitter.label("__rt_mb_strimwidth_width_one");
    emitter.instruction("mov x0, #1");                                          // every other code point has display width 1
    emitter.instruction("ret");                                                 // return the halfwidth result
}

/// Emits the Linux x86_64 implementation.
fn emit_mb_strimwidth_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_strimwidth (display-width trim) ---");
    emitter.label_global("__rt_mb_strimwidth");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for spills
    emitter.instruction("push rbx");                                            // preserve callee-saved rbx
    emitter.instruction("push r12");                                            // preserve callee-saved r12
    emitter.instruction("push r13");                                            // preserve callee-saved r13
    emitter.instruction("push r14");                                            // preserve callee-saved r14
    emitter.instruction("push r15");                                            // preserve callee-saved r15
    emitter.instruction("sub rsp, 192");                                        // reserve argument spills and the encoding-name buffer
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // spill the source string pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // spill the source string length
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // spill the signed start offset
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                        // spill the signed display-width budget
    emitter.instruction("mov QWORD PTR [rbp - 80], r9");                        // spill the trim-marker pointer
    emitter.instruction("mov QWORD PTR [rbp - 88], r10");                       // spill the trim-marker length
    emitter.instruction("mov QWORD PTR [rbp - 96], r11");                       // spill the optional encoding pointer
    emitter.instruction("mov QWORD PTR [rbp - 104], rdi");                      // spill the optional encoding length
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                        // default to the UTF-8 display-width scanner
    emitter.instruction("test r11, r11");                                       // omitted/null encoding is a null pointer
    emitter.instruction("jz __rt_mb_strimwidth_count_x86");                     // use UTF-8 when encoding is omitted/null
    emitter.instruction(&format!("cmp rdi, {}", MAX_ENCODING_NAME_LEN));        // does the encoding name fit the stack C-string buffer?
    emitter.instruction("ja __rt_mb_strimwidth_unknown_encoding_x86");          // reject names longer than every supported alias
    emitter.instruction("lea rsi, [rbp - 208]");                                // destination is the 64-byte encoding-name buffer
    emitter.instruction("xor rcx, rcx");                                        // copied-byte index starts at zero
    emitter.label("__rt_mb_strimwidth_encoding_copy_x86");
    emitter.instruction("cmp rcx, rdi");                                        // copied the whole explicit encoding name?
    emitter.instruction("jae __rt_mb_strimwidth_encoding_copied_x86");          // terminate the C string once every byte is copied
    emitter.instruction("mov r10b, BYTE PTR [r11 + rcx]");                      // load one encoding-name byte from the PHP string
    emitter.instruction("mov BYTE PTR [rsi + rcx], r10b");                      // append the byte to the stack C string
    emitter.instruction("inc rcx");                                             // advance the encoding-name byte index
    emitter.instruction("jmp __rt_mb_strimwidth_encoding_copy_x86");            // continue copying the remaining encoding-name bytes
    emitter.label("__rt_mb_strimwidth_encoding_copied_x86");
    emitter.instruction("mov BYTE PTR [rsi + rdi], 0");                         // NUL-terminate the explicit encoding name
    emitter.instruction("lea rdi, [rbp - 208]");                                // first strcasecmp argument is the copied encoding name
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_utf8_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with UTF-8
    emitter.instruction("test eax, eax");                                       // did the encoding match UTF-8?
    emitter.instruction("jz __rt_mb_strimwidth_count_x86");                     // UTF-8 uses the display-width scanner
    emitter.instruction("lea rdi, [rbp - 208]");                                // reload the copied encoding name
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_utf8_alias");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with UTF8
    emitter.instruction("test eax, eax");                                       // did the encoding match UTF8?
    emitter.instruction("jz __rt_mb_strimwidth_count_x86");                     // the UTF8 alias uses the same scanner
    emitter.instruction("lea rdi, [rbp - 208]");                                // reload the copied encoding name
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_8bit_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with 8bit
    emitter.instruction("test eax, eax");                                       // did the encoding match 8bit?
    emitter.instruction("jz __rt_mb_strimwidth_byte_mode_x86");                 // 8bit treats every byte as width 1
    emitter.instruction("lea rdi, [rbp - 208]");                                // reload the copied encoding name
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_binary_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with binary
    emitter.instruction("test eax, eax");                                       // did the encoding match binary?
    emitter.instruction("jz __rt_mb_strimwidth_byte_mode_x86");                 // binary is PHP's alias for 8bit
    emitter.instruction("lea rdi, [rbp - 208]");                                // reload the copied encoding name
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_7bit_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with 7bit
    emitter.instruction("test eax, eax");                                       // did the encoding match 7bit?
    emitter.instruction("jz __rt_mb_strimwidth_byte_mode_x86");                 // 7bit preserves one-character-per-byte width
    emitter.instruction("jmp __rt_mb_strimwidth_unknown_encoding_x86");         // any other name is an unknown encoding

    emitter.label("__rt_mb_strimwidth_byte_mode_x86");
    emitter.instruction("mov QWORD PTR [rbp - 112], 1");                        // mark the scanner as byte-width mode

    emitter.label("__rt_mb_strimwidth_count_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // count characters in the whole source
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // pass the source length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 112]");                      // pass the width mode
    emitter.instruction("call __rt_mb_strimwidth_count_chars_x86");             // rax = character count
    emitter.instruction("mov r12, rax");                                        // r12 = character count
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the signed start offset
    emitter.instruction("test rax, rax");                                       // is start zero?
    emitter.instruction("jz __rt_mb_strimwidth_start_ok_x86");                  // start 0 never needs a range check
    emitter.instruction("cmp rax, 0");                                          // is start negative?
    emitter.instruction("jge __rt_mb_strimwidth_start_nonneg_x86");             // a non-negative start is already an origin offset
    emitter.instruction("add rax, r12");                                        // negative start counts from the end
    emitter.label("__rt_mb_strimwidth_start_nonneg_x86");
    emitter.instruction("cmp rax, 0");                                          // did a negative start underflow the string?
    emitter.instruction("jl __rt_mb_strimwidth_start_error_x86");               // start below zero is out of range
    emitter.instruction("cmp rax, r12");                                        // is start past the last character?
    emitter.instruction("jg __rt_mb_strimwidth_start_error_x86");               // start > char_count is out of range
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // persist the resolved non-negative start
    emitter.label("__rt_mb_strimwidth_start_ok_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                         // is width negative?
    emitter.instruction("jge __rt_mb_strimwidth_width_ok_x86");                 // a non-negative width is already a remaining budget
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // measure the whole string's display width
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // pass the source length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 112]");                      // pass the width mode
    emitter.instruction("call __rt_mb_strimwidth_strwidth_x86");                // rax = total display width
    emitter.instruction("add QWORD PTR [rbp - 72], rax");                       // PHP adds the whole-string width first
    emitter.instruction("cmp QWORD PTR [rbp - 64], 0");                         // is start greater than zero?
    emitter.instruction("jle __rt_mb_strimwidth_neg_width_checked_x86");        // start 0 has no prefix width to subtract
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // skip the prefix so we can measure it
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // pass the source length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // skip this many characters
    emitter.instruction("mov rcx, QWORD PTR [rbp - 112]");                      // pass the width mode
    emitter.instruction("call __rt_mb_strimwidth_skip_x86");                    // rax = prefix byte length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // prefix starts at the source pointer
    emitter.instruction("mov rsi, rax");                                        // measure only the skipped prefix
    emitter.instruction("mov rdx, QWORD PTR [rbp - 112]");                      // pass the width mode
    emitter.instruction("call __rt_mb_strimwidth_strwidth_x86");                // rax = prefix display width
    emitter.instruction("sub QWORD PTR [rbp - 72], rax");                       // subtract the skipped prefix width
    emitter.label("__rt_mb_strimwidth_neg_width_checked_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                         // did the adjusted width underflow?
    emitter.instruction("jl __rt_mb_strimwidth_width_error_x86");               // a still-negative width is out of range
    emitter.label("__rt_mb_strimwidth_width_ok_x86");

    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // skip `start` characters to find the kept suffix
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // pass the source length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // pass the resolved start
    emitter.instruction("mov rcx, QWORD PTR [rbp - 112]");                      // pass the width mode
    emitter.instruction("call __rt_mb_strimwidth_skip_x86");                    // rax = start byte offset
    emitter.instruction("mov QWORD PTR [rbp - 120], rax");                      // persist the suffix origin
    emitter.instruction("mov rbx, QWORD PTR [rbp - 48]");                       // reload the source pointer
    emitter.instruction("add rbx, rax");                                        // rbx = suffix pointer
    emitter.instruction("mov r12, QWORD PTR [rbp - 56]");                       // reload the source length
    emitter.instruction("sub r12, rax");                                        // r12 = suffix byte length
    emitter.instruction("mov rdi, rbx");                                        // measure the suffix display width
    emitter.instruction("mov rsi, r12");                                        // pass the suffix length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 112]");                      // pass the width mode
    emitter.instruction("call __rt_mb_strimwidth_strwidth_x86");                // rax = suffix display width
    emitter.instruction("cmp rax, QWORD PTR [rbp - 72]");                       // does the suffix already fit?
    emitter.instruction("jbe __rt_mb_strimwidth_copy_suffix_x86");              // yes → return the suffix unchanged
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // measure the trim marker's display width
    emitter.instruction("mov rsi, QWORD PTR [rbp - 88]");                       // pass the marker length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 112]");                      // pass the width mode
    emitter.instruction("call __rt_mb_strimwidth_strwidth_x86");                // rax = marker display width
    emitter.instruction("cmp QWORD PTR [rbp - 72], rax");                       // is the budget no larger than the marker?
    emitter.instruction("jbe __rt_mb_strimwidth_copy_marker_x86");              // yes → PHP returns the marker alone
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // remaining budget starts as the requested width
    emitter.instruction("sub rdx, rax");                                        // subtract the marker width
    emitter.instruction("mov rdi, rbx");                                        // take a prefix of the suffix
    emitter.instruction("mov rsi, r12");                                        // pass the suffix length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 112]");                      // pass the width mode
    emitter.instruction("call __rt_mb_strimwidth_take_x86");                    // rax = kept source byte count
    emitter.instruction("mov r13, rax");                                        // r13 = kept source bytes
    emitter.instruction("mov rax, r13");                                        // reserve source prefix plus marker
    emitter.instruction("add rax, QWORD PTR [rbp - 88]");                       // add the marker length
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage
    emitter.instruction("mov QWORD PTR [rbp - 128], rax");                      // persist the result start pointer
    emitter.instruction("mov rdi, rax");                                        // memcpy destination is the reservation
    emitter.instruction("mov rsi, rbx");                                        // memcpy source is the suffix
    emitter.instruction("mov rdx, r13");                                        // memcpy length is the kept prefix
    emitter.instruction("call memmove");                                        // copy the kept source prefix into the reservation
    emitter.instruction("mov rdi, QWORD PTR [rbp - 128]");                      // reload the result start
    emitter.instruction("add rdi, r13");                                        // destination for the trim marker
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // memcpy source is the marker
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // memcpy length is the marker length
    emitter.instruction("call memmove");                                        // append the trim marker
    emitter.instruction("mov rax, QWORD PTR [rbp - 128]");                      // result pointer is the reservation start
    emitter.instruction("mov rdx, r13");                                        // start with the kept source bytes
    emitter.instruction("add rdx, QWORD PTR [rbp - 88]");                       // add the marker length
    emitter.instruction("call __rt_concat_publish");                            // publish the concat-scratch write offset
    emitter.instruction("jmp __rt_mb_strimwidth_return_x86");                   // restore callee-saved regs and return

    emitter.label("__rt_mb_strimwidth_copy_suffix_x86");
    emitter.instruction("mov rax, r12");                                        // reserve exactly the suffix bytes
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage
    emitter.instruction("mov QWORD PTR [rbp - 128], rax");                      // persist the result start pointer
    emitter.instruction("mov rdi, rax");                                        // memcpy destination is the reservation
    emitter.instruction("mov rsi, rbx");                                        // memcpy source is the suffix
    emitter.instruction("mov rdx, r12");                                        // memcpy length is the suffix length
    emitter.instruction("call memmove");                                        // copy the untrimmed suffix
    emitter.instruction("mov rax, QWORD PTR [rbp - 128]");                      // result pointer is the reservation start
    emitter.instruction("mov rdx, r12");                                        // result length is the suffix length
    emitter.instruction("call __rt_concat_publish");                            // publish the concat-scratch write offset
    emitter.instruction("jmp __rt_mb_strimwidth_return_x86");                   // restore callee-saved regs and return

    emitter.label("__rt_mb_strimwidth_copy_marker_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // reserve exactly the marker bytes
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage
    emitter.instruction("mov QWORD PTR [rbp - 128], rax");                      // persist the result start pointer
    emitter.instruction("mov rdi, rax");                                        // memcpy destination is the reservation
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // memcpy source is the marker
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // memcpy length is the marker length
    emitter.instruction("call memmove");                                        // copy the marker alone
    emitter.instruction("mov rax, QWORD PTR [rbp - 128]");                      // result pointer is the reservation start
    emitter.instruction("mov rdx, QWORD PTR [rbp - 88]");                       // result length is the marker length
    emitter.instruction("call __rt_concat_publish");                            // publish the concat-scratch write offset

    emitter.label("__rt_mb_strimwidth_return_x86");
    emitter.instruction("add rsp, 192");                                        // release the argument-spill area
    emitter.instruction("pop r15");                                             // restore callee-saved r15
    emitter.instruction("pop r14");                                             // restore callee-saved r14
    emitter.instruction("pop r13");                                             // restore callee-saved r13
    emitter.instruction("pop r12");                                             // restore callee-saved r12
    emitter.instruction("pop rbx");                                             // restore callee-saved rbx
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the trimmed string pointer/length

    emitter.label("__rt_mb_strimwidth_unknown_encoding_x86");
    emitter.instruction("add rsp, 192");                                        // release the helper frame before throwing
    emitter.instruction("pop r15");                                             // restore callee-saved r15
    emitter.instruction("pop r14");                                             // restore callee-saved r14
    emitter.instruction("pop r13");                                             // restore callee-saved r13
    emitter.instruction("pop r12");                                             // restore callee-saved r12
    emitter.instruction("pop rbx");                                             // restore callee-saved rbx
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    value_error::emit_throw_value_error_x86_64(
        emitter,
        "_mb_strimwidth_unknown_encoding_msg",
        MB_STRIMWIDTH_UNKNOWN_ENCODING_MSG.len(),
    );

    emitter.label("__rt_mb_strimwidth_start_error_x86");
    emitter.instruction("add rsp, 192");                                        // release the helper frame before throwing
    emitter.instruction("pop r15");                                             // restore callee-saved r15
    emitter.instruction("pop r14");                                             // restore callee-saved r14
    emitter.instruction("pop r13");                                             // restore callee-saved r13
    emitter.instruction("pop r12");                                             // restore callee-saved r12
    emitter.instruction("pop rbx");                                             // restore callee-saved rbx
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    value_error::emit_throw_value_error_x86_64(
        emitter,
        "_mb_strimwidth_start_range_msg",
        MB_STRIMWIDTH_START_RANGE_MSG.len(),
    );

    emitter.label("__rt_mb_strimwidth_width_error_x86");
    emitter.instruction("add rsp, 192");                                        // release the helper frame before throwing
    emitter.instruction("pop r15");                                             // restore callee-saved r15
    emitter.instruction("pop r14");                                             // restore callee-saved r14
    emitter.instruction("pop r13");                                             // restore callee-saved r13
    emitter.instruction("pop r12");                                             // restore callee-saved r12
    emitter.instruction("pop rbx");                                             // restore callee-saved rbx
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    value_error::emit_throw_value_error_x86_64(
        emitter,
        "_mb_strimwidth_width_range_msg",
        MB_STRIMWIDTH_WIDTH_RANGE_MSG.len(),
    );

    emit_mb_strimwidth_walkers_x86_64(emitter);
}

/// Emits x86_64 character-walk helpers shared by the trim helper.
fn emit_mb_strimwidth_walkers_x86_64(emitter: &mut Emitter) {
    emitter.label_shared("__rt_mb_strimwidth_count_chars_x86");
    emitter.instruction("test rdx, rdx");                                       // byte mode returns the raw length
    emitter.instruction("jnz __rt_mb_strimwidth_count_bytes_x86");              // return rsi unchanged as the count
    emitter.instruction("push rbx");                                            // preserve callee-saved rbx
    emitter.instruction("push r12");                                            // preserve the scan pointer owner
    emitter.instruction("push r13");                                            // preserve the end pointer owner
    emitter.instruction("mov r12, rdi");                                        // r12 = scan pointer
    emitter.instruction("lea r13, [rdi + rsi]");                                // r13 = one-past-end pointer
    emitter.instruction("xor rbx, rbx");                                        // character count starts at zero
    emitter.label("__rt_mb_strimwidth_count_loop_x86");
    emitter.instruction("cmp r12, r13");                                        // scanned every byte?
    emitter.instruction("jae __rt_mb_strimwidth_count_done_x86");               // return the accumulated count
    emitter.instruction("call __rt_mb_strimwidth_next_x86");                    // r12 advances past one character
    emitter.instruction("inc rbx");                                             // count one more character
    emitter.instruction("jmp __rt_mb_strimwidth_count_loop_x86");               // keep scanning
    emitter.label("__rt_mb_strimwidth_count_done_x86");
    emitter.instruction("mov rax, rbx");                                        // return the character count
    emitter.instruction("pop r13");                                             // restore r13
    emitter.instruction("pop r12");                                             // restore r12
    emitter.instruction("pop rbx");                                             // restore rbx
    emitter.instruction("ret");                                                 // return the character count
    emitter.label("__rt_mb_strimwidth_count_bytes_x86");
    emitter.instruction("mov rax, rsi");                                        // byte encodings count one character per byte
    emitter.instruction("ret");                                                 // return the byte length

    emitter.label_shared("__rt_mb_strimwidth_strwidth_x86");
    emitter.instruction("test rdx, rdx");                                       // byte mode returns the raw length
    emitter.instruction("jnz __rt_mb_strimwidth_strwidth_bytes_x86");           // return rsi unchanged as the width
    emitter.instruction("push rbx");                                            // preserve callee-saved rbx
    emitter.instruction("push r12");                                            // preserve the scan pointer owner
    emitter.instruction("push r13");                                            // preserve the end pointer owner
    emitter.instruction("mov r12, rdi");                                        // r12 = scan pointer
    emitter.instruction("lea r13, [rdi + rsi]");                                // r13 = one-past-end pointer
    emitter.instruction("xor rbx, rbx");                                        // display width starts at zero
    emitter.label("__rt_mb_strimwidth_strwidth_loop_x86");
    emitter.instruction("cmp r12, r13");                                        // scanned every byte?
    emitter.instruction("jae __rt_mb_strimwidth_strwidth_done_x86");            // return the accumulated width
    emitter.instruction("call __rt_mb_strimwidth_next_x86");                    // eax = codepoint, r12 advances
    emitter.instruction("call __rt_mb_strimwidth_char_width_x86");              // ecx = 1 or 2
    emitter.instruction("add rbx, rcx");                                        // accumulate the character width
    emitter.instruction("jmp __rt_mb_strimwidth_strwidth_loop_x86");            // keep scanning
    emitter.label("__rt_mb_strimwidth_strwidth_done_x86");
    emitter.instruction("mov rax, rbx");                                        // return the display width
    emitter.instruction("pop r13");                                             // restore r13
    emitter.instruction("pop r12");                                             // restore r12
    emitter.instruction("pop rbx");                                             // restore rbx
    emitter.instruction("ret");                                                 // return the display width
    emitter.label("__rt_mb_strimwidth_strwidth_bytes_x86");
    emitter.instruction("mov rax, rsi");                                        // byte encodings have width equal to length
    emitter.instruction("ret");                                                 // return the byte length

    emitter.label_shared("__rt_mb_strimwidth_skip_x86");
    emitter.instruction("test rcx, rcx");                                       // byte mode skips `rdx` bytes
    emitter.instruction("jnz __rt_mb_strimwidth_skip_bytes_x86");               // clamp the byte skip to the string length
    emitter.instruction("push rbx");                                            // preserve callee-saved rbx
    emitter.instruction("push r12");                                            // preserve the scan pointer owner
    emitter.instruction("push r13");                                            // preserve the end pointer owner
    emitter.instruction("mov r12, rdi");                                        // r12 = scan pointer
    emitter.instruction("lea r13, [rdi + rsi]");                                // r13 = one-past-end pointer
    emitter.instruction("mov rbx, rdi");                                        // remember the origin for the returned offset
    emitter.instruction("mov r8, rdx");                                         // remaining characters to skip
    emitter.label("__rt_mb_strimwidth_skip_loop_x86");
    emitter.instruction("test r8, r8");                                         // finished skipping the requested count?
    emitter.instruction("jz __rt_mb_strimwidth_skip_done_x86");                 // return the consumed byte count
    emitter.instruction("cmp r12, r13");                                        // reached the end of the string?
    emitter.instruction("jae __rt_mb_strimwidth_skip_done_x86");                // cannot skip past the last character
    emitter.instruction("push r8");                                             // preserve the remaining skip count
    emitter.instruction("call __rt_mb_strimwidth_next_x86");                    // advance one character
    emitter.instruction("pop r8");                                              // restore the remaining skip count
    emitter.instruction("dec r8");                                              // one fewer character remains
    emitter.instruction("jmp __rt_mb_strimwidth_skip_loop_x86");                // keep skipping
    emitter.label("__rt_mb_strimwidth_skip_done_x86");
    emitter.instruction("mov rax, r12");                                        // current pointer minus origin
    emitter.instruction("sub rax, rbx");                                        // return the consumed byte count
    emitter.instruction("pop r13");                                             // restore r13
    emitter.instruction("pop r12");                                             // restore r12
    emitter.instruction("pop rbx");                                             // restore rbx
    emitter.instruction("ret");                                                 // return the skip offset
    emitter.label("__rt_mb_strimwidth_skip_bytes_x86");
    emitter.instruction("cmp rdx, rsi");                                        // would the skip pass the last byte?
    emitter.instruction("cmovb rax, rdx");                                      // use the requested count when it fits
    emitter.instruction("cmovae rax, rsi");                                     // otherwise clamp to the string length
    emitter.instruction("ret");                                                 // return the clamped byte offset

    emitter.label_shared("__rt_mb_strimwidth_take_x86");
    emitter.instruction("test rcx, rcx");                                       // byte mode takes `rdx` bytes
    emitter.instruction("jnz __rt_mb_strimwidth_take_bytes_x86");               // clamp the byte take to the string length
    emitter.instruction("push rbx");                                            // preserve callee-saved rbx
    emitter.instruction("push r12");                                            // preserve the scan pointer owner
    emitter.instruction("push r13");                                            // preserve the end pointer owner
    emitter.instruction("mov r12, rdi");                                        // r12 = scan pointer
    emitter.instruction("lea r13, [rdi + rsi]");                                // r13 = one-past-end pointer
    emitter.instruction("mov rbx, rdi");                                        // remember the origin for the returned length
    emitter.instruction("mov r8, rdx");                                         // remaining display-width budget
    emitter.label("__rt_mb_strimwidth_take_loop_x86");
    emitter.instruction("cmp r12, r13");                                        // reached the end of the string?
    emitter.instruction("jae __rt_mb_strimwidth_take_done_x86");                // take the whole suffix
    emitter.instruction("push r8");                                             // preserve the remaining budget
    emitter.instruction("push r12");                                            // remember the pointer before this character
    emitter.instruction("call __rt_mb_strimwidth_next_x86");                    // eax = codepoint, r12 tentatively advanced
    emitter.instruction("call __rt_mb_strimwidth_char_width_x86");              // ecx = 1 or 2
    emitter.instruction("pop r9");                                              // r9 = pointer before this character
    emitter.instruction("pop r8");                                              // restore the remaining budget
    emitter.instruction("cmp r8, rcx");                                         // does the character still fit the budget?
    emitter.instruction("jb __rt_mb_strimwidth_take_reject_x86");               // no → leave this character out
    emitter.instruction("sub r8, rcx");                                         // consume the character's display width
    emitter.instruction("jmp __rt_mb_strimwidth_take_loop_x86");                // keep taking
    emitter.label("__rt_mb_strimwidth_take_reject_x86");
    emitter.instruction("mov r12, r9");                                         // rewind to exclude the overflowing character
    emitter.label("__rt_mb_strimwidth_take_done_x86");
    emitter.instruction("mov rax, r12");                                        // current pointer minus origin
    emitter.instruction("sub rax, rbx");                                        // return the kept byte count
    emitter.instruction("pop r13");                                             // restore r13
    emitter.instruction("pop r12");                                             // restore r12
    emitter.instruction("pop rbx");                                             // restore rbx
    emitter.instruction("ret");                                                 // return the taken prefix length
    emitter.label("__rt_mb_strimwidth_take_bytes_x86");
    emitter.instruction("cmp rdx, rsi");                                        // would the take pass the last byte?
    emitter.instruction("cmovb rax, rdx");                                      // use the requested count when it fits
    emitter.instruction("cmovae rax, rsi");                                     // otherwise clamp to the string length
    emitter.instruction("ret");                                                 // return the clamped byte count

    emitter.label_shared("__rt_mb_strimwidth_next_x86");
    emitter.instruction("movzx eax, BYTE PTR [r12]");                           // load the next possible UTF-8 leading byte
    emitter.instruction("cmp al, 0x80");                                        // ASCII bytes are complete one-byte characters
    emitter.instruction("jb __rt_mb_strimwidth_next_ascii_x86");                // consume one ASCII byte
    emitter.instruction("cmp al, 0xc2");                                        // C0/C1 and continuation bytes are malformed leaders
    emitter.instruction("jb __rt_mb_strimwidth_next_invalid_x86");              // substitute one malformed byte
    emitter.instruction("cmp al, 0xe0");                                        // C2-DF introduce two-byte sequences
    emitter.instruction("jb __rt_mb_strimwidth_next_two_x86");                  // validate a two-byte character
    emitter.instruction("cmp al, 0xf0");                                        // E0-EF introduce three-byte sequences
    emitter.instruction("jb __rt_mb_strimwidth_next_three_x86");                // validate a three-byte character
    emitter.instruction("cmp al, 0xf5");                                        // F0-F4 introduce Unicode-range four-byte sequences
    emitter.instruction("jb __rt_mb_strimwidth_next_four_x86");                 // validate a four-byte character
    emitter.instruction("jmp __rt_mb_strimwidth_next_invalid_x86");             // F5-FF cannot begin valid UTF-8
    emitter.label("__rt_mb_strimwidth_next_ascii_x86");
    emitter.instruction("inc r12");                                             // consume the one-byte character
    emitter.instruction("ret");                                                 // eax already holds the ASCII code point
    emitter.label("__rt_mb_strimwidth_next_invalid_x86");
    emitter.instruction("mov eax, 0xffffffff");                                 // malformed bytes have no Unicode scalar
    emitter.instruction("inc r12");                                             // substitute one malformed byte
    emitter.instruction("ret");                                                 // return the substitution character
    emitter.label("__rt_mb_strimwidth_next_two_x86");
    emitter.instruction("lea r10, [r12 + 2]");                                  // two-byte sequences need one continuation
    emitter.instruction("cmp r10, r13");                                        // is the sequence truncated?
    emitter.instruction("ja __rt_mb_strimwidth_next_trunc_x86");                // group the truncated prefix as one character
    emitter.instruction("movzx r11d, BYTE PTR [r12 + 1]");                      // load the continuation byte
    emitter.instruction("mov r10d, r11d");                                      // copy the continuation for the prefix check
    emitter.instruction("and r10d, 0xc0");                                      // isolate the continuation-byte prefix
    emitter.instruction("cmp r10d, 0x80");                                      // does the second byte have the required 10xxxxxx shape?
    emitter.instruction("jne __rt_mb_strimwidth_next_invalid_x86");             // malformed continuation leaves the leader substituted
    emitter.instruction("and eax, 0x1f");                                       // keep the two-byte payload bits
    emitter.instruction("shl eax, 6");                                          // shift the leader payload into place
    emitter.instruction("and r11d, 0x3f");                                      // keep the continuation payload bits
    emitter.instruction("or eax, r11d");                                        // assemble the two-byte code point
    emitter.instruction("add r12, 2");                                          // consume the complete two-byte character
    emitter.instruction("ret");                                                 // return the two-byte character
    emitter.label("__rt_mb_strimwidth_next_three_x86");
    emitter.instruction("lea r10, [r12 + 3]");                                  // three-byte sequences need two continuations
    emitter.instruction("cmp r10, r13");                                        // is the sequence truncated?
    emitter.instruction("ja __rt_mb_strimwidth_next_trunc_x86");                // group the truncated prefix as one character
    emitter.instruction("movzx r10d, BYTE PTR [r12 + 1]");                      // load the first continuation byte
    emitter.instruction("movzx r11d, BYTE PTR [r12 + 2]");                      // load the second continuation byte
    emitter.instruction("mov r9d, r10d");                                       // copy the first continuation for the prefix check
    emitter.instruction("and r9d, 0xc0");                                       // isolate the first continuation prefix
    emitter.instruction("cmp r9d, 0x80");                                       // is the first continuation structurally valid?
    emitter.instruction("jne __rt_mb_strimwidth_next_invalid_x86");             // malformed continuation substitutes only the leader
    emitter.instruction("mov r9d, r11d");                                       // copy the second continuation for the prefix check
    emitter.instruction("and r9d, 0xc0");                                       // isolate the second continuation prefix
    emitter.instruction("cmp r9d, 0x80");                                       // is the second continuation structurally valid?
    emitter.instruction("jne __rt_mb_strimwidth_next_invalid_x86");             // malformed final byte substitutes only the leader
    emitter.instruction("and eax, 0x0f");                                       // keep the three-byte leader payload
    emitter.instruction("shl eax, 12");                                         // shift the leader payload into place
    emitter.instruction("and r10d, 0x3f");                                      // keep the first continuation payload
    emitter.instruction("shl r10d, 6");                                         // shift the first continuation into place
    emitter.instruction("and r11d, 0x3f");                                      // keep the second continuation payload
    emitter.instruction("or eax, r10d");                                        // merge leader and first continuation
    emitter.instruction("or eax, r11d");                                        // assemble the three-byte code point
    emitter.instruction("add r12, 3");                                          // consume the complete three-byte character
    emitter.instruction("ret");                                                 // return the three-byte character
    emitter.label("__rt_mb_strimwidth_next_four_x86");
    emitter.instruction("lea r10, [r12 + 4]");                                  // four-byte sequences need three continuations
    emitter.instruction("cmp r10, r13");                                        // is the sequence truncated?
    emitter.instruction("ja __rt_mb_strimwidth_next_trunc_x86");                // group the truncated prefix as one character
    emitter.instruction("movzx r10d, BYTE PTR [r12 + 1]");                      // load the first continuation byte
    emitter.instruction("movzx r11d, BYTE PTR [r12 + 2]");                      // load the second continuation byte
    emitter.instruction("movzx r9d, BYTE PTR [r12 + 3]");                       // load the third continuation byte
    emitter.instruction("mov r8d, r10d");                                       // copy the first continuation for the prefix check
    emitter.instruction("and r8d, 0xc0");                                       // isolate the first continuation prefix
    emitter.instruction("cmp r8d, 0x80");                                       // is the first continuation structurally valid?
    emitter.instruction("jne __rt_mb_strimwidth_next_invalid_x86");             // malformed continuation substitutes only the leader
    emitter.instruction("mov r8d, r11d");                                       // copy the second continuation for the prefix check
    emitter.instruction("and r8d, 0xc0");                                       // isolate the second continuation prefix
    emitter.instruction("cmp r8d, 0x80");                                       // is the second continuation structurally valid?
    emitter.instruction("jne __rt_mb_strimwidth_next_invalid_x86");             // malformed middle byte substitutes only the leader
    emitter.instruction("mov r8d, r9d");                                        // copy the third continuation for the prefix check
    emitter.instruction("and r8d, 0xc0");                                       // isolate the third continuation prefix
    emitter.instruction("cmp r8d, 0x80");                                       // is the third continuation structurally valid?
    emitter.instruction("jne __rt_mb_strimwidth_next_invalid_x86");             // malformed final byte substitutes only the leader
    emitter.instruction("and eax, 0x07");                                       // keep the four-byte leader payload
    emitter.instruction("shl eax, 18");                                         // shift the leader payload into place
    emitter.instruction("and r10d, 0x3f");                                      // keep the first continuation payload
    emitter.instruction("shl r10d, 12");                                        // shift the first continuation into place
    emitter.instruction("and r11d, 0x3f");                                      // keep the second continuation payload
    emitter.instruction("shl r11d, 6");                                         // shift the second continuation into place
    emitter.instruction("and r9d, 0x3f");                                       // keep the third continuation payload
    emitter.instruction("or eax, r10d");                                        // merge leader and first continuation
    emitter.instruction("or eax, r11d");                                        // merge the second continuation
    emitter.instruction("or eax, r9d");                                         // assemble the four-byte code point
    emitter.instruction("add r12, 4");                                          // consume the complete four-byte character
    emitter.instruction("ret");                                                 // return the four-byte character
    emitter.label("__rt_mb_strimwidth_next_trunc_x86");
    emitter.instruction("mov eax, 0xffffffff");                                 // a truncated prefix is one substitution character
    emitter.instruction("mov r12, r13");                                        // consume the remaining suffix bytes
    emitter.instruction("ret");                                                 // return the truncated-prefix character

    emitter.label_shared("__rt_mb_strimwidth_char_width_x86");
    emitter.instruction("cmp eax, 0x1100");                                     // code points below U+1100 are never fullwidth
    emitter.instruction("jb __rt_mb_strimwidth_width_one_x86");                 // return width 1 for the fast path
    abi::emit_symbol_address(emitter, "r10", "_mb_eaw_table");
    abi::emit_symbol_address(emitter, "r11", "_mb_eaw_table_count");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the number of inclusive width-2 ranges
    emitter.instruction("xor r8, r8");                                          // binary-search low index starts at zero
    emitter.label("__rt_mb_strimwidth_width_search_x86");
    emitter.instruction("cmp r8, r11");                                         // has the search range emptied?
    emitter.instruction("jae __rt_mb_strimwidth_width_one_x86");                // no range contained the code point
    emitter.instruction("mov r9, r8");                                          // probe = lo
    emitter.instruction("add r9, r11");                                         // probe = lo + hi
    emitter.instruction("shr r9, 1");                                           // probe = (lo + hi) / 2
    emitter.instruction("mov r14d, DWORD PTR [r10 + r9 * 8]");                  // load the inclusive range begin
    emitter.instruction("mov r15d, DWORD PTR [r10 + r9 * 8 + 4]");              // load the inclusive range end
    emitter.instruction("cmp eax, r14d");                                       // is the code point before this range?
    emitter.instruction("jb __rt_mb_strimwidth_width_left_x86");                // search the lower half
    emitter.instruction("cmp eax, r15d");                                       // is the code point after this range?
    emitter.instruction("ja __rt_mb_strimwidth_width_right_x86");               // search the upper half
    emitter.instruction("mov ecx, 2");                                          // a containing range means display width 2
    emitter.instruction("ret");                                                 // return the fullwidth result
    emitter.label("__rt_mb_strimwidth_width_left_x86");
    emitter.instruction("mov r11, r9");                                         // hi = probe
    emitter.instruction("jmp __rt_mb_strimwidth_width_search_x86");             // continue the binary search
    emitter.label("__rt_mb_strimwidth_width_right_x86");
    emitter.instruction("lea r8, [r9 + 1]");                                    // lo = probe + 1
    emitter.instruction("jmp __rt_mb_strimwidth_width_search_x86");             // continue the binary search
    emitter.label("__rt_mb_strimwidth_width_one_x86");
    emitter.instruction("mov ecx, 1");                                          // every other code point has display width 1
    emitter.instruction("ret");                                                 // return the halfwidth result
}