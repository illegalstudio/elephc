use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_json_decode: decode a JSON string value into the current string-only contract.
/// Input:  x1/x2 or rax/rdx = JSON string
/// Output: x1/x2 or rax/rdx = decoded string
///
/// Supported JSON inputs:
///   - Quoted strings: "hello" -> hello (with one-byte escape decoding)
///   - Numbers / true / false / null -> trimmed borrowed string representation
///   - Arrays / objects -> trimmed borrowed JSON slice (no structural parsing)
///
/// Standard one-byte escapes are decoded: \" \\ \/ \b \f \n \r \t.
/// Unsupported escapes such as \uXXXX are preserved literally for now.
pub fn emit_json_decode(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_decode_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_decode ---");
    emitter.label_global("__rt_json_decode");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack space for the trimmed source slice, concat cursors, and quoted-string loop indices
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address before decoding the JSON slice
    emitter.instruction("add x29, sp, #48");                                    // establish a stable frame pointer for the json_decode scratch slots

    // -- trim leading JSON whitespace from the borrowed input slice --
    emitter.instruction("cbz x2, __rt_json_decode_empty");                      // an empty input slice decodes to the empty string immediately
    emitter.instruction("mov x10, x1");                                         // start with the original source pointer as the left trim cursor
    emitter.instruction("add x11, x1, x2");                                     // compute the exclusive end pointer of the borrowed input slice for whitespace trimming
    emitter.label("__rt_json_decode_trim_left");
    emitter.instruction("cmp x10, x11");                                        // have the trim cursors crossed while skipping leading whitespace?
    emitter.instruction("b.ge __rt_json_decode_empty");                         // an all-whitespace input slice decodes to the empty string
    emitter.instruction("ldrb w9, [x10]");                                      // load the next leading byte to see whether JSON whitespace must be skipped
    emitter.instruction("cmp w9, #32");                                         // is the leading byte a space character?
    emitter.instruction("b.eq __rt_json_decode_trim_left_advance");             // skip leading spaces before decoding the meaningful JSON payload
    emitter.instruction("cmp w9, #9");                                          // is the leading byte a horizontal tab?
    emitter.instruction("b.eq __rt_json_decode_trim_left_advance");             // skip leading tabs before decoding the meaningful JSON payload
    emitter.instruction("cmp w9, #10");                                         // is the leading byte a newline?
    emitter.instruction("b.eq __rt_json_decode_trim_left_advance");             // skip leading newlines before decoding the meaningful JSON payload
    emitter.instruction("cmp w9, #13");                                         // is the leading byte a carriage return?
    emitter.instruction("b.ne __rt_json_decode_trim_right");                    // stop once the left edge reaches the first non-whitespace JSON byte
    emitter.label("__rt_json_decode_trim_left_advance");
    emitter.instruction("add x10, x10, #1");                                    // advance the left trim cursor past the consumed whitespace byte
    emitter.instruction("b __rt_json_decode_trim_left");                        // continue skipping leading JSON whitespace bytes

    // -- trim trailing JSON whitespace from the borrowed input slice --
    emitter.label("__rt_json_decode_trim_right");
    emitter.instruction("cmp x10, x11");                                        // did trimming the left edge already consume the whole JSON slice?
    emitter.instruction("b.ge __rt_json_decode_empty");                         // an all-whitespace input slice decodes to the empty string
    emitter.instruction("sub x12, x11, #1");                                    // point at the final byte that still belongs to the candidate JSON payload
    emitter.instruction("ldrb w9, [x12]");                                      // load the trailing byte to decide whether it is JSON whitespace
    emitter.instruction("cmp w9, #32");                                         // is the trailing byte a space character?
    emitter.instruction("b.eq __rt_json_decode_trim_right_advance");            // drop trailing spaces from the borrowed JSON slice
    emitter.instruction("cmp w9, #9");                                          // is the trailing byte a horizontal tab?
    emitter.instruction("b.eq __rt_json_decode_trim_right_advance");            // drop trailing tabs from the borrowed JSON slice
    emitter.instruction("cmp w9, #10");                                         // is the trailing byte a newline?
    emitter.instruction("b.eq __rt_json_decode_trim_right_advance");            // drop trailing newlines from the borrowed JSON slice
    emitter.instruction("cmp w9, #13");                                         // is the trailing byte a carriage return?
    emitter.instruction("b.ne __rt_json_decode_trim_done");                     // stop once the right edge reaches the last non-whitespace JSON byte
    emitter.label("__rt_json_decode_trim_right_advance");
    emitter.instruction("sub x11, x11, #1");                                    // move the exclusive right trim cursor left past the consumed whitespace byte
    emitter.instruction("b __rt_json_decode_trim_right");                       // continue skipping trailing JSON whitespace bytes

    // -- compute the trimmed JSON slice before deciding how to decode it --
    emitter.label("__rt_json_decode_trim_done");
    emitter.instruction("mov x1, x10");                                         // the trimmed JSON slice now starts at the left trim cursor
    emitter.instruction("sub x2, x11, x10");                                    // compute the trimmed JSON slice length from the trim cursor span
    emitter.instruction("cbz x2, __rt_json_decode_empty");                      // an empty trimmed JSON slice still decodes to the empty string
    emitter.instruction("ldrb w9, [x1]");                                       // inspect the first byte of the trimmed JSON slice to detect quoted strings
    emitter.instruction("cmp w9, #34");                                         // does the trimmed JSON payload begin with a double quote?
    emitter.instruction("b.ne __rt_json_decode_passthrough");                   // non-string JSON payloads return their trimmed borrowed representation

    // -- persist the trimmed quoted JSON slice for the decode loop --
    emitter.instruction("str x1, [sp, #0]");                                    // save the trimmed quoted JSON pointer across the decode loop and concat-buffer writes
    emitter.instruction("str x2, [sp, #8]");                                    // save the trimmed quoted JSON length across the decode loop and concat-buffer writes

    // -- get output position in concat_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat-buffer absolute offset before writing decoded string bytes
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // compute the concat-buffer write pointer where the decoded string should begin
    emitter.instruction("str x11, [sp, #16]");                                  // save the decoded-string start pointer for the final result slice
    emitter.instruction("str x11, [sp, #24]");                                  // save the current concat-buffer write pointer for the decode loop

    // -- skip the opening quote and stop before the closing quote --
    emitter.instruction("mov x12, #1");                                         // initialize the source index to the first byte after the opening JSON quote
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the trimmed quoted JSON length before deriving the closing-quote boundary
    emitter.instruction("sub x2, x2, #1");                                      // treat the final byte as the closing quote boundary for the decode loop

    emitter.label("__rt_json_decode_loop");
    emitter.instruction("cmp x12, x2");                                         // have we reached the closing quote boundary of the trimmed JSON string?
    emitter.instruction("b.ge __rt_json_decode_done");                          // finish once every quoted payload byte has been decoded
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the trimmed quoted JSON pointer for the current source-byte fetch
    emitter.instruction("ldrb w9, [x1, x12]");                                  // load the next quoted payload byte from the trimmed JSON string
    emitter.instruction("cmp w9, #92");                                         // does this payload byte start a JSON escape sequence?
    emitter.instruction("b.ne __rt_json_decode_literal");                       // ordinary payload bytes copy directly into the decoded output string

    // -- decode supported one-byte escapes or preserve unsupported escapes literally --
    emitter.instruction("add x12, x12, #1");                                    // advance past the backslash so the next load inspects the escaped JSON codepoint
    emitter.instruction("ldrb w9, [x1, x12]");                                  // load the escaped JSON codepoint that follows the backslash prefix
    emitter.instruction("cmp w9, #110");                                        // does the escape sequence encode a newline?
    emitter.instruction("b.ne __rt_json_decode_esc_not_n");                     // continue checking JSON escape families until one matches
    emitter.instruction("mov w9, #10");                                         // decode \\n into an actual newline byte in the output string
    emitter.instruction("b __rt_json_decode_literal");                          // write the decoded newline byte through the shared literal write path
    emitter.label("__rt_json_decode_esc_not_n");
    emitter.instruction("cmp w9, #116");                                        // does the escape sequence encode a horizontal tab?
    emitter.instruction("b.ne __rt_json_decode_esc_not_t");                     // continue checking JSON escape families until one matches
    emitter.instruction("mov w9, #9");                                          // decode \\t into an actual horizontal-tab byte in the output string
    emitter.instruction("b __rt_json_decode_literal");                          // write the decoded tab byte through the shared literal write path
    emitter.label("__rt_json_decode_esc_not_t");
    emitter.instruction("cmp w9, #114");                                        // does the escape sequence encode a carriage return?
    emitter.instruction("b.ne __rt_json_decode_esc_not_r");                     // continue checking JSON escape families until one matches
    emitter.instruction("mov w9, #13");                                         // decode \\r into an actual carriage-return byte in the output string
    emitter.instruction("b __rt_json_decode_literal");                          // write the decoded carriage-return byte through the shared literal write path
    emitter.label("__rt_json_decode_esc_not_r");
    emitter.instruction("cmp w9, #98");                                         // does the escape sequence encode a backspace control byte?
    emitter.instruction("b.ne __rt_json_decode_esc_not_b");                     // continue checking JSON escape families until one matches
    emitter.instruction("mov w9, #8");                                          // decode \\b into an actual backspace byte in the output string
    emitter.instruction("b __rt_json_decode_literal");                          // write the decoded backspace byte through the shared literal write path
    emitter.label("__rt_json_decode_esc_not_b");
    emitter.instruction("cmp w9, #102");                                        // does the escape sequence encode a form-feed control byte?
    emitter.instruction("b.ne __rt_json_decode_esc_maybe_plain");               // continue checking JSON escape families until one matches
    emitter.instruction("mov w9, #12");                                         // decode \\f into an actual form-feed byte in the output string
    emitter.instruction("b __rt_json_decode_literal");                          // write the decoded form-feed byte through the shared literal write path
    emitter.label("__rt_json_decode_esc_maybe_plain");
    emitter.instruction("cmp w9, #34");                                         // is this an escaped double quote that should lose the backslash?
    emitter.instruction("b.eq __rt_json_decode_literal");                       // copy only the quote byte into the decoded output string
    emitter.instruction("cmp w9, #92");                                         // is this an escaped backslash that should lose the escape prefix?
    emitter.instruction("b.eq __rt_json_decode_literal");                       // copy only the backslash byte into the decoded output string
    emitter.instruction("cmp w9, #47");                                         // is this an escaped solidus that should lose the escape prefix?
    emitter.instruction("b.eq __rt_json_decode_literal");                       // copy only the slash byte into the decoded output string
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the concat-buffer write pointer to preserve an unsupported escape literally
    emitter.instruction("mov w10, #92");                                        // materialize a backslash byte so unsupported escapes keep their original prefix
    emitter.instruction("strb w10, [x11]");                                     // write the preserved backslash prefix of the unsupported escape sequence
    emitter.instruction("strb w9, [x11, #1]");                                  // write the unsupported escaped codepoint after the preserved backslash prefix
    emitter.instruction("add x11, x11, #2");                                    // advance the concat-buffer write pointer past the preserved two-byte escape sequence
    emitter.instruction("str x11, [sp, #24]");                                  // persist the updated write pointer after preserving the unsupported escape sequence
    emitter.instruction("add x12, x12, #1");                                    // advance past the unsupported escaped codepoint before continuing the decode loop
    emitter.instruction("b __rt_json_decode_loop");                             // continue decoding the remaining quoted JSON payload bytes

    // -- write an ordinary or decoded one-byte payload into the output slice --
    emitter.label("__rt_json_decode_literal");
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the concat-buffer write pointer before appending the decoded payload byte
    emitter.instruction("strb w9, [x11]");                                      // write the decoded or literal payload byte into the concat buffer
    emitter.instruction("add x11, x11, #1");                                    // advance the concat-buffer write pointer after appending the decoded payload byte
    emitter.instruction("str x11, [sp, #24]");                                  // persist the updated write pointer for the next decode-loop iteration
    emitter.instruction("add x12, x12, #1");                                    // advance to the next quoted payload byte after consuming this literal or escape sequence
    emitter.instruction("b __rt_json_decode_loop");                             // continue decoding the remaining quoted JSON payload bytes

    // -- finalize the concat-backed decoded string result --
    emitter.label("__rt_json_decode_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return the decoded-string start pointer in the string result register pair
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the final concat-buffer write pointer before turning it into a slice length
    emitter.instruction("sub x2, x11, x1");                                     // compute the decoded-string length from write_end - write_start
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // reload the current concat-buffer absolute offset before publishing the decoded-string append
    emitter.instruction("add x10, x10, x2");                                    // advance the concat-buffer absolute offset by the decoded-string length
    emitter.instruction("str x10, [x9]");                                       // publish the updated concat-buffer absolute offset for later writers
    emitter.instruction("b __rt_json_decode_ret");                              // return the decoded concat-backed string slice through the shared epilogue

    // -- empty input decodes to the empty string slice --
    emitter.label("__rt_json_decode_empty");
    emitter.instruction("mov x1, #0");                                          // return a null pointer for the empty decoded string slice
    emitter.instruction("mov x2, #0");                                          // return a zero-length empty decoded string slice
    emitter.instruction("b __rt_json_decode_ret");                              // return the empty decoded string slice through the shared epilogue

    // -- non-string JSON payloads return their trimmed borrowed representation --
    emitter.label("__rt_json_decode_passthrough");
    emitter.instruction("b __rt_json_decode_ret");                              // return the trimmed borrowed JSON literal, array, or object slice as-is

    // -- tear down and return --
    emitter.label("__rt_json_decode_ret");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address before returning to generated code
    emitter.instruction("add sp, sp, #64");                                     // release the json_decode scratch frame before returning to generated code
    emitter.instruction("ret");                                                 // return the decoded or trimmed JSON string slice to generated code
}

fn emit_json_decode_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_decode ---");
    emitter.label_global("__rt_json_decode");

    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving json_decode scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the trimmed source slice and concat-buffer cursors
    emitter.instruction("sub rsp, 48");                                         // reserve local slots for the trimmed source slice, output pointers, write cursor, and decode-loop indices

    // -- trim leading JSON whitespace from the borrowed input slice --
    emitter.instruction("test rdx, rdx");                                       // does the incoming borrowed JSON slice contain any bytes at all?
    emitter.instruction("jz __rt_json_decode_empty");                           // an empty input slice decodes to the empty string immediately
    emitter.instruction("mov r8, rax");                                         // start with the original source pointer as the left trim cursor
    emitter.instruction("lea r9, [rax + rdx]");                                 // compute the exclusive end pointer of the borrowed input slice for whitespace trimming
    emitter.label("__rt_json_decode_trim_left");
    emitter.instruction("cmp r8, r9");                                          // have the trim cursors crossed while skipping leading whitespace?
    emitter.instruction("jae __rt_json_decode_empty");                          // an all-whitespace input slice decodes to the empty string
    emitter.instruction("movzx r10, BYTE PTR [r8]");                            // load the next leading byte to see whether JSON whitespace must be skipped
    emitter.instruction("cmp r10b, 32");                                        // is the leading byte a space character?
    emitter.instruction("je __rt_json_decode_trim_left_advance");               // skip leading spaces before decoding the meaningful JSON payload
    emitter.instruction("cmp r10b, 9");                                         // is the leading byte a horizontal tab?
    emitter.instruction("je __rt_json_decode_trim_left_advance");               // skip leading tabs before decoding the meaningful JSON payload
    emitter.instruction("cmp r10b, 10");                                        // is the leading byte a newline?
    emitter.instruction("je __rt_json_decode_trim_left_advance");               // skip leading newlines before decoding the meaningful JSON payload
    emitter.instruction("cmp r10b, 13");                                        // is the leading byte a carriage return?
    emitter.instruction("jne __rt_json_decode_trim_right");                     // stop once the left edge reaches the first non-whitespace JSON byte
    emitter.label("__rt_json_decode_trim_left_advance");
    emitter.instruction("add r8, 1");                                           // advance the left trim cursor past the consumed whitespace byte
    emitter.instruction("jmp __rt_json_decode_trim_left");                      // continue skipping leading JSON whitespace bytes

    // -- trim trailing JSON whitespace from the borrowed input slice --
    emitter.label("__rt_json_decode_trim_right");
    emitter.instruction("cmp r8, r9");                                          // did trimming the left edge already consume the whole JSON slice?
    emitter.instruction("jae __rt_json_decode_empty");                          // an all-whitespace input slice decodes to the empty string
    emitter.instruction("lea rcx, [r9 - 1]");                                   // point at the final byte that still belongs to the candidate JSON payload
    emitter.instruction("movzx r10, BYTE PTR [rcx]");                           // load the trailing byte to decide whether it is JSON whitespace
    emitter.instruction("cmp r10b, 32");                                        // is the trailing byte a space character?
    emitter.instruction("je __rt_json_decode_trim_right_advance");              // drop trailing spaces from the borrowed JSON slice
    emitter.instruction("cmp r10b, 9");                                         // is the trailing byte a horizontal tab?
    emitter.instruction("je __rt_json_decode_trim_right_advance");              // drop trailing tabs from the borrowed JSON slice
    emitter.instruction("cmp r10b, 10");                                        // is the trailing byte a newline?
    emitter.instruction("je __rt_json_decode_trim_right_advance");              // drop trailing newlines from the borrowed JSON slice
    emitter.instruction("cmp r10b, 13");                                        // is the trailing byte a carriage return?
    emitter.instruction("jne __rt_json_decode_trim_done");                      // stop once the right edge reaches the last non-whitespace JSON byte
    emitter.label("__rt_json_decode_trim_right_advance");
    emitter.instruction("sub r9, 1");                                           // move the exclusive right trim cursor left past the consumed whitespace byte
    emitter.instruction("jmp __rt_json_decode_trim_right");                     // continue skipping trailing JSON whitespace bytes

    // -- compute the trimmed JSON slice before deciding how to decode it --
    emitter.label("__rt_json_decode_trim_done");
    emitter.instruction("mov rax, r8");                                         // the trimmed JSON slice now starts at the left trim cursor
    emitter.instruction("mov rdx, r9");                                         // copy the exclusive right trim cursor before turning it into a trimmed length
    emitter.instruction("sub rdx, r8");                                         // compute the trimmed JSON slice length from the trim cursor span
    emitter.instruction("jz __rt_json_decode_empty");                           // an empty trimmed JSON slice still decodes to the empty string
    emitter.instruction("movzx r10, BYTE PTR [rax]");                           // inspect the first byte of the trimmed JSON slice to detect quoted strings
    emitter.instruction("cmp r10b, 34");                                        // does the trimmed JSON payload begin with a double quote?
    emitter.instruction("jne __rt_json_decode_passthrough");                    // non-string JSON payloads return their trimmed borrowed representation

    // -- persist the trimmed quoted JSON slice for the decode loop --
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the trimmed quoted JSON pointer across the decode loop and concat-buffer writes
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the trimmed quoted JSON length across the decode loop and concat-buffer writes

    // -- get output position in concat_buf --
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer absolute offset before writing decoded string bytes
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the decoded output slice
    emitter.instruction("add r11, r10");                                        // compute the concat-buffer write pointer where the decoded string should begin
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the decoded-string start pointer for the final result slice
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the current concat-buffer write pointer for the decode loop

    // -- skip the opening quote and stop before the closing quote --
    emitter.instruction("mov QWORD PTR [rbp - 40], 1");                         // initialize the source index to the first byte after the opening JSON quote
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the trimmed quoted JSON length before deriving the closing-quote boundary
    emitter.instruction("sub rcx, 1");                                          // treat the final byte as the closing quote boundary for the decode loop
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save the closing-quote boundary across the decode loop

    emitter.label("__rt_json_decode_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the current source index at the top of the quoted-string decode loop
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 48]");                       // have we reached the closing quote boundary of the trimmed JSON string?
    emitter.instruction("jae __rt_json_decode_done");                           // finish once every quoted payload byte has been decoded
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the trimmed quoted JSON pointer for the current source-byte fetch
    emitter.instruction("movzx r11, BYTE PTR [r10 + rcx]");                     // load the next quoted payload byte from the trimmed JSON string
    emitter.instruction("cmp r11b, 92");                                        // does this payload byte start a JSON escape sequence?
    emitter.instruction("jne __rt_json_decode_literal");                        // ordinary payload bytes copy directly into the decoded output string

    // -- decode supported one-byte escapes or preserve unsupported escapes literally --
    emitter.instruction("add rcx, 1");                                          // advance past the backslash so the next load inspects the escaped JSON codepoint
    emitter.instruction("movzx r11, BYTE PTR [r10 + rcx]");                     // load the escaped JSON codepoint that follows the backslash prefix
    emitter.instruction("cmp r11b, 110");                                       // does the escape sequence encode a newline?
    emitter.instruction("jne __rt_json_decode_esc_not_n");                      // continue checking JSON escape families until one matches
    emitter.instruction("mov r11b, 10");                                        // decode \\n into an actual newline byte in the output string
    emitter.instruction("jmp __rt_json_decode_literal");                        // write the decoded newline byte through the shared literal write path
    emitter.label("__rt_json_decode_esc_not_n");
    emitter.instruction("cmp r11b, 116");                                       // does the escape sequence encode a horizontal tab?
    emitter.instruction("jne __rt_json_decode_esc_not_t");                      // continue checking JSON escape families until one matches
    emitter.instruction("mov r11b, 9");                                         // decode \\t into an actual horizontal-tab byte in the output string
    emitter.instruction("jmp __rt_json_decode_literal");                        // write the decoded tab byte through the shared literal write path
    emitter.label("__rt_json_decode_esc_not_t");
    emitter.instruction("cmp r11b, 114");                                       // does the escape sequence encode a carriage return?
    emitter.instruction("jne __rt_json_decode_esc_not_r");                      // continue checking JSON escape families until one matches
    emitter.instruction("mov r11b, 13");                                        // decode \\r into an actual carriage-return byte in the output string
    emitter.instruction("jmp __rt_json_decode_literal");                        // write the decoded carriage-return byte through the shared literal write path
    emitter.label("__rt_json_decode_esc_not_r");
    emitter.instruction("cmp r11b, 98");                                        // does the escape sequence encode a backspace control byte?
    emitter.instruction("jne __rt_json_decode_esc_not_b");                      // continue checking JSON escape families until one matches
    emitter.instruction("mov r11b, 8");                                         // decode \\b into an actual backspace byte in the output string
    emitter.instruction("jmp __rt_json_decode_literal");                        // write the decoded backspace byte through the shared literal write path
    emitter.label("__rt_json_decode_esc_not_b");
    emitter.instruction("cmp r11b, 102");                                       // does the escape sequence encode a form-feed control byte?
    emitter.instruction("jne __rt_json_decode_esc_maybe_plain");                // continue checking JSON escape families until one matches
    emitter.instruction("mov r11b, 12");                                        // decode \\f into an actual form-feed byte in the output string
    emitter.instruction("jmp __rt_json_decode_literal");                        // write the decoded form-feed byte through the shared literal write path
    emitter.label("__rt_json_decode_esc_maybe_plain");
    emitter.instruction("cmp r11b, 34");                                        // is this an escaped double quote that should lose the backslash?
    emitter.instruction("je __rt_json_decode_literal");                         // copy only the quote byte into the decoded output string
    emitter.instruction("cmp r11b, 92");                                        // is this an escaped backslash that should lose the escape prefix?
    emitter.instruction("je __rt_json_decode_literal");                         // copy only the backslash byte into the decoded output string
    emitter.instruction("cmp r11b, 47");                                        // is this an escaped solidus that should lose the escape prefix?
    emitter.instruction("je __rt_json_decode_literal");                         // copy only the slash byte into the decoded output string
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the concat-buffer write pointer to preserve an unsupported escape literally
    emitter.instruction("mov BYTE PTR [r10], 92");                              // write the preserved backslash prefix of the unsupported escape sequence
    emitter.instruction("mov BYTE PTR [r10 + 1], r11b");                        // write the unsupported escaped codepoint after the preserved backslash prefix
    emitter.instruction("add r10, 2");                                          // advance the concat-buffer write pointer past the preserved two-byte escape sequence
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the updated write pointer after preserving the unsupported escape sequence
    emitter.instruction("add rcx, 1");                                          // advance past the unsupported escaped codepoint before continuing the decode loop
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // persist the updated source index after preserving the unsupported escape sequence
    emitter.instruction("jmp __rt_json_decode_loop");                           // continue decoding the remaining quoted JSON payload bytes

    // -- write an ordinary or decoded one-byte payload into the output slice --
    emitter.label("__rt_json_decode_literal");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the concat-buffer write pointer before appending the decoded payload byte
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // write the decoded or literal payload byte into the concat buffer
    emitter.instruction("add r10, 1");                                          // advance the concat-buffer write pointer after appending the decoded payload byte
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the updated write pointer for the next decode-loop iteration
    emitter.instruction("add rcx, 1");                                          // advance to the next quoted payload byte after consuming this literal or escape sequence
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // persist the updated source index for the next decode-loop iteration
    emitter.instruction("jmp __rt_json_decode_loop");                           // continue decoding the remaining quoted JSON payload bytes

    // -- finalize the concat-backed decoded string result --
    emitter.label("__rt_json_decode_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the decoded-string start pointer in the leading x86_64 string result register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // reload the final concat-buffer write pointer before turning it into a slice length
    emitter.instruction("sub rdx, rax");                                        // compute the decoded-string length from write_end - write_start
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the global offset update
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // copy the final concat-buffer write pointer before converting it into an absolute offset
    emitter.instruction("sub rcx, r10");                                        // compute the new absolute concat-buffer offset after the decoded string slice
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // publish the updated concat-buffer absolute offset for later writers
    emitter.instruction("jmp __rt_json_decode_ret");                            // return the decoded concat-backed string slice through the shared epilogue

    // -- empty input decodes to the empty string slice --
    emitter.label("__rt_json_decode_empty");
    emitter.instruction("xor rax, rax");                                        // return a null pointer for the empty decoded string slice
    emitter.instruction("xor rdx, rdx");                                        // return a zero-length empty decoded string slice
    emitter.instruction("jmp __rt_json_decode_ret");                            // return the empty decoded string slice through the shared epilogue

    // -- non-string JSON payloads return their trimmed borrowed representation --
    emitter.label("__rt_json_decode_passthrough");
    emitter.instruction("jmp __rt_json_decode_ret");                            // return the trimmed borrowed JSON literal, array, or object slice as-is

    // -- tear down and return --
    emitter.label("__rt_json_decode_ret");
    emitter.instruction("add rsp, 48");                                         // release the json_decode scratch frame before returning to generated code
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code
    emitter.instruction("ret");                                                 // return the decoded or trimmed JSON string slice to generated code
}
