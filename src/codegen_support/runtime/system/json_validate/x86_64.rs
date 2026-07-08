//! Purpose:
//! Emits x86_64 RFC 8259 JSON validator runtime helper.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system` during runtime emission.
//!
//! Key details:
//! - The SysV validator path must mirror AArch64 diagnostics and depth behavior.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::abi;

/// Emits the top-level `__rt_json_validate` entry point and all sub-helpers.
/// Publishes `_json_validate_ptr` / `_json_validate_len`, initializes depth to 0,
/// skips leading whitespace, validates exactly one JSON value, and checks that the
/// cursor reaches the end of input. Returns 1 in `rax` on success, 0 on failure.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_validate ---");
    emitter.label_global("__rt_json_validate");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base

    abi::emit_store_reg_to_symbol(emitter, "rax", "_json_validate_ptr", 0);     // publish the source pointer
    abi::emit_store_reg_to_symbol(emitter, "rdx", "_json_validate_len", 0);     // publish the source length
    abi::emit_store_zero_to_symbol(emitter, "_json_validate_idx", 0);           // start at the beginning of the input
    abi::emit_store_zero_to_symbol(emitter, "_json_active_depth", 0);           // begin at depth 0

    emitter.instruction("call __rt_json_validate_skip_ws_x");                   // call the json validate skip ws x helper
    emitter.instruction("call __rt_json_validate_value_x");                     // call the json validate value x helper
    emitter.instruction("test rax, rax");                                       // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_fail_x");                        // branch on the current JSON validator condition
    emitter.instruction("call __rt_json_validate_skip_ws_x");                   // call the json validate skip ws x helper
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_validate_idx", 0);      // load or prepare JSON validator state
    abi::emit_cmp_reg_to_symbol(emitter, "rcx", "_json_validate_len");          // check the current JSON validator condition
    emitter.instruction("jl __rt_json_validate_syntax_error_x");                // branch on the current JSON validator condition

    abi::emit_store_zero_to_symbol(emitter, "_json_last_error", 0);             // load or prepare JSON validator state
    emitter.instruction("mov rax, 1");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_fail_x");
    emitter.instruction("mov rax, 0");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_syntax_error_x");
    emitter.instruction("mov rax, 4");                                          // load or prepare JSON validator state
    emitter.instruction("call __rt_json_throw_error");                          // call the json throw error helper
    emitter.instruction("mov rax, 0");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emit_skip_ws_x(emitter);
    emit_value_x(emitter);
    emit_match_literal_x(emitter, "true", &['t', 'r', 'u', 'e']);
    emit_match_literal_x(emitter, "null", &['n', 'u', 'l', 'l']);
    emit_match_literal_x(emitter, "false", &['f', 'a', 'l', 's', 'e']);
    emit_string_parser_x(emitter);
    emit_number_parser_x(emitter);
    emit_array_parser_x(emitter);
    emit_object_parser_x(emitter);
}

/// Emits `__rt_json_validate_skip_ws_x`: advances `rcx` (cursor) past RFC 8259
/// whitespace (space, tab, LF, CR) until a non-whitespace byte or end of input.
/// Updates `_json_validate_idx` on exit.
fn emit_skip_ws_x(emitter: &mut Emitter) {
    emitter.label("__rt_json_validate_skip_ws_x");
    emitter.instruction("push rbp");                                            // preserve or restore JSON validator scratch state
    emitter.instruction("mov rbp, rsp");                                        // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_validate_idx", 0);      // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_validate_len", 0);      // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rax", "_json_validate_ptr", 0);      // load or prepare JSON validator state
    emitter.label("__rt_json_validate_skip_ws_loop_x");
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_skip_ws_done_x");               // branch on the current JSON validator condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 32");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_skip_ws_step_x");                // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 9");                                           // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_skip_ws_step_x");                // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 10");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_skip_ws_step_x");                // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 13");                                          // check the current JSON validator condition
    emitter.instruction("jne __rt_json_validate_skip_ws_done_x");               // branch on the current JSON validator condition
    emitter.label("__rt_json_validate_skip_ws_step_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.instruction("jmp __rt_json_validate_skip_ws_loop_x");               // continue in the JSON validator control path
    emitter.label("__rt_json_validate_skip_ws_done_x");
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
}

