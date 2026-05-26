//! Purpose:
//! Emits AArch64 RFC 8259 JSON validator runtime helper.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen::runtime::system` during runtime emission.
//!
//! Key details:
//! - Validation owns syntax, UTF-16 surrogate, and depth diagnostics without producing a decoded value.

use crate::codegen::emit::Emitter;

/// Emits the `__rt_json_validate` entry point and all JSON validator helper
/// routines. On entry `x0` holds the source pointer and `x1` holds the source
/// length; on exit `x0` is 1 on success or 0 on failure. Syntax errors and UTF-16
/// surrogate errors are recorded in `_json_last_error` and thrown via
/// `__rt_json_throw_error` when `JSON_THROW_ON_ERROR` is set.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_validate ---");
    emitter.label_global("__rt_json_validate");

    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a stable frame pointer

    // Park the source slice in BSS so every helper can reach it without
    // walking parent stack frames (the helpers recurse).
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_ptr");
    emitter.instruction("str x1, [x9]");                                        // publish the source pointer for parse helpers
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_len");
    emitter.instruction("str x2, [x9]");                                        // publish the source length for parse helpers
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("str xzr, [x9]");                                       // start parsing at the beginning of the input
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_depth");
    emitter.instruction("str xzr, [x9]");                                       // begin at depth 0 before consuming any open bracket

    emitter.instruction("bl __rt_json_validate_skip_ws");                       // skip leading whitespace
    emitter.instruction("bl __rt_json_validate_value");                         // parse exactly one JSON value
    emitter.instruction("cbz x0, __rt_json_validate_fail");                     // any failure short-circuits to the fail tail
    emitter.instruction("bl __rt_json_validate_skip_ws");                       // skip trailing whitespace
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x10, [x9]");                                       // load the post-value source index
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_validate_len");
    emitter.instruction("ldr x11, [x11]");                                      // reload the source length
    emitter.instruction("cmp x10, x11");                                        // is the index past the end?
    emitter.instruction("b.lt __rt_json_validate_syntax_error");                // trailing content after a complete value is invalid

    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_last_error");
    emitter.instruction("str xzr, [x9]");                                       // clear stale error so callers see JSON_ERROR_NONE
    emitter.instruction("mov x0, #1");                                          // report success
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return success

    emitter.label("__rt_json_validate_fail");
    emitter.instruction("mov x0, #0");                                          // report failure to the caller
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_syntax_error");
    emitter.instruction("mov x0, #4");                                          // JSON_ERROR_SYNTAX
    emitter.instruction("bl __rt_json_throw_error");                            // record + throw if requested
    emitter.instruction("mov x0, #0");                                          // report failure (only reached on the no-throw path)
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emit_skip_ws_aarch64(emitter);
    emit_value_aarch64(emitter);
    emit_match_literal_aarch64(emitter, "true", &['t', 'r', 'u', 'e']);
    emit_match_literal_aarch64(emitter, "null", &['n', 'u', 'l', 'l']);
    emit_match_literal_aarch64(emitter, "false", &['f', 'a', 'l', 's', 'e']);
    emit_string_parser_aarch64(emitter);
    emit_number_parser_aarch64(emitter);
    emit_array_parser_aarch64(emitter);
    emit_object_parser_aarch64(emitter);
}

/// Emits `__rt_json_validate_skip_ws`: advances the source index past all
/// RFC 8259 whitespace bytes (space, tab, LF, CR). Uses `_json_validate_idx`
/// as a persistent cursor; on exit the index has passed the last whitespace
/// byte or reached the end of input.
fn emit_skip_ws_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_json_validate_skip_ws");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save linkage
    emitter.instruction("mov x29, sp");                                         // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x12, [x9]");                                       // x12 = current source index
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_json_validate_len");
    emitter.instruction("ldr x10, [x10]");                                      // x10 = source length
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_validate_ptr");
    emitter.instruction("ldr x11, [x11]");                                      // x11 = source pointer
    emitter.label("__rt_json_validate_skip_ws_loop");
    emitter.instruction("cmp x12, x10");                                        // are we past the end of input?
    emitter.instruction("b.ge __rt_json_validate_skip_ws_done");                // branch on the current JSON validator condition
    emitter.instruction("ldrb w13, [x11, x12]");                                // load the next byte
    emitter.instruction("cmp w13, #32");                                        // space?
    emitter.instruction("b.eq __rt_json_validate_skip_ws_step");                // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #9");                                         // tab?
    emitter.instruction("b.eq __rt_json_validate_skip_ws_step");                // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #10");                                        // LF?
    emitter.instruction("b.eq __rt_json_validate_skip_ws_step");                // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #13");                                        // CR?
    emitter.instruction("b.ne __rt_json_validate_skip_ws_done");                // branch on the current JSON validator condition
    emitter.label("__rt_json_validate_skip_ws_step");
    emitter.instruction("add x12, x12, #1");                                    // consume the whitespace byte
    emitter.instruction("b __rt_json_validate_skip_ws_loop");                   // continue in the JSON validator control path
    emitter.label("__rt_json_validate_skip_ws_done");
    emitter.instruction("str x12, [x9]");                                       // republish the post-whitespace index
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
}

