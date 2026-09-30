//! Purpose:
//! Emits the helpers that record php's `Stack trace:` frames, and render one argument the way php
//! renders it inside a frame.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::exceptions`.
//! - `crate::codegen::lower_inst::exceptions`, which opens a frame for the builtin whose call is
//!   raising, then renders that call's arguments, then closes it.
//!
//! Key details:
//! - php captures a trace when a Throwable is CONSTRUCTED, not when it is thrown — measured:
//!   `$e = new RuntimeException("x"); throw $e;` reports `#0 {main}` and the `new`'s line. So the
//!   buffer is reset per construction and read at report time, and an exception that is caught and
//!   rethrown keeps the trace it was born with.
//! - The frame count is kept beside the text because php numbers the `{main}` sentinel after the
//!   real frames: no frame gives `#0 {main}`, one gives `#1 {main}`.
//! - Argument rendering was MEASURED rather than derived, one value shape at a time, from a php
//!   frame's own `getTraceAsString()`:
//!
//!   ```text
//!   42 · -7 · 3.5 · 'short' · 'a string that i...' · true · false · NULL · Array ·
//!   Object(stdClass) · Resource id #5
//!   ```
//!
//!   The string rule is the one worth stating: fifteen bytes, then a literal `...`, all INSIDE the
//!   quotes. A sixteen-byte string is not truncated silently to fifteen — the marker says so.
//! - The buffer is fixed and its capacity is a hard stop: appends past it are dropped rather than
//!   growing the allocation, because a trace is a diagnostic and must not be able to fail an
//!   allocation while an exception is already being constructed.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Bytes reserved for the rendered trace text.
pub(crate) const TRACE_BUF_BYTES: usize = 8192;

/// The longest string php shows in full inside a frame argument.
const TRACE_STR_LIMIT: usize = 15;

/// The literal pieces a rendered frame is assembled from.
///
/// Each is paired with the symbol the data section publishes it under, and every length reaches
/// the assembly through `.len()`: the first draft typed `"Resource id #"` as 15, and it is 13.
pub(crate) const TRACE_LITERALS: &[(&str, &str)] = &[
    ("_rt_trace_hash", "#"),
    ("_rt_trace_space", " "),
    ("_rt_trace_lparen", "("),
    ("_rt_trace_rparen", ")"),
    ("_rt_trace_rparen_colon", "): "),
    ("_rt_trace_rparen_nl", ")\n"),
    ("_rt_trace_comma", ", "),
    ("_rt_trace_quote", "'"),
    ("_rt_trace_ellipsis", "..."),
    ("_rt_trace_true", "true"),
    ("_rt_trace_false", "false"),
    ("_rt_trace_null", "NULL"),
    ("_rt_trace_array", "Array"),
    ("_rt_trace_object", "Object("),
    ("_rt_trace_resource", "Resource id #"),
    ("_rt_trace_header", "Stack trace:\n"),
    ("_rt_trace_main", " {main}\n"),
    ("_rt_trace_thrown_in", "  thrown in "),
    ("_rt_trace_on_line", " on line "),
    ("_rt_trace_nl", "\n"),
];

/// Returns the byte length of one published trace literal.
pub(crate) fn trace_literal_len(symbol: &str) -> usize {
    TRACE_LITERALS
        .iter()
        .find(|(name, _)| *name == symbol)
        .unwrap_or_else(|| panic!("{symbol} must be a published trace literal"))
        .1
        .len()
}

/// Emits every trace-buffer helper.
pub fn emit_trace_buffer(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_aarch64(emitter);
            emit_write_block_aarch64(emitter);
        }
        Arch::X86_64 => {
            emit_x86_64(emitter);
            emit_write_block_x86_64(emitter);
        }
    }
}