/// Emits `__rt_json_validate_value_x`: peeks the byte at the current cursor,
/// dispatching to the appropriate literal/container helper based on RFC 8259
/// production rules. Returns 1 in `rax` on success, 0 on propagate/failure.
fn emit_value_x(emitter: &mut Emitter) {
    emitter.label("__rt_json_validate_value_x");
    emitter.instruction("push rbp");                                            // preserve or restore JSON validator scratch state
    emitter.instruction("mov rbp, rsp");                                        // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_validate_idx", 0);      // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_validate_len", 0);      // load or prepare JSON validator state
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_value_syntax_x");               // branch on the current JSON validator condition
    abi::emit_load_symbol_to_reg(emitter, "rax", "_json_validate_ptr", 0);      // load or prepare JSON validator state
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 34");                                          // string opener?
    emitter.instruction("je __rt_json_validate_value_string_x");                // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 91");                                          // array opener?
    emitter.instruction("je __rt_json_validate_value_array_x");                 // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 123");                                         // object opener?
    emitter.instruction("je __rt_json_validate_value_object_x");                // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 116");                                         // 't'?
    emitter.instruction("je __rt_json_validate_value_true_x");                  // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 102");                                         // 'f'?
    emitter.instruction("je __rt_json_validate_value_false_x");                 // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 110");                                         // 'n'?
    emitter.instruction("je __rt_json_validate_value_null_x");                  // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 45");                                          // negative number?
    emitter.instruction("je __rt_json_validate_value_number_x");                // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 48");                                          // digit?
    emitter.instruction("jl __rt_json_validate_value_syntax_x");                // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 57");                                          // check the current JSON validator condition
    emitter.instruction("jle __rt_json_validate_value_number_x");               // branch on the current JSON validator condition

    emitter.label("__rt_json_validate_value_syntax_x");
    emitter.instruction("mov rax, 4");                                          // load or prepare JSON validator state
    emitter.instruction("call __rt_json_throw_error");                          // call the json throw error helper
    emitter.instruction("mov rax, 0");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_value_string_x");
    emitter.instruction("call __rt_json_validate_string_x");                    // call the json validate string x helper
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
    emitter.label("__rt_json_validate_value_number_x");
    emitter.instruction("call __rt_json_validate_number_x");                    // call the json validate number x helper
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
    emitter.label("__rt_json_validate_value_array_x");
    emitter.instruction("call __rt_json_validate_array_x");                     // call the json validate array x helper
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
    emitter.label("__rt_json_validate_value_object_x");
    emitter.instruction("call __rt_json_validate_object_x");                    // call the json validate object x helper
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
    emitter.label("__rt_json_validate_value_true_x");
    emitter.instruction("call __rt_json_validate_match_true_x");                // call the json validate match true x helper
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
    emitter.label("__rt_json_validate_value_false_x");
    emitter.instruction("call __rt_json_validate_match_false_x");               // call the json validate match false x helper
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
    emitter.label("__rt_json_validate_value_null_x");
    emitter.instruction("call __rt_json_validate_match_null_x");                // call the json validate match null x helper
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
}

/// Emits a literal-match helper (e.g. `__rt_json_validate_match_true_x`).
/// Validates `lit` (e.g. "true" → ['t','r','u','e']) at the current cursor.
/// On mismatch or short input jumps to a syntax-error path; on success returns 1.
fn emit_match_literal_x(emitter: &mut Emitter, suffix: &str, lit: &[char]) {
    let label = format!("__rt_json_validate_match_{}_x", suffix);
    let fail_label = format!("__rt_json_validate_match_{}_fail_x", suffix);
    emitter.label(&label);
    emitter.instruction("push rbp");                                            // preserve or restore JSON validator scratch state
    emitter.instruction("mov rbp, rsp");                                        // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_validate_idx", 0);      // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_validate_len", 0);      // load or prepare JSON validator state
    emitter.instruction(&format!("lea r8, [rcx + {}]", lit.len()));             // load or prepare JSON validator state
    emitter.instruction("cmp r8, rdx");                                         // check the current JSON validator condition
    emitter.instruction(&format!("jg {}", fail_label));                         // branch on the current JSON validator condition
    abi::emit_load_symbol_to_reg(emitter, "rax", "_json_validate_ptr", 0);      // load or prepare JSON validator state
    for (offset, &c) in lit.iter().enumerate() {
        emitter.instruction(&format!("movzx r9, BYTE PTR [rax + rcx + {}]", offset)); // load or prepare JSON validator state
        emitter.instruction(&format!("cmp r9, {}", c as u32));                  // check the current JSON validator condition
        emitter.instruction(&format!("jne {}", fail_label));                    // branch on the current JSON validator condition
    }
    emitter.instruction(&format!("add rcx, {}", lit.len()));                    // update the JSON validator cursor or counter
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // load or prepare JSON validator state
    emitter.instruction("mov rax, 1");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
    emitter.label(&fail_label);
    emitter.instruction("mov rax, 4");                                          // load or prepare JSON validator state
    emitter.instruction("call __rt_json_throw_error");                          // call the json throw error helper
    emitter.instruction("mov rax, 0");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
}