/// Emits `__rt_json_validate_value`: reads the byte at `_json_validate_idx`
/// and dispatches to the appropriate literal/object/array/number/string parser.
/// Returns 1 in `x0` on success, 0 on failure. Syntax errors jump to
/// `__rt_json_validate_value_syntax` which calls `__rt_json_throw_error`.
fn emit_value_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_json_validate_value");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save linkage
    emitter.instruction("mov x29, sp");                                         // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x12, [x9]");                                       // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_json_validate_len");
    emitter.instruction("ldr x10, [x10]");                                      // load or prepare JSON validator state
    emitter.instruction("cmp x12, x10");                                        // any byte to read?
    emitter.instruction("b.ge __rt_json_validate_value_syntax");                // branch on the current JSON validator condition
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_validate_ptr");
    emitter.instruction("ldr x11, [x11]");                                      // load or prepare JSON validator state
    emitter.instruction("ldrb w13, [x11, x12]");                                // peek the dispatch byte
    emitter.instruction("cmp w13, #34");                                        // string opener?
    emitter.instruction("b.eq __rt_json_validate_value_string");                // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #91");                                        // array opener?
    emitter.instruction("b.eq __rt_json_validate_value_array");                 // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #123");                                       // object opener?
    emitter.instruction("b.eq __rt_json_validate_value_object");                // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #116");                                       // 't' for "true"?
    emitter.instruction("b.eq __rt_json_validate_value_true");                  // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #102");                                       // 'f' for "false"?
    emitter.instruction("b.eq __rt_json_validate_value_false");                 // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #110");                                       // 'n' for "null"?
    emitter.instruction("b.eq __rt_json_validate_value_null");                  // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #45");                                        // negative number?
    emitter.instruction("b.eq __rt_json_validate_value_number");                // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #48");                                        // digit '0'..?
    emitter.instruction("b.lt __rt_json_validate_value_syntax");                // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #57");                                        // ..'9'?
    emitter.instruction("b.le __rt_json_validate_value_number");                // branch on the current JSON validator condition

    emitter.label("__rt_json_validate_value_syntax");
    emitter.instruction("mov x0, #4");                                          // JSON_ERROR_SYNTAX
    emitter.instruction("bl __rt_json_throw_error");                            // call the json throw error helper
    emitter.instruction("mov x0, #0");                                          // load or prepare JSON validator state
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_value_string");
    emitter.instruction("bl __rt_json_validate_string");                        // call the json validate string helper
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
    emitter.label("__rt_json_validate_value_number");
    emitter.instruction("bl __rt_json_validate_number");                        // call the json validate number helper
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
    emitter.label("__rt_json_validate_value_array");
    emitter.instruction("bl __rt_json_validate_array");                         // call the json validate array helper
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
    emitter.label("__rt_json_validate_value_object");
    emitter.instruction("bl __rt_json_validate_object");                        // call the json validate object helper
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
    emitter.label("__rt_json_validate_value_true");
    emitter.instruction("bl __rt_json_validate_match_true");                    // call the json validate match true helper
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
    emitter.label("__rt_json_validate_value_false");
    emitter.instruction("bl __rt_json_validate_match_false");                   // call the json validate match false helper
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
    emitter.label("__rt_json_validate_value_null");
    emitter.instruction("bl __rt_json_validate_match_null");                    // call the json validate match null helper
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
}

/// Emits a literal-match helper (`__rt_json_validate_match_<suffix>`) that
/// validates exactly `lit.len()` bytes against the supplied character sequence.
/// On success updates `_json_validate_idx` and returns 1; on mismatch or
/// truncated input jumps to the fail label which calls `__rt_json_throw_error`
/// with JSON_ERROR_SYNTAX and returns 0.
fn emit_match_literal_aarch64(emitter: &mut Emitter, suffix: &str, lit: &[char]) {
    let label = format!("__rt_json_validate_match_{}", suffix);
    let fail_label = format!("__rt_json_validate_match_{}_fail", suffix);
    let n = lit.len() as u64;
    emitter.label(&label);
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // store updated JSON validator state
    emitter.instruction("mov x29, sp");                                         // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x12, [x9]");                                       // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_json_validate_len");
    emitter.instruction("ldr x10, [x10]");                                      // load or prepare JSON validator state
    emitter.instruction(&format!("add x13, x12, #{}", n));                      // update the JSON validator cursor or counter
    emitter.instruction("cmp x13, x10");                                        // does the literal fit in the input?
    emitter.instruction(&format!("b.gt {}", fail_label));                       // branch on the current JSON validator condition
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_validate_ptr");
    emitter.instruction("ldr x11, [x11]");                                      // load or prepare JSON validator state
    for (offset, &c) in lit.iter().enumerate() {
        emitter.instruction(&format!("add x13, x12, #{}", offset));             // update the JSON validator cursor or counter
        emitter.instruction("ldrb w14, [x11, x13]");                            // load the candidate byte
        emitter.instruction(&format!("cmp w14, #{}", c as u32));                // check the current JSON validator condition
        emitter.instruction(&format!("b.ne {}", fail_label));                   // branch on the current JSON validator condition
    }
    emitter.instruction(&format!("add x12, x12, #{}", n));                      // update the JSON validator cursor or counter
    emitter.instruction("str x12, [x9]");                                       // republish the post-literal index
    emitter.instruction("mov x0, #1");                                          // load or prepare JSON validator state
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label(&fail_label);
    emitter.instruction("mov x0, #4");                                          // JSON_ERROR_SYNTAX
    emitter.instruction("bl __rt_json_throw_error");                            // call the json throw error helper
    emitter.instruction("mov x0, #0");                                          // load or prepare JSON validator state
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
}

