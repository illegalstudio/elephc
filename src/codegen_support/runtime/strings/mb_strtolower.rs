//! Purpose:
//! Emits `__rt_mb_strtolower`, the runtime helper for PHP's `mb_strtolower()`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//! - `crate::codegen::lower_inst::builtins::strings::lower_mb_strtolower()`.
//!
//! Key details:
//! - Omitted/null/`UTF-8`/`UTF8` apply PHP 8.5 full Unicode lowercase, including Final_Sigma
//!   and 1:N mappings such as `İ` → `i` + combining dot.
//! - `8bit`/`binary`/`7bit` lowercase ASCII `A-Z` per byte; unknown encodings throw
//!   a catchable `ValueError`.
//! - Mapping tables are generated from the same Unicode data Rust's `char::to_lowercase()`
//!   uses so AOT and Magician stay aligned.

use std::sync::OnceLock;

use crate::codegen_support::{
    abi,
    emit::Emitter,
    platform::Arch,
    runtime::{arrays::value_error, data::MB_STRTOLOWER_UNKNOWN_ENCODING_MSG},
};

/// Maximum explicit encoding-name length copied into the runtime's stack buffer.
const MAX_ENCODING_NAME_LEN: usize = 63;
/// Worst-case UTF-8 expansion factor for full Unicode lowercase.
const LOWERCASE_EXPAND: u64 = 4;
const GREEK_CAPITAL_SIGMA: u32 = 0x03A3;
const GREEK_SMALL_SIGMA: u32 = 0x03C3;
const GREEK_SMALL_FINAL_SIGMA: u32 = 0x03C2;

/// One 1:N lowercase expansion: source scalar plus up to three result scalars.
#[derive(Clone, Copy)]
struct LowerExpand {
    from: u32,
    count: u32,
    mapped: [u32; 3],
}

/// Generated Unicode lowercase tables shared by both target emitters.
struct LowerTables {
    one_to_one: Vec<(u32, u32)>,
    expansions: Vec<LowerExpand>,
    cased_ranges: Vec<(u32, u32)>,
    ignorable_ranges: Vec<(u32, u32)>,
}

/// Inclusive Case_Ignorable ranges used by PHP 8.5's language-agnostic Final_Sigma scan.
const IGNORABLE_RANGES: &[(u32, u32)] = &[
    (0x00AD, 0x00AD),
    (0x0300, 0x036F),
    (0x0483, 0x0489),
    (0x0591, 0x05BD),
    (0x05BF, 0x05BF),
    (0x05C1, 0x05C2),
    (0x05C4, 0x05C5),
    (0x05C7, 0x05C7),
    (0x0610, 0x061A),
    (0x064B, 0x065F),
    (0x0670, 0x0670),
    (0x06D6, 0x06DC),
    (0x06DF, 0x06E4),
    (0x06E7, 0x06E8),
    (0x06EA, 0x06ED),
    (0x0711, 0x0711),
    (0x0730, 0x074A),
    (0x07A6, 0x07B0),
    (0x07EB, 0x07F3),
    (0x07FD, 0x07FD),
    (0x0816, 0x0819),
    (0x081B, 0x0823),
    (0x0825, 0x0827),
    (0x0829, 0x082D),
    (0x0859, 0x085B),
    (0x0898, 0x089F),
    (0x08CA, 0x08E1),
    (0x08E3, 0x0902),
    (0x093A, 0x093A),
    (0x093C, 0x093C),
    (0x0941, 0x0948),
    (0x094D, 0x094D),
    (0x0951, 0x0957),
    (0x0962, 0x0963),
    (0x1AB0, 0x1ADE),
    (0x1AE0, 0x1AEB),
    (0x1DC0, 0x1DFF),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0x20D0, 0x20F0),
    (0xFE00, 0xFE0F),
    (0xFE20, 0xFE2F),
    (0xFEFF, 0xFEFF),
    (0xFF9E, 0xFF9F),
];

/// Emits `__rt_mb_strtolower(str_ptr, str_len, encoding_ptr, encoding_len) -> string`.
pub fn emit_mb_strtolower(emitter: &mut Emitter) {
    emit_mb_strtolower_tables(emitter);
    if emitter.target.arch == Arch::X86_64 {
        emit_mb_strtolower_x86_64(emitter);
    } else {
        emit_mb_strtolower_aarch64(emitter);
    }
}

/// Builds the Unicode lowercase, expansion, and cased-range tables once per process.
fn lower_tables() -> &'static LowerTables {
    static TABLES: OnceLock<LowerTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut one_to_one = Vec::new();
        let mut expansions = Vec::new();
        let mut cased = Vec::new();
        let mut cp = 0u32;
        while cp <= 0x10FFFF {
            if let Some(ch) = char::from_u32(cp) {
                let lower: Vec<char> = ch.to_lowercase().collect();
                let upper: Vec<char> = ch.to_uppercase().collect();
                if lower != [ch] || upper != [ch] {
                    cased.push(cp);
                }
                if lower.len() == 1 {
                    let mapped = lower[0] as u32;
                    if mapped != cp {
                        one_to_one.push((cp, mapped));
                    }
                } else if lower.len() > 1 {
                    let mut mapped = [0u32; 3];
                    for (index, next) in lower.iter().take(3).enumerate() {
                        mapped[index] = *next as u32;
                    }
                    expansions.push(LowerExpand {
                        from: cp,
                        count: lower.len().min(3) as u32,
                        mapped,
                    });
                }
            }
            cp += 1;
        }
        LowerTables {
            one_to_one,
            expansions,
            cased_ranges: merge_ranges(&cased),
            ignorable_ranges: IGNORABLE_RANGES.to_vec(),
        }
    })
}

/// Merges sorted scalars into inclusive `[start, end]` ranges.
fn merge_ranges(codepoints: &[u32]) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    let Some(mut start) = codepoints.first().copied() else {
        return ranges;
    };
    let mut end = start;
    for &cp in codepoints.iter().skip(1) {
        if cp == end + 1 {
            end = cp;
        } else {
            ranges.push((start, end));
            start = cp;
            end = cp;
        }
    }
    ranges.push((start, end));
    ranges
}

/// Emits the generated lowercase map, expansion table, and cased-range table.
fn emit_mb_strtolower_tables(emitter: &mut Emitter) {
    let tables = lower_tables();
    emitter.raw(".data");
    emitter.raw(".p2align 3");
    emitter.raw(".globl _mb_strtolower_map");
    emitter.raw("_mb_strtolower_map:");
    for (from, to) in &tables.one_to_one {
        emitter.raw(&format!("    .long {from}, {to}"));
    }
    emitter.raw(".globl _mb_strtolower_map_end");
    emitter.raw("_mb_strtolower_map_end:");
    emitter.raw(".globl _mb_strtolower_expand");
    emitter.raw("_mb_strtolower_expand:");
    for entry in &tables.expansions {
        emitter.raw(&format!(
            "    .long {}, {}, {}, {}, {}, 0",
            entry.from, entry.count, entry.mapped[0], entry.mapped[1], entry.mapped[2]
        ));
    }
    emitter.raw(".globl _mb_strtolower_expand_end");
    emitter.raw("_mb_strtolower_expand_end:");
    emitter.raw(".globl _mb_strtolower_cased");
    emitter.raw("_mb_strtolower_cased:");
    for (start, end) in &tables.cased_ranges {
        emitter.raw(&format!("    .long {start}, {end}"));
    }
    emitter.raw(".globl _mb_strtolower_cased_end");
    emitter.raw("_mb_strtolower_cased_end:");
    emitter.raw(".globl _mb_strtolower_ignorable");
    emitter.raw("_mb_strtolower_ignorable:");
    for (start, end) in &tables.ignorable_ranges {
        emitter.raw(&format!("    .long {start}, {end}"));
    }
    emitter.raw(".globl _mb_strtolower_ignorable_end");
    emitter.raw("_mb_strtolower_ignorable_end:");
    emitter.raw(".text");
}

/// Emits the AArch64 implementation for macOS and Linux.
fn emit_mb_strtolower_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_strtolower (UTF-8 full lowercase) ---");
    emitter.label_global("__rt_mb_strtolower");
    emitter.instruction("cbz x3, __rt_mb_strtolower_convert");                  // omitted/null encoding uses PHP 8.5 UTF-8 lowercase
    emitter.instruction(&format!("cmp x4, #{}", MAX_ENCODING_NAME_LEN));        // does the explicit encoding name fit the stack C-string buffer?
    emitter.instruction("b.hi __rt_mb_strtolower_unknown_encoding");            // reject names longer than every PHP-supported encoding alias
    emitter.instruction("sub sp, sp, #96");                                     // reserve encoding-name storage and a helper frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller frame and return address across libc calls
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("stp x1, x2, [sp, #0]");                                // preserve the source string across encoding-name compares

    // -- copy the length-delimited PHP encoding name into a stack C string --
    emitter.instruction("add x9, sp, #16");                                     // destination is the 64-byte encoding-name buffer
    emitter.instruction("mov x10, #0");                                         // copied-byte index starts at zero
    emitter.label("__rt_mb_strtolower_encoding_copy");
    emitter.instruction("cmp x10, x4");                                         // copied the whole explicit encoding name?
    emitter.instruction("b.hs __rt_mb_strtolower_encoding_copied");             // terminate the C string once every byte is copied
    emitter.instruction("ldrb w11, [x3, x10]");                                 // load one encoding-name byte from the PHP string
    emitter.instruction("strb w11, [x9, x10]");                                 // append the byte to the stack C string
    emitter.instruction("add x10, x10, #1");                                    // advance the encoding-name byte index
    emitter.instruction("b __rt_mb_strtolower_encoding_copy");                  // continue copying the remaining encoding-name bytes
    emitter.label("__rt_mb_strtolower_encoding_copied");
    emitter.instruction("strb wzr, [x9, x4]");                                  // NUL-terminate the explicit encoding name

    // -- accept UTF-8 names and byte-count encodings; reject everything else --
    emitter.instruction("add x0, sp, #16");                                     // first strcasecmp argument is the copied encoding name
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_utf8_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with UTF-8 case-insensitively
    emitter.instruction("cbz x0, __rt_mb_strtolower_use_utf8_framed");          // UTF-8 uses the Unicode lowercase converter
    emitter.instruction("add x0, sp, #16");                                     // reload the copied encoding name after strcasecmp
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_utf8_alias");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with PHP's UTF8 alias
    emitter.instruction("cbz x0, __rt_mb_strtolower_use_utf8_framed");          // the UTF8 alias uses the same Unicode converter
    emitter.instruction("add x0, sp, #16");                                     // reload the copied encoding name for the byte aliases
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_8bit_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with 8bit
    emitter.instruction("cbz x0, __rt_mb_strtolower_use_ascii_framed");         // 8bit lowercases ASCII A-Z per byte
    emitter.instruction("add x0, sp, #16");                                     // reload the copied encoding name for the binary alias
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_binary_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with binary
    emitter.instruction("cbz x0, __rt_mb_strtolower_use_ascii_framed");         // binary is PHP's alias for 8bit
    emitter.instruction("add x0, sp, #16");                                     // reload the copied encoding name for the 7bit encoding
    abi::emit_symbol_address(emitter, "x1", "_mb_strlen_7bit_name");
    emitter.bl_c("strcasecmp"); // compare the explicit encoding with 7bit
    emitter.instruction("cbz x0, __rt_mb_strtolower_use_ascii_framed");         // 7bit preserves PHP's per-byte ASCII lowercase
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame before throwing ValueError
    emitter.instruction("add sp, sp, #96");                                     // release the explicit-encoding helper frame before unwinding
    emitter.instruction("b __rt_mb_strtolower_unknown_encoding");               // unknown encoding names raise PHP's ValueError

    emitter.label("__rt_mb_strtolower_use_utf8_framed");
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // restore the PHP string for the UTF-8 converter
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #96");                                     // release the explicit-encoding helper frame
    emitter.instruction("mov w3, #0");                                          // request the Unicode UTF-8 converter
    emitter.instruction("b __rt_mb_strtolower_convert_mode");                   // tail-dispatch with the selected conversion mode
    emitter.label("__rt_mb_strtolower_use_ascii_framed");
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // restore the PHP string for the byte converter
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #96");                                     // release the explicit-encoding helper frame
    emitter.instruction("mov w3, #1");                                          // request the ASCII-byte converter
    emitter.instruction("b __rt_mb_strtolower_convert_mode");                   // tail-dispatch with the selected conversion mode

    emitter.label("__rt_mb_strtolower_unknown_encoding");
    value_error::emit_throw_value_error_aarch64(
        emitter,
        "_mb_strtolower_unknown_encoding_msg",
        MB_STRTOLOWER_UNKNOWN_ENCODING_MSG.len(),
    );

    emit_mb_strtolower_convert_aarch64(emitter);
}