/// Emits `__rt_json_validate_string_x`: validates a RFC 8259 string literal.
/// Consumes the opening `"`, scans content (allowable chars or escape sequences),
/// handles `\u`-escaped codepoints including UTF-16 surrogate pairs, and
/// consumes the closing `"`. Jumps to syntax error on malformed input; returns 1.
fn emit_string_parser_x(emitter: &mut Emitter) {
    emitter.label("__rt_json_validate_string_x");
    emitter.instruction("push rbp");                                            // preserve or restore JSON validator scratch state
    emitter.instruction("mov rbp, rsp");                                        // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_validate_idx", 0);      // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_validate_len", 0);      // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rax", "_json_validate_ptr", 0);      // load or prepare JSON validator state
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_string_syntax_x");              // branch on the current JSON validator condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 34");                                          // check the current JSON validator condition
    emitter.instruction("jne __rt_json_validate_string_syntax_x");              // branch on the current JSON validator condition
    emitter.instruction("add rcx, 1");                                          // consume opening quote

    emitter.label("__rt_json_validate_string_loop_x");
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_string_syntax_x");              // branch on the current JSON validator condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 34");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_string_close_x");                // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 92");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_string_escape_x");               // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 32");                                          // check the current JSON validator condition
    emitter.instruction("jl __rt_json_validate_string_ctrl_char_x");            // report JSON_ERROR_CTRL_CHAR for unescaped controls
    emitter.instruction("cmp r8, 128");                                         // ASCII literals can be consumed one byte at a time
    emitter.instruction("jl __rt_json_validate_string_literal_byte_x");         // consume a single-byte UTF-8/ASCII literal
    emitter.instruction("cmp r8, 194");                                         // reject continuation bytes and overlong two-byte leads
    emitter.instruction("jl __rt_json_validate_string_utf8_x");                 // malformed UTF-8 lead byte
    emitter.instruction("cmp r8, 223");                                         // is this a two-byte UTF-8 lead?
    emitter.instruction("jle __rt_json_validate_string_utf8_2_x");              // validate one continuation byte
    emitter.instruction("cmp r8, 239");                                         // is this a three-byte UTF-8 lead?
    emitter.instruction("jle __rt_json_validate_string_utf8_3_x");              // validate two continuation bytes
    emitter.instruction("cmp r8, 244");                                         // is this a four-byte UTF-8 lead within Unicode range?
    emitter.instruction("jle __rt_json_validate_string_utf8_4_x");              // validate three continuation bytes
    emitter.instruction("jmp __rt_json_validate_string_utf8_x");                // reject lead bytes above U+10FFFF
    emitter.label("__rt_json_validate_string_literal_byte_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.instruction("jmp __rt_json_validate_string_loop_x");                // continue in the JSON validator control path

    emitter.label("__rt_json_validate_string_utf8_2_x");
    emitter.instruction("lea r9, [rcx + 1]");                                   // point at the expected continuation byte
    emitter.instruction("cmp r9, rdx");                                         // ensure the continuation byte is inside the string
    emitter.instruction("jge __rt_json_validate_string_utf8_x");                // truncated UTF-8 sequence
    emitter.instruction("movzx r10, BYTE PTR [rax + r9]");                      // load the continuation byte
    emitter.instruction("cmp r10, 128");                                        // continuation bytes must be >= 0x80
    emitter.instruction("jl __rt_json_validate_string_utf8_x");                 // malformed UTF-8 continuation byte
    emitter.instruction("cmp r10, 191");                                        // continuation bytes must be <= 0xBF
    emitter.instruction("jg __rt_json_validate_string_utf8_x");                 // malformed UTF-8 continuation byte
    emitter.instruction("add rcx, 2");                                          // consume the two-byte UTF-8 sequence
    emitter.instruction("jmp __rt_json_validate_string_loop_x");                // continue after the UTF-8 sequence

    emitter.label("__rt_json_validate_string_utf8_3_x");
    emitter.instruction("lea r9, [rcx + 2]");                                   // point at the final expected continuation byte
    emitter.instruction("cmp r9, rdx");                                         // ensure both continuation bytes are inside the string
    emitter.instruction("jge __rt_json_validate_string_utf8_x");                // truncated UTF-8 sequence
    emitter.instruction("movzx r10, BYTE PTR [rax + rcx + 1]");                 // load the first continuation byte
    emitter.instruction("cmp r10, 128");                                        // continuation bytes must be >= 0x80
    emitter.instruction("jl __rt_json_validate_string_utf8_x");                 // malformed UTF-8 continuation byte
    emitter.instruction("cmp r10, 191");                                        // continuation bytes must be <= 0xBF
    emitter.instruction("jg __rt_json_validate_string_utf8_x");                 // malformed UTF-8 continuation byte
    emitter.instruction("movzx r10, BYTE PTR [rax + rcx + 2]");                 // load the second continuation byte
    emitter.instruction("cmp r10, 128");                                        // continuation bytes must be >= 0x80
    emitter.instruction("jl __rt_json_validate_string_utf8_x");                 // malformed UTF-8 continuation byte
    emitter.instruction("cmp r10, 191");                                        // continuation bytes must be <= 0xBF
    emitter.instruction("jg __rt_json_validate_string_utf8_x");                 // malformed UTF-8 continuation byte
    emitter.instruction("add rcx, 3");                                          // consume the three-byte UTF-8 sequence
    emitter.instruction("jmp __rt_json_validate_string_loop_x");                // continue after the UTF-8 sequence

    emitter.label("__rt_json_validate_string_utf8_4_x");
    emitter.instruction("lea r9, [rcx + 3]");                                   // point at the final expected continuation byte
    emitter.instruction("cmp r9, rdx");                                         // ensure all continuation bytes are inside the string
    emitter.instruction("jge __rt_json_validate_string_utf8_x");                // truncated UTF-8 sequence
    emitter.instruction("movzx r10, BYTE PTR [rax + rcx + 1]");                 // load the first continuation byte
    emitter.instruction("cmp r10, 128");                                        // continuation bytes must be >= 0x80
    emitter.instruction("jl __rt_json_validate_string_utf8_x");                 // malformed UTF-8 continuation byte
    emitter.instruction("cmp r10, 191");                                        // continuation bytes must be <= 0xBF
    emitter.instruction("jg __rt_json_validate_string_utf8_x");                 // malformed UTF-8 continuation byte
    emitter.instruction("movzx r10, BYTE PTR [rax + rcx + 2]");                 // load the second continuation byte
    emitter.instruction("cmp r10, 128");                                        // continuation bytes must be >= 0x80
    emitter.instruction("jl __rt_json_validate_string_utf8_x");                 // malformed UTF-8 continuation byte
    emitter.instruction("cmp r10, 191");                                        // continuation bytes must be <= 0xBF
    emitter.instruction("jg __rt_json_validate_string_utf8_x");                 // malformed UTF-8 continuation byte
    emitter.instruction("movzx r10, BYTE PTR [rax + rcx + 3]");                 // load the third continuation byte
    emitter.instruction("cmp r10, 128");                                        // continuation bytes must be >= 0x80
    emitter.instruction("jl __rt_json_validate_string_utf8_x");                 // malformed UTF-8 continuation byte
    emitter.instruction("cmp r10, 191");                                        // continuation bytes must be <= 0xBF
    emitter.instruction("jg __rt_json_validate_string_utf8_x");                 // malformed UTF-8 continuation byte
    emitter.instruction("add rcx, 4");                                          // consume the four-byte UTF-8 sequence
    emitter.instruction("jmp __rt_json_validate_string_loop_x");                // continue after the UTF-8 sequence

    emitter.label("__rt_json_validate_string_close_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // load or prepare JSON validator state
    emitter.instruction("mov rax, 1");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_string_escape_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_string_syntax_x");              // branch on the current JSON validator condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 34");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_string_escape_simple_x");        // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 92");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_string_escape_simple_x");        // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 47");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_string_escape_simple_x");        // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 98");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_string_escape_simple_x");        // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 102");                                         // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_string_escape_simple_x");        // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 110");                                         // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_string_escape_simple_x");        // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 114");                                         // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_string_escape_simple_x");        // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 116");                                         // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_string_escape_simple_x");        // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 117");                                         // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_string_escape_unicode_x");       // branch on the current JSON validator condition
    emitter.instruction("jmp __rt_json_validate_string_syntax_x");              // continue in the JSON validator control path

    emitter.label("__rt_json_validate_string_escape_simple_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.instruction("jmp __rt_json_validate_string_loop_x");                // continue in the JSON validator control path

    emitter.label("__rt_json_validate_string_escape_unicode_x");
    emitter.instruction("add rcx, 1");                                          // consume the 'u'
    emitter.instruction("xor r10, r10");                                        // codepoint accumulator (16-bit)
    emitter.instruction("mov r9, 4");                                           // remaining hex-digit count
    emit_uhex_loop_x(emitter, "high", "__rt_json_validate_string_syntax_x");    // validate + accumulate 4 hex digits

    // -- surrogate-pair validation (mirrors ARM64) --
    emitter.instruction("cmp r10, 0xD800");                                     // codepoint < 0xD800?
    emitter.instruction("jl __rt_json_validate_string_loop_x");                 // not a surrogate → resume content scan
    emitter.instruction("cmp r10, 0xDFFF");                                     // codepoint > 0xDFFF?
    emitter.instruction("jg __rt_json_validate_string_loop_x");                 // not a surrogate → resume content scan
    emitter.instruction("cmp r10, 0xDC00");                                     // is the codepoint a low surrogate?
    emitter.instruction("jge __rt_json_validate_string_utf16_x");               // lone low surrogate → JSON_ERROR_UTF16

    // High surrogate: require an immediately following `\u`.
    emitter.instruction("cmp rcx, rdx");                                        // any byte left?
    emitter.instruction("jge __rt_json_validate_string_utf16_x");               // truncated → UTF16 error
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // peek the byte after the high surrogate
    emitter.instruction("cmp r8, 92");                                          // backslash?
    emitter.instruction("jne __rt_json_validate_string_utf16_x");               // anything else → UTF16 error
    emitter.instruction("add rcx, 1");                                          // consume the backslash
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_string_utf16_x");               // branch on the current JSON validator condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 117");                                         // 'u'?
    emitter.instruction("jne __rt_json_validate_string_utf16_x");               // not a `\u` escape → UTF16 error
    emitter.instruction("add rcx, 1");                                          // consume the 'u'

    // Parse and accumulate the low surrogate's 4 hex digits.
    emitter.instruction("xor r10, r10");                                        // reset the accumulator for the second codepoint
    emitter.instruction("mov r9, 4");                                           // remaining hex-digit count
    emit_uhex_loop_x(emitter, "low", "__rt_json_validate_string_utf16_x");      // syntax errors in the second \u → UTF16 (PHP)

    // The second codepoint MUST be in the low-surrogate range.
    emitter.instruction("cmp r10, 0xDC00");                                     // is the second codepoint < 0xDC00?
    emitter.instruction("jl __rt_json_validate_string_utf16_x");                // not a low surrogate → UTF16 error
    emitter.instruction("cmp r10, 0xDFFF");                                     // is the second codepoint > 0xDFFF?
    emitter.instruction("jg __rt_json_validate_string_utf16_x");                // not a low surrogate → UTF16 error
    emitter.instruction("jmp __rt_json_validate_string_loop_x");                // valid surrogate pair → resume content scan

    emitter.label("__rt_json_validate_string_utf16_x");
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // commit the failure index for diagnostics
    emitter.instruction("add rax, rcx");                                        // convert validator index to an absolute source pointer
    emitter.instruction("call __rt_json_set_error_location");                   // store one-based line/column metadata for the JSON error
    emitter.instruction("mov rax, 10");                                         // JSON_ERROR_UTF16
    emitter.instruction("call __rt_json_throw_error");                          // record the error and throw on JSON_THROW_ON_ERROR
    emitter.instruction("mov rax, 0");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_string_ctrl_char_x");
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // commit the control-character failure index
    emitter.instruction("add rax, rcx");                                        // convert validator index to an absolute source pointer
    emitter.instruction("call __rt_json_set_error_location");                   // store one-based line/column metadata for the JSON error
    emitter.instruction("mov rax, 3");                                          // JSON_ERROR_CTRL_CHAR
    emitter.instruction("call __rt_json_throw_error");                          // record the control-character error and throw when requested
    emitter.instruction("mov rax, 0");                                          // report validation failure to the caller
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_string_utf8_x");
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // commit the malformed-UTF-8 failure index
    emitter.instruction("add rax, rcx");                                        // convert validator index to an absolute source pointer
    emitter.instruction("call __rt_json_set_error_location");                   // store one-based line/column metadata for the JSON error
    emitter.instruction("mov rax, 5");                                          // JSON_ERROR_UTF8
    emitter.instruction("call __rt_json_throw_error");                          // record the malformed-UTF-8 error and throw when requested
    emitter.instruction("mov rax, 0");                                          // report validation failure to the caller
    emitter.instruction("mov rsp, rbp");                                        // restore stack pointer
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_string_syntax_x");
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // load or prepare JSON validator state
    emitter.instruction("add rax, rcx");                                        // convert validator index to an absolute source pointer
    emitter.instruction("call __rt_json_set_error_location");                   // store one-based line/column metadata for the JSON error
    emitter.instruction("mov rax, 4");                                          // load or prepare JSON validator state
    emitter.instruction("call __rt_json_throw_error");                          // call the json throw error helper
    emitter.instruction("mov rax, 0");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
}