/// Emits `__rt_json_validate_string`: parses a JSON string from the source
/// starting at `_json_validate_idx` (expects the opening `"`). Validates all
/// escape sequences including `\uXXXX` and UTF-16 surrogate pairs per RFC 8259.
/// On success advances `_json_validate_idx` past the closing `"` and returns 1.
/// On syntax/UTF-16 error commits the failure index to `_json_validate_idx`,
/// sets `_json_last_error` to JSON_ERROR_SYNTAX (4) or JSON_ERROR_UTF16 (10),
/// and returns 0.
fn emit_string_parser_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_json_validate_string");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // store updated JSON validator state
    emitter.instruction("mov x29, sp");                                         // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x12, [x9]");                                       // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_json_validate_len");
    emitter.instruction("ldr x10, [x10]");                                      // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_validate_ptr");
    emitter.instruction("ldr x11, [x11]");                                      // load or prepare JSON validator state

    emitter.instruction("cmp x12, x10");                                        // any byte left?
    emitter.instruction("b.ge __rt_json_validate_string_syntax");               // branch on the current JSON validator condition
    emitter.instruction("ldrb w13, [x11, x12]");                                // expect '"'
    emitter.instruction("cmp w13, #34");                                        // check the current JSON validator condition
    emitter.instruction("b.ne __rt_json_validate_string_syntax");               // branch on the current JSON validator condition
    emitter.instruction("add x12, x12, #1");                                    // consume the opening quote

    emitter.label("__rt_json_validate_string_loop");
    emitter.instruction("cmp x12, x10");                                        // unterminated string at end of input?
    emitter.instruction("b.ge __rt_json_validate_string_syntax");               // branch on the current JSON validator condition
    emitter.instruction("ldrb w13, [x11, x12]");                                // load the next content byte
    emitter.instruction("cmp w13, #34");                                        // closing quote?
    emitter.instruction("b.eq __rt_json_validate_string_close");                // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #92");                                        // backslash escape?
    emitter.instruction("b.eq __rt_json_validate_string_escape");               // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #32");                                        // unescaped control characters (< 0x20) are invalid in JSON strings
    emitter.instruction("b.lt __rt_json_validate_string_syntax");               // branch on the current JSON validator condition
    emitter.instruction("add x12, x12, #1");                                    // consume the literal byte
    emitter.instruction("b __rt_json_validate_string_loop");                    // continue in the JSON validator control path

    emitter.label("__rt_json_validate_string_close");
    emitter.instruction("add x12, x12, #1");                                    // consume the closing quote
    emitter.instruction("str x12, [x9]");                                       // republish the post-string index
    emitter.instruction("mov x0, #1");                                          // load or prepare JSON validator state
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_string_escape");
    emitter.instruction("add x12, x12, #1");                                    // skip the backslash
    emitter.instruction("cmp x12, x10");                                        // truncated escape?
    emitter.instruction("b.ge __rt_json_validate_string_syntax");               // branch on the current JSON validator condition
    emitter.instruction("ldrb w13, [x11, x12]");                                // load the escape byte
    emitter.instruction("cmp w13, #34");                                        // \\\"?
    emitter.instruction("b.eq __rt_json_validate_string_escape_simple");        // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #92");                                        // \\\\?
    emitter.instruction("b.eq __rt_json_validate_string_escape_simple");        // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #47");                                        // \\/?
    emitter.instruction("b.eq __rt_json_validate_string_escape_simple");        // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #98");                                        // \\b?
    emitter.instruction("b.eq __rt_json_validate_string_escape_simple");        // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #102");                                       // \\f?
    emitter.instruction("b.eq __rt_json_validate_string_escape_simple");        // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #110");                                       // \\n?
    emitter.instruction("b.eq __rt_json_validate_string_escape_simple");        // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #114");                                       // \\r?
    emitter.instruction("b.eq __rt_json_validate_string_escape_simple");        // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #116");                                       // \\t?
    emitter.instruction("b.eq __rt_json_validate_string_escape_simple");        // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #117");                                       // \\u?
    emitter.instruction("b.eq __rt_json_validate_string_escape_unicode");       // branch on the current JSON validator condition
    emitter.instruction("b __rt_json_validate_string_syntax");                  // continue in the JSON validator control path

    emitter.label("__rt_json_validate_string_escape_simple");
    emitter.instruction("add x12, x12, #1");                                    // consume the simple escape byte
    emitter.instruction("b __rt_json_validate_string_loop");                    // continue in the JSON validator control path

    emitter.label("__rt_json_validate_string_escape_unicode");
    emitter.instruction("add x12, x12, #1");                                    // consume the 'u'
    emitter.instruction("mov w15, #0");                                         // codepoint accumulator (16-bit)
    emitter.instruction("mov x14, #4");                                         // remaining hex-digit count
    emit_uhex_loop_aarch64(emitter, "high", "__rt_json_validate_string_syntax"); // validate + accumulate 4 hex digits

    // -- surrogate-pair validation --
    // After accumulating the first \uXXXX, classify the codepoint:
    //   * 0xD800..0xDBFF → high surrogate; expect a following \uYYYY whose
    //     value is in 0xDC00..0xDFFF (low surrogate). Anything else triggers
    //     JSON_ERROR_UTF16 (per RFC 8259 §7 + PHP `Single unpaired UTF-16
    //     surrogate in unicode escape`).
    //   * 0xDC00..0xDFFF → lone low surrogate without a preceding high →
    //     JSON_ERROR_UTF16.
    //   * anything else  → ordinary BMP codepoint, resume content scanning.
    emitter.instruction("mov w17, #0xD800");                                    // start of high-surrogate range
    emitter.instruction("cmp w15, w17");                                        // codepoint < 0xD800?
    emitter.instruction("b.lt __rt_json_validate_string_loop");                 // not a surrogate → resume content scan
    emitter.instruction("mov w17, #0xDFFF");                                    // end of surrogate range
    emitter.instruction("cmp w15, w17");                                        // codepoint > 0xDFFF?
    emitter.instruction("b.gt __rt_json_validate_string_loop");                 // not a surrogate → resume content scan
    emitter.instruction("mov w17, #0xDC00");                                    // first low-surrogate value
    emitter.instruction("cmp w15, w17");                                        // is the codepoint a low surrogate?
    emitter.instruction("b.ge __rt_json_validate_string_utf16");                // lone low surrogate → JSON_ERROR_UTF16

    // High surrogate: require an immediately following `\u`.
    emitter.instruction("cmp x12, x10");                                        // any byte left?
    emitter.instruction("b.ge __rt_json_validate_string_utf16");                // truncated → UTF16 error
    emitter.instruction("ldrb w13, [x11, x12]");                                // peek the byte after the high surrogate
    emitter.instruction("cmp w13, #92");                                        // backslash?
    emitter.instruction("b.ne __rt_json_validate_string_utf16");                // anything else → UTF16 error
    emitter.instruction("add x12, x12, #1");                                    // consume the backslash
    emitter.instruction("cmp x12, x10");                                        // check the current JSON validator condition
    emitter.instruction("b.ge __rt_json_validate_string_utf16");                // branch on the current JSON validator condition
    emitter.instruction("ldrb w13, [x11, x12]");                                // load or prepare JSON validator state
    emitter.instruction("cmp w13, #117");                                       // 'u'?
    emitter.instruction("b.ne __rt_json_validate_string_utf16");                // not a `\u` escape → UTF16 error
    emitter.instruction("add x12, x12, #1");                                    // consume the 'u'

    // Parse and accumulate the low surrogate's 4 hex digits.
    emitter.instruction("mov w15, #0");                                         // reset the accumulator for the second codepoint
    emitter.instruction("mov x14, #4");                                         // remaining hex-digit count
    emit_uhex_loop_aarch64(emitter, "low", "__rt_json_validate_string_utf16");  // syntax errors in the second \u → UTF16 (PHP)

    // The second codepoint MUST be in the low-surrogate range.
    emitter.instruction("mov w17, #0xDC00");                                    // start of low-surrogate range
    emitter.instruction("cmp w15, w17");                                        // is the second codepoint < 0xDC00?
    emitter.instruction("b.lt __rt_json_validate_string_utf16");                // not a low surrogate → UTF16 error
    emitter.instruction("mov w17, #0xDFFF");                                    // end of low-surrogate range
    emitter.instruction("cmp w15, w17");                                        // is the second codepoint > 0xDFFF?
    emitter.instruction("b.gt __rt_json_validate_string_utf16");                // not a low surrogate → UTF16 error
    emitter.instruction("b __rt_json_validate_string_loop");                    // valid surrogate pair → resume content scan

    emitter.label("__rt_json_validate_string_utf16");
    emitter.instruction("str x12, [x9]");                                       // commit the failure index for diagnostics
    emitter.instruction("mov x0, #10");                                         // JSON_ERROR_UTF16
    emitter.instruction("bl __rt_json_throw_error");                            // record the error and throw on JSON_THROW_ON_ERROR
    emitter.instruction("mov x0, #0");                                          // load or prepare JSON validator state
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_string_syntax");
    emitter.instruction("str x12, [x9]");                                       // commit the failure index for downstream diagnostics
    emitter.instruction("mov x0, #4");                                          // JSON_ERROR_SYNTAX
    emitter.instruction("bl __rt_json_throw_error");                            // call the json throw error helper
    emitter.instruction("mov x0, #0");                                          // load or prepare JSON validator state
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
}