/// Emits UTF-8 and ASCII conversion for the AArch64 runtime.
fn emit_mb_strtolower_convert_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtolower_convert");
    emitter.instruction("mov w3, #0");                                          // default omitted/null encoding is UTF-8 Unicode lowercase
    emitter.label("__rt_mb_strtolower_convert_mode");
    emitter.instruction("sub sp, sp, #96");                                     // reserve source, destination, and conversion-state slots
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller frame across reservation and publish
    emitter.instruction("add x29, sp, #80");                                    // establish the converter frame pointer
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save the source pointer and length across reservation
    emitter.instruction("str w3, [sp, #16]");                                   // save the selected conversion mode (0 = UTF-8, 1 = ASCII bytes)
    emitter.instruction("str wzr, [sp, #48]");                                  // no preceding cased letter at the start of the string
    emitter.instruction(&format!("mov x9, #{}", LOWERCASE_EXPAND));             // worst-case expansion factor for full Unicode lowercase
    emitter.instruction("umulh x10, x2, x9");                                   // capture the high half of the 4 * length product
    emitter.instruction("cbnz x10, __rt_mb_strtolower_size_overflow");          // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("mul x0, x2, x9");                                      // compute the worst-case expanded result size
    emitter.instruction("bl __rt_concat_reserve");                              // reserve scratch or heap storage for the lowered string
    emitter.instruction("mov x9, x0");                                          // destination write pointer starts at the reserved buffer
    emitter.instruction("str x0, [sp, #24]");                                   // remember the result start for the returned pointer
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload the borrowed source pointer and length
    emitter.instruction("ldr w3, [sp, #16]");                                   // reload the selected conversion mode
    emitter.instruction("cbnz w3, __rt_mb_strtolower_ascii_loop");              // byte encodings skip Unicode decoding

    emitter.label("__rt_mb_strtolower_utf8_loop");
    emitter.instruction("cbz x2, __rt_mb_strtolower_done");                     // publish once every source byte has been consumed
    emitter.instruction("bl __rt_mb_strtolower_decode");                        // decode the next UTF-8 scalar or malformed byte group
    emitter.instruction("cbz w4, __rt_mb_strtolower_copy_invalid");             // malformed bytes are copied unchanged
    emitter.instruction(&format!("mov w10, #{GREEK_CAPITAL_SIGMA}"));           // Greek capital sigma needs the Final_Sigma rule
    emitter.instruction("cmp w0, w10");                                         // is the current scalar capital sigma?
    emitter.instruction("b.eq __rt_mb_strtolower_sigma");                       // apply lookbehind/lookahead before emitting sigma
    emitter.instruction("bl __rt_mb_strtolower_map_emit");                      // map and encode one Unicode lowercase result
    emitter.instruction("b __rt_mb_strtolower_utf8_loop");                      // continue with the remaining source bytes

    emitter.label("__rt_mb_strtolower_sigma");
    emitter.instruction("stp x1, x2, [sp, #32]");                               // preserve the source cursor while peeking the next cased letter
    emitter.instruction("str w0, [sp, #20]");                                   // remember capital sigma while consulting neighbors
    emitter.instruction("bl __rt_mb_strtolower_next_cased");                    // lookahead: is a cased letter waiting after ignorable marks?
    emitter.instruction("mov w6, w0");                                          // w6 = following-cased flag
    emitter.instruction("ldp x1, x2, [sp, #32]");                               // restore the source cursor after the lookahead peek
    emitter.instruction("ldr w7, [sp, #48]");                                   // load the preceding-cased flag written by the last emitted scalar
    emitter.instruction("cbz w7, __rt_mb_strtolower_sigma_small");              // a leading sigma is never final
    emitter.instruction("cbnz w6, __rt_mb_strtolower_sigma_small");             // a medial sigma stays the ordinary small sigma
    emitter.instruction(&format!("mov w0, #{GREEK_SMALL_FINAL_SIGMA}"));        // end-of-word capital sigma becomes final small sigma
    emitter.instruction("b __rt_mb_strtolower_sigma_emit");                     // encode the chosen sigma
    emitter.label("__rt_mb_strtolower_sigma_small");
    emitter.instruction(&format!("mov w0, #{GREEK_SMALL_SIGMA}"));              // otherwise emit ordinary Greek small sigma
    emitter.label("__rt_mb_strtolower_sigma_emit");
    emitter.instruction("bl __rt_mb_strtolower_encode");                        // write the chosen sigma as UTF-8
    emitter.instruction("mov w8, #1");                                          // sigma is a cased letter for the next lookbehind
    emitter.instruction("str w8, [sp, #48]");                                   // persist the preceding-cased flag
    emitter.instruction("b __rt_mb_strtolower_utf8_loop");                      // continue after the sigma

    emitter.label("__rt_mb_strtolower_copy_invalid");
    emitter.instruction("strb w0, [x9], #1");                                   // copy one malformed byte unchanged
    emitter.instruction("str wzr, [sp, #48]");                                  // invalid bytes reset the Final_Sigma lookbehind
    emitter.instruction("b __rt_mb_strtolower_utf8_loop");                      // continue after the malformed group

    emitter.label("__rt_mb_strtolower_ascii_loop");
    emitter.instruction("cbz x2, __rt_mb_strtolower_done");                     // publish once every source byte has been consumed
    emitter.instruction("ldrb w0, [x1], #1");                                   // load the next source byte
    emitter.instruction("sub x2, x2, #1");                                      // consume one source byte
    emitter.instruction("cmp w0, #'A'");                                        // ASCII uppercase starts at 'A'
    emitter.instruction("b.lo __rt_mb_strtolower_ascii_store");                 // bytes below 'A' stay unchanged
    emitter.instruction("cmp w0, #'Z'");                                        // ASCII uppercase ends at 'Z'
    emitter.instruction("b.hi __rt_mb_strtolower_ascii_store");                 // bytes above 'Z' stay unchanged
    emitter.instruction("add w0, w0, #32");                                     // convert A-Z to a-z
    emitter.label("__rt_mb_strtolower_ascii_store");
    emitter.instruction("strb w0, [x9], #1");                                   // store the (possibly lowered) byte
    emitter.instruction("b __rt_mb_strtolower_ascii_loop");                     // continue with the remaining bytes

    emitter.label("__rt_mb_strtolower_done");
    emitter.instruction("ldr x1, [sp, #24]");                                   // result pointer is the reserved buffer start
    emitter.instruction("sub x2, x9, x1");                                      // result length is the number of bytes written
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch results
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #96");                                     // release the converter frame
    emitter.instruction("ret");                                                 // return the lowered string
    emitter.label("__rt_mb_strtolower_size_overflow");
    emitter.instruction("b __rt_alloc_overflow");                               // report PHP's allocation-overflow fatal

    emit_mb_strtolower_decode_aarch64(emitter);
    emit_mb_strtolower_map_emit_aarch64(emitter);
    emit_mb_strtolower_encode_aarch64(emitter);
    emit_mb_strtolower_lookup_aarch64(emitter);
    emit_mb_strtolower_next_cased_aarch64(emitter);
}

