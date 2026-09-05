//! Purpose:
//! Emits `__rt_mb_strwidth` and `__rt_mb_char_width` for PHP's `mb_strwidth()`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//! - `crate::codegen::lower_inst::builtins::strings::lower_mb_strwidth()`.
//!
//! Key details:
//! - Encoding selection matches `mb_strlen()`: omitted/null and UTF-8 aliases use the
//!   validated UTF-8 scanner; `8bit`/`binary`/`7bit` return the byte length; other
//!   names decode through libc `iconv` into UTF-32LE.
//! - Each decoded scalar uses PHP 8.5 East Asian Width; malformed/truncated input
//!   adds width 1, matching mbstring's bad-input marker.
//! - Unknown encodings throw a catchable `ValueError` through the normal unwinder.

use crate::codegen_support::{
    abi,
    emit::Emitter,
    platform::{Arch, Platform},
    runtime::{arrays::value_error, data::MB_STRWIDTH_UNKNOWN_ENCODING_MSG},
};

/// Maximum explicit encoding-name length copied into the runtime's stack buffer.
const MAX_ENCODING_NAME_LEN: usize = 63;

/// Number of inclusive East Asian Width ranges in PHP 8.5's `mbfl_eaw_table`.
const EAW_RANGE_COUNT: usize = elephc_builtin_contract::EAW_RANGES.len();

/// Emits `__rt_mb_strwidth(str_ptr, str_len, encoding_ptr, encoding_len) -> width`.
pub fn emit_mb_strwidth(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mb_char_width_x86_64(emitter);
        emit_mb_strwidth_x86_64(emitter);
    } else {
        emit_mb_char_width_aarch64(emitter);
        emit_mb_strwidth_aarch64(emitter);
    }
}

/// Emits the AArch64 PHP 8.5 East Asian Width lookup (`x0` in, width in `x0`).
fn emit_mb_char_width_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_char_width (PHP 8.5 East Asian Width) ---");
    emitter.label_global("__rt_mb_char_width");
    emitter.instruction("mov w1, #0x1100");                                     // first code point that can be double-width
    emitter.instruction("cmp w0, w1");                                          // is this code point below every wide range?
    emitter.instruction("b.lo __rt_mb_char_width_narrow");                      // ASCII and early BMP are always width 1
    emitter.instruction("mov w1, #0");                                          // binary-search low index starts at zero
    emitter.instruction(&format!("mov w2, #{}", EAW_RANGE_COUNT));              // binary-search high index starts past the last range
    abi::emit_symbol_address(emitter, "x3", "_mb_eaw_table");
    emitter.label("__rt_mb_char_width_loop");
    emitter.instruction("cmp w1, w2");                                          // have the remaining ranges collapsed?
    emitter.instruction("b.hs __rt_mb_char_width_narrow");                      // a miss is a halfwidth character
    emitter.instruction("add w4, w1, w2");                                      // probe is the midpoint of the remaining window
    emitter.instruction("lsr w4, w4, #1");                                      // integer midpoint index
    emitter.instruction("ubfiz x5, x4, #3, #32");                               // each table entry is two 32-bit bounds
    emitter.instruction("ldr w6, [x3, x5]");                                    // load the inclusive range begin
    emitter.instruction("cmp w0, w6");                                          // is the code point before this range?
    emitter.instruction("b.lo __rt_mb_char_width_hi");                          // search the lower half
    emitter.instruction("add x5, x5, #4");                                      // address the inclusive range end
    emitter.instruction("ldr w6, [x3, x5]");                                    // load the inclusive range end
    emitter.instruction("cmp w0, w6");                                          // is the code point after this range?
    emitter.instruction("b.hi __rt_mb_char_width_lo");                          // search the upper half
    emitter.instruction("mov w0, #2");                                          // a hit is a fullwidth / wide character
    emitter.instruction("ret");                                                 // return width 2
    emitter.label("__rt_mb_char_width_hi");
    emitter.instruction("mov w2, w4");                                          // shrink the high bound to the current probe
    emitter.instruction("b __rt_mb_char_width_loop");                           // continue the binary search
    emitter.label("__rt_mb_char_width_lo");
    emitter.instruction("add w1, w4, #1");                                      // skip the probe that ended below the code point
    emitter.instruction("b __rt_mb_char_width_loop");                           // continue the binary search
    emitter.label("__rt_mb_char_width_narrow");
    emitter.instruction("mov w0, #1");                                          // every other scalar is halfwidth
    emitter.instruction("ret");                                                 // return width 1
}

