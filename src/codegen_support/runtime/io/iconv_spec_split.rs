//! Purpose:
//! Emits `__rt_iconv_spec_split`, which pulls the two charset names out of a `convert.iconv.*`
//! stream-filter name that is only known at RUN time.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io::stream_filters`, on the dynamic-name attach path.
//!
//! Key details:
//! - A literal `stream_filter_append($h, "convert.iconv.UTF-8/ISO-8859-1")` splits its spec
//!   during lowering and hands the two halves to `iconv_open()` as data symbols. A name held in
//!   a variable has no spec to split at compile time, so the same attach sequence is emitted
//!   against two program-local BUFFERS and this helper fills them just before it runs. Nothing
//!   in the attach shapes changes: they already take the ADDRESS of a symbol, and the bytes
//!   there are now written rather than assembled.
//! - The buffers are NUL-terminated, because `iconv_open()` takes C strings.
//! - Measured on `php -n` 8.5.6: a name with no `/` after the prefix is NOT a filter — php
//!   answers `false` for `convert.iconv.` and `convert.iconv.UTF-8`. An EMPTY half is fine
//!   (`convert.iconv.UTF-8/` and `convert.iconv./UTF-8` both attach), because iconv reads the
//!   empty string as the current locale's charset. So only the missing separator fails here.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Bytes reserved for each half of the spec, NUL included.
///
/// Long enough for every charset name iconv ships with — the longest in glibc's table is under
/// 30 bytes — and a longer one is rejected rather than truncated, since a truncated charset name
/// would open the WRONG conversion instead of failing.
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
pub fn emit_iconv_spec_split(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// The prefix every iconv filter name carries.
const PREFIX: &str = "convert.iconv.";

/// The AArch64 map.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: split a run-time convert.iconv.<from>/<to> filter name ---");
    emitter.label_global("__rt_iconv_spec_split");
    let prefix_len = PREFIX.len() as i64;

    // -- the name must be longer than the prefix, or there is no spec at all --
    emitter.instruction(&format!("cmp x1, #{}", prefix_len));
    emitter.instruction("b.le __rt_icss_miss");

    // -- compare the prefix byte by byte --
    emitter.instruction("mov x9, #0");                                          // index into the prefix
    for (index, byte) in PREFIX.bytes().enumerate() {
        emitter.instruction(&format!("ldrb w10, [x0, #{index}]"));
        emitter.instruction(&format!("cmp w10, #{}", byte));
        emitter.instruction("b.ne __rt_icss_miss");
    }

    // -- find the separator in the remainder --
    emitter.instruction(&format!("add x9, x0, #{}", prefix_len));               // the spec start
    emitter.instruction(&format!("sub x10, x1, #{}", prefix_len));              // the spec length
    emitter.instruction("mov x11, #0");                                         // scan index
    emitter.label("__rt_icss_find");
    emitter.instruction("cmp x11, x10");
    emitter.instruction("b.ge __rt_icss_miss");                                 // no separator: not a filter name
    emitter.instruction("ldrb w12, [x9, x11]");
    emitter.instruction("cmp w12, #0x2f");                                      // '/'
    emitter.instruction("b.eq __rt_icss_found");
    emitter.instruction("add x11, x11, #1");
    emitter.instruction("b __rt_icss_find");
    emitter.label("__rt_icss_found");

    // -- copy the `from` half, which runs up to the separator --
    emitter.instruction(&format!("cmp x11, #{}", ICONV_SPEC_BUFFER_BYTES - 1));
    emitter.instruction("b.hi __rt_icss_miss");                                 // too long to hold: refuse, never truncate
    emitter.instruction("mov x13, #0");
    emitter.label("__rt_icss_from_copy");
    emitter.instruction("cmp x13, x11");
    emitter.instruction("b.ge __rt_icss_from_done");
    emitter.instruction("ldrb w12, [x9, x13]");
    emitter.instruction("strb w12, [x2, x13]");
    emitter.instruction("add x13, x13, #1");
    emitter.instruction("b __rt_icss_from_copy");
    emitter.label("__rt_icss_from_done");
    emitter.instruction("strb wzr, [x2, x13]");                                 // iconv_open takes C strings

    // -- copy the `to` half, which runs from just past the separator to the end --
    emitter.instruction("add x14, x11, #1");                                    // first byte after '/'
    emitter.instruction("sub x15, x10, x14");                                   // its length
    emitter.instruction(&format!("cmp x15, #{}", ICONV_SPEC_BUFFER_BYTES - 1));
    emitter.instruction("b.hi __rt_icss_miss");
    emitter.instruction("mov x13, #0");
    emitter.label("__rt_icss_to_copy");
    emitter.instruction("cmp x13, x15");
    emitter.instruction("b.ge __rt_icss_to_done");
    emitter.instruction("add x12, x14, x13");
    emitter.instruction("ldrb w12, [x9, x12]");
    emitter.instruction("strb w12, [x3, x13]");
    emitter.instruction("add x13, x13, #1");
    emitter.instruction("b __rt_icss_to_copy");
    emitter.label("__rt_icss_to_done");
    emitter.instruction("strb wzr, [x3, x13]");
    emitter.instruction("mov x0, #1");                                          // both halves are in the buffers
    emitter.instruction("ret");

    emitter.label("__rt_icss_miss");
    emitter.instruction("mov x0, #0");                                          // not an iconv filter name
    emitter.instruction("ret");
}