/// Emit a 4-hex-digit accumulator loop that walks the source slice and
/// builds up a 16-bit codepoint in `r10`. On entry: `r9 = 4`, `r10 = 0`,
/// `rcx` points at the first hex digit (already past `\u`),
/// `rdx = source length`, `rax = source pointer`. On exit (the
/// `__rt_json_validate_uhex_done_<suffix>_x` label) `r10` holds the
/// validated codepoint and `rcx` has advanced past the four digits.
fn emit_uhex_loop_x(emitter: &mut Emitter, suffix: &str, error_label: &str) {
    emitter.label(&format!("__rt_json_validate_uhex_loop_{suffix}_x"));
    emitter.instruction("cmp r9, 0");                                           // 4 digits consumed?
    emitter.instruction(&format!("je __rt_json_validate_uhex_done_{suffix}_x")); // exit loop with r10 = codepoint
    emitter.instruction("cmp rcx, rdx");                                        // bounds check
    emitter.instruction(&format!("jge {error_label}"));                         // branch on the current JSON validator condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 48");                                          // '0'?
    emitter.instruction(&format!("jl {error_label}"));                          // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 57");                                          // ..'9'?
    emitter.instruction(&format!("jle __rt_json_validate_uhex_dec_{suffix}_x")); // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 65");                                          // 'A'?
    emitter.instruction(&format!("jl {error_label}"));                          // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 70");                                          // ..'F'?
    emitter.instruction(&format!("jle __rt_json_validate_uhex_upper_{suffix}_x")); // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 97");                                          // 'a'?
    emitter.instruction(&format!("jl {error_label}"));                          // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 102");                                         // ..'f'?
    emitter.instruction(&format!("jg {error_label}"));                          // branch on the current JSON validator condition
    emitter.instruction("sub r8, 87");                                          // 'a'..'f' → 10..15
    emitter.instruction(&format!("jmp __rt_json_validate_uhex_acc_{suffix}_x")); // continue in the JSON validator control path
    emitter.label(&format!("__rt_json_validate_uhex_dec_{suffix}_x"));
    emitter.instruction("sub r8, 48");                                          // '0'..'9' → 0..9
    emitter.instruction(&format!("jmp __rt_json_validate_uhex_acc_{suffix}_x")); // continue in the JSON validator control path
    emitter.label(&format!("__rt_json_validate_uhex_upper_{suffix}_x"));
    emitter.instruction("sub r8, 55");                                          // 'A'..'F' → 10..15
    emitter.label(&format!("__rt_json_validate_uhex_acc_{suffix}_x"));
    emitter.instruction("shl r10, 4");                                          // shift accumulator nibble
    emitter.instruction("or r10, r8");                                          // OR in the digit value
    emitter.instruction("add rcx, 1");                                          // advance past the digit
    emitter.instruction("sub r9, 1");                                           // one fewer digit to scan
    emitter.instruction(&format!("jmp __rt_json_validate_uhex_loop_{suffix}_x")); // continue in the JSON validator control path
    emitter.label(&format!("__rt_json_validate_uhex_done_{suffix}_x"));
}

