//! Purpose:
//! Emits `__rt_iconv_spec_split`, which pulls the two charset names and the
//! `//TRANSLIT`/`//IGNORE` option bits out of a `convert.iconv.*` stream-filter name that is
//! only known at run time.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::platform` during the I/O runtime section.
//! - `crate::codegen::lower_inst::builtins::io::stream_filters`, on the dynamic-name attach path.
//!
//! Key details:
//! - A literal `stream_filter_append($h, "convert.iconv.UTF-8/ISO-8859-1")` splits its spec
//!   during lowering and hands the two halves to `iconv_open()` as data symbols. A name held in
//!   a variable has no spec to split at compile time, so the same attach sequence is emitted
//!   against two program-local BUFFERS and this helper fills them just before it runs.
//! - The buffers are NUL-terminated, because `iconv_open()` takes C strings.
//! - The option bits mirror `iconv_target_option_bits()`: bit 0 is `//TRANSLIT`, bit 1 is
//!   `//IGNORE`, matched case-insensitively anywhere in the target half. Apple's iconv shares
//!   both options between every descriptor of one charset pair, so the write filter records
//!   these bits per descriptor and restores them before each conversion.
//! - A name with no `/` after the prefix is not a filter: PHP answers `false` for
//!   `convert.iconv.` and `convert.iconv.UTF-8`. An empty half is valid.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Bytes reserved for each half of the spec, NUL included.
///
/// Long enough for every charset name iconv ships with, and a longer one is rejected rather
/// than truncated, since a truncated charset name would open the wrong conversion.
pub(crate) const ICONV_SPEC_BUFFER_BYTES: usize = 64;