/// Emit a 4-hex-digit accumulator loop that walks the source slice and
/// builds up a 16-bit codepoint in `w15`. On entry: `x14 = 4`,
/// `w15 = 0`, `x12` points at the first hex digit (already past `\u`),
/// `x10 = source length`, `x11 = source pointer`. On exit (the
/// `__rt_json_validate_uhex_done_<suffix>` label) `w15` holds the
/// validated codepoint and `x12` has advanced past the four digits.
/// On any non-hex byte (or a truncated tail) the helper jumps to the
/// shared `__rt_json_validate_string_syntax` label, so the codepoint
/// classification logic that follows can assume well-formed digits.
fn emit_uhex_loop_aarch64(emitter: &mut Emitter, suffix: &str, error_label: &str) {
    emitter.label(&format!("__rt_json_validate_uhex_loop_{suffix}"));
    emitter.instruction("cmp x14, #0");                                         // 4 digits consumed?
    emitter.instruction(&format!("b.eq __rt_json_validate_uhex_done_{suffix}")); // exit loop with w15 = codepoint
    emitter.instruction("cmp x12, x10");                                        // bounds check
    emitter.instruction(&format!("b.ge {error_label}"));                        // branch on the current JSON validator condition
    emitter.instruction("ldrb w13, [x11, x12]");                                // load or prepare JSON validator state
    emitter.instruction("cmp w13, #48");                                        // '0'?
    emitter.instruction(&format!("b.lt {error_label}"));                        // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #57");                                        // ..'9'?
    emitter.instruction(&format!("b.le __rt_json_validate_uhex_dec_{suffix}")); // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #65");                                        // 'A'?
    emitter.instruction(&format!("b.lt {error_label}"));                        // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #70");                                        // ..'F'?
    emitter.instruction(&format!("b.le __rt_json_validate_uhex_upper_{suffix}")); // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #97");                                        // 'a'?
    emitter.instruction(&format!("b.lt {error_label}"));                        // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #102");                                       // ..'f'?
    emitter.instruction(&format!("b.gt {error_label}"));                        // branch on the current JSON validator condition
    emitter.instruction("sub w13, w13, #87");                                   // 'a'..'f' → 10..15
    emitter.instruction(&format!("b __rt_json_validate_uhex_acc_{suffix}"));    // continue in the JSON validator control path
    emitter.label(&format!("__rt_json_validate_uhex_dec_{suffix}"));
    emitter.instruction("sub w13, w13, #48");                                   // '0'..'9' → 0..9
    emitter.instruction(&format!("b __rt_json_validate_uhex_acc_{suffix}"));    // continue in the JSON validator control path
    emitter.label(&format!("__rt_json_validate_uhex_upper_{suffix}"));
    emitter.instruction("sub w13, w13, #55");                                   // 'A'..'F' → 10..15
    emitter.label(&format!("__rt_json_validate_uhex_acc_{suffix}"));
    emitter.instruction("lsl w15, w15, #4");                                    // shift accumulator nibble
    emitter.instruction("orr w15, w15, w13");                                   // OR in the digit value
    emitter.instruction("add x12, x12, #1");                                    // advance past the digit
    emitter.instruction("sub x14, x14, #1");                                    // one fewer digit to scan
    emitter.instruction(&format!("b __rt_json_validate_uhex_loop_{suffix}"));   // continue in the JSON validator control path
    emitter.label(&format!("__rt_json_validate_uhex_done_{suffix}"));
}