/// Emits `__rt_json_validate_number_x`: validates a RFC 8259 number token.
/// Handles an optional leading `-`, zero vs. non-zero integer, optional fractional
/// part, and optional exponent. Returns 1 on success; syntax error on malformed input.
fn emit_number_parser_x(emitter: &mut Emitter) {
    emitter.label("__rt_json_validate_number_x");
    emitter.instruction("push rbp");                                            // preserve or restore JSON validator scratch state
    emitter.instruction("mov rbp, rsp");                                        // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_validate_idx", 0);      // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_validate_len", 0);      // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rax", "_json_validate_ptr", 0);      // load or prepare JSON validator state

    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_number_syntax_x");              // branch on the current JSON validator condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 45");                                          // '-'?
    emitter.instruction("jne __rt_json_validate_number_int_start_x");           // branch on the current JSON validator condition
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_number_syntax_x");              // branch on the current JSON validator condition

    emitter.label("__rt_json_validate_number_int_start_x");
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 48");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_number_zero_x");                 // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 49");                                          // check the current JSON validator condition
    emitter.instruction("jl __rt_json_validate_number_syntax_x");               // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 57");                                          // check the current JSON validator condition
    emitter.instruction("jg __rt_json_validate_number_syntax_x");               // branch on the current JSON validator condition
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.label("__rt_json_validate_number_int_loop_x");
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_number_done_x");                // branch on the current JSON validator condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("mov r9, r8");                                          // load or prepare JSON validator state
    emitter.instruction("sub r9, 48");                                          // update the JSON validator cursor or counter
    emitter.instruction("cmp r9, 9");                                           // check the current JSON validator condition
    emitter.instruction("ja __rt_json_validate_number_after_int_x");            // branch on the current JSON validator condition
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.instruction("jmp __rt_json_validate_number_int_loop_x");            // continue in the JSON validator control path

    emitter.label("__rt_json_validate_number_zero_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_number_done_x");                // branch on the current JSON validator condition

    emitter.label("__rt_json_validate_number_after_int_x");
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 46");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_number_frac_x");                 // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 101");                                         // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_number_exp_x");                  // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 69");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_number_exp_x");                  // branch on the current JSON validator condition
    emitter.instruction("jmp __rt_json_validate_number_done_x");                // continue in the JSON validator control path

    emitter.label("__rt_json_validate_number_frac_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_number_syntax_x");              // branch on the current JSON validator condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("mov r9, r8");                                          // load or prepare JSON validator state
    emitter.instruction("sub r9, 48");                                          // update the JSON validator cursor or counter
    emitter.instruction("cmp r9, 9");                                           // check the current JSON validator condition
    emitter.instruction("ja __rt_json_validate_number_syntax_x");               // branch on the current JSON validator condition
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.label("__rt_json_validate_number_frac_loop_x");
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_number_done_x");                // branch on the current JSON validator condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("mov r9, r8");                                          // load or prepare JSON validator state
    emitter.instruction("sub r9, 48");                                          // update the JSON validator cursor or counter
    emitter.instruction("cmp r9, 9");                                           // check the current JSON validator condition
    emitter.instruction("ja __rt_json_validate_number_after_frac_x");           // branch on the current JSON validator condition
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.instruction("jmp __rt_json_validate_number_frac_loop_x");           // continue in the JSON validator control path

    emitter.label("__rt_json_validate_number_after_frac_x");
    emitter.instruction("cmp r8, 101");                                         // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_number_exp_x");                  // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 69");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_number_exp_x");                  // branch on the current JSON validator condition
    emitter.instruction("jmp __rt_json_validate_number_done_x");                // continue in the JSON validator control path

    emitter.label("__rt_json_validate_number_exp_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_number_syntax_x");              // branch on the current JSON validator condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 43");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_number_exp_sign_consume_x");     // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 45");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_number_exp_sign_consume_x");     // branch on the current JSON validator condition
    emitter.instruction("jmp __rt_json_validate_number_exp_first_x");           // continue in the JSON validator control path
    emitter.label("__rt_json_validate_number_exp_sign_consume_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_number_syntax_x");              // branch on the current JSON validator condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.label("__rt_json_validate_number_exp_first_x");
    emitter.instruction("mov r9, r8");                                          // load or prepare JSON validator state
    emitter.instruction("sub r9, 48");                                          // update the JSON validator cursor or counter
    emitter.instruction("cmp r9, 9");                                           // check the current JSON validator condition
    emitter.instruction("ja __rt_json_validate_number_syntax_x");               // branch on the current JSON validator condition
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.label("__rt_json_validate_number_exp_loop_x");
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_number_done_x");                // branch on the current JSON validator condition
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("mov r9, r8");                                          // load or prepare JSON validator state
    emitter.instruction("sub r9, 48");                                          // update the JSON validator cursor or counter
    emitter.instruction("cmp r9, 9");                                           // check the current JSON validator condition
    emitter.instruction("ja __rt_json_validate_number_done_x");                 // branch on the current JSON validator condition
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    emitter.instruction("jmp __rt_json_validate_number_exp_loop_x");            // continue in the JSON validator control path

    emitter.label("__rt_json_validate_number_done_x");
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // load or prepare JSON validator state
    emitter.instruction("mov rax, 1");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_number_syntax_x");
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // load or prepare JSON validator state
    emitter.instruction("add rax, rcx");                                        // convert validator index to an absolute source pointer
    emitter.instruction("call __rt_json_set_error_location");                   // store one-based line/column metadata for the JSON error
    emitter.instruction("mov rax, 4");                                          // load or prepare JSON validator state
    emitter.instruction("call __rt_json_throw_error");                          // call the json throw error helper
    emitter.instruction("mov rax, 0");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
}