/// Emits the AArch64 helpers.
fn emit_aarch64(emitter: &mut Emitter) {
    // ------------------------------------------------------------- append ---
    // Input: x1 = bytes, x2 = length. Clobbers x9-x14.
    emitter.blank();
    emitter.comment("--- runtime: trace_append ---");
    emitter.label_global("__rt_trace_append");
    abi::emit_symbol_address(emitter, "x9", "_rt_trace_len");
    emitter.instruction("ldr x10, [x9]");                                       // bytes already written
    abi::emit_symbol_address(emitter, "x11", "_rt_trace_buf");
    emitter.instruction("mov x12, #0");                                         // index into the incoming piece
    emitter.label("__rt_trace_append_loop");
    emitter.instruction("cmp x12, x2");
    emitter.instruction("b.ge __rt_trace_append_done");
    emitter.instruction("add x13, x10, x12");                                   // where this byte would land
    emitter.instruction(&format!("mov x14, #{}", TRACE_BUF_BYTES - 1));
    emitter.instruction("cmp x13, x14");
    emitter.instruction("b.ge __rt_trace_append_done");                         // a full buffer drops the rest rather than growing
    emitter.instruction("ldrb w14, [x1, x12]");
    emitter.instruction("strb w14, [x11, x13]");
    emitter.instruction("add x12, x12, #1");
    emitter.instruction("b __rt_trace_append_loop");
    emitter.label("__rt_trace_append_done");
    emitter.instruction("add x10, x10, x12");
    emitter.instruction(&format!("mov x14, #{}", TRACE_BUF_BYTES - 1));
    emitter.instruction("cmp x10, x14");
    emitter.instruction("csel x10, x14, x10, gt");                              // never report more than the buffer holds
    abi::emit_symbol_address(emitter, "x9", "_rt_trace_len");
    emitter.instruction("str x10, [x9]");
    emitter.instruction("ret");

    // -------------------------------------------------------------- reset ---
    emitter.blank();
    emitter.comment("--- runtime: trace_reset ---");
    emitter.label_global("__rt_trace_reset");
    abi::emit_symbol_address(emitter, "x9", "_rt_trace_len");
    emitter.instruction("str xzr, [x9]");
    abi::emit_symbol_address(emitter, "x9", "_rt_trace_count");
    emitter.instruction("str xzr, [x9]");
    emitter.instruction("ret");

    // --------------------------------------------------------- frame open ---
    // Input: x0 = call-site line, x1 = function name pointer, x2 = its length.
    emitter.blank();
    emitter.comment("--- runtime: trace_frame_open ---");
    emitter.label_global("__rt_trace_frame_open");
    emitter.instruction("sub sp, sp, #48");
    emitter.instruction("stp x29, x30, [sp, #32]");
    emitter.instruction("add x29, sp, #32");
    emitter.instruction("stp x1, x2, [sp, #0]");                                // the name, appended after the number
    emitter.instruction("str x0, [sp, #16]");                                   // the line

    abi::emit_symbol_address(emitter, "x1", "_rt_trace_hash");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_hash")));                                          // "#"
    emitter.instruction("bl __rt_trace_append");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_rt_trace_count", 0);
    emitter.instruction("bl __rt_itoa");                                        // the frame number, in x1/x2
    emitter.instruction("bl __rt_trace_append");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_space");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_space")));                                          // " "
    emitter.instruction("bl __rt_trace_append");
    abi::emit_symbol_address(emitter, "x1", "_script_source_file");
    abi::emit_load_symbol_to_reg(emitter, "x2", "_script_source_file_len", 0);
    emitter.instruction("bl __rt_trace_append");                                // php names the file in every frame
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_lparen");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_lparen")));                                          // "("
    emitter.instruction("bl __rt_trace_append");
    emitter.instruction("ldr x0, [sp, #16]");
    emitter.instruction("bl __rt_itoa");                                        // the call-site line
    emitter.instruction("bl __rt_trace_append");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_rparen_colon");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_rparen_colon")));                                          // "): "
    emitter.instruction("bl __rt_trace_append");
    emitter.instruction("ldp x1, x2, [sp, #0]");
    emitter.instruction("bl __rt_trace_append");                                // the function name
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_lparen");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_lparen")));                                          // "("
    emitter.instruction("bl __rt_trace_append");
    abi::emit_symbol_address(emitter, "x9", "_rt_trace_argc");
    emitter.instruction("str xzr, [x9]");                                       // this frame has printed no argument yet

    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");

    // -------------------------------------------------------- frame close ---
    emitter.blank();
    emitter.comment("--- runtime: trace_frame_close ---");
    emitter.label_global("__rt_trace_frame_close");
    emitter.instruction("stp x29, x30, [sp, #-32]!");
    emitter.instruction("add x29, sp, #0");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_rparen_nl");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_rparen_nl")));                                          // ")\n"
    emitter.instruction("bl __rt_trace_append");
    abi::emit_symbol_address(emitter, "x9", "_rt_trace_count");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("str x10, [x9]");                                       // the next frame numbers itself after this one
    emitter.instruction("ldp x29, x30, [sp], #32");
    emitter.instruction("ret");

    // ----------------------------------------------------------------- arg ---
    // Input: x0 = runtime tag, x1 = payload low word, x2 = payload high word.
    emitter.blank();
    emitter.comment("--- runtime: trace_arg ---");
    emitter.label_global("__rt_trace_arg");
    emitter.instruction("sub sp, sp, #48");
    emitter.instruction("stp x29, x30, [sp, #32]");
    emitter.instruction("add x29, sp, #32");
    emitter.instruction("stp x1, x2, [sp, #0]");                                // the payload, across the separator append
    emitter.instruction("str x0, [sp, #16]");                                   // the tag

    abi::emit_symbol_address(emitter, "x9", "_rt_trace_argc");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("cbz x10, __rt_trace_arg_first");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_comma");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_comma")));                                          // ", "
    emitter.instruction("bl __rt_trace_append");
    emitter.label("__rt_trace_arg_first");
    abi::emit_symbol_address(emitter, "x9", "_rt_trace_argc");
    emitter.instruction("ldr x10, [x9]");
    emitter.instruction("add x10, x10, #1");
    emitter.instruction("str x10, [x9]");

    emitter.instruction("ldr x9, [sp, #16]");                                   // the tag decides the rendering
    emitter.instruction("cmp x9, #1");
    emitter.instruction("b.eq __rt_trace_arg_str");
    emitter.instruction("cmp x9, #2");
    emitter.instruction("b.eq __rt_trace_arg_float");
    emitter.instruction("cmp x9, #3");
    emitter.instruction("b.eq __rt_trace_arg_bool");
    emitter.instruction("cmp x9, #8");
    emitter.instruction("b.eq __rt_trace_arg_null");
    emitter.instruction("cmp x9, #9");
    emitter.instruction("b.eq __rt_trace_arg_resource");
    emitter.instruction("cmp x9, #4");
    emitter.instruction("b.eq __rt_trace_arg_array");
    emitter.instruction("cmp x9, #5");
    emitter.instruction("b.eq __rt_trace_arg_array");
    emitter.instruction("cmp x9, #6");
    emitter.instruction("b.eq __rt_trace_arg_object");
    // Tag 0 and anything else print as an integer, which is what php shows for one.
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("bl __rt_itoa");
    emitter.instruction("bl __rt_trace_append");
    emitter.instruction("b __rt_trace_arg_done");

    emitter.label("__rt_trace_arg_bool");
    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction("cbz x9, __rt_trace_arg_false");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_true");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_true")));                                          // "true"
    emitter.instruction("bl __rt_trace_append");
    emitter.instruction("b __rt_trace_arg_done");
    emitter.label("__rt_trace_arg_false");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_false");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_false")));                                          // "false"
    emitter.instruction("bl __rt_trace_append");
    emitter.instruction("b __rt_trace_arg_done");

    emitter.label("__rt_trace_arg_null");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_null");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_null")));                                          // "NULL", uppercase as php prints it
    emitter.instruction("bl __rt_trace_append");
    emitter.instruction("b __rt_trace_arg_done");

    emitter.label("__rt_trace_arg_array");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_array");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_array")));                                          // "Array", never the contents
    emitter.instruction("bl __rt_trace_append");
    emitter.instruction("b __rt_trace_arg_done");

    emitter.label("__rt_trace_arg_object");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_object");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_object")));                                          // "Object("
    emitter.instruction("bl __rt_trace_append");
    emitter.instruction("ldr x0, [sp, #0]");
    // The same dense table `get_class()` reads: `[obj]` is the class id, `_class_name_count`
    // bounds it, and each 16-byte row of `_class_name_entries` is a (pointer, length) pair.
    emitter.instruction("cbz x0, __rt_trace_arg_object_close");                 // a null object has no name to print
    emitter.instruction("ldr x9, [x0]");                                        // the object's concrete runtime class id
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");
    emitter.instruction("ldr x10, [x10]");
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.hs __rt_trace_arg_object_close");                    // an id outside the table names nothing
    abi::emit_symbol_address(emitter, "x11", "_class_name_entries");
    emitter.instruction("lsl x12, x9, #4");                                     // rows are 16 bytes
    emitter.instruction("add x11, x11, x12");
    emitter.instruction("ldr x1, [x11]");
    emitter.instruction("ldr x2, [x11, #8]");
    emitter.instruction("bl __rt_trace_append");
    emitter.label("__rt_trace_arg_object_close");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_rparen");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_rparen")));                                          // ")"
    emitter.instruction("bl __rt_trace_append");
    emitter.instruction("b __rt_trace_arg_done");

    emitter.label("__rt_trace_arg_resource");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_resource");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_resource")));                                         // "Resource id #"
    emitter.instruction("bl __rt_trace_append");
    emitter.instruction("ldr x0, [sp, #0]");
    emitter.instruction("bl __rt_resource_id_of");                              // the id php shows, not the handle
    emitter.instruction("bl __rt_itoa");
    emitter.instruction("bl __rt_trace_append");
    emitter.instruction("b __rt_trace_arg_done");

    emitter.label("__rt_trace_arg_float");
    emitter.instruction("ldr x9, [sp, #0]");
    emitter.instruction("fmov d0, x9");                                         // __rt_ftoa_repr renders from d0
    emitter.instruction("bl __rt_ftoa_repr");
    emitter.instruction("bl __rt_trace_append");
    emitter.instruction("b __rt_trace_arg_done");

    emitter.label("__rt_trace_arg_str");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_quote");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_quote")));                                          // "'"
    emitter.instruction("bl __rt_trace_append");
    emitter.instruction("ldp x1, x2, [sp, #0]");
    emitter.instruction(&format!("mov x9, #{}", TRACE_STR_LIMIT));
    emitter.instruction("cmp x2, x9");
    emitter.instruction("b.le __rt_trace_arg_str_whole");
    emitter.instruction("mov x2, x9");                                          // php shows fifteen bytes...
    emitter.instruction("bl __rt_trace_append");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_ellipsis");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_ellipsis")));                                          // ...then says so, INSIDE the quotes
    emitter.instruction("bl __rt_trace_append");
    emitter.instruction("b __rt_trace_arg_str_close");
    emitter.label("__rt_trace_arg_str_whole");
    emitter.instruction("bl __rt_trace_append");
    emitter.label("__rt_trace_arg_str_close");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_quote");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_quote")));                                          // "'"
    emitter.instruction("bl __rt_trace_append");

    emitter.label("__rt_trace_arg_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");
}