/// Emits `__rt_iconv_spec_split`.
///
/// # Input
/// - `x0`/`rdi`: the filter name pointer
/// - `x1`/`rsi`: its byte length
/// - `x2`/`rdx`: the `from` buffer, `ICONV_SPEC_BUFFER_BYTES` wide
/// - `x3`/`rcx`: the `to` buffer, the same width
///
/// # Output
/// - `x0`/`rax`: 1 when the name is `convert.iconv.<from>/<to>` and both halves fit, else 0
/// - `x1`/`rdx`: the target option bits on a hit (1 = `//TRANSLIT`, 2 = `//IGNORE`)
pub(crate) fn emit_iconv_spec_split(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// The prefix every iconv filter name carries.
const PREFIX: &str = "convert.iconv.";

/// The target suffixes whose presence selects a conversion option.
const TRANSLIT: &[u8] = b"translit";
const IGNORE: &[u8] = b"ignore";

/// Emits the AArch64 split helper.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: split a run-time convert.iconv.<from>/<to> filter name ---");
    emitter.label_global("__rt_iconv_spec_split");
    let prefix_len = PREFIX.len() as i64;

    // -- the name must be longer than the prefix, or there is no spec at all --
    emitter.instruction(&format!("cmp x1, #{}", prefix_len));                   // is the name longer than the prefix?
    emitter.instruction("b.le __rt_icss_miss");                                 // no spec without bytes after the prefix
    for (index, byte) in PREFIX.bytes().enumerate() {
        emitter.instruction(&format!("ldrb w10, [x0, #{index}]"));              // load the next prefix byte
        emitter.instruction(&format!("cmp w10, #{}", byte));                    // does it match convert.iconv.?
        emitter.instruction("b.ne __rt_icss_miss");                             // a different filter name is not ours
    }

    // -- find the separator in the remainder --
    emitter.instruction(&format!("add x9, x0, #{}", prefix_len));               // the spec start
    emitter.instruction(&format!("sub x10, x1, #{}", prefix_len));              // the spec length
    emitter.instruction("mov x11, #0");                                         // scan index
    emitter.label("__rt_icss_find");
    emitter.instruction("cmp x11, x10");                                        // scanned every spec byte?
    emitter.instruction("b.ge __rt_icss_miss");                                 // no separator: not a filter name
    emitter.instruction("ldrb w12, [x9, x11]");                                 // load the next spec byte
    emitter.instruction("cmp w12, #0x2f");                                      // is it the '/' separator?
    emitter.instruction("b.eq __rt_icss_found");                                // split the spec here
    emitter.instruction("add x11, x11, #1");                                    // advance to the next byte
    emitter.instruction("b __rt_icss_find");                                    // keep looking for the separator
    emitter.label("__rt_icss_found");

    // -- copy the `from` half, which runs up to the separator --
    emitter.instruction(&format!("cmp x11, #{}", ICONV_SPEC_BUFFER_BYTES - 1)); // does the source fit the buffer?
    emitter.instruction("b.hi __rt_icss_miss");                                 // too long to hold: refuse, never truncate
    emitter.instruction("mov x13, #0");                                         // copy index
    emitter.label("__rt_icss_from_copy");
    emitter.instruction("cmp x13, x11");                                        // copied every source byte?
    emitter.instruction("b.ge __rt_icss_from_done");                            // source half is complete
    emitter.instruction("ldrb w12, [x9, x13]");                                 // load the next source byte
    emitter.instruction("strb w12, [x2, x13]");                                 // store it in the persistent buffer
    emitter.instruction("add x13, x13, #1");                                    // advance to the next byte
    emitter.instruction("b __rt_icss_from_copy");                               // continue copying
    emitter.label("__rt_icss_from_done");
    emitter.instruction("strb wzr, [x2, x13]");                                 // iconv_open takes C strings

    // -- copy the `to` half, which runs from just past the separator to the end --
    emitter.instruction("add x14, x11, #1");                                    // first byte after '/'
    emitter.instruction("sub x15, x10, x14");                                   // its length
    emitter.instruction(&format!("cmp x15, #{}", ICONV_SPEC_BUFFER_BYTES - 1)); // does the target fit the buffer?
    emitter.instruction("b.hi __rt_icss_miss");                                 // too long to hold: refuse, never truncate
    emitter.instruction("mov x13, #0");                                         // copy index
    emitter.label("__rt_icss_to_copy");
    emitter.instruction("cmp x13, x15");                                        // copied every target byte?
    emitter.instruction("b.ge __rt_icss_to_done");                              // target half is complete
    emitter.instruction("add x12, x14, x13");                                   // source index of the next target byte
    emitter.instruction("ldrb w12, [x9, x12]");                                 // load the next target byte
    emitter.instruction("strb w12, [x3, x13]");                                 // store it in the persistent buffer
    emitter.instruction("add x13, x13, #1");                                    // advance to the next byte
    emitter.instruction("b __rt_icss_to_copy");                                 // continue copying
    emitter.label("__rt_icss_to_done");
    emitter.instruction("strb wzr, [x3, x13]");                                 // iconv_open takes C strings

    // -- scan the target half for the //TRANSLIT and //IGNORE suffixes --
    emitter.instruction("mov x16, #0");                                         // option bits: 1 = //TRANSLIT, 2 = //IGNORE
    emitter.instruction("mov x13, #0");                                         // scan index
    emitter.label("__rt_icss_options");
    emitter.instruction("add x17, x13, #1");                                    // index after a one-byte suffix prefix
    emitter.instruction("cmp x17, x15");                                        // is there room for a two-byte '//'?
    emitter.instruction("b.hs __rt_icss_options_done");                         // no more suffixes fit
    emitter.instruction("ldrb w12, [x3, x13]");                                 // load the candidate first byte
    emitter.instruction("cmp w12, #0x2f");                                      // is it the first '/'?
    emitter.instruction("b.ne __rt_icss_options_next");                         // every suffix starts with '//'
    emitter.instruction("ldrb w12, [x3, x17]");                                 // load the candidate second byte
    emitter.instruction("cmp w12, #0x2f");                                      // is it the second '/'?
    emitter.instruction("b.ne __rt_icss_options_next");                         // not a suffix marker
    emitter.instruction("add x17, x13, #10");                                   // end index of //TRANSLIT
    emitter.instruction("cmp x17, x15");                                        // does //TRANSLIT fit?
    emitter.instruction("b.hi __rt_icss_options_ignore");                       // no: try the shorter //IGNORE
    emit_suffix_compare_arm64(emitter, TRANSLIT, 2, "__rt_icss_options_ignore");
    emitter.instruction("orr x16, x16, #1");                                    // record this descriptor's //TRANSLIT
    emitter.instruction("b __rt_icss_options_next");                            // continue scanning after the suffix
    emitter.label("__rt_icss_options_ignore");
    emitter.instruction("add x17, x13, #8");                                    // end index of //IGNORE
    emitter.instruction("cmp x17, x15");                                        // does //IGNORE fit?
    emitter.instruction("b.hi __rt_icss_options_next");                         // no: keep scanning
    emit_suffix_compare_arm64(emitter, IGNORE, 2, "__rt_icss_options_next");
    emitter.instruction("orr x16, x16, #2");                                    // record this descriptor's //IGNORE
    emitter.label("__rt_icss_options_next");
    emitter.instruction("add x13, x13, #1");                                    // advance to the next candidate
    emitter.instruction("b __rt_icss_options");                                 // continue scanning the target
    emitter.label("__rt_icss_options_done");
    emitter.instruction("mov x1, x16");                                         // return the target option bits
    emitter.instruction("mov x0, #1");                                          // both halves are in the buffers
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_icss_miss");
    emitter.instruction("mov x0, #0");                                          // not an iconv filter name
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the case-insensitive comparison of one lowercase suffix on AArch64.
///
/// `offset` is the byte after `//`. The comparison branches to `mismatch` on the first
/// differing byte.
fn emit_suffix_compare_arm64(
    emitter: &mut Emitter,
    suffix: &[u8],
    offset: usize,
    mismatch: &str,
) {
    for (index, expected) in suffix.iter().enumerate() {
        let displacement = offset + index;
        emitter.instruction(&format!("add x17, x13, #{}", displacement));       // index of this suffix byte
        emitter.instruction(&format!("ldrb w12, [x3, x17]"));                   // load the suffix byte
        emitter.instruction("orr w12, w12, #0x20");                             // lowercase ASCII letters
        emitter.instruction(&format!("cmp w12, #{}", expected));                // does it match the suffix?
        emitter.instruction(&format!("b.ne {}", mismatch));                     // a different suffix
    }
}

/// Emits the x86_64 split helper.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: split a run-time convert.iconv.<from>/<to> filter name ---");
    emitter.label_global("__rt_iconv_spec_split");
    let prefix_len = PREFIX.len() as i64;

    // -- the name must be longer than the prefix, or there is no spec at all --
    emitter.instruction(&format!("cmp rsi, {}", prefix_len));                   // is the name longer than the prefix?
    emitter.instruction("jle __rt_icss_miss_x");                                // no spec without bytes after the prefix
    for (index, byte) in PREFIX.bytes().enumerate() {
        emitter.instruction(&format!("movzx r10d, BYTE PTR [rdi + {index}]"));  // load the next prefix byte
        emitter.instruction(&format!("cmp r10d, {}", byte));                    // does it match convert.iconv.?
        emitter.instruction("jne __rt_icss_miss_x");                            // a different filter name is not ours
    }

    // -- find the separator in the remainder --
    emitter.instruction(&format!("lea r9, [rdi + {}]", prefix_len));            // the spec start
    emitter.instruction("mov r10, rsi");                                        // copy the name length
    emitter.instruction(&format!("sub r10, {}", prefix_len));                   // the spec length
    emitter.instruction("xor r11d, r11d");                                      // scan index
    emitter.label("__rt_icss_find_x");
    emitter.instruction("cmp r11, r10");                                        // scanned every spec byte?
    emitter.instruction("jge __rt_icss_miss_x");                                // no separator: not a filter name
    emitter.instruction("movzx eax, BYTE PTR [r9 + r11]");                      // load the next spec byte
    emitter.instruction("cmp eax, 0x2f");                                       // is it the '/' separator?
    emitter.instruction("je __rt_icss_found_x");                                // split the spec here
    emitter.instruction("add r11, 1");                                          // advance to the next byte
    emitter.instruction("jmp __rt_icss_find_x");                                // keep looking for the separator
    emitter.label("__rt_icss_found_x");

    // -- copy the `from` half, which runs up to the separator --
    emitter.instruction(&format!("cmp r11, {}", ICONV_SPEC_BUFFER_BYTES - 1));  // does the source fit the buffer?
    emitter.instruction("ja __rt_icss_miss_x");                                 // too long to hold: refuse, never truncate
    emitter.instruction("xor r8d, r8d");                                        // copy index
    emitter.label("__rt_icss_from_copy_x");
    emitter.instruction("cmp r8, r11");                                         // copied every source byte?
    emitter.instruction("jge __rt_icss_from_done_x");                           // source half is complete
    emitter.instruction("movzx eax, BYTE PTR [r9 + r8]");                       // load the next source byte
    emitter.instruction("mov BYTE PTR [rdx + r8], al");                         // store it in the persistent buffer
    emitter.instruction("add r8, 1");                                           // advance to the next byte
    emitter.instruction("jmp __rt_icss_from_copy_x");                           // continue copying
    emitter.label("__rt_icss_from_done_x");
    emitter.instruction("mov BYTE PTR [rdx + r8], 0");                          // iconv_open takes C strings

    // -- copy the `to` half, which runs from just past the separator to the end --
    emitter.instruction("lea r11, [r11 + 1]");                                  // first byte after '/'
    emitter.instruction("mov rax, r10");                                        // copy the spec length
    emitter.instruction("sub rax, r11");                                        // the target length
    emitter.instruction(&format!("cmp rax, {}", ICONV_SPEC_BUFFER_BYTES - 1));  // does the target fit the buffer?
    emitter.instruction("ja __rt_icss_miss_x");                                 // too long to hold: refuse, never truncate
    emitter.instruction("xor r8d, r8d");                                        // copy index
    emitter.label("__rt_icss_to_copy_x");
    emitter.instruction("cmp r8, rax");                                         // copied every target byte?
    emitter.instruction("jge __rt_icss_to_done_x");                             // target half is complete
    emitter.instruction("mov r10, r11");                                        // source start of the target half
    emitter.instruction("add r10, r8");                                         // source index of the next target byte
    emitter.instruction("movzx r10d, BYTE PTR [r9 + r10]");                     // load the next target byte
    emitter.instruction("mov BYTE PTR [rcx + r8], r10b");                       // store it in the persistent buffer
    emitter.instruction("add r8, 1");                                           // advance to the next byte
    emitter.instruction("jmp __rt_icss_to_copy_x");                             // continue copying
    emitter.label("__rt_icss_to_done_x");
    emitter.instruction("mov BYTE PTR [rcx + r8], 0");                          // iconv_open takes C strings

    // -- scan the target half for the //TRANSLIT and //IGNORE suffixes --
    emitter.instruction("xor r11d, r11d");                                      // option bits: 1 = //TRANSLIT, 2 = //IGNORE
    emitter.instruction("xor r8d, r8d");                                        // scan index
    emitter.label("__rt_icss_options_x");
    emitter.instruction("lea r9, [r8 + 1]");                                    // index after a one-byte suffix prefix
    emitter.instruction("cmp r9, rax");                                         // is there room for a two-byte '//'?
    emitter.instruction("jae __rt_icss_options_done_x");                        // no more suffixes fit
    emitter.instruction("movzx r10d, BYTE PTR [rcx + r8]");                     // load the candidate first byte
    emitter.instruction("cmp r10d, 0x2f");                                      // is it the first '/'?
    emitter.instruction("jne __rt_icss_options_next_x");                        // every suffix starts with '//'
    emitter.instruction("movzx r10d, BYTE PTR [rcx + r9]");                     // load the candidate second byte
    emitter.instruction("cmp r10d, 0x2f");                                      // is it the second '/'?
    emitter.instruction("jne __rt_icss_options_next_x");                        // not a suffix marker
    emitter.instruction("lea r9, [r8 + 10]");                                   // end index of //TRANSLIT
    emitter.instruction("cmp r9, rax");                                         // does //TRANSLIT fit?
    emitter.instruction("ja __rt_icss_options_ignore_x");                       // no: try the shorter //IGNORE
    emit_suffix_compare_x86(emitter, TRANSLIT, 2, "__rt_icss_options_ignore_x");
    emitter.instruction("or r11d, 1");                                          // record this descriptor's //TRANSLIT
    emitter.instruction("jmp __rt_icss_options_next_x");                        // continue scanning after the suffix
    emitter.label("__rt_icss_options_ignore_x");
    emitter.instruction("lea r9, [r8 + 8]");                                    // end index of //IGNORE
    emitter.instruction("cmp r9, rax");                                         // does //IGNORE fit?
    emitter.instruction("ja __rt_icss_options_next_x");                         // no: keep scanning
    emit_suffix_compare_x86(emitter, IGNORE, 2, "__rt_icss_options_next_x");
    emitter.instruction("or r11d, 2");                                          // record this descriptor's //IGNORE
    emitter.label("__rt_icss_options_next_x");
    emitter.instruction("add r8, 1");                                           // advance to the next candidate
    emitter.instruction("jmp __rt_icss_options_x");                             // continue scanning the target
    emitter.label("__rt_icss_options_done_x");
    emitter.instruction("mov rdx, r11");                                        // return the target option bits
    emitter.instruction("mov rax, 1");                                          // both halves are in the buffers
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_icss_miss_x");
    emitter.instruction("xor eax, eax");                                        // not an iconv filter name
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the case-insensitive comparison of one lowercase suffix on x86_64.
///
/// `offset` is the byte after `//`; the comparison branches to `mismatch` on a differing byte.
fn emit_suffix_compare_x86(emitter: &mut Emitter, suffix: &[u8], offset: usize, mismatch: &str) {
    for (index, expected) in suffix.iter().enumerate() {
        let displacement = offset + index;
        emitter.instruction(&format!("lea r9, [r8 + {}]", displacement));       // index of this suffix byte
        emitter.instruction("movzx r10d, BYTE PTR [rcx + r9]");                 // load the suffix byte
        emitter.instruction("or r10d, 0x20");                                   // lowercase ASCII letters
        emitter.instruction(&format!("cmp r10d, {}", expected));                // does it match the suffix?
        emitter.instruction(&format!("jne {}", mismatch));                      // a different suffix
    }
}