/// Emits `__rt_json_validate_array_x`: validates a JSON array.
/// Increments `_json_active_depth` and checks against `_json_depth_limit`.
/// Parses elements recursively via `__rt_json_validate_value_x`, separated by `,`.
/// Returns 1 on success; propagates 0 on element failure; syntax error on malformed.
fn emit_array_parser_x(emitter: &mut Emitter) {
    emitter.label("__rt_json_validate_array_x");
    emitter.instruction("push rbp");                                            // preserve or restore JSON validator scratch state
    emitter.instruction("mov rbp, rsp");                                        // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_active_depth", 0);      // load or prepare JSON validator state
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_active_depth", 0);     // load or prepare JSON validator state
    abi::emit_cmp_reg_to_symbol(emitter, "rcx", "_json_depth_limit");           // check the current JSON validator condition
    emitter.instruction("jg __rt_json_validate_array_depth_x");                 // branch on the current JSON validator condition

    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_validate_idx", 0);      // load or prepare JSON validator state
    emitter.instruction("add rcx, 1");                                          // consume '['
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // load or prepare JSON validator state
    emitter.instruction("call __rt_json_validate_skip_ws_x");                   // call the json validate skip ws x helper

    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_validate_idx", 0);      // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_validate_len", 0);      // load or prepare JSON validator state
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_array_syntax_x");               // branch on the current JSON validator condition
    abi::emit_load_symbol_to_reg(emitter, "rax", "_json_validate_ptr", 0);      // load or prepare JSON validator state
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 93");                                          // ']'?
    emitter.instruction("je __rt_json_validate_array_close_x");                 // branch on the current JSON validator condition

    emitter.label("__rt_json_validate_array_elem_x");
    emitter.instruction("call __rt_json_validate_value_x");                     // call the json validate value x helper
    emitter.instruction("test rax, rax");                                       // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_array_propagate_x");             // branch on the current JSON validator condition
    emitter.instruction("call __rt_json_validate_skip_ws_x");                   // call the json validate skip ws x helper
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_validate_idx", 0);      // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_validate_len", 0);      // load or prepare JSON validator state
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_array_syntax_x");               // branch on the current JSON validator condition
    abi::emit_load_symbol_to_reg(emitter, "rax", "_json_validate_ptr", 0);      // load or prepare JSON validator state
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 93");                                          // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_array_close_x");                 // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 44");                                          // check the current JSON validator condition
    emitter.instruction("jne __rt_json_validate_array_syntax_x");               // branch on the current JSON validator condition
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // load or prepare JSON validator state
    emitter.instruction("call __rt_json_validate_skip_ws_x");                   // call the json validate skip ws x helper
    emitter.instruction("jmp __rt_json_validate_array_elem_x");                 // continue in the JSON validator control path

    emitter.label("__rt_json_validate_array_close_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_active_depth", 0);      // load or prepare JSON validator state
    emitter.instruction("sub rcx, 1");                                          // update the JSON validator cursor or counter
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_active_depth", 0);     // load or prepare JSON validator state
    emitter.instruction("mov rax, 1");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_array_propagate_x");
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_array_syntax_x");
    emitter.instruction("mov rax, 4");                                          // load or prepare JSON validator state
    emitter.instruction("call __rt_json_throw_error");                          // call the json throw error helper
    emitter.instruction("mov rax, 0");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_array_depth_x");
    emitter.instruction("mov rax, 1");                                          // JSON_ERROR_DEPTH
    emitter.instruction("call __rt_json_throw_error");                          // call the json throw error helper
    emitter.instruction("mov rax, 0");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
}