/// Emits one UTF-8 decode step: scalar in `w0`, consumed length already subtracted from `x2`.
///
/// `w4` is 1 for a valid scalar and 0 for a single malformed byte left in `w0`.
fn emit_mb_strtolower_decode_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtolower_decode");
    emitter.instruction("ldrb w0, [x1]");                                       // load the next possible UTF-8 leading byte
    emitter.instruction("cmp w0, #0x80");                                       // ASCII bytes are complete one-byte scalars
    emitter.instruction("b.lo __rt_mb_strtolower_decode_ascii");                // consume one ASCII byte
    emitter.instruction("cmp w0, #0xC2");                                       // C0/C1 and continuation bytes are malformed leaders
    emitter.instruction("b.lo __rt_mb_strtolower_decode_invalid");              // substitute one malformed byte
    emitter.instruction("cmp w0, #0xE0");                                       // C2-DF introduce two-byte sequences
    emitter.instruction("b.lo __rt_mb_strtolower_decode_two");                  // validate a two-byte character
    emitter.instruction("cmp w0, #0xF0");                                       // E0-EF introduce three-byte sequences
    emitter.instruction("b.lo __rt_mb_strtolower_decode_three");                // validate a three-byte character
    emitter.instruction("cmp w0, #0xF5");                                       // F0-F4 introduce Unicode-range four-byte sequences
    emitter.instruction("b.lo __rt_mb_strtolower_decode_four");                 // validate a four-byte character
    emitter.instruction("b __rt_mb_strtolower_decode_invalid");                 // F5-FF cannot begin valid UTF-8

    emitter.label("__rt_mb_strtolower_decode_ascii");
    emitter.instruction("add x1, x1, #1");                                      // consume the ASCII byte
    emitter.instruction("sub x2, x2, #1");                                      // one fewer source byte remains
    emitter.instruction("mov w4, #1");                                          // ASCII is a valid scalar
    emitter.instruction("ret");                                                 // return the one-byte code point
    emitter.label("__rt_mb_strtolower_decode_invalid");
    emitter.instruction("add x1, x1, #1");                                      // consume one malformed byte
    emitter.instruction("sub x2, x2, #1");                                      // one fewer source byte remains
    emitter.instruction("mov w4, #0");                                          // tell the caller to copy the byte unchanged
    emitter.instruction("ret");                                                 // return the malformed byte in w0

    emitter.label("__rt_mb_strtolower_decode_two");
    emitter.instruction("cmp x2, #2");                                          // is the sequence truncated before its continuation byte?
    emitter.instruction("b.lo __rt_mb_strtolower_decode_invalid");              // a truncated prefix is one malformed group
    emitter.instruction("ldrb w5, [x1, #1]");                                   // load the two-byte sequence continuation
    emitter.instruction("and w6, w5, #0xC0");                                   // isolate the continuation-byte prefix
    emitter.instruction("cmp w6, #0x80");                                       // does the second byte have the required 10xxxxxx shape?
    emitter.instruction("b.ne __rt_mb_strtolower_decode_invalid");              // malformed continuation leaves the leader substituted alone
    emitter.instruction("and w0, w0, #0x1F");                                   // keep the two-byte payload bits
    emitter.instruction("lsl w0, w0, #6");                                      // shift the leader payload into place
    emitter.instruction("and w5, w5, #0x3F");                                   // keep the continuation payload bits
    emitter.instruction("orr w0, w0, w5");                                      // assemble the two-byte scalar
    emitter.instruction("add x1, x1, #2");                                      // consume the complete two-byte character
    emitter.instruction("sub x2, x2, #2");                                      // two fewer source bytes remain
    emitter.instruction("mov w4, #1");                                          // the two-byte sequence is valid
    emitter.instruction("ret");                                                 // return the decoded scalar

    emitter.label("__rt_mb_strtolower_decode_three");
    emitter.instruction("cmp x2, #3");                                          // are both continuation bytes available?
    emitter.instruction("b.lo __rt_mb_strtolower_decode_invalid");              // a truncated three-byte prefix is malformed here
    emitter.instruction("ldrb w5, [x1, #1]");                                   // load the first three-byte continuation
    emitter.instruction("and w6, w5, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w6, #0x80");                                       // is the first continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strtolower_decode_invalid");              // malformed continuation substitutes only the leader
    emitter.instruction("cmp w0, #0xE0");                                       // E0 requires a second byte at least A0 to avoid overlong UTF-8
    emitter.instruction("b.ne __rt_mb_strtolower_decode_three_not_e0");         // skip the E0 lower-bound check for other leaders
    emitter.instruction("cmp w5, #0xA0");                                       // is the E0 continuation inside the non-overlong range?
    emitter.instruction("b.lo __rt_mb_strtolower_decode_invalid");              // reject an overlong three-byte sequence
    emitter.label("__rt_mb_strtolower_decode_three_not_e0");
    emitter.instruction("cmp w0, #0xED");                                       // ED requires a second byte below A0 to exclude UTF-16 surrogates
    emitter.instruction("b.ne __rt_mb_strtolower_decode_three_second");         // skip the surrogate bound for other leaders
    emitter.instruction("cmp w5, #0xA0");                                       // does the ED continuation enter the surrogate range?
    emitter.instruction("b.hs __rt_mb_strtolower_decode_invalid");              // reject UTF-8 encodings of surrogate code points
    emitter.label("__rt_mb_strtolower_decode_three_second");
    emitter.instruction("ldrb w6, [x1, #2]");                                   // load the final three-byte continuation
    emitter.instruction("and w7, w6, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w7, #0x80");                                       // is the final continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strtolower_decode_invalid");              // malformed final byte substitutes only the leader
    emitter.instruction("and w0, w0, #0x0F");                                   // keep the three-byte leader payload
    emitter.instruction("lsl w0, w0, #12");                                     // shift the leader payload into place
    emitter.instruction("and w5, w5, #0x3F");                                   // keep the first continuation payload
    emitter.instruction("lsl w5, w5, #6");                                      // shift the first continuation into place
    emitter.instruction("orr w0, w0, w5");                                      // merge leader and first continuation
    emitter.instruction("and w6, w6, #0x3F");                                   // keep the final continuation payload
    emitter.instruction("orr w0, w0, w6");                                      // assemble the three-byte scalar
    emitter.instruction("add x1, x1, #3");                                      // consume the complete three-byte character
    emitter.instruction("sub x2, x2, #3");                                      // three fewer source bytes remain
    emitter.instruction("mov w4, #1");                                          // the three-byte sequence is valid
    emitter.instruction("ret");                                                 // return the decoded scalar

    emitter.label("__rt_mb_strtolower_decode_four");
    emitter.instruction("cmp x2, #4");                                          // are all three continuation bytes available?
    emitter.instruction("b.lo __rt_mb_strtolower_decode_invalid");              // a truncated four-byte prefix is malformed here
    emitter.instruction("ldrb w5, [x1, #1]");                                   // load the first four-byte continuation
    emitter.instruction("and w6, w5, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w6, #0x80");                                       // is the first continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strtolower_decode_invalid");              // malformed continuation substitutes only the leader
    emitter.instruction("cmp w0, #0xF0");                                       // F0 requires a second byte at least 90 to avoid overlong UTF-8
    emitter.instruction("b.ne __rt_mb_strtolower_decode_four_not_f0");          // skip the F0 lower-bound check for other leaders
    emitter.instruction("cmp w5, #0x90");                                       // is the F0 continuation inside the non-overlong range?
    emitter.instruction("b.lo __rt_mb_strtolower_decode_invalid");              // reject an overlong four-byte sequence
    emitter.label("__rt_mb_strtolower_decode_four_not_f0");
    emitter.instruction("cmp w0, #0xF4");                                       // F4 requires a second byte below 90 for Unicode's maximum scalar
    emitter.instruction("b.ne __rt_mb_strtolower_decode_four_rest");            // skip the upper bound for F0-F3
    emitter.instruction("cmp w5, #0x90");                                       // does the F4 continuation exceed U+10FFFF?
    emitter.instruction("b.hs __rt_mb_strtolower_decode_invalid");              // reject out-of-range four-byte sequences
    emitter.label("__rt_mb_strtolower_decode_four_rest");
    emitter.instruction("ldrb w6, [x1, #2]");                                   // load the second four-byte continuation
    emitter.instruction("and w7, w6, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w7, #0x80");                                       // is the second continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strtolower_decode_invalid");              // malformed continuation substitutes only the leader
    emitter.instruction("ldrb w7, [x1, #3]");                                   // load the final four-byte continuation
    emitter.instruction("and w8, w7, #0xC0");                                   // isolate its continuation-byte prefix
    emitter.instruction("cmp w8, #0x80");                                       // is the final continuation structurally valid?
    emitter.instruction("b.ne __rt_mb_strtolower_decode_invalid");              // malformed continuation substitutes only the leader
    emitter.instruction("and w0, w0, #0x07");                                   // keep the four-byte leader payload
    emitter.instruction("lsl w0, w0, #18");                                     // shift the leader payload into place
    emitter.instruction("and w5, w5, #0x3F");                                   // keep the first continuation payload
    emitter.instruction("lsl w5, w5, #12");                                     // shift the first continuation into place
    emitter.instruction("orr w0, w0, w5");                                      // merge leader and first continuation
    emitter.instruction("and w6, w6, #0x3F");                                   // keep the second continuation payload
    emitter.instruction("lsl w6, w6, #6");                                      // shift the second continuation into place
    emitter.instruction("orr w0, w0, w6");                                      // merge the second continuation
    emitter.instruction("and w7, w7, #0x3F");                                   // keep the final continuation payload
    emitter.instruction("orr w0, w0, w7");                                      // assemble the four-byte scalar
    emitter.instruction("add x1, x1, #4");                                      // consume the complete four-byte character
    emitter.instruction("sub x2, x2, #4");                                      // four fewer source bytes remain
    emitter.instruction("mov w4, #1");                                          // the four-byte sequence is valid
    emitter.instruction("ret");                                                 // return the decoded scalar
}

/// Maps one scalar through the 1:1 / expansion tables and encodes the result.
fn emit_mb_strtolower_map_emit_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtolower_map_emit");
    emitter.instruction("str x30, [sp, #64]");                                  // preserve the converter return address across nested helper calls
    emitter.instruction("str w0, [sp, #20]");                                   // remember the original scalar for the lookbehind update
    emitter.instruction("bl __rt_mb_strtolower_expand_lookup");                 // try a 1:N full-case mapping first
    emitter.instruction("cbnz w5, __rt_mb_strtolower_map_expand");              // expansions write every mapped scalar
    emitter.instruction("ldr w0, [sp, #20]");                                   // restore the original scalar for the 1:1 table
    emitter.instruction("bl __rt_mb_strtolower_lookup");                        // replace it with its simple/full 1:1 lowercase mapping
    emitter.instruction("bl __rt_mb_strtolower_encode");                        // write the mapped scalar as UTF-8
    emitter.instruction("b __rt_mb_strtolower_map_cased");                      // update the preceding-cased flag from the original scalar
    emitter.label("__rt_mb_strtolower_map_expand");
    emitter.instruction("mov w10, w5");                                         // remaining expansion scalars
    emitter.instruction("mov x11, x6");                                         // pointer to the first mapped scalar
    emitter.label("__rt_mb_strtolower_map_expand_loop");
    emitter.instruction("cbz w10, __rt_mb_strtolower_map_cased");               // all expansion scalars have been encoded
    emitter.instruction("ldr w0, [x11], #4");                                   // load the next mapped scalar
    emitter.instruction("str w10, [sp, #52]");                                  // preserve the remaining expansion count
    emitter.instruction("str x11, [sp, #56]");                                  // preserve the expansion cursor
    emitter.instruction("bl __rt_mb_strtolower_encode");                        // write one expanded scalar as UTF-8
    emitter.instruction("ldr w10, [sp, #52]");                                  // restore the remaining expansion count
    emitter.instruction("ldr x11, [sp, #56]");                                  // restore the expansion cursor
    emitter.instruction("sub w10, w10, #1");                                    // one fewer expansion scalar remains
    emitter.instruction("b __rt_mb_strtolower_map_expand_loop");                // continue encoding the expansion
    emitter.label("__rt_mb_strtolower_map_cased");
    emitter.instruction("ldr w0, [sp, #20]");                                   // restore the original scalar
    emitter.instruction("bl __rt_mb_strtolower_is_cased");                      // was the original scalar a cased letter?
    emitter.instruction("cbnz w0, __rt_mb_strtolower_map_set_cased");           // a cased letter becomes the new lookbehind
    emitter.instruction("ldr w0, [sp, #20]");                                   // restore the original scalar for the ignorable test
    emitter.instruction("bl __rt_mb_strtolower_is_ignorable");                  // Case_Ignorable marks keep the previous lookbehind
    emitter.instruction("cbnz w0, __rt_mb_strtolower_map_keep_cased");          // combining marks do not reset Final_Sigma lookbehind
    emitter.instruction("str wzr, [sp, #48]");                                  // a non-cased, non-ignorable scalar is a word boundary
    emitter.instruction("b __rt_mb_strtolower_map_keep_cased");                 // restore the converter return address
    emitter.label("__rt_mb_strtolower_map_set_cased");
    emitter.instruction("mov w0, #1");                                          // persist a cased lookbehind
    emitter.instruction("str w0, [sp, #48]");                                   // store the preceding-cased flag for Final_Sigma
    emitter.label("__rt_mb_strtolower_map_keep_cased");
    emitter.instruction("ldr x30, [sp, #64]");                                  // restore the converter return address
    emitter.instruction("ret");                                                 // return to the UTF-8 conversion loop
}