/// The x86_64 map.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: split a run-time convert.iconv.<from>/<to> filter name ---");
    emitter.label_global("__rt_iconv_spec_split");
    let prefix_len = PREFIX.len() as i64;

    emitter.instruction(&format!("cmp rsi, {}", prefix_len));
    emitter.instruction("jle __rt_icss_miss_x");

    for (index, byte) in PREFIX.bytes().enumerate() {
        emitter.instruction(&format!("movzx r10d, BYTE PTR [rdi + {index}]"));
        emitter.instruction(&format!("cmp r10d, {}", byte));
        emitter.instruction("jne __rt_icss_miss_x");
    }

    emitter.instruction(&format!("lea r9, [rdi + {}]", prefix_len));            // the spec start
    emitter.instruction("mov r10, rsi");
    emitter.instruction(&format!("sub r10, {}", prefix_len));                   // the spec length
    emitter.instruction("xor r11d, r11d");                                      // scan index
    emitter.label("__rt_icss_find_x");
    emitter.instruction("cmp r11, r10");
    emitter.instruction("jge __rt_icss_miss_x");                                // no separator: not a filter name
    emitter.instruction("movzx eax, BYTE PTR [r9 + r11]");
    emitter.instruction("cmp eax, 0x2f");                                       // '/'
    emitter.instruction("je __rt_icss_found_x");
    emitter.instruction("add r11, 1");
    emitter.instruction("jmp __rt_icss_find_x");
    emitter.label("__rt_icss_found_x");

    emitter.instruction(&format!("cmp r11, {}", ICONV_SPEC_BUFFER_BYTES - 1));
    emitter.instruction("ja __rt_icss_miss_x");                                 // too long to hold: refuse, never truncate
    emitter.instruction("xor r8d, r8d");
    emitter.label("__rt_icss_from_copy_x");
    emitter.instruction("cmp r8, r11");
    emitter.instruction("jge __rt_icss_from_done_x");
    emitter.instruction("movzx eax, BYTE PTR [r9 + r8]");
    emitter.instruction("mov BYTE PTR [rdx + r8], al");
    emitter.instruction("add r8, 1");
    emitter.instruction("jmp __rt_icss_from_copy_x");
    emitter.label("__rt_icss_from_done_x");
    emitter.instruction("mov BYTE PTR [rdx + r8], 0");                          // iconv_open takes C strings

    emitter.instruction("lea rax, [r11 + 1]");                                  // first byte after '/'
    emitter.instruction("mov r11, rax");
    emitter.instruction("mov rax, r10");
    emitter.instruction("sub rax, r11");                                        // its length
    emitter.instruction(&format!("cmp rax, {}", ICONV_SPEC_BUFFER_BYTES - 1));
    emitter.instruction("ja __rt_icss_miss_x");
    emitter.instruction("xor r8d, r8d");
    emitter.label("__rt_icss_to_copy_x");
    emitter.instruction("cmp r8, rax");
    emitter.instruction("jge __rt_icss_to_done_x");
    emitter.instruction("mov r10, r11");
    emitter.instruction("add r10, r8");
    emitter.instruction("movzx r10d, BYTE PTR [r9 + r10]");
    emitter.instruction("mov BYTE PTR [rcx + r8], r10b");
    emitter.instruction("add r8, 1");
    emitter.instruction("jmp __rt_icss_to_copy_x");
    emitter.label("__rt_icss_to_done_x");
    emitter.instruction("mov BYTE PTR [rcx + r8], 0");
    emitter.instruction("mov rax, 1");                                          // both halves are in the buffers
    emitter.instruction("ret");

    emitter.label("__rt_icss_miss_x");
    emitter.instruction("xor eax, eax");                                        // not an iconv filter name
    emitter.instruction("ret");
}

/// Silences the unused-import warning when neither arm references the ABI helper.
#[allow(dead_code)]
fn _abi_used(emitter: &mut Emitter) {
    let _ = abi::int_result_reg(emitter);
}