/// Emits the AArch64 implementation for macOS and Linux.
fn emit_mb_strwidth_aarch64(emitter: &mut Emitter) {
    let errno_function = match emitter.platform {
        Platform::MacOS => "__error",
        Platform::Linux => "__errno_location",
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    };

    emitter.blank();
    emitter.comment("--- runtime: mb_strwidth (encoding-aware display width) ---");
    emitter.label_global("__rt_mb_strwidth");
    emitter.instruction("cbz x3, __rt_mb_strwidth_utf8");                       // omitted/null encoding uses the default UTF-8 scanner
    emitter.instruction(&format!("cmp x4, #{}", MAX_ENCODING_NAME_LEN));        // does the explicit encoding name fit the stack C-string buffer?
    emitter.instruction("b.hi __rt_mb_strwidth_unknown_encoding");              // reject names longer than every PHP-supported encoding alias
    emitter.instruction("sub sp, sp, #176");                                    // reserve iconv state, output scratch, and encoding-name storage
    emitter.instruction("stp x29, x30, [sp, #160]");                            // preserve the caller frame and return address across libc calls
    emitter.instruction("add x29, sp, #160");                                   // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // iconv input pointer variable starts at the PHP string bytes
    emitter.instruction("str x2, [sp, #8]");                                    // iconv input byte count variable starts at the PHP string length
    emitter.instruction("str xzr, [sp, #16]");                                  // decoded display width starts at zero

    // -- copy the length-delimited PHP encoding name into a stack C string --
    emitter.instruction("add x9, sp, #80");                                     // destination is the 64-byte encoding-name buffer
    emitter.instruction("mov x10, #0");                                         // copied-byte index starts at zero
    emitter.label("__rt_mb_strwidth_encoding_copy");
    emitter.instruction("cmp x10, x4");                                         // copied the whole explicit encoding name?
    emitter.instruction("b.hs __rt_mb_strwidth_encoding_copied");               // terminate the C string once every byte is copied
    emitter.instruction("ldrb w11, [x3, x10]");                                 // load one encoding-name byte from the PHP string
    emitter.instruction("strb w11, [x9, x10]");                                 // append the byte to the stack C string
    emitter.instruction("add x10, x10, #1");                                    // advance the encoding-name byte index
    emitter.instruction("b __rt_mb_strwidth_encoding_copy");                    // continue copying the remaining encoding-name bytes
    emitter.label("__rt_mb_strwidth_encoding_copied");
    emitter.instruction("strb wzr, [x9, x4]");                                  // NUL-terminate the explicit encoding name

    // -- fast-path PHP's default UTF-8 names and byte-count encodings --
    emitter.instruction("add x0, sp, #80");                                     // first strcasecmp argument is the copied encoding name
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_utf8_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with UTF-8 case-insensitively
    emitter.instruction("cbz x0, __rt_mb_strwidth_use_utf8_framed");            // UTF-8 uses the allocation-free validated scanner
    emitter.instruction("add x0, sp, #80");                                     // reload the copied encoding name after strcasecmp
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_utf8_alias");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with PHP's UTF8 alias
    emitter.instruction("cbz x0, __rt_mb_strwidth_use_utf8_framed");            // the UTF8 alias uses the same validated scanner
    emitter.instruction("add x0, sp, #80");                                     // reload the copied encoding name for the byte-count aliases
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_8bit_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with 8bit
    emitter.instruction("cbz x0, __rt_mb_strwidth_use_byte_length");            // 8bit bytes are all below the first wide code point
    emitter.instruction("add x0, sp, #80");                                     // reload the copied encoding name for the binary alias
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_binary_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with binary
    emitter.instruction("cbz x0, __rt_mb_strwidth_use_byte_length");            // binary is PHP's alias for 8bit
    emitter.instruction("add x0, sp, #80");                                     // reload the copied encoding name for the 7bit encoding
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_7bit_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with 7bit
    emitter.instruction("cbz x0, __rt_mb_strwidth_use_byte_length");            // 7bit preserves PHP's one-column-per-byte width

    // -- open a decoder from the requested encoding to fixed-width UTF-32LE --
    abi::emit_symbol_address(emitter, "x0", "_mb_strlen_utf32le_name");
    emitter.instruction("add x1, sp, #80");                                     // iconv source encoding is the copied explicit name
    emitter.bl_c("iconv_open"); // create the encoding-to-UTF-32LE conversion descriptor
    emitter.instruction("cmn x0, #1");                                          // did iconv_open return the `(iconv_t)-1` failure sentinel?
    emitter.instruction("b.eq __rt_mb_strwidth_unknown_encoding_framed");       // unknown encoding names raise PHP's ValueError
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the iconv descriptor across conversion iterations

    // -- decode chunks into 16 bytes of UTF-32LE and add each scalar's width --
    emitter.label("__rt_mb_strwidth_iconv_loop");
    emitter.instruction("ldr x9, [sp, #8]");                                    // load the number of input bytes still undecoded
    emitter.instruction("cbz x9, __rt_mb_strwidth_iconv_done");                 // close the descriptor after all bytes are consumed
    emitter.instruction("add x9, sp, #48");                                     // point at the fixed 16-byte UTF-32LE output scratch
    emitter.instruction("str x9, [sp, #32]");                                   // initialize iconv's mutable output pointer
    emitter.instruction("mov x9, #16");                                         // each conversion iteration has 16 output bytes available
    emitter.instruction("str x9, [sp, #40]");                                   // initialize iconv's mutable output-byte count
    emitter.instruction("ldr x0, [sp, #24]");                                   // iconv argument 0 is the conversion descriptor
    emitter.instruction("add x1, sp, #0");                                      // iconv argument 1 is `&input_ptr`
    emitter.instruction("add x2, sp, #8");                                      // iconv argument 2 is `&input_bytes_left`
    emitter.instruction("add x3, sp, #32");                                     // iconv argument 3 is `&output_ptr`
    emitter.instruction("add x4, sp, #40");                                     // iconv argument 4 is `&output_bytes_left`
    emitter.bl_c("iconv"); // decode as many complete characters as fit in the fixed output scratch
    emitter.instruction("str x0, [sp, #64]");                                   // preserve iconv's status while walking produced scalars
    emitter.instruction("ldr x9, [sp, #40]");                                   // load unused output bytes after the conversion attempt
    emitter.instruction("mov x10, #16");                                        // reload the fixed output scratch capacity
    emitter.instruction("sub x10, x10, x9");                                    // compute the number of UTF-32LE bytes produced
    emitter.instruction("add x17, sp, #48");                                    // walk the scratch from its first produced scalar
    emitter.instruction("mov x16, x10");                                        // remaining produced bytes still to measure
    emitter.label("__rt_mb_strwidth_iconv_add");
    emitter.instruction("cbz x16, __rt_mb_strwidth_iconv_added");               // continue after every produced scalar is measured
    emitter.instruction("ldr w0, [x17]");                                       // load one little-endian UTF-32 code point
    emitter.instruction("add x17, x17, #4");                                    // advance to the next produced scalar
    emitter.instruction("sub x16, x16, #4");                                    // consume four output bytes
    emitter.instruction("str x16, [sp, #72]");                                  // preserve the walk remaining-byte count
    emitter.instruction("str x17, [sp, #32]");                                  // preserve the walk pointer across the width lookup
    abi::emit_call_label(emitter, "__rt_mb_char_width");
    emitter.instruction("ldr x11, [sp, #16]");                                  // load the accumulated display width
    emitter.instruction("add x11, x11, x0");                                    // add this scalar's width
    emitter.instruction("str x11, [sp, #16]");                                  // persist the updated display width
    emitter.instruction("ldr x16, [sp, #72]");                                  // restore the walk remaining-byte count
    emitter.instruction("ldr x17, [sp, #32]");                                  // restore the walk pointer
    emitter.instruction("b __rt_mb_strwidth_iconv_add");                        // measure any remaining produced scalars
    emitter.label("__rt_mb_strwidth_iconv_added");
    emitter.instruction("ldr x0, [sp, #64]");                                   // restore iconv's return status
    emitter.instruction("cmn x0, #1");                                          // did iconv report an incomplete, malformed, or full-output condition?
    emitter.instruction("b.ne __rt_mb_strwidth_iconv_loop");                    // successful partial progress continues until input is exhausted
    emitter.instruction("ldr x9, [sp, #40]");                                   // inspect remaining output capacity before consulting errno
    emitter.instruction("cbz x9, __rt_mb_strwidth_iconv_loop");                 // a full output buffer is E2BIG and only requires another iteration
    emitter.bl_c(errno_function); // fetch the platform thread-local errno written by iconv
    emitter.instruction("ldr w9, [x0]");                                        // load iconv's errno value
    emitter.instruction("cmp w9, #22");                                         // EINVAL means the input ends in a valid but truncated sequence
    emitter.instruction("b.eq __rt_mb_strwidth_iconv_incomplete");              // mbstring groups that truncated prefix as width 1
    emitter.instruction("ldr x9, [sp, #8]");                                    // load bytes remaining at a malformed sequence
    emitter.instruction("cbz x9, __rt_mb_strwidth_iconv_done");                 // defensive completion if iconv consumed the final byte
    emitter.instruction("ldr x10, [sp, #0]");                                   // load iconv's current input pointer
    emitter.instruction("add x10, x10, #1");                                    // skip one malformed input byte like mbstring substitution
    emitter.instruction("str x10, [sp, #0]");                                   // persist the advanced input pointer
    emitter.instruction("sub x9, x9, #1");                                      // remove the malformed byte from the remaining input count
    emitter.instruction("str x9, [sp, #8]");                                    // persist the reduced input byte count
    emitter.instruction("ldr x10, [sp, #16]");                                  // load the accumulated display width
    emitter.instruction("add x10, x10, #1");                                    // one malformed byte becomes one column
    emitter.instruction("str x10, [sp, #16]");                                  // persist the substitution width
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the iconv descriptor for a state reset
    emitter.instruction("mov x1, #0");                                          // null input pointer requests iconv shift-state reset
    emitter.instruction("mov x2, #0");                                          // no input byte count participates in the reset
    emitter.instruction("mov x3, #0");                                          // no output pointer participates in the reset
    emitter.instruction("mov x4, #0");                                          // no output byte count participates in the reset
    emitter.bl_c("iconv"); // reset stateful decoders after substituting one malformed byte
    emitter.instruction("b __rt_mb_strwidth_iconv_loop");                       // continue decoding after the malformed byte

    emitter.label("__rt_mb_strwidth_iconv_incomplete");
    emitter.instruction("ldr x9, [sp, #16]");                                   // load the width before the truncated suffix
    emitter.instruction("add x9, x9, #1");                                      // count the whole truncated valid prefix as width 1
    emitter.instruction("str x9, [sp, #16]");                                   // persist the final display width
    emitter.instruction("str xzr, [sp, #8]");                                   // mark the truncated suffix as fully handled
    emitter.label("__rt_mb_strwidth_iconv_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // iconv_close argument is the active conversion descriptor
    emitter.bl_c("iconv_close"); // release the conversion descriptor before returning
    emitter.instruction("ldr x0, [sp, #16]");                                   // return the accumulated display width
    emitter.instruction("ldp x29, x30, [sp, #160]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #176");                                    // release the iconv helper frame
    emitter.instruction("ret");                                                 // return the encoding-aware display width

    emitter.label("__rt_mb_strwidth_use_utf8_framed");
    emitter.instruction("ldr x1, [sp, #0]");                                    // restore the PHP string pointer for the UTF-8 scanner
    emitter.instruction("ldr x2, [sp, #8]");                                    // restore the PHP string length for the UTF-8 scanner
    emitter.instruction("ldp x29, x30, [sp, #160]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #176");                                    // release the explicit-encoding helper frame
    emitter.instruction("b __rt_mb_strwidth_utf8");                             // tail-dispatch to the validated UTF-8 scanner

    emitter.label("__rt_mb_strwidth_use_byte_length");
    emitter.instruction("ldr x0, [sp, #8]");                                    // byte encodings are all width 1, so width equals byte length
    emitter.instruction("ldp x29, x30, [sp, #160]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #176");                                    // release the explicit-encoding helper frame
    emitter.instruction("ret");                                                 // return the original byte length

    emitter.label("__rt_mb_strwidth_unknown_encoding_framed");
    emitter.instruction("ldp x29, x30, [sp, #160]");                            // restore the caller frame before throwing ValueError
    emitter.instruction("add sp, sp, #176");                                    // release the explicit-encoding helper frame before unwinding
    emitter.label("__rt_mb_strwidth_unknown_encoding");
    value_error::emit_throw_value_error_aarch64(
        emitter,
        "_mb_strwidth_unknown_encoding_msg",
        MB_STRWIDTH_UNKNOWN_ENCODING_MSG.len(),
    );

    emit_utf8_scanner_aarch64(emitter);
}

/// Emits PHP-compatible validated UTF-8 display-width scanning for AArch64.
fn emit_utf8_scanner_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strwidth_utf8");
    emitter.instruction("mov x0, #0");                                          // UTF-8 display width starts at zero
    emitter.instruction("mov x4, #0");                                          // byte index starts at zero
    emitter.label("__rt_mb_strwidth_utf8_loop");
    emitter.instruction("cmp x4, x2");                                          // processed every source byte?
    emitter.instruction("b.hs __rt_mb_strwidth_utf8_done");                     // return once the byte index reaches the string length
    emitter.instruction("ldrb w5, [x1, x4]");                                   // load the next possible UTF-8 leading byte
    emitter.instruction("cmp w5, #0x80");                                       // ASCII bytes are complete one-column characters
    emitter.instruction("b.lo __rt_mb_strwidth_utf8_ascii");                    // consume one ASCII byte
    emitter.instruction("cmp w5, #0xC2");                                       // C0/C1 and continuation bytes are malformed leaders
    emitter.instruction("b.lo __rt_mb_strwidth_utf8_invalid");                  // substitute one malformed byte
    emitter.instruction("cmp w5, #0xE0");                                       // C2-DF introduce two-byte sequences
    emitter.instruction("b.lo __rt_mb_strwidth_utf8_two");                      // validate a two-byte character
    emitter.instruction("cmp w5, #0xF0");                                       // E0-EF introduce three-byte sequences
    emitter.instruction("b.lo __rt_mb_strwidth_utf8_three");                    // validate a three-byte character
    emitter.instruction("cmp w5, #0xF5");                                       // F0-F4 introduce Unicode-range four-byte sequences
    emitter.instruction("b.lo __rt_mb_strwidth_utf8_four");                     // validate a four-byte character
    emitter.instruction("b __rt_mb_strwidth_utf8_invalid");                     // F5-FF cannot begin valid UTF-8

    emitter.label("__rt_mb_strwidth_utf8_two");
    emitter.instruction("sub x6, x2, x4");                                      // compute bytes remaining from the two-byte leader
    emitter.instruction("cmp x6, #2");                                          // is the sequence truncated before its continuation byte?
    emitter.instruction("b.lo __rt_mb_strwidth_utf8_truncated");                // a valid truncated prefix counts as width 1
    emitter.instruction("add x8, x4, #1");                                      // address index of the required continuation byte
    emitter.instruction("ldrb w7, [x1, x8]");                                   // load the two-byte sequence continuation
    emitter.instruction("and w7, w7, #0xC0");                                   // isolate the continuation-byte prefix
    emitter.instruction("cmp w7, #0x80");                                       // does the second byte have the required 10xxxxxx shape?
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_invalid");                  // malformed continuation leaves the leader substituted alone
    emitter.instruction("add x4, x4, #2");                                      // consume the complete two-byte character
    emitter.instruction("b __rt_mb_strwidth_utf8_narrow");                      // two-byte scalars are below the first wide code point

    emitter.label("__rt_mb_strwidth_utf8_three");
    emitter.instruction("sub x6, x2, x4");                                      // compute bytes remaining from the three-byte leader
    emitter.instruction("cmp x6, #3");                                          // are all two continuation bytes available?
    emitter.instruction("b.lo __rt_mb_strwidth_utf8_three_partial");            // validate the available prefix before grouping truncation
    emitter.instruction("add x8, x4, #1");                                      // address index of the first continuation byte
    emitter.instruction("ldrb w7, [x1, x8]");                                   // load the first three-byte continuation
    emitter.instruction("and w8, w7, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w8, #0x80");                                       // is the first continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_invalid");                  // malformed continuation substitutes only the leader
    emitter.instruction("cmp w5, #0xE0");                                       // E0 requires a second byte at least A0 to avoid overlong UTF-8
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_three_not_e0");             // skip the E0 lower-bound check for other leaders
    emitter.instruction("cmp w7, #0xA0");                                       // is the E0 continuation inside the non-overlong range?
    emitter.instruction("b.lo __rt_mb_strwidth_utf8_invalid");                  // reject an overlong three-byte sequence
    emitter.label("__rt_mb_strwidth_utf8_three_not_e0");
    emitter.instruction("cmp w5, #0xED");                                       // ED requires a second byte below A0 to exclude UTF-16 surrogates
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_three_second");             // skip the surrogate bound for other leaders
    emitter.instruction("cmp w7, #0xA0");                                       // does the ED continuation enter the surrogate range?
    emitter.instruction("b.hs __rt_mb_strwidth_utf8_invalid");                  // reject UTF-8 encodings of surrogate code points
    emitter.label("__rt_mb_strwidth_utf8_three_second");
    emitter.instruction("add x8, x4, #2");                                      // address index of the second continuation byte
    emitter.instruction("ldrb w7, [x1, x8]");                                   // load the final three-byte continuation
    emitter.instruction("and w7, w7, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w7, #0x80");                                       // is the final continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_invalid");                  // malformed final byte substitutes only the leader
    emitter.instruction("ldrb w8, [x1, x4]");                                   // reload the three-byte leader to decode the scalar
    emitter.instruction("and w8, w8, #0x0F");                                   // isolate the leader payload
    emitter.instruction("lsl w8, w8, #12");                                     // place the leader bits in the code-point high nibble
    emitter.instruction("add x9, x4, #1");                                      // address the first continuation
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the first continuation payload
    emitter.instruction("and w10, w10, #0x3F");                                 // isolate its six data bits
    emitter.instruction("lsl w10, w10, #6");                                    // shift them into the code-point middle
    emitter.instruction("orr w8, w8, w10");                                     // merge the first continuation
    emitter.instruction("add x9, x4, #2");                                      // address the second continuation
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the final continuation payload
    emitter.instruction("and w10, w10, #0x3F");                                 // isolate its six data bits
    emitter.instruction("orr w8, w8, w10");                                     // finish the decoded BMP code point
    emitter.instruction("add x4, x4, #3");                                      // consume the complete three-byte character
    emitter.instruction("b __rt_mb_strwidth_utf8_wide");                        // look up East Asian Width for this scalar

    emitter.label("__rt_mb_strwidth_utf8_three_partial");
    emitter.instruction("cmp x6, #1");                                          // is only the valid three-byte leader available?
    emitter.instruction("b.eq __rt_mb_strwidth_utf8_truncated");                // group a lone valid leader as width 1
    emitter.instruction("add x8, x4, #1");                                      // address index of the available continuation byte
    emitter.instruction("ldrb w7, [x1, x8]");                                   // load the partial sequence continuation
    emitter.instruction("and w8, w7, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w8, #0x80");                                       // is the available continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_invalid");                  // malformed partial prefix substitutes only the leader
    emitter.instruction("cmp w5, #0xE0");                                       // apply E0's non-overlong lower bound to partial prefixes
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_three_partial_not_e0");     // other leaders do not need the E0 bound
    emitter.instruction("cmp w7, #0xA0");                                       // is the E0 continuation non-overlong?
    emitter.instruction("b.lo __rt_mb_strwidth_utf8_invalid");                  // reject an overlong partial prefix
    emitter.label("__rt_mb_strwidth_utf8_three_partial_not_e0");
    emitter.instruction("cmp w5, #0xED");                                       // apply ED's surrogate exclusion to partial prefixes
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_truncated");                // every other valid prefix is width 1
    emitter.instruction("cmp w7, #0xA0");                                       // does the ED continuation enter the surrogate range?
    emitter.instruction("b.hs __rt_mb_strwidth_utf8_invalid");                  // reject a surrogate partial prefix
    emitter.instruction("b __rt_mb_strwidth_utf8_truncated");                   // group the valid truncated prefix as width 1

    emitter.label("__rt_mb_strwidth_utf8_four");
    emitter.instruction("sub x6, x2, x4");                                      // compute bytes remaining from the four-byte leader
    emitter.instruction("cmp x6, #4");                                          // are all three continuation bytes available?
    emitter.instruction("b.lo __rt_mb_strwidth_utf8_four_partial");             // validate the available prefix before grouping truncation
    emitter.instruction("add x8, x4, #1");                                      // address index of the first continuation byte
    emitter.instruction("ldrb w7, [x1, x8]");                                   // load the first four-byte continuation
    emitter.instruction("and w8, w7, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w8, #0x80");                                       // is the first continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_invalid");                  // malformed continuation substitutes only the leader
    emitter.instruction("cmp w5, #0xF0");                                       // F0 requires a second byte at least 90 to avoid overlong UTF-8
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_four_not_f0");              // skip the F0 lower-bound check for other leaders
    emitter.instruction("cmp w7, #0x90");                                       // is the F0 continuation inside the non-overlong range?
    emitter.instruction("b.lo __rt_mb_strwidth_utf8_invalid");                  // reject an overlong four-byte sequence
    emitter.label("__rt_mb_strwidth_utf8_four_not_f0");
    emitter.instruction("cmp w5, #0xF4");                                       // F4 requires a second byte below 90 for Unicode's maximum scalar
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_four_rest");                // skip the upper bound for F0-F3
    emitter.instruction("cmp w7, #0x90");                                       // does the F4 continuation exceed U+10FFFF?
    emitter.instruction("b.hs __rt_mb_strwidth_utf8_invalid");                  // reject out-of-range four-byte sequences
    emitter.label("__rt_mb_strwidth_utf8_four_rest");
    emitter.instruction("add x8, x4, #2");                                      // address index of the second continuation byte
    emitter.instruction("ldrb w7, [x1, x8]");                                   // load the second four-byte continuation
    emitter.instruction("and w7, w7, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w7, #0x80");                                       // is the second continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_invalid");                  // malformed continuation substitutes only the leader
    emitter.instruction("add x8, x4, #3");                                      // address index of the third continuation byte
    emitter.instruction("ldrb w7, [x1, x8]");                                   // load the final four-byte continuation
    emitter.instruction("and w7, w7, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w7, #0x80");                                       // is the final continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_invalid");                  // malformed continuation substitutes only the leader
    emitter.instruction("ldrb w8, [x1, x4]");                                   // reload the four-byte leader to decode the scalar
    emitter.instruction("and w8, w8, #0x07");                                   // isolate the leader payload
    emitter.instruction("lsl w8, w8, #18");                                     // place the leader bits in the supplementary plane
    emitter.instruction("add x9, x4, #1");                                      // address the first continuation
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the first continuation payload
    emitter.instruction("and w10, w10, #0x3F");                                 // isolate its six data bits
    emitter.instruction("lsl w10, w10, #12");                                   // shift them into the code-point high middle
    emitter.instruction("orr w8, w8, w10");                                     // merge the first continuation
    emitter.instruction("add x9, x4, #2");                                      // address the second continuation
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the second continuation payload
    emitter.instruction("and w10, w10, #0x3F");                                 // isolate its six data bits
    emitter.instruction("lsl w10, w10, #6");                                    // shift them into the code-point low middle
    emitter.instruction("orr w8, w8, w10");                                     // merge the second continuation
    emitter.instruction("add x9, x4, #3");                                      // address the third continuation
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load the final continuation payload
    emitter.instruction("and w10, w10, #0x3F");                                 // isolate its six data bits
    emitter.instruction("orr w8, w8, w10");                                     // finish the decoded supplementary code point
    emitter.instruction("add x4, x4, #4");                                      // consume the complete four-byte character
    emitter.instruction("b __rt_mb_strwidth_utf8_wide");                        // look up East Asian Width for this scalar

    emitter.label("__rt_mb_strwidth_utf8_four_partial");
    emitter.instruction("cmp x6, #1");                                          // is only the valid four-byte leader available?
    emitter.instruction("b.eq __rt_mb_strwidth_utf8_truncated");                // group a lone valid leader as width 1
    emitter.instruction("add x8, x4, #1");                                      // address index of the available first continuation
    emitter.instruction("ldrb w7, [x1, x8]");                                   // load the first partial continuation
    emitter.instruction("and w8, w7, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w8, #0x80");                                       // is the first partial continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_invalid");                  // malformed partial prefix substitutes only the leader
    emitter.instruction("cmp w5, #0xF0");                                       // apply F0's non-overlong lower bound to partial prefixes
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_four_partial_not_f0");      // other leaders do not need the F0 bound
    emitter.instruction("cmp w7, #0x90");                                       // is the F0 continuation non-overlong?
    emitter.instruction("b.lo __rt_mb_strwidth_utf8_invalid");                  // reject an overlong partial prefix
    emitter.label("__rt_mb_strwidth_utf8_four_partial_not_f0");
    emitter.instruction("cmp w5, #0xF4");                                       // apply F4's Unicode maximum bound to partial prefixes
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_four_partial_tail");        // F0-F3 continue validating any available tail
    emitter.instruction("cmp w7, #0x90");                                       // does the F4 continuation exceed U+10FFFF?
    emitter.instruction("b.hs __rt_mb_strwidth_utf8_invalid");                  // reject an out-of-range partial prefix
    emitter.label("__rt_mb_strwidth_utf8_four_partial_tail");
    emitter.instruction("cmp x6, #2");                                          // are only the leader and first continuation available?
    emitter.instruction("b.eq __rt_mb_strwidth_utf8_truncated");                // group that valid truncated prefix as width 1
    emitter.instruction("add x8, x4, #2");                                      // address index of the available second continuation
    emitter.instruction("ldrb w7, [x1, x8]");                                   // load the second partial continuation
    emitter.instruction("and w7, w7, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w7, #0x80");                                       // is the second partial continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strwidth_utf8_invalid");                  // malformed partial tail substitutes only the leader
    emitter.instruction("b __rt_mb_strwidth_utf8_truncated");                   // group the valid three-byte prefix as width 1

    emitter.label("__rt_mb_strwidth_utf8_ascii");
    emitter.instruction("add x4, x4, #1");                                      // consume one ASCII byte
    emitter.instruction("b __rt_mb_strwidth_utf8_narrow");                      // ASCII is always width 1
    emitter.label("__rt_mb_strwidth_utf8_invalid");
    emitter.instruction("add x4, x4, #1");                                      // consume one malformed byte for mbstring substitution
    emitter.label("__rt_mb_strwidth_utf8_narrow");
    emitter.instruction("add x0, x0, #1");                                      // count one column for ASCII, 2-byte, or substituted input
    emitter.instruction("b __rt_mb_strwidth_utf8_loop");                        // continue scanning the remaining bytes
    emitter.label("__rt_mb_strwidth_utf8_wide");
    emitter.instruction("stp x0, x1, [sp, #-32]!");                             // preserve width and string pointer across the lookup
    emitter.instruction("stp x2, x4, [sp, #16]");                               // preserve length and byte index across the lookup
    emitter.instruction("mov w0, w8");                                          // width-lookup argument is the decoded code point
    emitter.instruction("str x30, [sp, #-16]!");                                // preserve the original return address of the frameless scanner
    emitter.instruction("bl __rt_mb_char_width");                               // look up PHP 8.5 East Asian Width
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the original return address
    emitter.instruction("mov w8, w0");                                          // keep the looked-up width
    emitter.instruction("ldp x2, x4, [sp, #16]");                               // restore length and byte index
    emitter.instruction("ldp x0, x1, [sp], #32");                               // restore accumulated width and string pointer
    emitter.instruction("add x0, x0, x8");                                      // add the looked-up width
    emitter.instruction("b __rt_mb_strwidth_utf8_loop");                        // continue scanning the remaining bytes
    emitter.label("__rt_mb_strwidth_utf8_truncated");
    emitter.instruction("add x0, x0, #1");                                      // count the final valid truncated prefix as width 1
    emitter.instruction("ret");                                                 // no bytes remain beyond the truncated prefix
    emitter.label("__rt_mb_strwidth_utf8_done");
    emitter.instruction("ret");                                                 // return the validated UTF-8 display width
}