/// Emits `__rt_json_validate_number`: parses a JSON number from the source
/// starting at `_json_validate_idx`. Accepts an optional leading `-`, a
/// non-zero digit for the integer part (or `0` alone), an optional fractional
/// part, and an optional exponent (`e`/`E` with optional `+`/`-`). On success
/// advances `_json_validate_idx` past the last digit and returns 1. On invalid
/// syntax returns 0 and calls `__rt_json_throw_error`.
fn emit_number_parser_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_json_validate_number");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // store updated JSON validator state
    emitter.instruction("mov x29, sp");                                         // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x12, [x9]");                                       // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_json_validate_len");
    emitter.instruction("ldr x10, [x10]");                                      // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_validate_ptr");
    emitter.instruction("ldr x11, [x11]");                                      // load or prepare JSON validator state

    emitter.instruction("cmp x12, x10");                                        // any byte to read?
    emitter.instruction("b.ge __rt_json_validate_number_syntax");               // branch on the current JSON validator condition
    emitter.instruction("ldrb w13, [x11, x12]");                                // peek the first byte
    emitter.instruction("cmp w13, #45");                                        // '-'?
    emitter.instruction("b.ne __rt_json_validate_number_int_start");            // branch on the current JSON validator condition
    emitter.instruction("add x12, x12, #1");                                    // consume the minus sign
    emitter.instruction("cmp x12, x10");                                        // bare '-' is invalid
    emitter.instruction("b.ge __rt_json_validate_number_syntax");               // branch on the current JSON validator condition

    emitter.label("__rt_json_validate_number_int_start");
    emitter.instruction("ldrb w13, [x11, x12]");                                // load the first digit of the integer part
    emitter.instruction("cmp w13, #48");                                        // '0' alone allowed
    emitter.instruction("b.eq __rt_json_validate_number_zero");                 // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #49");                                        // '1'..
    emitter.instruction("b.lt __rt_json_validate_number_syntax");               // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #57");                                        // ..'9'?
    emitter.instruction("b.gt __rt_json_validate_number_syntax");               // branch on the current JSON validator condition
    emitter.instruction("add x12, x12, #1");                                    // consume the leading nonzero digit
    emitter.label("__rt_json_validate_number_int_loop");
    emitter.instruction("cmp x12, x10");                                        // check the current JSON validator condition
    emitter.instruction("b.ge __rt_json_validate_number_done");                 // branch on the current JSON validator condition
    emitter.instruction("ldrb w13, [x11, x12]");                                // load or prepare JSON validator state
    emitter.instruction("sub w14, w13, #48");                                   // is it '0'..'9'?
    emitter.instruction("cmp w14, #9");                                         // check the current JSON validator condition
    emitter.instruction("b.hi __rt_json_validate_number_after_int");            // branch on the current JSON validator condition
    emitter.instruction("add x12, x12, #1");                                    // consume the digit
    emitter.instruction("b __rt_json_validate_number_int_loop");                // continue in the JSON validator control path

    emitter.label("__rt_json_validate_number_zero");
    emitter.instruction("add x12, x12, #1");                                    // consume the '0'
    emitter.instruction("cmp x12, x10");                                        // check the current JSON validator condition
    emitter.instruction("b.ge __rt_json_validate_number_done");                 // branch on the current JSON validator condition

    emitter.label("__rt_json_validate_number_after_int");
    emitter.instruction("ldrb w13, [x11, x12]");                                // load or prepare JSON validator state
    emitter.instruction("cmp w13, #46");                                        // '.'?
    emitter.instruction("b.eq __rt_json_validate_number_frac");                 // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #101");                                       // 'e'?
    emitter.instruction("b.eq __rt_json_validate_number_exp");                  // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #69");                                        // 'E'?
    emitter.instruction("b.eq __rt_json_validate_number_exp");                  // branch on the current JSON validator condition
    emitter.instruction("b __rt_json_validate_number_done");                    // any other byte ends the number

    emitter.label("__rt_json_validate_number_frac");
    emitter.instruction("add x12, x12, #1");                                    // consume the dot
    emitter.instruction("cmp x12, x10");                                        // truncated fraction?
    emitter.instruction("b.ge __rt_json_validate_number_syntax");               // branch on the current JSON validator condition
    emitter.instruction("ldrb w13, [x11, x12]");                                // load the first fraction digit
    emitter.instruction("sub w14, w13, #48");                                   // update the JSON validator cursor or counter
    emitter.instruction("cmp w14, #9");                                         // check the current JSON validator condition
    emitter.instruction("b.hi __rt_json_validate_number_syntax");               // branch on the current JSON validator condition
    emitter.instruction("add x12, x12, #1");                                    // consume it
    emitter.label("__rt_json_validate_number_frac_loop");
    emitter.instruction("cmp x12, x10");                                        // check the current JSON validator condition
    emitter.instruction("b.ge __rt_json_validate_number_done");                 // branch on the current JSON validator condition
    emitter.instruction("ldrb w13, [x11, x12]");                                // load or prepare JSON validator state
    emitter.instruction("sub w14, w13, #48");                                   // update the JSON validator cursor or counter
    emitter.instruction("cmp w14, #9");                                         // check the current JSON validator condition
    emitter.instruction("b.hi __rt_json_validate_number_after_frac");           // branch on the current JSON validator condition
    emitter.instruction("add x12, x12, #1");                                    // consume the digit
    emitter.instruction("b __rt_json_validate_number_frac_loop");               // continue in the JSON validator control path

    emitter.label("__rt_json_validate_number_after_frac");
    emitter.instruction("cmp w13, #101");                                       // 'e'?
    emitter.instruction("b.eq __rt_json_validate_number_exp");                  // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #69");                                        // 'E'?
    emitter.instruction("b.eq __rt_json_validate_number_exp");                  // branch on the current JSON validator condition
    emitter.instruction("b __rt_json_validate_number_done");                    // continue in the JSON validator control path

    emitter.label("__rt_json_validate_number_exp");
    emitter.instruction("add x12, x12, #1");                                    // consume the 'e' or 'E'
    emitter.instruction("cmp x12, x10");                                        // truncated exponent?
    emitter.instruction("b.ge __rt_json_validate_number_syntax");               // branch on the current JSON validator condition
    emitter.instruction("ldrb w13, [x11, x12]");                                // peek the next byte
    emitter.instruction("cmp w13, #43");                                        // optional '+'?
    emitter.instruction("b.eq __rt_json_validate_number_exp_sign_consume");     // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #45");                                        // optional '-'?
    emitter.instruction("b.eq __rt_json_validate_number_exp_sign_consume");     // branch on the current JSON validator condition
    emitter.instruction("b __rt_json_validate_number_exp_first");               // continue in the JSON validator control path
    emitter.label("__rt_json_validate_number_exp_sign_consume");
    emitter.instruction("add x12, x12, #1");                                    // consume the sign
    emitter.instruction("cmp x12, x10");                                        // check the current JSON validator condition
    emitter.instruction("b.ge __rt_json_validate_number_syntax");               // branch on the current JSON validator condition
    emitter.instruction("ldrb w13, [x11, x12]");                                // load or prepare JSON validator state
    emitter.label("__rt_json_validate_number_exp_first");
    emitter.instruction("sub w14, w13, #48");                                   // update the JSON validator cursor or counter
    emitter.instruction("cmp w14, #9");                                         // check the current JSON validator condition
    emitter.instruction("b.hi __rt_json_validate_number_syntax");               // branch on the current JSON validator condition
    emitter.instruction("add x12, x12, #1");                                    // consume the first exponent digit
    emitter.label("__rt_json_validate_number_exp_loop");
    emitter.instruction("cmp x12, x10");                                        // check the current JSON validator condition
    emitter.instruction("b.ge __rt_json_validate_number_done");                 // branch on the current JSON validator condition
    emitter.instruction("ldrb w13, [x11, x12]");                                // load or prepare JSON validator state
    emitter.instruction("sub w14, w13, #48");                                   // update the JSON validator cursor or counter
    emitter.instruction("cmp w14, #9");                                         // check the current JSON validator condition
    emitter.instruction("b.hi __rt_json_validate_number_done");                 // branch on the current JSON validator condition
    emitter.instruction("add x12, x12, #1");                                    // update the JSON validator cursor or counter
    emitter.instruction("b __rt_json_validate_number_exp_loop");                // continue in the JSON validator control path

    emitter.label("__rt_json_validate_number_done");
    emitter.instruction("str x12, [x9]");                                       // store updated JSON validator state
    emitter.instruction("mov x0, #1");                                          // load or prepare JSON validator state
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_number_syntax");
    emitter.instruction("str x12, [x9]");                                       // store updated JSON validator state
    emitter.instruction("mov x0, #4");                                          // JSON_ERROR_SYNTAX
    emitter.instruction("bl __rt_json_throw_error");                            // call the json throw error helper
    emitter.instruction("mov x0, #0");                                          // load or prepare JSON validator state
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
}