/// Encodes the scalar in `w0` as UTF-8 at `x9` and advances `x9`.
fn emit_mb_strtolower_encode_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtolower_encode");
    emitter.instruction("cmp w0, #0x80");                                       // one-byte UTF-8 is the ASCII range
    emitter.instruction("b.lo __rt_mb_strtolower_encode_1");                    // store a single ASCII byte
    emitter.instruction("cmp w0, #0x800");                                      // two-byte UTF-8 goes up to U+07FF
    emitter.instruction("b.lo __rt_mb_strtolower_encode_2");                    // store a two-byte sequence
    emitter.instruction("cmp w0, #0x10000");                                    // three-byte UTF-8 goes up to U+FFFF
    emitter.instruction("b.lo __rt_mb_strtolower_encode_3");                    // store a three-byte sequence
    emitter.instruction("lsr w5, w0, #18");                                     // four-byte leader payload
    emitter.instruction("orr w5, w5, #0xF0");                                   // 11110xxx leader
    emitter.instruction("strb w5, [x9], #1");                                   // store the four-byte leader
    emitter.instruction("lsr w5, w0, #12");                                     // second-byte payload
    emitter.instruction("and w5, w5, #0x3F");                                   // keep six bits
    emitter.instruction("orr w5, w5, #0x80");                                   // 10xxxxxx continuation
    emitter.instruction("strb w5, [x9], #1");                                   // store the second byte
    emitter.instruction("lsr w5, w0, #6");                                      // third-byte payload
    emitter.instruction("and w5, w5, #0x3F");                                   // keep six bits
    emitter.instruction("orr w5, w5, #0x80");                                   // 10xxxxxx continuation
    emitter.instruction("strb w5, [x9], #1");                                   // store the third byte
    emitter.instruction("and w5, w0, #0x3F");                                   // final six bits
    emitter.instruction("orr w5, w5, #0x80");                                   // 10xxxxxx continuation
    emitter.instruction("strb w5, [x9], #1");                                   // store the fourth byte
    emitter.instruction("ret");                                                 // return after the four-byte store
    emitter.label("__rt_mb_strtolower_encode_1");
    emitter.instruction("strb w0, [x9], #1");                                   // store the ASCII byte
    emitter.instruction("ret");                                                 // return after the one-byte store
    emitter.label("__rt_mb_strtolower_encode_2");
    emitter.instruction("lsr w5, w0, #6");                                      // two-byte leader payload
    emitter.instruction("orr w5, w5, #0xC0");                                   // 110xxxxx leader
    emitter.instruction("strb w5, [x9], #1");                                   // store the two-byte leader
    emitter.instruction("and w5, w0, #0x3F");                                   // low six bits
    emitter.instruction("orr w5, w5, #0x80");                                   // 10xxxxxx continuation
    emitter.instruction("strb w5, [x9], #1");                                   // store the continuation
    emitter.instruction("ret");                                                 // return after the two-byte store
    emitter.label("__rt_mb_strtolower_encode_3");
    emitter.instruction("lsr w5, w0, #12");                                     // three-byte leader payload
    emitter.instruction("orr w5, w5, #0xE0");                                   // 1110xxxx leader
    emitter.instruction("strb w5, [x9], #1");                                   // store the three-byte leader
    emitter.instruction("lsr w5, w0, #6");                                      // second-byte payload
    emitter.instruction("and w5, w5, #0x3F");                                   // keep six bits
    emitter.instruction("orr w5, w5, #0x80");                                   // 10xxxxxx continuation
    emitter.instruction("strb w5, [x9], #1");                                   // store the second byte
    emitter.instruction("and w5, w0, #0x3F");                                   // final six bits
    emitter.instruction("orr w5, w5, #0x80");                                   // 10xxxxxx continuation
    emitter.instruction("strb w5, [x9], #1");                                   // store the third byte
    emitter.instruction("ret");                                                 // return after the three-byte store
}

/// Binary-searches the 1:1 map and linearly searches the small expansion table.
fn emit_mb_strtolower_lookup_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtolower_lookup");
    abi::emit_symbol_address(emitter, "x5", "_mb_strtolower_map");
    abi::emit_symbol_address(emitter, "x6", "_mb_strtolower_map_end");
    emitter.label("__rt_mb_strtolower_lookup_loop");
    emitter.instruction("cmp x5, x6");                                          // empty remaining search window?
    emitter.instruction("b.hs __rt_mb_strtolower_lookup_miss");                 // unmapped scalars stay unchanged
    emitter.instruction("sub x7, x6, x5");                                      // byte length of the remaining window
    emitter.instruction("lsr x7, x7, #4");                                      // half the entry count, in entries (`(count/2)*8`)
    emitter.instruction("lsl x7, x7, #3");                                      // convert that midpoint back to a byte offset
    emitter.instruction("add x8, x5, x7");                                      // point at the midpoint entry
    emitter.instruction("ldr w10, [x8]");                                       // load the midpoint source scalar
    emitter.instruction("cmp w10, w0");                                         // compare against the query
    emitter.instruction("b.eq __rt_mb_strtolower_lookup_hit");                  // exact mapping found
    emitter.instruction("b.lo __rt_mb_strtolower_lookup_right");                // query is above this midpoint
    emitter.instruction("mov x6, x8");                                          // shrink the window to the left half
    emitter.instruction("b __rt_mb_strtolower_lookup_loop");                    // continue the search
    emitter.label("__rt_mb_strtolower_lookup_right");
    emitter.instruction("add x5, x8, #8");                                      // shrink the window to the right half
    emitter.instruction("b __rt_mb_strtolower_lookup_loop");                    // continue the search
    emitter.label("__rt_mb_strtolower_lookup_hit");
    emitter.instruction("ldr w0, [x8, #4]");                                    // replace the query with its lowercase mapping
    emitter.label("__rt_mb_strtolower_lookup_miss");
    emitter.instruction("ret");                                                 // return the mapped or original scalar

    emitter.label("__rt_mb_strtolower_expand_lookup");
    abi::emit_symbol_address(emitter, "x11", "_mb_strtolower_expand");
    abi::emit_symbol_address(emitter, "x6", "_mb_strtolower_expand_end");
    emitter.label("__rt_mb_strtolower_expand_loop");
    emitter.instruction("cmp x11, x6");                                         // scanned every expansion entry?
    emitter.instruction("b.hs __rt_mb_strtolower_expand_done");                 // no 1:N mapping for this scalar
    emitter.instruction("ldr w10, [x11]");                                      // load this expansion's source scalar
    emitter.instruction("cmp w10, w0");                                         // does this entry match the query?
    emitter.instruction("b.eq __rt_mb_strtolower_expand_hit");                  // use this expansion
    emitter.instruction("add x11, x11, #24");                                   // advance to the next 24-byte expansion record
    emitter.instruction("b __rt_mb_strtolower_expand_loop");                    // continue the linear scan
    emitter.label("__rt_mb_strtolower_expand_hit");
    emitter.instruction("ldr w5, [x11, #4]");                                   // load the expansion scalar count
    emitter.instruction("add x6, x11, #8");                                     // point at the first mapped scalar
    emitter.instruction("ret");                                                 // return count and mapped-scalar pointer
    emitter.label("__rt_mb_strtolower_expand_done");
    emitter.instruction("mov w5, #0");                                          // no expansion for this scalar
    emitter.instruction("ret");                                                 // return count=0 when no expansion matched
}

/// Peeks whether a cased letter follows after skipping Case_Ignorable scalars.
fn emit_mb_strtolower_next_cased_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtolower_next_cased");
    emitter.instruction("str x30, [sp, #64]");                                  // preserve the converter return address across decode/cased calls
    emitter.label("__rt_mb_strtolower_next_cased_loop");
    emitter.instruction("cbz x2, __rt_mb_strtolower_next_cased_no");            // end of string: no following cased letter
    emitter.instruction("bl __rt_mb_strtolower_decode");                        // decode the next scalar or malformed byte without committing it to output
    emitter.instruction("cbz w4, __rt_mb_strtolower_next_cased_no");            // malformed bytes break the word
    emitter.instruction("str w0, [sp, #20]");                                   // remember the peeked scalar across the ignorable test
    emitter.instruction("bl __rt_mb_strtolower_is_ignorable");                  // skip Case_Ignorable marks
    emitter.instruction("cbnz w0, __rt_mb_strtolower_next_cased_loop");         // continue past ignorable marks
    emitter.instruction("ldr w0, [sp, #20]");                                   // restore the first non-ignorable peeked scalar
    emitter.instruction("bl __rt_mb_strtolower_is_cased");                      // is the first non-ignorable scalar a cased letter?
    emitter.instruction("ldr x30, [sp, #64]");                                  // restore the converter return address
    emitter.instruction("ret");                                                 // w0 is the following-cased flag
    emitter.label("__rt_mb_strtolower_next_cased_no");
    emitter.instruction("mov w0, #0");                                          // no following cased letter
    emitter.instruction("ldr x30, [sp, #64]");                                  // restore the converter return address
    emitter.instruction("ret");                                                 // report a word boundary

    emitter.label("__rt_mb_strtolower_is_cased");
    abi::emit_symbol_address(emitter, "x5", "_mb_strtolower_cased");
    abi::emit_symbol_address(emitter, "x6", "_mb_strtolower_cased_end");
    emitter.label("__rt_mb_strtolower_is_cased_loop");
    emitter.instruction("cmp x5, x6");                                          // empty remaining range window?
    emitter.instruction("b.hs __rt_mb_strtolower_is_cased_no");                 // not in any cased range
    emitter.instruction("sub x7, x6, x5");                                      // byte length of the remaining window
    emitter.instruction("lsr x7, x7, #4");                                      // half the range count, in ranges
    emitter.instruction("lsl x7, x7, #3");                                      // convert that midpoint back to a byte offset
    emitter.instruction("add x8, x5, x7");                                      // point at the midpoint range
    emitter.instruction("ldr w10, [x8]");                                       // load the range start
    emitter.instruction("ldr w11, [x8, #4]");                                   // load the inclusive range end
    emitter.instruction("cmp w0, w10");                                         // is the query below this range?
    emitter.instruction("b.lo __rt_mb_strtolower_is_cased_left");               // search the lower half
    emitter.instruction("cmp w0, w11");                                         // is the query above this range?
    emitter.instruction("b.hi __rt_mb_strtolower_is_cased_right");              // search the upper half
    emitter.instruction("mov w0, #1");                                          // the query sits inside a cased range
    emitter.instruction("ret");                                                 // report a cased letter
    emitter.label("__rt_mb_strtolower_is_cased_left");
    emitter.instruction("mov x6, x8");                                          // shrink the window to the left half
    emitter.instruction("b __rt_mb_strtolower_is_cased_loop");                  // continue the search
    emitter.label("__rt_mb_strtolower_is_cased_right");
    emitter.instruction("add x5, x8, #8");                                      // shrink the window to the right half
    emitter.instruction("b __rt_mb_strtolower_is_cased_loop");                  // continue the search
    emitter.label("__rt_mb_strtolower_is_cased_no");
    emitter.instruction("mov w0, #0");                                          // not a cased letter
    emitter.instruction("ret");                                                 // report an uncased scalar

    emitter.label("__rt_mb_strtolower_is_ignorable");
    abi::emit_symbol_address(emitter, "x5", "_mb_strtolower_ignorable");
    abi::emit_symbol_address(emitter, "x6", "_mb_strtolower_ignorable_end");
    emitter.label("__rt_mb_strtolower_is_ignorable_loop");
    emitter.instruction("cmp x5, x6");                                          // empty remaining range window?
    emitter.instruction("b.hs __rt_mb_strtolower_is_ignorable_no");             // not in any ignorable range
    emitter.instruction("sub x7, x6, x5");                                      // byte length of the remaining window
    emitter.instruction("lsr x7, x7, #4");                                      // half the range count, in ranges
    emitter.instruction("lsl x7, x7, #3");                                      // convert that midpoint back to a byte offset
    emitter.instruction("add x8, x5, x7");                                      // point at the midpoint range
    emitter.instruction("ldr w10, [x8]");                                       // load the range start
    emitter.instruction("ldr w11, [x8, #4]");                                   // load the inclusive range end
    emitter.instruction("cmp w0, w10");                                         // is the query below this range?
    emitter.instruction("b.lo __rt_mb_strtolower_is_ignorable_left");           // search the lower half
    emitter.instruction("cmp w0, w11");                                         // is the query above this range?
    emitter.instruction("b.hi __rt_mb_strtolower_is_ignorable_right");          // search the upper half
    emitter.instruction("mov w0, #1");                                          // the query sits inside an ignorable range
    emitter.instruction("ret");                                                 // report a Case_Ignorable mark
    emitter.label("__rt_mb_strtolower_is_ignorable_left");
    emitter.instruction("mov x6, x8");                                          // shrink the window to the left half
    emitter.instruction("b __rt_mb_strtolower_is_ignorable_loop");              // continue the search
    emitter.label("__rt_mb_strtolower_is_ignorable_right");
    emitter.instruction("add x5, x8, #8");                                      // shrink the window to the right half
    emitter.instruction("b __rt_mb_strtolower_is_ignorable_loop");              // continue the search
    emitter.label("__rt_mb_strtolower_is_ignorable_no");
    emitter.instruction("mov w0, #0");                                          // not Case_Ignorable
    emitter.instruction("ret");                                                 // report a non-ignorable scalar
}