/// Emits the Linux x86_64 PHP 8.5 East Asian Width lookup (`edi` in, width in `eax`).
fn emit_mb_char_width_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_char_width (PHP 8.5 East Asian Width) ---");
    emitter.label_global("__rt_mb_char_width");
    emitter.instruction("cmp edi, 0x1100");                                     // first code point that can be double-width
    emitter.instruction("jb __rt_mb_char_width_narrow_x86");                    // ASCII and early BMP are always width 1
    emitter.instruction("xor ecx, ecx");                                        // binary-search low index starts at zero
    emitter.instruction(&format!("mov edx, {}", EAW_RANGE_COUNT));              // binary-search high index starts past the last range
    abi::emit_symbol_address(emitter, "rsi", "_mb_eaw_table");
    emitter.label("__rt_mb_char_width_loop_x86");
    emitter.instruction("cmp ecx, edx");                                        // have the remaining ranges collapsed?
    emitter.instruction("jae __rt_mb_char_width_narrow_x86");                   // a miss is a halfwidth character
    emitter.instruction("mov r8d, ecx");                                        // copy the low bound to form the midpoint
    emitter.instruction("add r8d, edx");                                        // add the high bound
    emitter.instruction("shr r8d, 1");                                          // integer midpoint index
    emitter.instruction("mov r9, r8");                                          // copy the probe for the byte offset
    emitter.instruction("shl r9, 3");                                           // each table entry is two 32-bit bounds
    emitter.instruction("mov r10d, DWORD PTR [rsi + r9]");                      // load the inclusive range begin
    emitter.instruction("cmp edi, r10d");                                       // is the code point before this range?
    emitter.instruction("jb __rt_mb_char_width_hi_x86");                        // search the lower half
    emitter.instruction("mov r10d, DWORD PTR [rsi + r9 + 4]");                  // load the inclusive range end
    emitter.instruction("cmp edi, r10d");                                       // is the code point after this range?
    emitter.instruction("ja __rt_mb_char_width_lo_x86");                        // search the upper half
    emitter.instruction("mov eax, 2");                                          // a hit is a fullwidth / wide character
    emitter.instruction("ret");                                                 // return width 2
    emitter.label("__rt_mb_char_width_hi_x86");
    emitter.instruction("mov edx, r8d");                                        // shrink the high bound to the current probe
    emitter.instruction("jmp __rt_mb_char_width_loop_x86");                     // continue the binary search
    emitter.label("__rt_mb_char_width_lo_x86");
    emitter.instruction("lea ecx, [r8 + 1]");                                   // skip the probe that ended below the code point
    emitter.instruction("jmp __rt_mb_char_width_loop_x86");                     // continue the binary search
    emitter.label("__rt_mb_char_width_narrow_x86");
    emitter.instruction("mov eax, 1");                                          // every other scalar is halfwidth
    emitter.instruction("ret");                                                 // return width 1
}