/// Emits `__rt_json_validate_array`: parses a JSON array beginning at
/// `_json_validate_idx` (expects `[`). Increments `_json_active_depth` and
/// checks against `_json_depth_limit` before descending; on overflow jumps
/// to `__rt_json_validate_array_depth` which sets `_json_last_error` to
/// JSON_ERROR_DEPTH (1) and calls `__rt_json_throw_error`. Recursively parses
/// elements with `__rt_json_validate_value`. On success decrements depth,
/// advances `_json_validate_idx` past `]`, and returns 1. On any failure
/// propagates 0 to the caller without writing depth (the caller cleans up).
fn emit_array_parser_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_json_validate_array");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // store updated JSON validator state
    emitter.instruction("mov x29, sp");                                         // load or prepare JSON validator state
    // Increment depth and check the limit.
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_depth");
    emitter.instruction("ldr x12, [x9]");                                       // load or prepare JSON validator state
    emitter.instruction("add x12, x12, #1");                                    // update the JSON validator cursor or counter
    emitter.instruction("str x12, [x9]");                                       // store updated JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_json_depth_limit");
    emitter.instruction("ldr x10, [x10]");                                      // load or prepare JSON validator state
    emitter.instruction("cmp x12, x10");                                        // depth overflow?
    emitter.instruction("b.gt __rt_json_validate_array_depth");                 // branch on the current JSON validator condition

    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x12, [x9]");                                       // load or prepare JSON validator state
    emitter.instruction("add x12, x12, #1");                                    // consume the '['
    emitter.instruction("str x12, [x9]");                                       // store updated JSON validator state
    emitter.instruction("bl __rt_json_validate_skip_ws");                       // call the json validate skip ws helper

    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x12, [x9]");                                       // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_json_validate_len");
    emitter.instruction("ldr x10, [x10]");                                      // load or prepare JSON validator state
    emitter.instruction("cmp x12, x10");                                        // unterminated array?
    emitter.instruction("b.ge __rt_json_validate_array_syntax");                // branch on the current JSON validator condition
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_validate_ptr");
    emitter.instruction("ldr x11, [x11]");                                      // load or prepare JSON validator state
    emitter.instruction("ldrb w13, [x11, x12]");                                // peek the next byte
    emitter.instruction("cmp w13, #93");                                        // ']'?
    emitter.instruction("b.eq __rt_json_validate_array_close");                 // branch on the current JSON validator condition

    emitter.label("__rt_json_validate_array_elem");
    emitter.instruction("bl __rt_json_validate_value");                         // parse one array element
    emitter.instruction("cbz x0, __rt_json_validate_array_propagate");          // branch on the current JSON validator condition
    emitter.instruction("bl __rt_json_validate_skip_ws");                       // call the json validate skip ws helper
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x12, [x9]");                                       // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_json_validate_len");
    emitter.instruction("ldr x10, [x10]");                                      // load or prepare JSON validator state
    emitter.instruction("cmp x12, x10");                                        // truncated input after element?
    emitter.instruction("b.ge __rt_json_validate_array_syntax");                // branch on the current JSON validator condition
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_validate_ptr");
    emitter.instruction("ldr x11, [x11]");                                      // load or prepare JSON validator state
    emitter.instruction("ldrb w13, [x11, x12]");                                // peek the separator byte
    emitter.instruction("cmp w13, #93");                                        // ']' closes
    emitter.instruction("b.eq __rt_json_validate_array_close");                 // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #44");                                        // ',' continues
    emitter.instruction("b.ne __rt_json_validate_array_syntax");                // branch on the current JSON validator condition
    emitter.instruction("add x12, x12, #1");                                    // consume the comma
    emitter.instruction("str x12, [x9]");                                       // store updated JSON validator state
    emitter.instruction("bl __rt_json_validate_skip_ws");                       // call the json validate skip ws helper
    emitter.instruction("b __rt_json_validate_array_elem");                     // continue in the JSON validator control path

    emitter.label("__rt_json_validate_array_close");
    emitter.instruction("add x12, x12, #1");                                    // consume the ']'
    emitter.instruction("str x12, [x9]");                                       // store updated JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_json_active_depth");
    emitter.instruction("ldr x12, [x10]");                                      // load or prepare JSON validator state
    emitter.instruction("sub x12, x12, #1");                                    // ascend
    emitter.instruction("str x12, [x10]");                                      // store updated JSON validator state
    emitter.instruction("mov x0, #1");                                          // load or prepare JSON validator state
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_array_propagate");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_array_syntax");
    emitter.instruction("mov x0, #4");                                          // load or prepare JSON validator state
    emitter.instruction("bl __rt_json_throw_error");                            // call the json throw error helper
    emitter.instruction("mov x0, #0");                                          // load or prepare JSON validator state
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_array_depth");
    emitter.instruction("mov x0, #1");                                          // JSON_ERROR_DEPTH
    emitter.instruction("bl __rt_json_throw_error");                            // call the json throw error helper
    emitter.instruction("mov x0, #0");                                          // load or prepare JSON validator state
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
}