/// Emits the x86_64 System V helpers.
fn emit_x86_64(emitter: &mut Emitter) {
    // ------------------------------------------------------------- append ---
    // Input: rdi = bytes, rsi = length.
    emitter.blank();
    emitter.comment("--- runtime: trace_append ---");
    emitter.label_global("__rt_trace_append");
    abi::emit_symbol_address(emitter, "r9", "_rt_trace_len");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // bytes already written
    abi::emit_symbol_address(emitter, "r11", "_rt_trace_buf");
    emitter.instruction("xor ecx, ecx");                                        // index into the incoming piece
    emitter.label("__rt_trace_append_loop_x");
    emitter.instruction("cmp rcx, rsi");
    emitter.instruction("jge __rt_trace_append_done_x");
    emitter.instruction("mov rax, r10");
    emitter.instruction("add rax, rcx");                                        // where this byte would land
    emitter.instruction(&format!("cmp rax, {}", TRACE_BUF_BYTES - 1));
    emitter.instruction("jge __rt_trace_append_done_x");                        // a full buffer drops the rest rather than growing
    emitter.instruction("mov dl, BYTE PTR [rdi + rcx]");
    emitter.instruction("mov BYTE PTR [r11 + rax], dl");
    emitter.instruction("add rcx, 1");
    emitter.instruction("jmp __rt_trace_append_loop_x");
    emitter.label("__rt_trace_append_done_x");
    emitter.instruction("add r10, rcx");
    emitter.instruction(&format!("cmp r10, {}", TRACE_BUF_BYTES - 1));
    emitter.instruction("jle __rt_trace_append_store_x");
    emitter.instruction(&format!("mov r10, {}", TRACE_BUF_BYTES - 1));          // never report more than the buffer holds
    emitter.label("__rt_trace_append_store_x");
    abi::emit_symbol_address(emitter, "r9", "_rt_trace_len");
    emitter.instruction("mov QWORD PTR [r9], r10");
    emitter.instruction("ret");

    // -------------------------------------------------------------- reset ---
    emitter.blank();
    emitter.comment("--- runtime: trace_reset ---");
    emitter.label_global("__rt_trace_reset");
    abi::emit_symbol_address(emitter, "r9", "_rt_trace_len");
    emitter.instruction("mov QWORD PTR [r9], 0");
    abi::emit_symbol_address(emitter, "r9", "_rt_trace_count");
    emitter.instruction("mov QWORD PTR [r9], 0");
    emitter.instruction("ret");

    // --------------------------------------------------------- frame open ---
    // Input: rdi = call-site line, rsi = name pointer, rdx = its length.
    emitter.blank();
    emitter.comment("--- runtime: trace_frame_open ---");
    emitter.label_global("__rt_trace_frame_open");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 32");
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // the name, appended after the number
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // the line

    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_hash");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_hash")));                                          // "#"
    emitter.instruction("call __rt_trace_append");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_rt_trace_count", 0);
    emitter.instruction("call __rt_itoa");                                      // the frame number, in rax/rdx
    emitter.instruction("mov rdi, rax");
    emitter.instruction("mov rsi, rdx");
    emitter.instruction("call __rt_trace_append");
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_space");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_space")));                                          // " "
    emitter.instruction("call __rt_trace_append");
    abi::emit_symbol_address(emitter, "rdi", "_script_source_file");
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_script_source_file_len", 0);
    emitter.instruction("call __rt_trace_append");                              // php names the file in every frame
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_lparen");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_lparen")));                                          // "("
    emitter.instruction("call __rt_trace_append");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");
    emitter.instruction("call __rt_itoa");                                      // the call-site line
    emitter.instruction("mov rdi, rax");
    emitter.instruction("mov rsi, rdx");
    emitter.instruction("call __rt_trace_append");
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_rparen_colon");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_rparen_colon")));                                          // "): "
    emitter.instruction("call __rt_trace_append");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction("call __rt_trace_append");                              // the function name
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_lparen");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_lparen")));                                          // "("
    emitter.instruction("call __rt_trace_append");
    abi::emit_symbol_address(emitter, "r9", "_rt_trace_argc");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // this frame has printed no argument yet

    emitter.instruction("add rsp, 32");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");

    // -------------------------------------------------------- frame close ---
    emitter.blank();
    emitter.comment("--- runtime: trace_frame_close ---");
    emitter.label_global("__rt_trace_frame_close");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_rparen_nl");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_rparen_nl")));                                          // ")\n"
    emitter.instruction("call __rt_trace_append");
    abi::emit_symbol_address(emitter, "r9", "_rt_trace_count");
    emitter.instruction("mov r10, QWORD PTR [r9]");
    emitter.instruction("add r10, 1");
    emitter.instruction("mov QWORD PTR [r9], r10");                             // the next frame numbers itself after this one
    emitter.instruction("pop rbp");
    emitter.instruction("ret");

    // ----------------------------------------------------------------- arg ---
    // Input: rdi = runtime tag, rsi = payload low word, rdx = payload high word.
    emitter.blank();
    emitter.comment("--- runtime: trace_arg ---");
    emitter.label_global("__rt_trace_arg");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 32");
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // the payload, across the separator append
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // the tag

    abi::emit_symbol_address(emitter, "r9", "_rt_trace_argc");
    emitter.instruction("cmp QWORD PTR [r9], 0");
    emitter.instruction("je __rt_trace_arg_first_x");
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_comma");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_comma")));                                          // ", "
    emitter.instruction("call __rt_trace_append");
    emitter.label("__rt_trace_arg_first_x");
    abi::emit_symbol_address(emitter, "r9", "_rt_trace_argc");
    emitter.instruction("mov r10, QWORD PTR [r9]");
    emitter.instruction("add r10, 1");
    emitter.instruction("mov QWORD PTR [r9], r10");

    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // the tag decides the rendering
    emitter.instruction("cmp r10, 1");
    emitter.instruction("je __rt_trace_arg_str_x");
    emitter.instruction("cmp r10, 2");
    emitter.instruction("je __rt_trace_arg_float_x");
    emitter.instruction("cmp r10, 3");
    emitter.instruction("je __rt_trace_arg_bool_x");
    emitter.instruction("cmp r10, 8");
    emitter.instruction("je __rt_trace_arg_null_x");
    emitter.instruction("cmp r10, 9");
    emitter.instruction("je __rt_trace_arg_resource_x");
    emitter.instruction("cmp r10, 4");
    emitter.instruction("je __rt_trace_arg_array_x");
    emitter.instruction("cmp r10, 5");
    emitter.instruction("je __rt_trace_arg_array_x");
    emitter.instruction("cmp r10, 6");
    emitter.instruction("je __rt_trace_arg_object_x");
    // Tag 0 and anything else print as an integer, which is what php shows for one.
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");
    emitter.instruction("call __rt_itoa");
    emitter.instruction("mov rdi, rax");
    emitter.instruction("mov rsi, rdx");
    emitter.instruction("call __rt_trace_append");
    emitter.instruction("jmp __rt_trace_arg_done_x");

    emitter.label("__rt_trace_arg_bool_x");
    emitter.instruction("cmp QWORD PTR [rbp - 8], 0");
    emitter.instruction("je __rt_trace_arg_false_x");
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_true");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_true")));                                          // "true"
    emitter.instruction("call __rt_trace_append");
    emitter.instruction("jmp __rt_trace_arg_done_x");
    emitter.label("__rt_trace_arg_false_x");
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_false");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_false")));                                          // "false"
    emitter.instruction("call __rt_trace_append");
    emitter.instruction("jmp __rt_trace_arg_done_x");

    emitter.label("__rt_trace_arg_null_x");
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_null");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_null")));                                          // "NULL", uppercase as php prints it
    emitter.instruction("call __rt_trace_append");
    emitter.instruction("jmp __rt_trace_arg_done_x");

    emitter.label("__rt_trace_arg_array_x");
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_array");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_array")));                                          // "Array", never the contents
    emitter.instruction("call __rt_trace_append");
    emitter.instruction("jmp __rt_trace_arg_done_x");

    emitter.label("__rt_trace_arg_object_x");
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_object");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_object")));                                          // "Object("
    emitter.instruction("call __rt_trace_append");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");
    // The same dense table `get_class()` reads.
    emitter.instruction("test rax, rax");
    emitter.instruction("je __rt_trace_arg_object_close_x");                    // a null object has no name to print
    emitter.instruction("mov r8, QWORD PTR [rax]");                             // the object's concrete runtime class id
    abi::emit_symbol_address(emitter, "r9", "_class_name_count");
    emitter.instruction("mov r9, QWORD PTR [r9]");
    emitter.instruction("cmp r8, r9");
    emitter.instruction("jae __rt_trace_arg_object_close_x");                   // an id outside the table names nothing
    abi::emit_symbol_address(emitter, "r10", "_class_name_entries");
    emitter.instruction("shl r8, 4");                                           // rows are 16 bytes
    emitter.instruction("mov rdi, QWORD PTR [r10 + r8]");
    emitter.instruction("mov rsi, QWORD PTR [r10 + r8 + 8]");
    emitter.instruction("call __rt_trace_append");
    emitter.label("__rt_trace_arg_object_close_x");
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_rparen");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_rparen")));                                          // ")"
    emitter.instruction("call __rt_trace_append");
    emitter.instruction("jmp __rt_trace_arg_done_x");

    emitter.label("__rt_trace_arg_resource_x");
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_resource");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_resource")));                                         // "Resource id #"
    emitter.instruction("call __rt_trace_append");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");
    emitter.instruction("call __rt_resource_id_of");                            // the id php shows, not the handle
    emitter.instruction("call __rt_itoa");
    emitter.instruction("mov rdi, rax");
    emitter.instruction("mov rsi, rdx");
    emitter.instruction("call __rt_trace_append");
    emitter.instruction("jmp __rt_trace_arg_done_x");

    emitter.label("__rt_trace_arg_float_x");
    emitter.instruction("movq xmm0, QWORD PTR [rbp - 8]");                      // __rt_ftoa_repr renders from xmm0
    emitter.instruction("call __rt_ftoa_repr");
    emitter.instruction("mov rdi, rax");
    emitter.instruction("mov rsi, rdx");
    emitter.instruction("call __rt_trace_append");
    emitter.instruction("jmp __rt_trace_arg_done_x");

    emitter.label("__rt_trace_arg_str_x");
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_quote");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_quote")));                                          // "'"
    emitter.instruction("call __rt_trace_append");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
    emitter.instruction(&format!("cmp rsi, {}", TRACE_STR_LIMIT));
    emitter.instruction("jle __rt_trace_arg_str_whole_x");
    emitter.instruction(&format!("mov rsi, {}", TRACE_STR_LIMIT));              // php shows fifteen bytes...
    emitter.instruction("call __rt_trace_append");
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_ellipsis");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_ellipsis")));                                          // ...then says so, INSIDE the quotes
    emitter.instruction("call __rt_trace_append");
    emitter.instruction("jmp __rt_trace_arg_str_close_x");
    emitter.label("__rt_trace_arg_str_whole_x");
    emitter.instruction("call __rt_trace_append");
    emitter.label("__rt_trace_arg_str_close_x");
    abi::emit_symbol_address(emitter, "rdi", "_rt_trace_quote");
    emitter.instruction(&format!("mov rsi, {}", trace_literal_len("_rt_trace_quote")));                                          // "'"
    emitter.instruction("call __rt_trace_append");

    emitter.label("__rt_trace_arg_done_x");
    emitter.instruction("add rsp, 32");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// Emits `__rt_trace_as_string`, php's `Throwable::getTraceAsString()` text.