/// Emits the Linux x86_64 implementation.
fn emit_mb_strtolower_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_strtolower (UTF-8 full lowercase) ---");
    emitter.label_global("__rt_mb_strtolower");
    emitter.instruction("test r8, r8");                                         // omitted/null encoding is represented by a null pointer
    emitter.instruction("jz __rt_mb_strtolower_convert_x86");                   // use UTF-8 Unicode lowercase when encoding is omitted/null
    emitter.instruction(&format!("cmp r9, {}", MAX_ENCODING_NAME_LEN));         // does the encoding name fit the stack C-string buffer?
    emitter.instruction("ja __rt_mb_strtolower_unknown_encoding_x86");          // reject names longer than every PHP-supported alias
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across libc calls
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned helper frame
    emitter.instruction("sub rsp, 96");                                         // reserve encoding-name storage and the source string
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the source string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the source string length

    // -- copy the length-delimited PHP encoding name into a stack C string --
    emitter.instruction("lea rdi, [rbp - 80]");                                 // destination is the 64-byte encoding-name buffer
    emitter.instruction("xor rcx, rcx");                                        // copied-byte index starts at zero
    emitter.label("__rt_mb_strtolower_encoding_copy_x86");
    emitter.instruction("cmp rcx, r9");                                         // copied the whole explicit encoding name?
    emitter.instruction("jae __rt_mb_strtolower_encoding_copied_x86");          // terminate the C string once every byte is copied
    emitter.instruction("mov r10b, BYTE PTR [r8 + rcx]");                       // load one encoding-name byte from the PHP string
    emitter.instruction("mov BYTE PTR [rdi + rcx], r10b");                      // append the byte to the stack C string
    emitter.instruction("inc rcx");                                             // advance the encoding-name byte index
    emitter.instruction("jmp __rt_mb_strtolower_encoding_copy_x86");            // continue copying the remaining encoding-name bytes
    emitter.label("__rt_mb_strtolower_encoding_copied_x86");
    emitter.instruction("mov BYTE PTR [rdi + r9], 0");                          // NUL-terminate the explicit encoding name

    emitter.instruction("lea rdi, [rbp - 80]");                                 // first strcasecmp argument is the copied encoding name
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_utf8_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with UTF-8 case-insensitively
    emitter.instruction("test eax, eax");                                       // did the encoding match UTF-8?
    emitter.instruction("jz __rt_mb_strtolower_use_utf8_framed_x86");           // UTF-8 uses the Unicode lowercase converter
    emitter.instruction("lea rdi, [rbp - 80]");                                 // reload the copied encoding name after strcasecmp
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_utf8_alias");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with PHP's UTF8 alias
    emitter.instruction("test eax, eax");                                       // did the encoding match UTF8?
    emitter.instruction("jz __rt_mb_strtolower_use_utf8_framed_x86");           // the UTF8 alias uses the same Unicode converter
    emitter.instruction("lea rdi, [rbp - 80]");                                 // reload the copied encoding name for the byte aliases
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_8bit_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with 8bit
    emitter.instruction("test eax, eax");                                       // did the encoding match 8bit?
    emitter.instruction("jz __rt_mb_strtolower_use_ascii_framed_x86");          // 8bit lowercases ASCII A-Z per byte
    emitter.instruction("lea rdi, [rbp - 80]");                                 // reload the copied encoding name for the binary alias
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_binary_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with binary
    emitter.instruction("test eax, eax");                                       // did the encoding match binary?
    emitter.instruction("jz __rt_mb_strtolower_use_ascii_framed_x86");          // binary is PHP's alias for 8bit
    emitter.instruction("lea rdi, [rbp - 80]");                                 // reload the copied encoding name for the 7bit encoding
    abi::emit_symbol_address(emitter, "rsi", "_mb_strlen_7bit_name");
    emitter.instruction("call strcasecmp");                                     // compare the explicit encoding with 7bit
    emitter.instruction("test eax, eax");                                       // did the encoding match 7bit?
    emitter.instruction("jz __rt_mb_strtolower_use_ascii_framed_x86");          // 7bit preserves PHP's per-byte ASCII lowercase
    emitter.instruction("leave");                                               // release the explicit-encoding helper frame before unwinding
    emitter.instruction("jmp __rt_mb_strtolower_unknown_encoding_x86");         // unknown encoding names raise PHP's ValueError

    emitter.label("__rt_mb_strtolower_use_utf8_framed_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the PHP string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the PHP string length
    emitter.instruction("leave");                                               // release the explicit-encoding helper frame
    emitter.instruction("xor r8d, r8d");                                        // request the Unicode UTF-8 converter
    emitter.instruction("jmp __rt_mb_strtolower_convert_mode_x86");             // tail-dispatch with the selected conversion mode
    emitter.label("__rt_mb_strtolower_use_ascii_framed_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the PHP string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the PHP string length
    emitter.instruction("leave");                                               // release the explicit-encoding helper frame
    emitter.instruction("mov r8d, 1");                                          // request the ASCII-byte converter
    emitter.instruction("jmp __rt_mb_strtolower_convert_mode_x86");             // tail-dispatch with the selected conversion mode

    emitter.label("__rt_mb_strtolower_unknown_encoding_x86");
    value_error::emit_throw_value_error_x86_64(
        emitter,
        "_mb_strtolower_unknown_encoding_msg",
        MB_STRTOLOWER_UNKNOWN_ENCODING_MSG.len(),
    );

    emit_mb_strtolower_convert_x86_64(emitter);
}

/// Emits UTF-8 and ASCII conversion for the Linux x86_64 runtime.
fn emit_mb_strtolower_convert_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtolower_convert_x86");
    emitter.instruction("xor r8d, r8d");                                        // default omitted/null encoding is UTF-8 Unicode lowercase
    emitter.label("__rt_mb_strtolower_convert_mode_x86");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish an aligned converter frame
    emitter.instruction("sub rsp, 64");                                         // reserve source, destination, and conversion-state slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source pointer across reservation
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source length across reservation
    emitter.instruction("mov DWORD PTR [rbp - 20], r8d");                       // save the selected conversion mode
    emitter.instruction("mov DWORD PTR [rbp - 52], 0");                         // no preceding cased letter at the start of the string
    emitter.instruction("imul rax, rdx, 4");                                    // compute the worst-case expanded result size
    emitter.instruction("jo __rt_mb_strtolower_size_overflow_x86");             // reject a wrapped size instead of reserving a too-small destination
    emitter.instruction("call __rt_concat_reserve");                            // reserve scratch or heap storage for the lowered string
    emitter.instruction("mov r9, rax");                                         // destination write pointer starts at the reserved buffer
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // remember the result start for the returned pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the borrowed source pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the borrowed source length
    emitter.instruction("cmp DWORD PTR [rbp - 20], 0");                         // UTF-8 mode?
    emitter.instruction("jne __rt_mb_strtolower_ascii_loop_x86");               // byte encodings skip Unicode decoding

    emitter.label("__rt_mb_strtolower_utf8_loop_x86");
    emitter.instruction("test rdx, rdx");                                       // any source bytes remaining?
    emitter.instruction("jz __rt_mb_strtolower_done_x86");                      // publish once every source byte has been consumed
    emitter.instruction("call __rt_mb_strtolower_decode_x86");                  // decode the next UTF-8 scalar or malformed byte group
    emitter.instruction("test r8d, r8d");                                       // valid scalar?
    emitter.instruction("jz __rt_mb_strtolower_copy_invalid_x86");              // malformed bytes are copied unchanged
    emitter.instruction(&format!("cmp eax, {GREEK_CAPITAL_SIGMA}"));            // is the current scalar capital sigma?
    emitter.instruction("je __rt_mb_strtolower_sigma_x86");                     // apply lookbehind/lookahead before emitting sigma
    emitter.instruction("call __rt_mb_strtolower_map_emit_x86");                // map and encode one Unicode lowercase result
    emitter.instruction("jmp __rt_mb_strtolower_utf8_loop_x86");                // continue with the remaining source bytes

    emitter.label("__rt_mb_strtolower_sigma_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rsi");                       // preserve the source pointer while peeking
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // preserve the remaining length while peeking
    emitter.instruction("call __rt_mb_strtolower_next_cased_x86");              // lookahead: is a cased letter waiting after ignorable marks?
    emitter.instruction("mov r10d, eax");                                       // r10d = following-cased flag
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // restore the source pointer after the lookahead peek
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // restore the remaining length after the lookahead peek
    emitter.instruction("cmp DWORD PTR [rbp - 52], 0");                         // preceding-cased flag
    emitter.instruction("je __rt_mb_strtolower_sigma_small_x86");               // a leading sigma is never final
    emitter.instruction("test r10d, r10d");                                     // a following cased letter makes this medial
    emitter.instruction("jnz __rt_mb_strtolower_sigma_small_x86");              // a medial sigma stays the ordinary small sigma
    emitter.instruction(&format!("mov eax, {GREEK_SMALL_FINAL_SIGMA}"));        // end-of-word capital sigma becomes final small sigma
    emitter.instruction("jmp __rt_mb_strtolower_sigma_emit_x86");               // encode the chosen sigma
    emitter.label("__rt_mb_strtolower_sigma_small_x86");
    emitter.instruction(&format!("mov eax, {GREEK_SMALL_SIGMA}"));              // otherwise emit ordinary Greek small sigma
    emitter.label("__rt_mb_strtolower_sigma_emit_x86");
    emitter.instruction("call __rt_mb_strtolower_encode_x86");                  // write the chosen sigma as UTF-8
    emitter.instruction("mov DWORD PTR [rbp - 52], 1");                         // sigma is a cased letter for the next lookbehind
    emitter.instruction("jmp __rt_mb_strtolower_utf8_loop_x86");                // continue after the sigma

    emitter.label("__rt_mb_strtolower_copy_invalid_x86");
    emitter.instruction("mov BYTE PTR [r9], al");                               // copy one malformed byte unchanged
    emitter.instruction("inc r9");                                              // advance the destination write pointer
    emitter.instruction("mov DWORD PTR [rbp - 52], 0");                         // invalid bytes reset the Final_Sigma lookbehind
    emitter.instruction("jmp __rt_mb_strtolower_utf8_loop_x86");                // continue after the malformed group

    emitter.label("__rt_mb_strtolower_ascii_loop_x86");
    emitter.instruction("test rdx, rdx");                                       // any source bytes remaining?
    emitter.instruction("jz __rt_mb_strtolower_done_x86");                      // publish once every source byte has been consumed
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the next source byte
    emitter.instruction("inc rsi");                                             // advance the source pointer
    emitter.instruction("dec rdx");                                             // consume one source byte
    emitter.instruction("cmp al, 'A'");                                         // ASCII uppercase starts at 'A'
    emitter.instruction("jb __rt_mb_strtolower_ascii_store_x86");               // bytes below 'A' stay unchanged
    emitter.instruction("cmp al, 'Z'");                                         // ASCII uppercase ends at 'Z'
    emitter.instruction("ja __rt_mb_strtolower_ascii_store_x86");               // bytes above 'Z' stay unchanged
    emitter.instruction("add al, 32");                                          // convert A-Z to a-z
    emitter.label("__rt_mb_strtolower_ascii_store_x86");
    emitter.instruction("mov BYTE PTR [r9], al");                               // store the (possibly lowered) byte
    emitter.instruction("inc r9");                                              // advance the destination write pointer
    emitter.instruction("jmp __rt_mb_strtolower_ascii_loop_x86");               // continue with the remaining bytes

    emitter.label("__rt_mb_strtolower_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // result pointer is the reserved buffer start
    emitter.instruction("mov rdx, r9");                                         // result end is the current write pointer
    emitter.instruction("sub rdx, rax");                                        // result length is the number of bytes written
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch results
    emitter.instruction("leave");                                               // release the converter frame
    emitter.instruction("ret");                                                 // return the lowered string
    emitter.label("__rt_mb_strtolower_size_overflow_x86");
    emitter.instruction("jmp __rt_alloc_overflow");                             // report PHP's allocation-overflow fatal

    emit_mb_strtolower_helpers_x86_64(emitter);
}

/// Emits decode, map, encode, lookup, and Final_Sigma helpers for Linux x86_64.
fn emit_mb_strtolower_helpers_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_mb_strtolower_decode_x86");
    emitter.instruction("movzx eax, BYTE PTR [rsi]");                           // load the next possible UTF-8 leading byte
    emitter.instruction("cmp eax, 0x80");                                       // ASCII bytes are complete one-byte scalars
    emitter.instruction("jb __rt_mb_strtolower_decode_ascii_x86");              // consume one ASCII byte
    emitter.instruction("cmp eax, 0xC2");                                       // C0/C1 and continuation bytes are malformed leaders
    emitter.instruction("jb __rt_mb_strtolower_decode_invalid_x86");            // substitute one malformed byte
    emitter.instruction("cmp eax, 0xE0");                                       // C2-DF introduce two-byte sequences
    emitter.instruction("jb __rt_mb_strtolower_decode_two_x86");                // validate a two-byte character
    emitter.instruction("cmp eax, 0xF0");                                       // E0-EF introduce three-byte sequences
    emitter.instruction("jb __rt_mb_strtolower_decode_three_x86");              // validate a three-byte character
    emitter.instruction("cmp eax, 0xF5");                                       // F0-F4 introduce Unicode-range four-byte sequences
    emitter.instruction("jb __rt_mb_strtolower_decode_four_x86");               // validate a four-byte character
    emitter.instruction("jmp __rt_mb_strtolower_decode_invalid_x86");           // F5-FF cannot begin valid UTF-8
    emitter.label("__rt_mb_strtolower_decode_ascii_x86");
    emitter.instruction("inc rsi");                                             // consume the ASCII byte
    emitter.instruction("dec rdx");                                             // one fewer source byte remains
    emitter.instruction("mov r8d, 1");                                          // ASCII is a valid scalar
    emitter.instruction("ret");                                                 // return the one-byte code point
    emitter.label("__rt_mb_strtolower_decode_invalid_x86");
    emitter.instruction("inc rsi");                                             // consume one malformed byte
    emitter.instruction("dec rdx");                                             // one fewer source byte remains
    emitter.instruction("xor r8d, r8d");                                        // tell the caller to copy the byte unchanged
    emitter.instruction("ret");                                                 // return the malformed byte in eax

    emitter.label("__rt_mb_strtolower_decode_two_x86");
    emitter.instruction("cmp rdx, 2");                                          // is the sequence truncated before its continuation byte?
    emitter.instruction("jb __rt_mb_strtolower_decode_invalid_x86");            // a truncated prefix is one malformed group
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 1]");                      // load the two-byte sequence continuation
    emitter.instruction("mov r11d, r10d");                                      // copy the continuation for the prefix check
    emitter.instruction("and r11d, 0xC0");                                      // isolate the continuation-byte prefix
    emitter.instruction("cmp r11d, 0x80");                                      // does the second byte have the required 10xxxxxx shape?
    emitter.instruction("jne __rt_mb_strtolower_decode_invalid_x86");           // malformed continuation leaves the leader substituted alone
    emitter.instruction("and eax, 0x1F");                                       // keep the two-byte payload bits
    emitter.instruction("shl eax, 6");                                          // shift the leader payload into place
    emitter.instruction("and r10d, 0x3F");                                      // keep the continuation payload bits
    emitter.instruction("or eax, r10d");                                        // assemble the two-byte scalar
    emitter.instruction("add rsi, 2");                                          // consume the complete two-byte character
    emitter.instruction("sub rdx, 2");                                          // two fewer source bytes remain
    emitter.instruction("mov r8d, 1");                                          // the two-byte sequence is valid
    emitter.instruction("ret");                                                 // return the decoded scalar

    emitter.label("__rt_mb_strtolower_decode_three_x86");
    emitter.instruction("cmp rdx, 3");                                          // are both continuation bytes available?
    emitter.instruction("jb __rt_mb_strtolower_decode_invalid_x86");            // a truncated three-byte prefix is malformed here
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 1]");                      // load the first three-byte continuation
    emitter.instruction("mov r11d, r10d");                                      // copy the continuation for the prefix check
    emitter.instruction("and r11d, 0xC0");                                      // isolate its continuation-byte prefix
    emitter.instruction("cmp r11d, 0x80");                                      // is the first continuation structurally valid?
    emitter.instruction("jne __rt_mb_strtolower_decode_invalid_x86");           // malformed continuation substitutes only the leader
    emitter.instruction("cmp eax, 0xE0");                                       // E0 requires a second byte at least A0 to avoid overlong UTF-8
    emitter.instruction("jne __rt_mb_strtolower_decode_three_not_e0_x86");      // skip the E0 lower-bound check for other leaders
    emitter.instruction("cmp r10d, 0xA0");                                      // is the E0 continuation inside the non-overlong range?
    emitter.instruction("jb __rt_mb_strtolower_decode_invalid_x86");            // reject an overlong three-byte sequence
    emitter.label("__rt_mb_strtolower_decode_three_not_e0_x86");
    emitter.instruction("cmp eax, 0xED");                                       // ED requires a second byte below A0 to exclude UTF-16 surrogates
    emitter.instruction("jne __rt_mb_strtolower_decode_three_second_x86");      // skip the surrogate bound for other leaders
    emitter.instruction("cmp r10d, 0xA0");                                      // does the ED continuation enter the surrogate range?
    emitter.instruction("jae __rt_mb_strtolower_decode_invalid_x86");           // reject UTF-8 encodings of surrogate code points
    emitter.label("__rt_mb_strtolower_decode_three_second_x86");
    emitter.instruction("movzx r11d, BYTE PTR [rsi + 2]");                      // load the final three-byte continuation
    emitter.instruction("mov ecx, r11d");                                       // copy the continuation for the prefix check
    emitter.instruction("and ecx, 0xC0");                                       // isolate its continuation-byte prefix
    emitter.instruction("cmp ecx, 0x80");                                       // is the final continuation structurally valid?
    emitter.instruction("jne __rt_mb_strtolower_decode_invalid_x86");           // malformed final byte substitutes only the leader
    emitter.instruction("and eax, 0x0F");                                       // keep the three-byte leader payload
    emitter.instruction("shl eax, 12");                                         // shift the leader payload into place
    emitter.instruction("and r10d, 0x3F");                                      // keep the first continuation payload
    emitter.instruction("shl r10d, 6");                                         // shift the first continuation into place
    emitter.instruction("or eax, r10d");                                        // merge leader and first continuation
    emitter.instruction("and r11d, 0x3F");                                      // keep the final continuation payload
    emitter.instruction("or eax, r11d");                                        // assemble the three-byte scalar
    emitter.instruction("add rsi, 3");                                          // consume the complete three-byte character
    emitter.instruction("sub rdx, 3");                                          // three fewer source bytes remain
    emitter.instruction("mov r8d, 1");                                          // the three-byte sequence is valid
    emitter.instruction("ret");                                                 // return the decoded scalar

    emitter.label("__rt_mb_strtolower_decode_four_x86");
    emitter.instruction("cmp rdx, 4");                                          // are all three continuation bytes available?
    emitter.instruction("jb __rt_mb_strtolower_decode_invalid_x86");            // a truncated four-byte prefix is malformed here
    emitter.instruction("movzx r10d, BYTE PTR [rsi + 1]");                      // load the first four-byte continuation
    emitter.instruction("mov r11d, r10d");                                      // copy the continuation for the prefix check
    emitter.instruction("and r11d, 0xC0");                                      // isolate its continuation-byte prefix
    emitter.instruction("cmp r11d, 0x80");                                      // is the first continuation structurally valid?
    emitter.instruction("jne __rt_mb_strtolower_decode_invalid_x86");           // malformed continuation substitutes only the leader
    emitter.instruction("cmp eax, 0xF0");                                       // F0 requires a second byte at least 90 to avoid overlong UTF-8
    emitter.instruction("jne __rt_mb_strtolower_decode_four_not_f0_x86");       // skip the F0 lower-bound check for other leaders
    emitter.instruction("cmp r10d, 0x90");                                      // is the F0 continuation inside the non-overlong range?
    emitter.instruction("jb __rt_mb_strtolower_decode_invalid_x86");            // reject an overlong four-byte sequence
    emitter.label("__rt_mb_strtolower_decode_four_not_f0_x86");
    emitter.instruction("cmp eax, 0xF4");                                       // F4 requires a second byte below 90 for Unicode's maximum scalar
    emitter.instruction("jne __rt_mb_strtolower_decode_four_rest_x86");         // skip the upper bound for F0-F3
    emitter.instruction("cmp r10d, 0x90");                                      // does the F4 continuation exceed U+10FFFF?
    emitter.instruction("jae __rt_mb_strtolower_decode_invalid_x86");           // reject out-of-range four-byte sequences
    emitter.label("__rt_mb_strtolower_decode_four_rest_x86");
    emitter.instruction("movzx r11d, BYTE PTR [rsi + 2]");                      // load the second four-byte continuation
    emitter.instruction("mov ecx, r11d");                                       // copy the continuation for the prefix check
    emitter.instruction("and ecx, 0xC0");                                       // isolate its continuation-byte prefix
    emitter.instruction("cmp ecx, 0x80");                                       // is the second continuation structurally valid?
    emitter.instruction("jne __rt_mb_strtolower_decode_invalid_x86");           // malformed continuation substitutes only the leader
    emitter.instruction("movzx ecx, BYTE PTR [rsi + 3]");                       // load the final four-byte continuation
    emitter.instruction("mov r8d, ecx");                                        // copy the continuation for the prefix check
    emitter.instruction("and r8d, 0xC0");                                       // isolate its continuation-byte prefix
    emitter.instruction("cmp r8d, 0x80");                                       // is the final continuation structurally valid?
    emitter.instruction("jne __rt_mb_strtolower_decode_invalid_x86");           // malformed continuation substitutes only the leader
    emitter.instruction("and eax, 0x07");                                       // keep the four-byte leader payload
    emitter.instruction("shl eax, 18");                                         // shift the leader payload into place
    emitter.instruction("and r10d, 0x3F");                                      // keep the first continuation payload
    emitter.instruction("shl r10d, 12");                                        // shift the first continuation into place
    emitter.instruction("or eax, r10d");                                        // merge leader and first continuation
    emitter.instruction("and r11d, 0x3F");                                      // keep the second continuation payload
    emitter.instruction("shl r11d, 6");                                         // shift the second continuation into place
    emitter.instruction("or eax, r11d");                                        // merge the second continuation
    emitter.instruction("and ecx, 0x3F");                                       // keep the final continuation payload
    emitter.instruction("or eax, ecx");                                         // assemble the four-byte scalar
    emitter.instruction("add rsi, 4");                                          // consume the complete four-byte character
    emitter.instruction("sub rdx, 4");                                          // four fewer source bytes remain
    emitter.instruction("mov r8d, 1");                                          // the four-byte sequence is valid
    emitter.instruction("ret");                                                 // return the decoded scalar

    emitter.label("__rt_mb_strtolower_map_emit_x86");
    emitter.instruction("mov DWORD PTR [rbp - 56], eax");                       // remember the original scalar for the lookbehind update
    emitter.instruction("call __rt_mb_strtolower_expand_lookup_x86");           // try a 1:N full-case mapping first
    emitter.instruction("test ecx, ecx");                                       // did a 1:N expansion match?
    emitter.instruction("jnz __rt_mb_strtolower_map_expand_x86");               // expansions write every mapped scalar
    emitter.instruction("mov eax, DWORD PTR [rbp - 56]");                       // restore the original scalar for the 1:1 table
    emitter.instruction("call __rt_mb_strtolower_lookup_x86");                  // replace it with its simple/full 1:1 lowercase mapping
    emitter.instruction("call __rt_mb_strtolower_encode_x86");                  // write the mapped scalar as UTF-8
    emitter.instruction("jmp __rt_mb_strtolower_map_cased_x86");                // update the preceding-cased flag from the original scalar
    emitter.label("__rt_mb_strtolower_map_expand_x86");
    emitter.instruction("mov DWORD PTR [rbp - 48], ecx");                       // remaining expansion scalars
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // pointer to the first mapped scalar
    emitter.label("__rt_mb_strtolower_map_expand_loop_x86");
    emitter.instruction("cmp DWORD PTR [rbp - 48], 0");                         // all expansion scalars have been encoded?
    emitter.instruction("je __rt_mb_strtolower_map_cased_x86");                 // update lookbehind after the expansion
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the expansion cursor
    emitter.instruction("mov eax, DWORD PTR [r10]");                            // load the next mapped scalar
    emitter.instruction("add r10, 4");                                          // advance the expansion cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the expansion cursor across encode
    emitter.instruction("call __rt_mb_strtolower_encode_x86");                  // write one expanded scalar as UTF-8
    emitter.instruction("dec DWORD PTR [rbp - 48]");                            // one fewer expansion scalar remains
    emitter.instruction("jmp __rt_mb_strtolower_map_expand_loop_x86");          // continue encoding the expansion
    emitter.label("__rt_mb_strtolower_map_cased_x86");
    emitter.instruction("mov eax, DWORD PTR [rbp - 56]");                       // restore the original scalar
    emitter.instruction("call __rt_mb_strtolower_is_cased_x86");                // was the original scalar a cased letter?
    emitter.instruction("test eax, eax");                                       // cased letters become the new lookbehind
    emitter.instruction("jnz __rt_mb_strtolower_map_set_cased_x86");            // persist a cased lookbehind
    emitter.instruction("mov eax, DWORD PTR [rbp - 56]");                       // restore the original scalar for the ignorable test
    emitter.instruction("call __rt_mb_strtolower_is_ignorable_x86");            // Case_Ignorable marks keep the previous lookbehind
    emitter.instruction("test eax, eax");                                       // combining marks do not reset Final_Sigma lookbehind
    emitter.instruction("jnz __rt_mb_strtolower_map_keep_cased_x86");           // leave the preceding-cased flag unchanged
    emitter.instruction("mov DWORD PTR [rbp - 52], 0");                         // a non-cased, non-ignorable scalar is a word boundary
    emitter.instruction("jmp __rt_mb_strtolower_map_keep_cased_x86");           // return to the UTF-8 conversion loop
    emitter.label("__rt_mb_strtolower_map_set_cased_x86");
    emitter.instruction("mov DWORD PTR [rbp - 52], 1");                         // persist the preceding-cased flag for Final_Sigma
    emitter.label("__rt_mb_strtolower_map_keep_cased_x86");
    emitter.instruction("ret");                                                 // return to the UTF-8 conversion loop

    // -- encode one Unicode scalar as UTF-8 at r9 --
    emitter.label("__rt_mb_strtolower_encode_x86");
    emitter.instruction("cmp eax, 0x80");                                       // one-byte UTF-8 is the ASCII range
    emitter.instruction("jb __rt_mb_strtolower_encode_1_x86");                  // store a single ASCII byte
    emitter.instruction("cmp eax, 0x800");                                      // two-byte UTF-8 goes up to U+07FF
    emitter.instruction("jb __rt_mb_strtolower_encode_2_x86");                  // store a two-byte sequence
    emitter.instruction("cmp eax, 0x10000");                                    // three-byte UTF-8 goes up to U+FFFF
    emitter.instruction("jb __rt_mb_strtolower_encode_3_x86");                  // store a three-byte sequence
    emitter.instruction("mov r10d, eax");                                       // copy the four-byte scalar
    emitter.instruction("shr r10d, 18");                                        // four-byte leader payload
    emitter.instruction("or r10d, 0xF0");                                       // 11110xxx leader
    emitter.instruction("mov BYTE PTR [r9], r10b");                             // store the four-byte leader
    emitter.instruction("inc r9");                                              // advance the destination write pointer
    emitter.instruction("mov r10d, eax");                                       // copy the four-byte scalar
    emitter.instruction("shr r10d, 12");                                        // second-byte payload
    emitter.instruction("and r10d, 0x3F");                                      // keep six bits
    emitter.instruction("or r10d, 0x80");                                       // 10xxxxxx continuation
    emitter.instruction("mov BYTE PTR [r9], r10b");                             // store the second byte
    emitter.instruction("inc r9");                                              // advance the destination write pointer
    emitter.instruction("mov r10d, eax");                                       // copy the four-byte scalar
    emitter.instruction("shr r10d, 6");                                         // third-byte payload
    emitter.instruction("and r10d, 0x3F");                                      // keep six bits
    emitter.instruction("or r10d, 0x80");                                       // 10xxxxxx continuation
    emitter.instruction("mov BYTE PTR [r9], r10b");                             // store the third byte
    emitter.instruction("inc r9");                                              // advance the destination write pointer
    emitter.instruction("mov r10d, eax");                                       // copy the four-byte scalar
    emitter.instruction("and r10d, 0x3F");                                      // final six bits
    emitter.instruction("or r10d, 0x80");                                       // 10xxxxxx continuation
    emitter.instruction("mov BYTE PTR [r9], r10b");                             // store the fourth byte
    emitter.instruction("inc r9");                                              // advance the destination write pointer
    emitter.instruction("ret");                                                 // return after the four-byte store
    emitter.label("__rt_mb_strtolower_encode_1_x86");
    emitter.instruction("mov BYTE PTR [r9], al");                               // store the ASCII byte
    emitter.instruction("inc r9");                                              // advance the destination write pointer
    emitter.instruction("ret");                                                 // return after the one-byte store
    emitter.label("__rt_mb_strtolower_encode_2_x86");
    emitter.instruction("mov r10d, eax");                                       // copy the two-byte scalar
    emitter.instruction("shr r10d, 6");                                         // two-byte leader payload
    emitter.instruction("or r10d, 0xC0");                                       // 110xxxxx leader
    emitter.instruction("mov BYTE PTR [r9], r10b");                             // store the two-byte leader
    emitter.instruction("inc r9");                                              // advance the destination write pointer
    emitter.instruction("and eax, 0x3F");                                       // low six bits
    emitter.instruction("or eax, 0x80");                                        // 10xxxxxx continuation
    emitter.instruction("mov BYTE PTR [r9], al");                               // store the continuation
    emitter.instruction("inc r9");                                              // advance the destination write pointer
    emitter.instruction("ret");                                                 // return after the two-byte store
    emitter.label("__rt_mb_strtolower_encode_3_x86");
    emitter.instruction("mov r10d, eax");                                       // copy the three-byte scalar
    emitter.instruction("shr r10d, 12");                                        // three-byte leader payload
    emitter.instruction("or r10d, 0xE0");                                       // 1110xxxx leader
    emitter.instruction("mov BYTE PTR [r9], r10b");                             // store the three-byte leader
    emitter.instruction("inc r9");                                              // advance the destination write pointer
    emitter.instruction("mov r10d, eax");                                       // copy the three-byte scalar
    emitter.instruction("shr r10d, 6");                                         // second-byte payload
    emitter.instruction("and r10d, 0x3F");                                      // keep six bits
    emitter.instruction("or r10d, 0x80");                                       // 10xxxxxx continuation
    emitter.instruction("mov BYTE PTR [r9], r10b");                             // store the second byte
    emitter.instruction("inc r9");                                              // advance the destination write pointer
    emitter.instruction("and eax, 0x3F");                                       // final six bits
    emitter.instruction("or eax, 0x80");                                        // 10xxxxxx continuation
    emitter.instruction("mov BYTE PTR [r9], al");                               // store the third byte
    emitter.instruction("inc r9");                                              // advance the destination write pointer
    emitter.instruction("ret");                                                 // return after the three-byte store

    // -- binary-search the 1:1 lowercase map --
    emitter.label("__rt_mb_strtolower_lookup_x86");
    abi::emit_symbol_address(emitter, "r10", "_mb_strtolower_map");
    abi::emit_symbol_address(emitter, "r11", "_mb_strtolower_map_end");
    emitter.label("__rt_mb_strtolower_lookup_loop_x86");
    emitter.instruction("cmp r10, r11");                                        // empty remaining map window?
    emitter.instruction("jae __rt_mb_strtolower_lookup_miss_x86");              // unmapped scalars stay unchanged
    emitter.instruction("mov rcx, r11");                                        // copy the exclusive end pointer
    emitter.instruction("sub rcx, r10");                                        // byte length of the remaining window
    emitter.instruction("shr rcx, 4");                                          // half the entry count, in entries
    emitter.instruction("shl rcx, 3");                                          // convert that midpoint back to a byte offset
    emitter.instruction("lea r8, [r10 + rcx]");                                 // point at the midpoint entry
    emitter.instruction("cmp eax, DWORD PTR [r8]");                             // compare the query with this mapping's source
    emitter.instruction("je __rt_mb_strtolower_lookup_hit_x86");                // exact mapping found
    emitter.instruction("jb __rt_mb_strtolower_lookup_left_x86");               // search the lower half
    emitter.instruction("lea r10, [r8 + 8]");                                   // shrink the window to the right half
    emitter.instruction("jmp __rt_mb_strtolower_lookup_loop_x86");              // continue the search
    emitter.label("__rt_mb_strtolower_lookup_left_x86");
    emitter.instruction("mov r11, r8");                                         // shrink the window to the left half
    emitter.instruction("jmp __rt_mb_strtolower_lookup_loop_x86");              // continue the search
    emitter.label("__rt_mb_strtolower_lookup_hit_x86");
    emitter.instruction("mov eax, DWORD PTR [r8 + 4]");                         // replace the query with its lowercase mapping
    emitter.label("__rt_mb_strtolower_lookup_miss_x86");
    emitter.instruction("ret");                                                 // return the mapped or original scalar

    // -- linear-search the small 1:N expansion table --
    emitter.label("__rt_mb_strtolower_expand_lookup_x86");
    abi::emit_symbol_address(emitter, "r10", "_mb_strtolower_expand");
    abi::emit_symbol_address(emitter, "r11", "_mb_strtolower_expand_end");
    emitter.instruction("xor ecx, ecx");                                        // default: no expansion
    emitter.label("__rt_mb_strtolower_expand_loop_x86");
    emitter.instruction("cmp r10, r11");                                        // scanned every expansion entry?
    emitter.instruction("jae __rt_mb_strtolower_expand_done_x86");              // no 1:N mapping for this scalar
    emitter.instruction("cmp eax, DWORD PTR [r10]");                            // does this entry match the query?
    emitter.instruction("je __rt_mb_strtolower_expand_hit_x86");                // use this expansion
    emitter.instruction("add r10, 24");                                         // advance to the next 24-byte expansion record
    emitter.instruction("jmp __rt_mb_strtolower_expand_loop_x86");              // continue the linear scan
    emitter.label("__rt_mb_strtolower_expand_hit_x86");
    emitter.instruction("mov ecx, DWORD PTR [r10 + 4]");                        // load the expansion scalar count
    emitter.instruction("add r10, 8");                                          // point at the first mapped scalar
    emitter.label("__rt_mb_strtolower_expand_done_x86");
    emitter.instruction("ret");                                                 // return count in ecx and mapped pointer in r10

    // -- peek whether a cased letter follows after Case_Ignorable marks --
    emitter.label("__rt_mb_strtolower_next_cased_x86");
    emitter.label("__rt_mb_strtolower_next_cased_loop_x86");
    emitter.instruction("test rdx, rdx");                                       // any source bytes remaining?
    emitter.instruction("jz __rt_mb_strtolower_next_cased_no_x86");             // end of string: no following cased letter
    emitter.instruction("call __rt_mb_strtolower_decode_x86");                  // decode the next scalar or malformed byte without committing it to output
    emitter.instruction("test r8d, r8d");                                       // valid scalar?
    emitter.instruction("jz __rt_mb_strtolower_next_cased_no_x86");             // malformed bytes break the word
    emitter.instruction("mov DWORD PTR [rbp - 56], eax");                       // remember the peeked scalar across the ignorable test
    emitter.instruction("call __rt_mb_strtolower_is_ignorable_x86");            // skip Case_Ignorable marks
    emitter.instruction("test eax, eax");                                       // was the peeked scalar Case_Ignorable?
    emitter.instruction("jnz __rt_mb_strtolower_next_cased_loop_x86");          // continue past ignorable marks
    emitter.instruction("mov eax, DWORD PTR [rbp - 56]");                       // restore the first non-ignorable peeked scalar
    emitter.instruction("call __rt_mb_strtolower_is_cased_x86");                // is the first non-ignorable scalar a cased letter?
    emitter.instruction("ret");                                                 // eax is the following-cased flag
    emitter.label("__rt_mb_strtolower_next_cased_no_x86");
    emitter.instruction("xor eax, eax");                                        // no following cased letter
    emitter.instruction("ret");                                                 // report a word boundary

    // -- binary-search the cased range table --
    emitter.label("__rt_mb_strtolower_is_cased_x86");
    abi::emit_symbol_address(emitter, "r10", "_mb_strtolower_cased");
    abi::emit_symbol_address(emitter, "r11", "_mb_strtolower_cased_end");
    emitter.label("__rt_mb_strtolower_is_cased_loop_x86");
    emitter.instruction("cmp r10, r11");                                        // empty remaining range window?
    emitter.instruction("jae __rt_mb_strtolower_is_cased_no_x86");              // not in any cased range
    emitter.instruction("mov rcx, r11");                                        // copy the exclusive end pointer
    emitter.instruction("sub rcx, r10");                                        // byte length of the remaining window
    emitter.instruction("shr rcx, 4");                                          // half the range count, in ranges
    emitter.instruction("shl rcx, 3");                                          // convert that midpoint back to a byte offset
    emitter.instruction("lea r8, [r10 + rcx]");                                 // point at the midpoint range
    emitter.instruction("cmp eax, DWORD PTR [r8]");                             // is the query below this range?
    emitter.instruction("jb __rt_mb_strtolower_is_cased_left_x86");             // search the lower half
    emitter.instruction("cmp eax, DWORD PTR [r8 + 4]");                         // is the query above this range?
    emitter.instruction("ja __rt_mb_strtolower_is_cased_right_x86");            // search the upper half
    emitter.instruction("mov eax, 1");                                          // the query sits inside a cased range
    emitter.instruction("ret");                                                 // report a positive match
    emitter.label("__rt_mb_strtolower_is_cased_left_x86");
    emitter.instruction("mov r11, r8");                                         // shrink the window to the left half
    emitter.instruction("jmp __rt_mb_strtolower_is_cased_loop_x86");            // continue the search
    emitter.label("__rt_mb_strtolower_is_cased_right_x86");
    emitter.instruction("lea r10, [r8 + 8]");                                   // shrink the window to the right half
    emitter.instruction("jmp __rt_mb_strtolower_is_cased_loop_x86");            // continue the search
    emitter.label("__rt_mb_strtolower_is_cased_no_x86");
    emitter.instruction("xor eax, eax");                                        // not a cased letter
    emitter.instruction("ret");                                                 // report a negative match

    // -- binary-search the ignorable range table --
    emitter.label("__rt_mb_strtolower_is_ignorable_x86");
    abi::emit_symbol_address(emitter, "r10", "_mb_strtolower_ignorable");
    abi::emit_symbol_address(emitter, "r11", "_mb_strtolower_ignorable_end");
    emitter.label("__rt_mb_strtolower_is_ignorable_loop_x86");
    emitter.instruction("cmp r10, r11");                                        // empty remaining range window?
    emitter.instruction("jae __rt_mb_strtolower_is_ignorable_no_x86");          // not in any ignorable range
    emitter.instruction("mov rcx, r11");                                        // copy the exclusive end pointer
    emitter.instruction("sub rcx, r10");                                        // byte length of the remaining window
    emitter.instruction("shr rcx, 4");                                          // half the range count, in ranges
    emitter.instruction("shl rcx, 3");                                          // convert that midpoint back to a byte offset
    emitter.instruction("lea r8, [r10 + rcx]");                                 // point at the midpoint range
    emitter.instruction("cmp eax, DWORD PTR [r8]");                             // is the query below this range?
    emitter.instruction("jb __rt_mb_strtolower_is_ignorable_left_x86");         // search the lower half
    emitter.instruction("cmp eax, DWORD PTR [r8 + 4]");                         // is the query above this range?
    emitter.instruction("ja __rt_mb_strtolower_is_ignorable_right_x86");        // search the upper half
    emitter.instruction("mov eax, 1");                                          // the query sits inside an ignorable range
    emitter.instruction("ret");                                                 // report a positive match
    emitter.label("__rt_mb_strtolower_is_ignorable_left_x86");
    emitter.instruction("mov r11, r8");                                         // shrink the window to the left half
    emitter.instruction("jmp __rt_mb_strtolower_is_ignorable_loop_x86");        // continue the search
    emitter.label("__rt_mb_strtolower_is_ignorable_right_x86");
    emitter.instruction("lea r10, [r8 + 8]");                                   // shrink the window to the right half
    emitter.instruction("jmp __rt_mb_strtolower_is_ignorable_loop_x86");        // continue the search
    emitter.label("__rt_mb_strtolower_is_ignorable_no_x86");
    emitter.instruction("xor eax, eax");                                        // not Case_Ignorable
    emitter.instruction("ret");                                                 // report a negative match
}