/// Emits `__rt_json_validate_object`: parses a JSON object beginning at
/// `_json_validate_idx` (expects `{`). Increments `_json_active_depth` and
/// checks against `_json_depth_limit` before descending; on overflow jumps
/// to `__rt_json_validate_object_depth` which sets `_json_last_error` to
/// JSON_ERROR_DEPTH (1) and calls `__rt_json_throw_error`. Each member must be
/// a string key followed by `:`, then a value parsed with `__rt_json_validate_value`.
/// On success decrements depth, advances `_json_validate_idx` past `}`, and returns 1.
/// On any failure propagates 0 to the caller without writing depth (the caller cleans up).
fn emit_object_parser_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_json_validate_object");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // store updated JSON validator state
    emitter.instruction("mov x29, sp");                                         // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_depth");
    emitter.instruction("ldr x12, [x9]");                                       // load or prepare JSON validator state
    emitter.instruction("add x12, x12, #1");                                    // update the JSON validator cursor or counter
    emitter.instruction("str x12, [x9]");                                       // store updated JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_json_depth_limit");
    emitter.instruction("ldr x10, [x10]");                                      // load or prepare JSON validator state
    emitter.instruction("cmp x12, x10");                                        // depth overflow?
    emitter.instruction("b.gt __rt_json_validate_object_depth");                // branch on the current JSON validator condition

    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x12, [x9]");                                       // load or prepare JSON validator state
    emitter.instruction("add x12, x12, #1");                                    // consume the '{'
    emitter.instruction("str x12, [x9]");                                       // store updated JSON validator state
    emitter.instruction("bl __rt_json_validate_skip_ws");                       // call the json validate skip ws helper

    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x12, [x9]");                                       // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_json_validate_len");
    emitter.instruction("ldr x10, [x10]");                                      // load or prepare JSON validator state
    emitter.instruction("cmp x12, x10");                                        // unterminated object?
    emitter.instruction("b.ge __rt_json_validate_object_syntax");               // branch on the current JSON validator condition
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_validate_ptr");
    emitter.instruction("ldr x11, [x11]");                                      // load or prepare JSON validator state
    emitter.instruction("ldrb w13, [x11, x12]");                                // peek the next byte
    emitter.instruction("cmp w13, #125");                                       // '}'?
    emitter.instruction("b.eq __rt_json_validate_object_close");                // branch on the current JSON validator condition

    emitter.label("__rt_json_validate_object_pair");
    emitter.instruction("bl __rt_json_validate_string");                        // key must be a JSON string
    emitter.instruction("cbz x0, __rt_json_validate_object_propagate");         // branch on the current JSON validator condition
    emitter.instruction("bl __rt_json_validate_skip_ws");                       // call the json validate skip ws helper
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x12, [x9]");                                       // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_json_validate_len");
    emitter.instruction("ldr x10, [x10]");                                      // load or prepare JSON validator state
    emitter.instruction("cmp x12, x10");                                        // truncated input after key?
    emitter.instruction("b.ge __rt_json_validate_object_syntax");               // branch on the current JSON validator condition
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_validate_ptr");
    emitter.instruction("ldr x11, [x11]");                                      // load or prepare JSON validator state
    emitter.instruction("ldrb w13, [x11, x12]");                                // peek the colon
    emitter.instruction("cmp w13, #58");                                        // ':'?
    emitter.instruction("b.ne __rt_json_validate_object_syntax");               // branch on the current JSON validator condition
    emitter.instruction("add x12, x12, #1");                                    // consume the colon
    emitter.instruction("str x12, [x9]");                                       // store updated JSON validator state
    emitter.instruction("bl __rt_json_validate_skip_ws");                       // call the json validate skip ws helper
    emitter.instruction("bl __rt_json_validate_value");                         // parse the value
    emitter.instruction("cbz x0, __rt_json_validate_object_propagate");         // branch on the current JSON validator condition
    emitter.instruction("bl __rt_json_validate_skip_ws");                       // call the json validate skip ws helper
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_validate_idx");
    emitter.instruction("ldr x12, [x9]");                                       // load or prepare JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_json_validate_len");
    emitter.instruction("ldr x10, [x10]");                                      // load or prepare JSON validator state
    emitter.instruction("cmp x12, x10");                                        // check the current JSON validator condition
    emitter.instruction("b.ge __rt_json_validate_object_syntax");               // branch on the current JSON validator condition
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_json_validate_ptr");
    emitter.instruction("ldr x11, [x11]");                                      // load or prepare JSON validator state
    emitter.instruction("ldrb w13, [x11, x12]");                                // peek the separator
    emitter.instruction("cmp w13, #125");                                       // '}' closes
    emitter.instruction("b.eq __rt_json_validate_object_close");                // branch on the current JSON validator condition
    emitter.instruction("cmp w13, #44");                                        // ',' continues
    emitter.instruction("b.ne __rt_json_validate_object_syntax");               // branch on the current JSON validator condition
    emitter.instruction("add x12, x12, #1");                                    // consume the comma
    emitter.instruction("str x12, [x9]");                                       // store updated JSON validator state
    emitter.instruction("bl __rt_json_validate_skip_ws");                       // call the json validate skip ws helper
    emitter.instruction("b __rt_json_validate_object_pair");                    // continue in the JSON validator control path

    emitter.label("__rt_json_validate_object_close");
    emitter.instruction("add x12, x12, #1");                                    // consume the '}'
    emitter.instruction("str x12, [x9]");                                       // store updated JSON validator state
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_json_active_depth");
    emitter.instruction("ldr x12, [x10]");                                      // load or prepare JSON validator state
    emitter.instruction("sub x12, x12, #1");                                    // ascend
    emitter.instruction("str x12, [x10]");                                      // store updated JSON validator state
    emitter.instruction("mov x0, #1");                                          // load or prepare JSON validator state
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_object_propagate");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_object_syntax");
    emitter.instruction("mov x0, #4");                                          // load or prepare JSON validator state
    emitter.instruction("bl __rt_json_throw_error");                            // call the json throw error helper
    emitter.instruction("mov x0, #0");                                          // load or prepare JSON validator state
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_object_depth");
    emitter.instruction("mov x0, #1");                                          // JSON_ERROR_DEPTH
    emitter.instruction("bl __rt_json_throw_error");                            // call the json throw error helper
    emitter.instruction("mov x0, #0");                                          // load or prepare JSON validator state
    emitter.instruction("ldp x29, x30, [sp], #16");                             // load or prepare JSON validator state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
}

// x86_64 implementation ----------------------------------------------------