///
/// Input: `x0`/`rdi` = the exception's own completeness proof. Output: the string result pair.
///
/// The same frames the report prints, without the `Stack trace:` header and without the tail —
/// and, unlike the report, WITHOUT a trailing newline: php's frames are newline-SEPARATED, so the
/// final `#N {main}` ends the string. Each recorded frame already ends in one, which is why the
/// sentinel is appended from the shared `_rt_trace_main` literal one byte SHORT.
///
/// The sentinel is appended into the trace buffer itself and the length is then put back, so the
/// buffer a later report reads is the one it would have read anyway. Calling this twice answers
/// the same string twice.
///
/// A proof of zero answers the empty string. That is not php's answer — php always has at least
/// `#0 {main}` — but it is the same silence the report keeps, and for the same reason: a trace
/// that is SHORT asserts an empty stack, which is a wrong answer rather than a missing one.
pub fn emit_trace_as_string(emitter: &mut Emitter) {
    let main_len = trace_literal_len("_rt_trace_main") - 1;
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.blank();
            emitter.comment("--- runtime: trace_as_string ---");
            emitter.label_global("__rt_trace_as_string");
            emitter.instruction("stp x29, x30, [sp, #-32]!");
            emitter.instruction("add x29, sp, #0");
            emitter.instruction("cbz x0, __rt_trace_str_empty");                // no proof: the same silence the report keeps
            abi::emit_load_symbol_to_reg(emitter, "x9", "_rt_trace_len", 0);
            emitter.instruction("str x9, [sp, #16]");                           // the length to restore once the text is built

            abi::emit_symbol_address(emitter, "x1", "_rt_trace_hash");
            emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_hash")));
            emitter.instruction("bl __rt_trace_append");                        // "#"
            abi::emit_load_symbol_to_reg(emitter, "x0", "_rt_trace_count", 0);
            emitter.instruction("bl __rt_itoa");                                // php numbers {main} after the real frames
            emitter.instruction("bl __rt_trace_append");
            abi::emit_symbol_address(emitter, "x1", "_rt_trace_main");
            emitter.instruction(&format!("mov x2, #{main_len}"));               // " {main}" without the report's newline
            emitter.instruction("bl __rt_trace_append");

            abi::emit_symbol_address(emitter, "x1", "_rt_trace_buf");
            abi::emit_load_symbol_to_reg(emitter, "x2", "_rt_trace_len", 0);    // the assembled text
            emitter.instruction("ldr x9, [sp, #16]");
            emitter.instruction("str x2, [sp, #24]");                           // keep the text length across the store below
            abi::emit_store_reg_to_symbol(emitter, "x9", "_rt_trace_len", 0);   // put the buffer back for the report
            abi::emit_symbol_address(emitter, "x1", "_rt_trace_buf");
            emitter.instruction("ldr x2, [sp, #24]");
            emitter.instruction("bl __rt_str_persist");                         // hand the caller its own copy
            emitter.instruction("ldp x29, x30, [sp], #32");
            emitter.instruction("ret");

            emitter.label("__rt_trace_str_empty");
            abi::emit_symbol_address(emitter, "x1", "_rt_trace_buf");
            emitter.instruction("mov x2, #0");                                  // the empty string
            emitter.instruction("ldp x29, x30, [sp], #32");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.blank();
            emitter.comment("--- runtime: trace_as_string ---");
            emitter.label_global("__rt_trace_as_string");
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 32");
            emitter.instruction("test rdi, rdi");
            emitter.instruction("jz __rt_trace_str_empty_x");                   // no proof: the same silence the report keeps
            abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_trace_len", 0);
            emitter.instruction("mov QWORD PTR [rbp - 8], r10");                // the length to restore once the text is built

            abi::emit_symbol_address(emitter, "rsi", "_rt_trace_hash");
            emitter.instruction(&format!("mov rdx, {}", trace_literal_len("_rt_trace_hash")));
            emitter.instruction("call __rt_trace_append");                      // "#"
            abi::emit_load_symbol_to_reg(emitter, "rax", "_rt_trace_count", 0);
            emitter.instruction("call __rt_itoa");                              // php numbers {main} after the real frames
            emitter.instruction("mov rsi, rax");
            emitter.instruction("call __rt_trace_append");
            abi::emit_symbol_address(emitter, "rsi", "_rt_trace_main");
            emitter.instruction(&format!("mov rdx, {main_len}"));               // " {main}" without the report's newline
            emitter.instruction("call __rt_trace_append");

            abi::emit_load_symbol_to_reg(emitter, "rdx", "_rt_trace_len", 0);   // the assembled text
            emitter.instruction("mov QWORD PTR [rbp - 16], rdx");
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
            abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_trace_len", 0);  // put the buffer back for the report
            abi::emit_symbol_address(emitter, "rax", "_rt_trace_buf");
            emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");
            emitter.instruction("call __rt_str_persist");                       // hand the caller its own copy
            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");

            emitter.label("__rt_trace_str_empty_x");
            abi::emit_symbol_address(emitter, "rax", "_rt_trace_buf");
            emitter.instruction("xor edx, edx");                                // the empty string
            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits php's `Stack trace:` block, AArch64.
///
/// Input: `x0` = the line php prints in the tail, `x1` = a per-SITE completeness override.
///
/// Silent unless the frame list is known complete. A trace that is SHORT is not an approximation:
/// `#0 {main}` where php names a frame asserts the stack was empty, so printing nothing is the
/// only honest alternative.
///
/// Two authorities answer that, and they are not the same question. `_rt_trace_exact` is a MODULE
/// property — "no user function here could hide a frame" — and it is all the runtime unwinder can
/// consult, because it cannot know whose frame it is unwinding. A LOWERED throw knows more: a
/// builtin called directly from `main` has a complete chain whatever else the module declares, and
/// says so through `x1`. Zero defers to the module flag.
fn emit_write_block_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: trace_write_block ---");
    emitter.label_global("__rt_trace_write_block");
    emitter.instruction("stp x29, x30, [sp, #-32]!");
    emitter.instruction("add x29, sp, #0");
    emitter.instruction("str x0, [sp, #16]");                                   // the line php prints in the tail
    emitter.instruction("cbnz x1, __rt_uncaught_trace_go");                     // the SITE proved this chain complete
    abi::emit_load_symbol_to_reg(emitter, "x9", "_rt_trace_exact", 0);
    emitter.instruction("cbz x9, __rt_uncaught_trace_done");                    // this module can hide a frame: say nothing
    emitter.label("__rt_uncaught_trace_go");

    abi::emit_symbol_address(emitter, "x1", "_rt_trace_header");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_header")));
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    abi::emit_load_symbol_to_reg(emitter, "x2", "_rt_trace_len", 0);            // the frames recorded so far
    emitter.instruction("cbz x2, __rt_uncaught_trace_main");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_buf");
    abi::emit_load_symbol_to_reg(emitter, "x2", "_rt_trace_len", 0);
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    emitter.label("__rt_uncaught_trace_main");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_hash");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_hash")));
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);
    abi::emit_load_symbol_to_reg(emitter, "x0", "_rt_trace_count", 0);          // php numbers {main} after the real frames
    emitter.instruction("bl __rt_itoa");                                        // pointer in x1, length in x2
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_main");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_main")));
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    // -- `  thrown in FILE on line N`, using the construction line php reports twice --
    emitter.instruction("ldr x10, [sp, #16]");
    emitter.instruction("cbz x10, __rt_uncaught_trace_done");                   // no origin: omit rather than invent
    abi::emit_load_symbol_to_reg(emitter, "x11", "_script_source_file_len", 0);
    emitter.instruction("cbz x11, __rt_uncaught_trace_done");
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_thrown_in");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_thrown_in")));
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);
    abi::emit_symbol_address(emitter, "x1", "_script_source_file");
    abi::emit_load_symbol_to_reg(emitter, "x2", "_script_source_file_len", 0);
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_on_line");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_on_line")));
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);
    emitter.instruction("ldr x0, [sp, #16]");
    emitter.instruction("bl __rt_itoa");
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);
    abi::emit_symbol_address(emitter, "x1", "_rt_trace_nl");
    emitter.instruction(&format!("mov x2, #{}", trace_literal_len("_rt_trace_nl")));
    emitter.instruction("mov x0, #1");
    emitter.syscall(4);

    emitter.label("__rt_uncaught_trace_done");
    emitter.instruction("ldp x29, x30, [sp], #32");
    emitter.instruction("ret");
}