/// Emits the Linux x86_64 implementation.
fn emit_mb_strwidth_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_strwidth (encoding-aware display width) ---");
    emitter.label_global("__rt_mb_strwidth");
    emitter.instruction("test r8, r8");                                         // omitted/null encoding is represented by a null pointer
    emitter.instruction("jz __rt_mb_strwidth_utf8_x86");                        // use the default UTF-8 scanner when encoding is omitted/null
    emitter.instruction(&format!("cmp r9, {}", MAX_ENCODING_NAME_LEN));         // does the encoding name fit the stack C-string buffer?
    emitter.instruction("ja __rt_mb_strwidth_unknown_encoding_x86");            // reject names longer than every PHP-supported alias
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across libc calls
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned helper frame
    emitter.instruction("sub rsp, 160");                                        // reserve iconv state, output scratch, and encoding-name storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // iconv input pointer variable starts at the PHP string bytes
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // iconv input byte count variable starts at the PHP string length
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // decoded display width starts at zero

    // -- copy the length-delimited PHP encoding name into a stack C string --
    emitter.instruction("lea rdi, [rbp - 160]");                                // destination is the 64-byte encoding-name buffer
    emitter.instruction("xor rcx, rcx");                                        // copied-byte index starts at zero
    emitter.label("__rt_mb_strwidth_encoding_copy_x86");
    emitter.instruction("cmp rcx, r9");                                         // copied the whole explicit encoding name?
    emitter.instruction("jae __rt_mb_strwidth_encoding_copied_x86");            // terminate the C string once every byte is copied
    emitter.instruction("mov r10b, BYTE PTR [r8 + rcx]");                       // load one encoding-name byte from the PHP string
    emitter.instruction("mov BYTE PTR [rdi + rcx], r10b");                      // append the byte to the stack C string
    emitter.instruction("inc rcx");                                             // advance the encoding-name byte index
    emitter.instruction("jmp __rt_mb_strwidth_encoding_copy_x86");              // continue copying the remaining encoding-name bytes
    emitter.label("__rt_mb_strwidth_encoding_copied_x86");
    emitter.instruction("mov BYTE PTR [rdi + r9], 0");                          // NUL-terminate the explicit encoding name

    // -- fast-path PHP's default UTF-8 names and byte-count encodings --
    emitter.instruction("lea rdi, [rbp - 160]");                                // first strcasecmp argument is the copied encoding name
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_utf8_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with UTF-8 case-insensitively
    emitter.instruction("test eax, eax");                                       // did the encoding match UTF-8?
    emitter.instruction("jz __rt_mb_strwidth_use_utf8_framed_x86");             // UTF-8 uses the allocation-free validated scanner
    emitter.instruction("lea rdi, [rbp - 160]");                                // reload the copied encoding name after strcasecmp
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_utf8_alias");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with PHP's UTF8 alias
    emitter.instruction("test eax, eax");                                       // did the encoding match UTF8?
    emitter.instruction("jz __rt_mb_strwidth_use_utf8_framed_x86");             // the UTF8 alias uses the same validated scanner
    emitter.instruction("lea rdi, [rbp - 160]");                                // reload the copied encoding name for the byte-count aliases
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_8bit_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with 8bit
    emitter.instruction("test eax, eax");                                       // did the encoding match 8bit?
    emitter.instruction("jz __rt_mb_strwidth_use_byte_length_x86");             // 8bit bytes are all below the first wide code point
    emitter.instruction("lea rdi, [rbp - 160]");                                // reload the copied encoding name for the binary alias
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_binary_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with binary
    emitter.instruction("test eax, eax");                                       // did the encoding match binary?
    emitter.instruction("jz __rt_mb_strwidth_use_byte_length_x86");             // binary is PHP's alias for 8bit
    emitter.instruction("lea rdi, [rbp - 160]");                                // reload the copied encoding name for the 7bit encoding
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_7bit_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with 7bit
    emitter.instruction("test eax, eax");                                       // did the encoding match 7bit?
    emitter.instruction("jz __rt_mb_strwidth_use_byte_length_x86");             // 7bit preserves PHP's one-column-per-byte width

    // -- open a decoder from the requested encoding to fixed-width UTF-32LE --
    abi::emit_symbol_address(emitter, "rdi", "_mb_strlen_utf32le_name");
    emitter.instruction("lea rsi, [rbp - 160]");                                // iconv source encoding is the copied explicit name
    emitter.instruction("call iconv_open");                                     // create the encoding-to-UTF-32LE conversion descriptor
    emitter.instruction("cmp rax, -1");                                         // did iconv_open return the failure sentinel?
    emitter.instruction("je __rt_mb_strwidth_unknown_encoding_framed_x86");     // unknown encoding names raise PHP's ValueError
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the iconv descriptor across conversion iterations

    // -- decode chunks into 16 bytes of UTF-32LE and add each scalar's width --
    emitter.label("__rt_mb_strwidth_iconv_loop_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // are any input bytes still undecoded?
    emitter.instruction("je __rt_mb_strwidth_iconv_done_x86");                  // close the descriptor after all bytes are consumed
    emitter.instruction("lea r10, [rbp - 80]");                                 // point at the fixed 16-byte UTF-32LE output scratch
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // initialize iconv's mutable output pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 16");                        // initialize iconv's mutable output-byte count
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // iconv argument 0 is the conversion descriptor
    emitter.instruction("lea rsi, [rbp - 8]");                                  // iconv argument 1 is `&input_ptr`
    emitter.instruction("lea rdx, [rbp - 16]");                                 // iconv argument 2 is `&input_bytes_left`
    emitter.instruction("lea rcx, [rbp - 40]");                                 // iconv argument 3 is `&output_ptr`
    emitter.instruction("lea r8, [rbp - 48]");                                  // iconv argument 4 is `&output_bytes_left`
    emitter.instruction("call iconv");                                          // decode as many complete characters as fit in the output scratch
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // preserve iconv's status while walking produced scalars
    emitter.instruction("mov r10, 16");                                         // reload the fixed output scratch capacity
    emitter.instruction("sub r10, QWORD PTR [rbp - 48]");                       // compute the number of UTF-32LE bytes produced
    emitter.instruction("lea r11, [rbp - 80]");                                 // walk the scratch from its first produced scalar
    emitter.label("__rt_mb_strwidth_iconv_add_x86");
    emitter.instruction("test r10, r10");                                       // any produced scalars still unmeasured?
    emitter.instruction("jz __rt_mb_strwidth_iconv_added_x86");                 // continue after every produced scalar is measured
    emitter.instruction("mov edi, DWORD PTR [r11]");                            // load one little-endian UTF-32 code point
    emitter.instruction("add r11, 4");                                          // advance to the next produced scalar
    emitter.instruction("sub r10, 4");                                          // consume four output bytes
    emitter.instruction("push r10");                                            // preserve remaining produced bytes across the lookup
    emitter.instruction("push r11");                                            // preserve the walk pointer and keep the call 16-byte aligned
    emitter.instruction("call __rt_mb_char_width");                             // look up PHP 8.5 East Asian Width for this scalar
    emitter.instruction("add QWORD PTR [rbp - 24], rax");                       // add this scalar's width
    emitter.instruction("pop r11");                                             // restore the walk pointer
    emitter.instruction("pop r10");                                             // restore remaining produced bytes
    emitter.instruction("jmp __rt_mb_strwidth_iconv_add_x86");                  // measure any remaining produced scalars
    emitter.label("__rt_mb_strwidth_iconv_added_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 56], -1");                        // did iconv report an error condition?
    emitter.instruction("jne __rt_mb_strwidth_iconv_loop_x86");                 // successful partial progress continues until input is exhausted
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // did iconv merely fill the output scratch?
    emitter.instruction("je __rt_mb_strwidth_iconv_loop_x86");                  // E2BIG only requires another conversion iteration
    emitter.instruction("call __errno_location");                               // fetch the Linux thread-local errno written by iconv
    emitter.instruction("cmp DWORD PTR [rax], 22");                             // EINVAL means a valid but truncated final sequence
    emitter.instruction("je __rt_mb_strwidth_iconv_incomplete_x86");            // mbstring groups that truncated prefix as width 1
    emitter.instruction("cmp QWORD PTR [rbp - 16], 0");                         // are bytes still present at the malformed sequence?
    emitter.instruction("je __rt_mb_strwidth_iconv_done_x86");                  // defensive completion if iconv consumed the final byte
    emitter.instruction("add QWORD PTR [rbp - 8], 1");                          // skip one malformed input byte like mbstring substitution
    emitter.instruction("sub QWORD PTR [rbp - 16], 1");                         // remove the malformed byte from the remaining input count
    emitter.instruction("add QWORD PTR [rbp - 24], 1");                         // one malformed byte becomes one column
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the iconv descriptor for a state reset
    emitter.instruction("xor rsi, rsi");                                        // null input pointer requests iconv shift-state reset
    emitter.instruction("xor rdx, rdx");                                        // no input byte count participates in the reset
    emitter.instruction("xor rcx, rcx");                                        // no output pointer participates in the reset
    emitter.instruction("xor r8, r8");                                          // no output byte count participates in the reset
    emitter.instruction("call iconv");                                          // reset stateful decoders after substituting one malformed byte
    emitter.instruction("jmp __rt_mb_strwidth_iconv_loop_x86");                 // continue decoding after the malformed byte

    emitter.label("__rt_mb_strwidth_iconv_incomplete_x86");
    emitter.instruction("add QWORD PTR [rbp - 24], 1");                         // count the whole truncated valid prefix as width 1
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // mark the truncated suffix as fully handled
    emitter.label("__rt_mb_strwidth_iconv_done_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // iconv_close argument is the active conversion descriptor
    emitter.instruction("call iconv_close");                                    // release the conversion descriptor before returning
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the accumulated display width
    emitter.instruction("leave");                                               // release the iconv helper frame and restore rbp
    emitter.instruction("ret");                                                 // return the encoding-aware display width

    emitter.label("__rt_mb_strwidth_use_utf8_framed_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the PHP string pointer for the UTF-8 scanner
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the PHP string length for the UTF-8 scanner
    emitter.instruction("leave");                                               // release the explicit-encoding helper frame
    emitter.instruction("jmp __rt_mb_strwidth_utf8_x86");                       // tail-dispatch to the validated UTF-8 scanner

    emitter.label("__rt_mb_strwidth_use_byte_length_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // byte encodings are all width 1, so width equals byte length
    emitter.instruction("leave");                                               // release the explicit-encoding helper frame
    emitter.instruction("ret");                                                 // return the original byte length

    emitter.label("__rt_mb_strwidth_unknown_encoding_framed_x86");
    emitter.instruction("leave");                                               // release the explicit-encoding helper frame before unwinding
    emitter.label("__rt_mb_strwidth_unknown_encoding_x86");
    value_error::emit_throw_value_error_x86_64(
        emitter,
        "_mb_strwidth_unknown_encoding_msg",
        MB_STRWIDTH_UNKNOWN_ENCODING_MSG.len(),
    );

    emit_utf8_scanner_x86_64(emitter);
}

/// Emits PHP-compatible validated UTF-8 display-width scanning for Linux x86_64.
fn emit_utf8_scanner_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strwidth_utf8_x86");
    emitter.instruction("mov rsi, rax");                                        // preserve the source pointer while rax becomes the width
    emitter.instruction("xor eax, eax");                                        // UTF-8 display width starts at zero
    emitter.instruction("xor r8, r8");                                          // byte index starts at zero
    emitter.label("__rt_mb_strwidth_utf8_loop_x86");
    emitter.instruction("cmp r8, rdx");                                         // processed every source byte?
    emitter.instruction("jae __rt_mb_strwidth_utf8_done_x86");                  // return once the byte index reaches the string length
    emitter.instruction("movzx r9d, BYTE PTR [rsi + r8]");                      // load the next possible UTF-8 leading byte
    emitter.instruction("cmp r9d, 0x80");                                       // ASCII bytes are complete one-column characters
    emitter.instruction("jb __rt_mb_strwidth_utf8_ascii_x86");                  // consume one ASCII byte
    emitter.instruction("cmp r9d, 0xC2");                                       // C0/C1 and continuation bytes are malformed leaders
    emitter.instruction("jb __rt_mb_strwidth_utf8_invalid_x86");                // substitute one malformed byte
    emitter.instruction("cmp r9d, 0xE0");                                       // C2-DF introduce two-byte sequences
    emitter.instruction("jb __rt_mb_strwidth_utf8_two_x86");                    // validate a two-byte character
    emitter.instruction("cmp r9d, 0xF0");                                       // E0-EF introduce three-byte sequences
    emitter.instruction("jb __rt_mb_strwidth_utf8_three_x86");                  // validate a three-byte character
    emitter.instruction("cmp r9d, 0xF5");                                       // F0-F4 introduce Unicode-range four-byte sequences
    emitter.instruction("jb __rt_mb_strwidth_utf8_four_x86");                   // validate a four-byte character
    emitter.instruction("jmp __rt_mb_strwidth_utf8_invalid_x86");               // F5-FF cannot begin valid UTF-8

    emitter.label("__rt_mb_strwidth_utf8_two_x86");
    emitter.instruction("mov r10, rdx");                                        // copy the total byte length to compute remaining bytes
    emitter.instruction("sub r10, r8");                                         // compute bytes remaining from the two-byte leader
    emitter.instruction("cmp r10, 2");                                          // is the sequence truncated before its continuation byte?
    emitter.instruction("jb __rt_mb_strwidth_utf8_truncated_x86");              // a valid truncated prefix counts as width 1
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 1]");                 // load the two-byte sequence continuation
    emitter.instruction("and r11d, 0xC0");                                      // isolate the continuation-byte prefix
    emitter.instruction("cmp r11d, 0x80");                                      // does the second byte have the required 10xxxxxx shape?
    emitter.instruction("jne __rt_mb_strwidth_utf8_invalid_x86");               // malformed continuation leaves the leader substituted alone
    emitter.instruction("add r8, 2");                                           // consume the complete two-byte character
    emitter.instruction("jmp __rt_mb_strwidth_utf8_narrow_x86");                // two-byte scalars are below the first wide code point

    emitter.label("__rt_mb_strwidth_utf8_three_x86");
    emitter.instruction("mov r10, rdx");                                        // copy the total byte length to compute remaining bytes
    emitter.instruction("sub r10, r8");                                         // compute bytes remaining from the three-byte leader
    emitter.instruction("cmp r10, 3");                                          // are all two continuation bytes available?
    emitter.instruction("jb __rt_mb_strwidth_utf8_three_partial_x86");          // validate the available prefix before grouping truncation
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 1]");                 // load the first three-byte continuation
    emitter.instruction("mov ecx, r11d");                                       // preserve the continuation value while checking its prefix
    emitter.instruction("and ecx, 0xC0");                                       // isolate its continuation-byte prefix
    emitter.instruction("cmp ecx, 0x80");                                       // is the first continuation structurally valid?
    emitter.instruction("jne __rt_mb_strwidth_utf8_invalid_x86");               // malformed continuation substitutes only the leader
    emitter.instruction("cmp r9d, 0xE0");                                       // E0 requires a second byte at least A0 to avoid overlong UTF-8
    emitter.instruction("jne __rt_mb_strwidth_utf8_three_not_e0_x86");          // skip the E0 lower-bound check for other leaders
    emitter.instruction("cmp r11d, 0xA0");                                      // is the E0 continuation inside the non-overlong range?
    emitter.instruction("jb __rt_mb_strwidth_utf8_invalid_x86");                // reject an overlong three-byte sequence
    emitter.label("__rt_mb_strwidth_utf8_three_not_e0_x86");
    emitter.instruction("cmp r9d, 0xED");                                       // ED requires a second byte below A0 to exclude UTF-16 surrogates
    emitter.instruction("jne __rt_mb_strwidth_utf8_three_second_x86");          // skip the surrogate bound for other leaders
    emitter.instruction("cmp r11d, 0xA0");                                      // does the ED continuation enter the surrogate range?
    emitter.instruction("jae __rt_mb_strwidth_utf8_invalid_x86");               // reject UTF-8 encodings of surrogate code points
    emitter.label("__rt_mb_strwidth_utf8_three_second_x86");
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 2]");                 // load the final three-byte continuation
    emitter.instruction("and r11d, 0xC0");                                      // isolate its continuation-byte prefix
    emitter.instruction("cmp r11d, 0x80");                                      // is the final continuation structurally valid?
    emitter.instruction("jne __rt_mb_strwidth_utf8_invalid_x86");               // malformed final byte substitutes only the leader
    emitter.instruction("movzx ecx, BYTE PTR [rsi + r8]");                      // reload the three-byte leader to decode the scalar
    emitter.instruction("and ecx, 0x0F");                                       // isolate the leader payload
    emitter.instruction("shl ecx, 12");                                         // place the leader bits in the code-point high nibble
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 1]");                 // load the first continuation payload
    emitter.instruction("and r11d, 0x3F");                                      // isolate its six data bits
    emitter.instruction("shl r11d, 6");                                         // shift them into the code-point middle
    emitter.instruction("or ecx, r11d");                                        // merge the first continuation
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 2]");                 // load the final continuation payload
    emitter.instruction("and r11d, 0x3F");                                      // isolate its six data bits
    emitter.instruction("or ecx, r11d");                                        // finish the decoded BMP code point
    emitter.instruction("add r8, 3");                                           // consume the complete three-byte character
    emitter.instruction("jmp __rt_mb_strwidth_utf8_wide_x86");                  // look up East Asian Width for this scalar

    emitter.label("__rt_mb_strwidth_utf8_three_partial_x86");
    emitter.instruction("cmp r10, 1");                                          // is only the valid three-byte leader available?
    emitter.instruction("je __rt_mb_strwidth_utf8_truncated_x86");              // group a lone valid leader as width 1
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 1]");                 // load the partial sequence continuation
    emitter.instruction("mov ecx, r11d");                                       // preserve the continuation value while checking its prefix
    emitter.instruction("and ecx, 0xC0");                                       // isolate its continuation-byte prefix
    emitter.instruction("cmp ecx, 0x80");                                       // is the available continuation structurally valid?
    emitter.instruction("jne __rt_mb_strwidth_utf8_invalid_x86");               // malformed partial prefix substitutes only the leader
    emitter.instruction("cmp r9d, 0xE0");                                       // apply E0's non-overlong lower bound to partial prefixes
    emitter.instruction("jne __rt_mb_strwidth_utf8_three_partial_not_e0_x86");  // other leaders do not need the E0 bound
    emitter.instruction("cmp r11d, 0xA0");                                      // is the E0 continuation non-overlong?
    emitter.instruction("jb __rt_mb_strwidth_utf8_invalid_x86");                // reject an overlong partial prefix
    emitter.label("__rt_mb_strwidth_utf8_three_partial_not_e0_x86");
    emitter.instruction("cmp r9d, 0xED");                                       // apply ED's surrogate exclusion to partial prefixes
    emitter.instruction("jne __rt_mb_strwidth_utf8_truncated_x86");             // every other valid prefix is width 1
    emitter.instruction("cmp r11d, 0xA0");                                      // does the ED continuation enter the surrogate range?
    emitter.instruction("jae __rt_mb_strwidth_utf8_invalid_x86");               // reject a surrogate partial prefix
    emitter.instruction("jmp __rt_mb_strwidth_utf8_truncated_x86");             // group the valid truncated prefix as width 1

    emitter.label("__rt_mb_strwidth_utf8_four_x86");
    emitter.instruction("mov r10, rdx");                                        // copy the total byte length to compute remaining bytes
    emitter.instruction("sub r10, r8");                                         // compute bytes remaining from the four-byte leader
    emitter.instruction("cmp r10, 4");                                          // are all three continuation bytes available?
    emitter.instruction("jb __rt_mb_strwidth_utf8_four_partial_x86");           // validate the available prefix before grouping truncation
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 1]");                 // load the first four-byte continuation
    emitter.instruction("mov ecx, r11d");                                       // preserve the continuation value while checking its prefix
    emitter.instruction("and ecx, 0xC0");                                       // isolate its continuation-byte prefix
    emitter.instruction("cmp ecx, 0x80");                                       // is the first continuation structurally valid?
    emitter.instruction("jne __rt_mb_strwidth_utf8_invalid_x86");               // malformed continuation substitutes only the leader
    emitter.instruction("cmp r9d, 0xF0");                                       // F0 requires a second byte at least 90 to avoid overlong UTF-8
    emitter.instruction("jne __rt_mb_strwidth_utf8_four_not_f0_x86");           // skip the F0 lower-bound check for other leaders
    emitter.instruction("cmp r11d, 0x90");                                      // is the F0 continuation inside the non-overlong range?
    emitter.instruction("jb __rt_mb_strwidth_utf8_invalid_x86");                // reject an overlong four-byte sequence
    emitter.label("__rt_mb_strwidth_utf8_four_not_f0_x86");
    emitter.instruction("cmp r9d, 0xF4");                                       // F4 requires a second byte below 90 for Unicode's maximum scalar
    emitter.instruction("jne __rt_mb_strwidth_utf8_four_rest_x86");             // skip the upper bound for F0-F3
    emitter.instruction("cmp r11d, 0x90");                                      // does the F4 continuation exceed U+10FFFF?
    emitter.instruction("jae __rt_mb_strwidth_utf8_invalid_x86");               // reject out-of-range four-byte sequences
    emitter.label("__rt_mb_strwidth_utf8_four_rest_x86");
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 2]");                 // load the second four-byte continuation
    emitter.instruction("and r11d, 0xC0");                                      // isolate its continuation-byte prefix
    emitter.instruction("cmp r11d, 0x80");                                      // is the second continuation structurally valid?
    emitter.instruction("jne __rt_mb_strwidth_utf8_invalid_x86");               // malformed continuation substitutes only the leader
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 3]");                 // load the final four-byte continuation
    emitter.instruction("and r11d, 0xC0");                                      // isolate its continuation-byte prefix
    emitter.instruction("cmp r11d, 0x80");                                      // is the final continuation structurally valid?
    emitter.instruction("jne __rt_mb_strwidth_utf8_invalid_x86");               // malformed continuation substitutes only the leader
    emitter.instruction("movzx ecx, BYTE PTR [rsi + r8]");                      // reload the four-byte leader to decode the scalar
    emitter.instruction("and ecx, 0x07");                                       // isolate the leader payload
    emitter.instruction("shl ecx, 18");                                         // place the leader bits in the supplementary plane
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 1]");                 // load the first continuation payload
    emitter.instruction("and r11d, 0x3F");                                      // isolate its six data bits
    emitter.instruction("shl r11d, 12");                                        // shift them into the code-point high middle
    emitter.instruction("or ecx, r11d");                                        // merge the first continuation
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 2]");                 // load the second continuation payload
    emitter.instruction("and r11d, 0x3F");                                      // isolate its six data bits
    emitter.instruction("shl r11d, 6");                                         // shift them into the code-point low middle
    emitter.instruction("or ecx, r11d");                                        // merge the second continuation
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 3]");                 // load the final continuation payload
    emitter.instruction("and r11d, 0x3F");                                      // isolate its six data bits
    emitter.instruction("or ecx, r11d");                                        // finish the decoded supplementary code point
    emitter.instruction("add r8, 4");                                           // consume the complete four-byte character
    emitter.instruction("jmp __rt_mb_strwidth_utf8_wide_x86");                  // look up East Asian Width for this scalar

    emitter.label("__rt_mb_strwidth_utf8_four_partial_x86");
    emitter.instruction("cmp r10, 1");                                          // is only the valid four-byte leader available?
    emitter.instruction("je __rt_mb_strwidth_utf8_truncated_x86");              // group a lone valid leader as width 1
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 1]");                 // load the first partial continuation
    emitter.instruction("mov ecx, r11d");                                       // preserve the continuation value while checking its prefix
    emitter.instruction("and ecx, 0xC0");                                       // isolate its continuation-byte prefix
    emitter.instruction("cmp ecx, 0x80");                                       // is the first partial continuation structurally valid?
    emitter.instruction("jne __rt_mb_strwidth_utf8_invalid_x86");               // malformed partial prefix substitutes only the leader
    emitter.instruction("cmp r9d, 0xF0");                                       // apply F0's non-overlong lower bound to partial prefixes
    emitter.instruction("jne __rt_mb_strwidth_utf8_four_partial_not_f0_x86");   // other leaders do not need the F0 bound
    emitter.instruction("cmp r11d, 0x90");                                      // is the F0 continuation non-overlong?
    emitter.instruction("jb __rt_mb_strwidth_utf8_invalid_x86");                // reject an overlong partial prefix
    emitter.label("__rt_mb_strwidth_utf8_four_partial_not_f0_x86");
    emitter.instruction("cmp r9d, 0xF4");                                       // apply F4's Unicode maximum bound to partial prefixes
    emitter.instruction("jne __rt_mb_strwidth_utf8_four_partial_tail_x86");     // F0-F3 continue validating any available tail
    emitter.instruction("cmp r11d, 0x90");                                      // does the F4 continuation exceed U+10FFFF?
    emitter.instruction("jae __rt_mb_strwidth_utf8_invalid_x86");               // reject an out-of-range partial prefix
    emitter.label("__rt_mb_strwidth_utf8_four_partial_tail_x86");
    emitter.instruction("cmp r10, 2");                                          // are only the leader and first continuation available?
    emitter.instruction("je __rt_mb_strwidth_utf8_truncated_x86");              // group that valid truncated prefix as width 1
    emitter.instruction("movzx r11d, BYTE PTR [rsi + r8 + 2]");                 // load the second partial continuation
    emitter.instruction("and r11d, 0xC0");                                      // isolate its continuation-byte prefix
    emitter.instruction("cmp r11d, 0x80");                                      // is the second partial continuation structurally valid?
    emitter.instruction("jne __rt_mb_strwidth_utf8_invalid_x86");               // malformed partial tail substitutes only the leader
    emitter.instruction("jmp __rt_mb_strwidth_utf8_truncated_x86");             // group the valid three-byte prefix as width 1

    emitter.label("__rt_mb_strwidth_utf8_ascii_x86");
    emitter.instruction("inc r8");                                              // consume one ASCII byte
    emitter.instruction("jmp __rt_mb_strwidth_utf8_narrow_x86");                // ASCII is always width 1
    emitter.label("__rt_mb_strwidth_utf8_invalid_x86");
    emitter.instruction("inc r8");                                              // consume one malformed byte for mbstring substitution
    emitter.label("__rt_mb_strwidth_utf8_narrow_x86");
    emitter.instruction("inc rax");                                             // count one column for ASCII, 2-byte, or substituted input
    emitter.instruction("jmp __rt_mb_strwidth_utf8_loop_x86");                  // continue scanning the remaining bytes
    emitter.label("__rt_mb_strwidth_utf8_wide_x86");
    emitter.instruction("push rax");                                            // preserve accumulated width across the lookup
    emitter.instruction("push rsi");                                            // preserve the source pointer
    emitter.instruction("push rdx");                                            // preserve the source length
    emitter.instruction("push r8");                                             // preserve the byte index and keep the call 16-byte aligned
    emitter.instruction("mov edi, ecx");                                        // width-lookup argument is the decoded code point
    emitter.instruction("call __rt_mb_char_width");                             // look up PHP 8.5 East Asian Width
    emitter.instruction("mov ecx, eax");                                        // keep the looked-up width
    emitter.instruction("pop r8");                                              // restore the byte index
    emitter.instruction("pop rdx");                                             // restore the source length
    emitter.instruction("pop rsi");                                             // restore the source pointer
    emitter.instruction("pop rax");                                             // restore accumulated width
    emitter.instruction("add rax, rcx");                                        // add the looked-up width
    emitter.instruction("jmp __rt_mb_strwidth_utf8_loop_x86");                  // continue scanning the remaining bytes
    emitter.label("__rt_mb_strwidth_utf8_truncated_x86");
    emitter.instruction("inc rax");                                             // count the final valid truncated prefix as width 1
    emitter.instruction("ret");                                                 // no bytes remain beyond the truncated prefix
    emitter.label("__rt_mb_strwidth_utf8_done_x86");
    emitter.instruction("ret");                                                 // return the validated UTF-8 display width
}