/// Emits `__rt_json_validate_object_x`: validates a JSON object.
/// Increments `_json_active_depth` and checks against `_json_depth_limit`.
/// Parses `"key": value` pairs recursively via `__rt_json_validate_string_x`
/// and `__rt_json_validate_value_x`. Returns 1 on success; propagates 0 on
/// failure; syntax error on malformed input.
fn emit_object_parser_x(emitter: &mut Emitter) {
    emitter.label("__rt_json_validate_object_x");
    emitter.instruction("push rbp");                                            // preserve or restore JSON validator scratch state
    emitter.instruction("mov rbp, rsp");                                        // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_active_depth", 0);      // load or prepare JSON validator state
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_active_depth", 0);     // load or prepare JSON validator state
    abi::emit_cmp_reg_to_symbol(emitter, "rcx", "_json_depth_limit");           // check the current JSON validator condition
    emitter.instruction("jg __rt_json_validate_object_depth_x");                // branch on the current JSON validator condition

    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_validate_idx", 0);      // load or prepare JSON validator state
    emitter.instruction("add rcx, 1");                                          // consume '{'
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // load or prepare JSON validator state
    emitter.instruction("call __rt_json_validate_skip_ws_x");                   // call the json validate skip ws x helper

    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_validate_idx", 0);      // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_validate_len", 0);      // load or prepare JSON validator state
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_object_syntax_x");              // branch on the current JSON validator condition
    abi::emit_load_symbol_to_reg(emitter, "rax", "_json_validate_ptr", 0);      // load or prepare JSON validator state
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 125");                                         // '}'?
    emitter.instruction("je __rt_json_validate_object_close_x");                // branch on the current JSON validator condition

    emitter.label("__rt_json_validate_object_pair_x");
    emitter.instruction("call __rt_json_validate_string_x");                    // call the json validate string x helper
    emitter.instruction("test rax, rax");                                       // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_object_propagate_x");            // branch on the current JSON validator condition
    emitter.instruction("call __rt_json_validate_skip_ws_x");                   // call the json validate skip ws x helper
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_validate_idx", 0);      // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_validate_len", 0);      // load or prepare JSON validator state
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_object_syntax_x");              // branch on the current JSON validator condition
    abi::emit_load_symbol_to_reg(emitter, "rax", "_json_validate_ptr", 0);      // load or prepare JSON validator state
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 58");                                          // check the current JSON validator condition
    emitter.instruction("jne __rt_json_validate_object_syntax_x");              // branch on the current JSON validator condition
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // load or prepare JSON validator state
    emitter.instruction("call __rt_json_validate_skip_ws_x");                   // call the json validate skip ws x helper
    emitter.instruction("call __rt_json_validate_value_x");                     // call the json validate value x helper
    emitter.instruction("test rax, rax");                                       // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_object_propagate_x");            // branch on the current JSON validator condition
    emitter.instruction("call __rt_json_validate_skip_ws_x");                   // call the json validate skip ws x helper
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_validate_idx", 0);      // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_json_validate_len", 0);      // load or prepare JSON validator state
    emitter.instruction("cmp rcx, rdx");                                        // check the current JSON validator condition
    emitter.instruction("jge __rt_json_validate_object_syntax_x");              // branch on the current JSON validator condition
    abi::emit_load_symbol_to_reg(emitter, "rax", "_json_validate_ptr", 0);      // load or prepare JSON validator state
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load or prepare JSON validator state
    emitter.instruction("cmp r8, 125");                                         // check the current JSON validator condition
    emitter.instruction("je __rt_json_validate_object_close_x");                // branch on the current JSON validator condition
    emitter.instruction("cmp r8, 44");                                          // check the current JSON validator condition
    emitter.instruction("jne __rt_json_validate_object_syntax_x");              // branch on the current JSON validator condition
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // load or prepare JSON validator state
    emitter.instruction("call __rt_json_validate_skip_ws_x");                   // call the json validate skip ws x helper
    emitter.instruction("jmp __rt_json_validate_object_pair_x");                // continue in the JSON validator control path

    emitter.label("__rt_json_validate_object_close_x");
    emitter.instruction("add rcx, 1");                                          // update the JSON validator cursor or counter
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_validate_idx", 0);     // load or prepare JSON validator state
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_json_active_depth", 0);      // load or prepare JSON validator state
    emitter.instruction("sub rcx, 1");                                          // update the JSON validator cursor or counter
    abi::emit_store_reg_to_symbol(emitter, "rcx", "_json_active_depth", 0);     // load or prepare JSON validator state
    emitter.instruction("mov rax, 1");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_object_propagate_x");
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_object_syntax_x");
    emitter.instruction("mov rax, 4");                                          // load or prepare JSON validator state
    emitter.instruction("call __rt_json_throw_error");                          // call the json throw error helper
    emitter.instruction("mov rax, 0");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper

    emitter.label("__rt_json_validate_object_depth_x");
    emitter.instruction("mov rax, 1");                                          // JSON_ERROR_DEPTH
    emitter.instruction("call __rt_json_throw_error");                          // call the json throw error helper
    emitter.instruction("mov rax, 0");                                          // load or prepare JSON validator state
    emitter.instruction("mov rsp, rbp");                                        // load or prepare JSON validator state
    emitter.instruction("pop rbp");                                             // preserve or restore JSON validator scratch state
    emitter.instruction("ret");                                                 // return from the JSON validator helper
}