/// Emits php's `Stack trace:` block, x86_64. Mirrors the AArch64 arm exactly.
fn emit_write_block_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: trace_write_block ---");
    emitter.label_global("__rt_trace_write_block");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 16");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the line php prints in the tail
    emitter.instruction("test rsi, rsi");                                       // the SITE proved this chain complete
    emitter.instruction("jnz __rt_uncaught_trace_go_x86");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_trace_exact", 0);
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_uncaught_trace_done");                         // this module can hide a frame: say nothing
    emitter.label("__rt_uncaught_trace_go_x86");

    abi::emit_symbol_address(emitter, "rsi", "_rt_trace_header");
    emitter.instruction(&format!("mov edx, {}", trace_literal_len("_rt_trace_header")));
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    abi::emit_load_symbol_to_reg(emitter, "rdx", "_rt_trace_len", 0);           // the frames recorded so far
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_uncaught_trace_main");
    abi::emit_symbol_address(emitter, "rsi", "_rt_trace_buf");
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_rt_trace_len", 0);
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    emitter.label("__rt_uncaught_trace_main");
    abi::emit_symbol_address(emitter, "rsi", "_rt_trace_hash");
    emitter.instruction(&format!("mov edx, {}", trace_literal_len("_rt_trace_hash")));
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_rt_trace_count", 0);         // php numbers {main} after the real frames
    emitter.instruction("call __rt_itoa");                                      // pointer in rax, length in rdx
    emitter.instruction("mov rsi, rax");                                        // out of rax before it becomes the syscall number
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");
    abi::emit_symbol_address(emitter, "rsi", "_rt_trace_main");
    emitter.instruction(&format!("mov edx, {}", trace_literal_len("_rt_trace_main")));
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    // -- `  thrown in FILE on line N`, using the construction line php reports twice --
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_uncaught_trace_done");                         // no origin: omit rather than invent
    abi::emit_load_symbol_to_reg(emitter, "r11", "_script_source_file_len", 0);
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_uncaught_trace_done");
    abi::emit_symbol_address(emitter, "rsi", "_rt_trace_thrown_in");
    emitter.instruction(&format!("mov edx, {}", trace_literal_len("_rt_trace_thrown_in")));
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");
    abi::emit_symbol_address(emitter, "rsi", "_script_source_file");
    abi::emit_load_symbol_to_reg(emitter, "rdx", "_script_source_file_len", 0);
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");
    abi::emit_symbol_address(emitter, "rsi", "_rt_trace_on_line");
    emitter.instruction(&format!("mov edx, {}", trace_literal_len("_rt_trace_on_line")));
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");
    emitter.instruction("call __rt_itoa");
    emitter.instruction("mov rsi, rax");
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");
    abi::emit_symbol_address(emitter, "rsi", "_rt_trace_nl");
    emitter.instruction(&format!("mov edx, {}", trace_literal_len("_rt_trace_nl")));
    emitter.instruction("mov edi, 1");
    emitter.instruction("mov eax, 1");
    emitter.instruction("syscall");

    emitter.label("__rt_uncaught_trace_done");
    emitter.instruction("add rsp, 16");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}
