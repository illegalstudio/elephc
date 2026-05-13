use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_json_pretty_apply: optionally pretty-print a compact JSON slice.
///
/// Reads `_json_active_flags`. If `JSON_PRETTY_PRINT` (bit 128) is set,
/// rewrites the compact JSON in place by inserting newlines and 4-space
/// indentation between container elements, and a single space after every
/// `:` separator. Otherwise returns the input slice unchanged.
///
/// The transformation uses scratch space at concat_buf past the source
/// payload, then memcpys the pretty output back over the original slice
/// and republishes `_concat_off` to point past it.
///
/// Input:
///   ARM64: x1 = src ptr (in concat_buf), x2 = src len
///   x86_64: rax = src ptr, rdx = src len
/// Output: same registers carry (ptr, len) of the post-processed slice.
pub(crate) fn emit_json_pretty_apply(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_pretty_apply_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_pretty_apply ---");
    emitter.label_global("__rt_json_pretty_apply");

    // Stack layout (96 bytes):
    //   [sp, #0]   = src ptr
    //   [sp, #8]   = src len
    //   [sp, #16]  = src index
    //   [sp, #24]  = scratch write pointer
    //   [sp, #32]  = scratch start pointer
    //   [sp, #40]  = depth
    //   [sp, #48]  = in_string flag
    //   [sp, #56]  = need_indent flag
    //   [sp, #64]  = scratch
    //   [sp, #72]  = saved x29
    //   [sp, #80]  = saved x30
    emitter.instruction("sub sp, sp, #96");                                     // allocate the pretty-printer scratch frame
    emitter.instruction("stp x29, x30, [sp, #72]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #72");                                    // establish the new frame pointer

    // Fast path: if PRETTY_PRINT bit is clear, return inputs unchanged.
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x9, [x9]");                                        // load the active flag bitmask
    emitter.instruction("tst x9, #128");                                        // is JSON_PRETTY_PRINT (bit 128) set?
    emitter.instruction("b.eq __rt_json_pretty_skip");                          // when the flag is clear, return the compact slice as-is

    // Persist inputs and set up the scratch area immediately past the source.
    emitter.instruction("str x1, [sp, #0]");                                    // save the source pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the source length
    emitter.instruction("add x12, x1, x2");                                     // scratch_start = src_ptr + src_len (one byte past the compact output)
    emitter.instruction("add x12, x12, #8");                                    // add a small gap before the scratch area to avoid byte aliasing
    emitter.instruction("str x12, [sp, #32]");                                  // save the scratch start pointer
    emitter.instruction("str x12, [sp, #24]");                                  // initialize the scratch write pointer at the start
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize the source index to zero
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize the depth counter to zero
    emitter.instruction("str xzr, [sp, #48]");                                  // initialize the in_string flag to false
    emitter.instruction("str xzr, [sp, #56]");                                  // initialize the need_indent flag to false

    // Main loop: read each source byte and dispatch.
    emitter.label("__rt_json_pretty_loop");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the current source index
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the source length
    emitter.instruction("cmp x10, x11");                                        // have we consumed every source byte?
    emitter.instruction("b.ge __rt_json_pretty_finalize");                      // exit the loop when the entire source has been processed
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the source pointer
    emitter.instruction("ldrb w13, [x12, x10]");                                // load the next source byte
    emitter.instruction("ldr x14, [sp, #48]");                                  // reload the in_string flag
    emitter.instruction("cbnz x14, __rt_json_pretty_in_string");                // when inside a JSON string, copy bytes verbatim with escape handling

    // Outside a string: dispatch on the byte.
    emitter.instruction("cmp w13, #34");                                        // is the byte an opening quote?
    emitter.instruction("b.eq __rt_json_pretty_open_quote");                    // enter the string-copy path
    emitter.instruction("cmp w13, #123");                                       // is the byte '{'?
    emitter.instruction("b.eq __rt_json_pretty_open_container");                // enter the open-container path
    emitter.instruction("cmp w13, #91");                                        // is the byte '['?
    emitter.instruction("b.eq __rt_json_pretty_open_container");                // enter the open-container path
    emitter.instruction("cmp w13, #125");                                       // is the byte '}'?
    emitter.instruction("b.eq __rt_json_pretty_close_container");               // enter the close-container path
    emitter.instruction("cmp w13, #93");                                        // is the byte ']'?
    emitter.instruction("b.eq __rt_json_pretty_close_container");               // enter the close-container path
    emitter.instruction("cmp w13, #44");                                        // is the byte ','?
    emitter.instruction("b.eq __rt_json_pretty_comma");                         // enter the comma path
    emitter.instruction("cmp w13, #58");                                        // is the byte ':'?
    emitter.instruction("b.eq __rt_json_pretty_colon");                         // enter the colon path

    // Default: a value byte (digit, sign, t/f/n). Honor pending indent first.
    emitter.instruction("bl __rt_json_pretty_emit_indent_if_needed");           // emit a pending newline+indent before the value byte
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the source index after the helper call
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the source pointer after the helper call
    emitter.instruction("ldrb w13, [x12, x10]");                                // reload the current source byte after the helper call
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the scratch write pointer
    emitter.instruction("strb w13, [x11]");                                     // copy the value byte to the scratch buffer
    emitter.instruction("add x11, x11, #1");                                    // advance the scratch write pointer past the value byte
    emitter.instruction("str x11, [sp, #24]");                                  // save the updated scratch write pointer
    emitter.instruction("add x10, x10, #1");                                    // advance the source index past the consumed byte
    emitter.instruction("str x10, [sp, #16]");                                  // save the updated source index
    emitter.instruction("b __rt_json_pretty_loop");                             // continue the main scan

    emitter.label("__rt_json_pretty_in_string");
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the scratch write pointer
    emitter.instruction("strb w13, [x11]");                                     // copy the byte verbatim while inside a JSON string
    emitter.instruction("add x11, x11, #1");                                    // advance the scratch write pointer
    emitter.instruction("str x11, [sp, #24]");                                  // save the updated scratch write pointer
    emitter.instruction("add x10, x10, #1");                                    // advance the source index past the consumed byte
    emitter.instruction("str x10, [sp, #16]");                                  // save the updated source index
    emitter.instruction("cmp w13, #92");                                        // was the byte a backslash escape lead-in?
    emitter.instruction("b.eq __rt_json_pretty_in_string_escape");              // copy the next byte verbatim regardless of its semantic meaning
    emitter.instruction("cmp w13, #34");                                        // was the byte the closing JSON quote?
    emitter.instruction("b.ne __rt_json_pretty_loop");                          // ordinary string bytes return to the main loop
    emitter.instruction("str xzr, [sp, #48]");                                  // clear the in_string flag once the closing quote is seen
    emitter.instruction("b __rt_json_pretty_loop");                             // continue the main scan

    emitter.label("__rt_json_pretty_in_string_escape");
    // Copy one extra byte verbatim — even if it's a quote — to honor the escape.
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the source length to bound the lookahead
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the (already-advanced) source index
    emitter.instruction("cmp x10, x11");                                        // is there another byte to consume?
    emitter.instruction("b.ge __rt_json_pretty_loop");                          // bail out when the source ends mid-escape
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the source pointer
    emitter.instruction("ldrb w13, [x12, x10]");                                // load the byte that follows the backslash
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the scratch write pointer
    emitter.instruction("strb w13, [x11]");                                     // copy the escape continuation byte verbatim
    emitter.instruction("add x11, x11, #1");                                    // advance the scratch write pointer past the escape continuation
    emitter.instruction("str x11, [sp, #24]");                                  // save the updated scratch write pointer
    emitter.instruction("add x10, x10, #1");                                    // advance the source index past the escape continuation byte
    emitter.instruction("str x10, [sp, #16]");                                  // save the updated source index
    emitter.instruction("b __rt_json_pretty_loop");                             // continue the main scan still inside the string

    emitter.label("__rt_json_pretty_open_quote");
    emitter.instruction("bl __rt_json_pretty_emit_indent_if_needed");           // emit pending indent before the string token
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the source index after the helper call
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the source pointer after the helper call
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the scratch write pointer
    emitter.instruction("mov w13, #34");                                        // ASCII '"'
    emitter.instruction("strb w13, [x11]");                                     // emit the opening quote into the scratch buffer
    emitter.instruction("add x11, x11, #1");                                    // advance the scratch write pointer
    emitter.instruction("str x11, [sp, #24]");                                  // save the updated scratch write pointer
    emitter.instruction("mov x14, #1");                                         // mark the in_string flag as true
    emitter.instruction("str x14, [sp, #48]");                                  // save the updated in_string flag
    emitter.instruction("add x10, x10, #1");                                    // advance the source index past the opening quote
    emitter.instruction("str x10, [sp, #16]");                                  // save the updated source index
    emitter.instruction("b __rt_json_pretty_loop");                             // continue the main scan

    emitter.label("__rt_json_pretty_open_container");
    emitter.instruction("bl __rt_json_pretty_emit_indent_if_needed");           // emit pending indent before the nested container open
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the source index after the helper call
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the source pointer after the helper call
    emitter.instruction("ldrb w13, [x12, x10]");                                // reload the open-container byte
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the scratch write pointer
    emitter.instruction("strb w13, [x11]");                                     // emit the open-container byte
    emitter.instruction("add x11, x11, #1");                                    // advance the scratch write pointer
    emitter.instruction("str x11, [sp, #24]");                                  // save the updated scratch write pointer
    emitter.instruction("ldr x14, [sp, #40]");                                  // reload the depth counter
    emitter.instruction("add x14, x14, #1");                                    // increment the depth counter
    emitter.instruction("str x14, [sp, #40]");                                  // save the updated depth counter
    emitter.instruction("mov x14, #1");                                         // set the need_indent flag
    emitter.instruction("str x14, [sp, #56]");                                  // save the updated need_indent flag
    emitter.instruction("add x10, x10, #1");                                    // advance the source index past the open-container byte
    emitter.instruction("str x10, [sp, #16]");                                  // save the updated source index
    emitter.instruction("b __rt_json_pretty_loop");                             // continue the main scan

    emitter.label("__rt_json_pretty_close_container");
    // Inspect the need_indent flag to distinguish empty vs non-empty containers.
    emitter.instruction("ldr x14, [sp, #56]");                                  // reload the need_indent flag
    emitter.instruction("cbz x14, __rt_json_pretty_close_with_indent");         // non-empty containers emit a closing newline+indent first
    // Empty container: just decrement depth and clear the flag.
    emitter.instruction("ldr x14, [sp, #40]");                                  // reload the depth counter
    emitter.instruction("sub x14, x14, #1");                                    // decrement the depth counter
    emitter.instruction("str x14, [sp, #40]");                                  // save the updated depth counter
    emitter.instruction("str xzr, [sp, #56]");                                  // clear the need_indent flag
    emitter.instruction("b __rt_json_pretty_close_emit");                       // jump to the closing-byte emission

    emitter.label("__rt_json_pretty_close_with_indent");
    emitter.instruction("ldr x14, [sp, #40]");                                  // reload the depth counter
    emitter.instruction("sub x14, x14, #1");                                    // decrement the depth counter (closing brace aligns with parent level)
    emitter.instruction("str x14, [sp, #40]");                                  // save the updated depth counter
    emitter.instruction("bl __rt_json_pretty_emit_indent_force");               // force-emit a newline + indent at the new depth

    emitter.label("__rt_json_pretty_close_emit");
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the source index
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the source pointer
    emitter.instruction("ldrb w13, [x12, x10]");                                // reload the close-container byte
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the scratch write pointer
    emitter.instruction("strb w13, [x11]");                                     // emit the close-container byte
    emitter.instruction("add x11, x11, #1");                                    // advance the scratch write pointer
    emitter.instruction("str x11, [sp, #24]");                                  // save the updated scratch write pointer
    emitter.instruction("add x10, x10, #1");                                    // advance the source index past the close-container byte
    emitter.instruction("str x10, [sp, #16]");                                  // save the updated source index
    emitter.instruction("b __rt_json_pretty_loop");                             // continue the main scan

    emitter.label("__rt_json_pretty_comma");
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the scratch write pointer
    emitter.instruction("mov w13, #44");                                        // ASCII ','
    emitter.instruction("strb w13, [x11]");                                     // emit the comma separator
    emitter.instruction("add x11, x11, #1");                                    // advance the scratch write pointer
    emitter.instruction("str x11, [sp, #24]");                                  // save the updated scratch write pointer
    emitter.instruction("mov x14, #1");                                         // set the need_indent flag for the next sibling
    emitter.instruction("str x14, [sp, #56]");                                  // save the updated need_indent flag
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the source index
    emitter.instruction("add x10, x10, #1");                                    // advance past the comma byte
    emitter.instruction("str x10, [sp, #16]");                                  // save the updated source index
    emitter.instruction("b __rt_json_pretty_loop");                             // continue the main scan

    emitter.label("__rt_json_pretty_colon");
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the scratch write pointer
    emitter.instruction("mov w13, #58");                                        // ASCII ':'
    emitter.instruction("strb w13, [x11]");                                     // emit the colon
    emitter.instruction("mov w13, #32");                                        // ASCII space
    emitter.instruction("strb w13, [x11, #1]");                                 // emit the trailing space after the colon
    emitter.instruction("add x11, x11, #2");                                    // advance the scratch write pointer past ': '
    emitter.instruction("str x11, [sp, #24]");                                  // save the updated scratch write pointer
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the source index
    emitter.instruction("add x10, x10, #1");                                    // advance past the colon byte
    emitter.instruction("str x10, [sp, #16]");                                  // save the updated source index
    emitter.instruction("b __rt_json_pretty_loop");                             // continue the main scan

    // Helper: emit "\n" + depth*4 spaces if need_indent is set; clear flag.
    emitter.label("__rt_json_pretty_emit_indent_if_needed");
    emitter.instruction("ldr x14, [sp, #56]");                                  // load the need_indent flag
    emitter.instruction("cbz x14, __rt_json_pretty_emit_indent_done");          // skip when no indent is pending
    emitter.instruction("str xzr, [sp, #56]");                                  // clear the need_indent flag before emitting
    emitter.instruction("b __rt_json_pretty_emit_indent_body");                 // emit the newline and indent

    emitter.label("__rt_json_pretty_emit_indent_force");
    emitter.instruction("str xzr, [sp, #56]");                                  // clear the need_indent flag for safety on the forced path
    // fall through to the common emission body.
    emitter.label("__rt_json_pretty_emit_indent_body");
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the scratch write pointer
    emitter.instruction("mov w13, #10");                                        // ASCII '\n'
    emitter.instruction("strb w13, [x11]");                                     // emit the newline
    emitter.instruction("add x11, x11, #1");                                    // advance the scratch write pointer past the newline
    emitter.instruction("ldr x14, [sp, #40]");                                  // reload the depth counter for the indent calculation
    emitter.instruction("lsl x14, x14, #2");                                    // depth * 4 → number of spaces to emit
    emitter.instruction("mov x9, #0");                                          // initialize the space-emission counter
    emitter.label("__rt_json_pretty_emit_indent_loop");
    emitter.instruction("cmp x9, x14");                                         // have we written every indent space?
    emitter.instruction("b.ge __rt_json_pretty_emit_indent_save");              // exit the indent loop once finished
    emitter.instruction("mov w13, #32");                                        // ASCII space
    emitter.instruction("strb w13, [x11]");                                     // emit a single indent space
    emitter.instruction("add x11, x11, #1");                                    // advance the scratch write pointer past the space
    emitter.instruction("add x9, x9, #1");                                      // increment the space-emission counter
    emitter.instruction("b __rt_json_pretty_emit_indent_loop");                 // continue emitting indent spaces
    emitter.label("__rt_json_pretty_emit_indent_save");
    emitter.instruction("str x11, [sp, #24]");                                  // save the updated scratch write pointer after the indent
    emitter.label("__rt_json_pretty_emit_indent_done");
    emitter.instruction("ret");                                                 // return to the caller within the pretty-printer

    // Finalization: copy the scratch buffer back over the source slice and
    // republish concat_off so the caller sees a stable (ptr, len) pair.
    emitter.label("__rt_json_pretty_finalize");
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the source pointer (= destination)
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the scratch start pointer
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the final scratch write pointer
    emitter.instruction("sub x9, x11, x10");                                    // x9 = total pretty length
    emitter.instruction("mov x14, #0");                                         // initialize the copy index
    emitter.label("__rt_json_pretty_copy_back");
    emitter.instruction("cmp x14, x9");                                         // have we copied every pretty byte back?
    emitter.instruction("b.ge __rt_json_pretty_publish");                       // exit the copy loop once finished
    emitter.instruction("ldrb w13, [x10, x14]");                                // load the next pretty byte from the scratch area
    emitter.instruction("strb w13, [x12, x14]");                                // store it at the original source offset
    emitter.instruction("add x14, x14, #1");                                    // advance the copy index
    emitter.instruction("b __rt_json_pretty_copy_back");                        // continue the copy loop
    emitter.label("__rt_json_pretty_publish");
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x11, x12, x10");                                   // compute the absolute concat-buffer offset for the source start
    emitter.instruction("add x11, x11, x9");                                    // advance the offset by the pretty length
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_off");
    emitter.instruction("str x11, [x10]");                                      // republish the concat-buffer offset past the pretty output
    emitter.instruction("mov x1, x12");                                         // x1 = result ptr (= original source ptr)
    emitter.instruction("mov x2, x9");                                          // x2 = result len (the pretty length)
    emitter.instruction("ldp x29, x30, [sp, #72]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the pretty-printer scratch frame
    emitter.instruction("ret");                                                 // return the pretty (ptr, len) pair

    // Skip path: no flag set → return inputs unchanged.
    emitter.label("__rt_json_pretty_skip");
    emitter.instruction("ldp x29, x30, [sp, #72]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the pretty-printer scratch frame
    emitter.instruction("ret");                                                 // return the compact slice unchanged
}

fn emit_json_pretty_apply_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_pretty_apply ---");
    emitter.label_global("__rt_json_pretty_apply");

    // Frame layout (rbp-relative, 80 bytes reserved):
    //   [rbp - 8]  = src ptr
    //   [rbp - 16] = src len
    //   [rbp - 24] = src index
    //   [rbp - 32] = scratch write pointer
    //   [rbp - 40] = scratch start pointer
    //   [rbp - 48] = depth counter
    //   [rbp - 56] = in_string flag
    //   [rbp - 64] = need_indent flag
    //   [rbp - 72] = (reserved for alignment / future use)
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving pretty-printer scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the pretty-printer state machine
    emitter.instruction("sub rsp, 80");                                         // reserve local slots; 80 keeps the call site 16-byte aligned

    // Fast path: if PRETTY_PRINT bit is clear, return inputs unchanged.
    emitter.instruction("mov r10, QWORD PTR [rip + _json_active_flags]");       // load the active flag bitmask
    emitter.instruction("test r10, 128");                                       // is JSON_PRETTY_PRINT (bit 128) set?
    emitter.instruction("je __rt_json_pretty_skip");                            // when the flag is clear, return the compact slice as-is

    // Persist inputs and set up the scratch area immediately past the source.
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source length
    emitter.instruction("lea r10, [rax + rdx]");                                // scratch_start = src_ptr + src_len
    emitter.instruction("add r10, 8");                                          // add a small gap before the scratch area to avoid byte aliasing
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the scratch start pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // initialize the scratch write pointer at the start
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the source index to zero
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the depth counter to zero
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // initialize the in_string flag to false
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // initialize the need_indent flag to false

    // Main loop: read each source byte and dispatch.
    emitter.label("__rt_json_pretty_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the current source index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // have we consumed every source byte?
    emitter.instruction("jge __rt_json_pretty_finalize");                       // exit the loop when the entire source has been processed
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source pointer
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load the next source byte
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // reload the in_string flag
    emitter.instruction("test r9, r9");                                         // check the current JSON encoder condition
    emitter.instruction("jne __rt_json_pretty_in_string");                      // when inside a JSON string, copy bytes verbatim with escape handling

    // Outside a string: dispatch on the byte.
    emitter.instruction("cmp r8, 34");                                          // is the byte an opening quote?
    emitter.instruction("je __rt_json_pretty_open_quote");                      // enter the string-copy path
    emitter.instruction("cmp r8, 123");                                         // is the byte '{'?
    emitter.instruction("je __rt_json_pretty_open_container");                  // enter the open-container path
    emitter.instruction("cmp r8, 91");                                          // is the byte '['?
    emitter.instruction("je __rt_json_pretty_open_container");                  // enter the open-container path
    emitter.instruction("cmp r8, 125");                                         // is the byte '}'?
    emitter.instruction("je __rt_json_pretty_close_container");                 // enter the close-container path
    emitter.instruction("cmp r8, 93");                                          // is the byte ']'?
    emitter.instruction("je __rt_json_pretty_close_container");                 // enter the close-container path
    emitter.instruction("cmp r8, 44");                                          // is the byte ','?
    emitter.instruction("je __rt_json_pretty_comma");                           // enter the comma path
    emitter.instruction("cmp r8, 58");                                          // is the byte ':'?
    emitter.instruction("je __rt_json_pretty_colon");                           // enter the colon path

    // Default: a value byte (digit, sign, t/f/n). Honor pending indent first.
    emitter.instruction("call __rt_json_pretty_emit_indent_if_needed");         // emit a pending newline+indent before the value byte
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the source index after the helper call
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source pointer after the helper call
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // reload the current source byte after the helper call
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the scratch write pointer
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // copy the value byte to the scratch buffer
    emitter.instruction("add r11, 1");                                          // advance the scratch write pointer past the value byte
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the updated scratch write pointer
    emitter.instruction("add rcx, 1");                                          // advance the source index past the consumed byte
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the updated source index
    emitter.instruction("jmp __rt_json_pretty_loop");                           // continue the main scan

    emitter.label("__rt_json_pretty_in_string");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the scratch write pointer
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // copy the byte verbatim while inside a JSON string
    emitter.instruction("add r11, 1");                                          // advance the scratch write pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the updated scratch write pointer
    emitter.instruction("add rcx, 1");                                          // advance the source index past the consumed byte
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the updated source index
    emitter.instruction("cmp r8, 92");                                          // was the byte a backslash escape lead-in?
    emitter.instruction("je __rt_json_pretty_in_string_escape");                // copy the next byte verbatim regardless of its semantic meaning
    emitter.instruction("cmp r8, 34");                                          // was the byte the closing JSON quote?
    emitter.instruction("jne __rt_json_pretty_loop");                           // ordinary string bytes return to the main loop
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // clear the in_string flag once the closing quote is seen
    emitter.instruction("jmp __rt_json_pretty_loop");                           // continue the main scan

    emitter.label("__rt_json_pretty_in_string_escape");
    // Copy one extra byte verbatim — even if it's a quote — to honor the escape.
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the (already-advanced) source index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // is there another byte to consume?
    emitter.instruction("jge __rt_json_pretty_loop");                           // bail out when the source ends mid-escape
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source pointer
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // load the byte that follows the backslash
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the scratch write pointer
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // copy the escape continuation byte verbatim
    emitter.instruction("add r11, 1");                                          // advance the scratch write pointer past the escape continuation
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the updated scratch write pointer
    emitter.instruction("add rcx, 1");                                          // advance the source index past the escape continuation byte
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the updated source index
    emitter.instruction("jmp __rt_json_pretty_loop");                           // continue the main scan still inside the string

    emitter.label("__rt_json_pretty_open_quote");
    emitter.instruction("call __rt_json_pretty_emit_indent_if_needed");         // emit pending indent before the string token
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the source index after the helper call
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the scratch write pointer
    emitter.instruction("mov BYTE PTR [r11], 34");                              // emit the opening quote into the scratch buffer
    emitter.instruction("add r11, 1");                                          // advance the scratch write pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the updated scratch write pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], 1");                         // mark the in_string flag as true
    emitter.instruction("add rcx, 1");                                          // advance the source index past the opening quote
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the updated source index
    emitter.instruction("jmp __rt_json_pretty_loop");                           // continue the main scan

    emitter.label("__rt_json_pretty_open_container");
    emitter.instruction("call __rt_json_pretty_emit_indent_if_needed");         // emit pending indent before the nested container open
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the source index after the helper call
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source pointer after the helper call
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // reload the open-container byte
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the scratch write pointer
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // emit the open-container byte
    emitter.instruction("add r11, 1");                                          // advance the scratch write pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the updated scratch write pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the depth counter
    emitter.instruction("add r9, 1");                                           // increment the depth counter
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the updated depth counter
    emitter.instruction("mov QWORD PTR [rbp - 64], 1");                         // set the need_indent flag
    emitter.instruction("add rcx, 1");                                          // advance the source index past the open-container byte
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the updated source index
    emitter.instruction("jmp __rt_json_pretty_loop");                           // continue the main scan

    emitter.label("__rt_json_pretty_close_container");
    // Inspect the need_indent flag to distinguish empty vs non-empty
    // containers. need_indent is set right after an opening brace and
    // cleared by emit_indent_if_needed once a value follows. So at the
    // closing bracket: 0 means a value preceded us (non-empty), 1 means
    // the container is empty.
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload the need_indent flag
    emitter.instruction("test r9, r9");                                         // check the current JSON encoder condition
    emitter.instruction("je __rt_json_pretty_close_with_indent");               // need_indent==0 → non-empty container, emit a closing newline+indent
    // Empty container: just decrement depth and clear the flag.
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the depth counter
    emitter.instruction("sub r9, 1");                                           // decrement the depth counter
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the updated depth counter
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // clear the need_indent flag
    emitter.instruction("jmp __rt_json_pretty_close_emit");                     // jump to the closing-byte emission

    emitter.label("__rt_json_pretty_close_with_indent");
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the depth counter
    emitter.instruction("sub r9, 1");                                           // decrement the depth counter (closing brace aligns with parent level)
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the updated depth counter
    emitter.instruction("call __rt_json_pretty_emit_indent_force");             // force-emit a newline + indent at the new depth

    emitter.label("__rt_json_pretty_close_emit");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the source index
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source pointer
    emitter.instruction("movzx r8, BYTE PTR [rax + rcx]");                      // reload the close-container byte
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the scratch write pointer
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // emit the close-container byte
    emitter.instruction("add r11, 1");                                          // advance the scratch write pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the updated scratch write pointer
    emitter.instruction("add rcx, 1");                                          // advance the source index past the close-container byte
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the updated source index
    emitter.instruction("jmp __rt_json_pretty_loop");                           // continue the main scan

    emitter.label("__rt_json_pretty_comma");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the scratch write pointer
    emitter.instruction("mov BYTE PTR [r11], 44");                              // emit the comma separator
    emitter.instruction("add r11, 1");                                          // advance the scratch write pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the updated scratch write pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], 1");                         // set the need_indent flag for the next sibling
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the source index
    emitter.instruction("add rcx, 1");                                          // advance past the comma byte
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the updated source index
    emitter.instruction("jmp __rt_json_pretty_loop");                           // continue the main scan

    emitter.label("__rt_json_pretty_colon");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the scratch write pointer
    emitter.instruction("mov BYTE PTR [r11], 58");                              // emit the colon
    emitter.instruction("mov BYTE PTR [r11 + 1], 32");                          // emit the trailing space after the colon
    emitter.instruction("add r11, 2");                                          // advance the scratch write pointer past ': '
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the updated scratch write pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the source index
    emitter.instruction("add rcx, 1");                                          // advance past the colon byte
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the updated source index
    emitter.instruction("jmp __rt_json_pretty_loop");                           // continue the main scan

    // Helper: emit "\n" + depth*4 spaces if need_indent is set; clear flag.
    emitter.label("__rt_json_pretty_emit_indent_if_needed");
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // load the need_indent flag
    emitter.instruction("test r9, r9");                                         // check the current JSON encoder condition
    emitter.instruction("je __rt_json_pretty_emit_indent_done");                // skip when no indent is pending
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // clear the need_indent flag before emitting
    emitter.instruction("jmp __rt_json_pretty_emit_indent_body");               // emit the newline and indent

    emitter.label("__rt_json_pretty_emit_indent_force");
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // clear the need_indent flag for safety on the forced path
    // fall through to the common emission body.
    emitter.label("__rt_json_pretty_emit_indent_body");
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the scratch write pointer
    emitter.instruction("mov BYTE PTR [r11], 10");                              // emit the newline
    emitter.instruction("add r11, 1");                                          // advance the scratch write pointer past the newline
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the depth counter for the indent calculation
    emitter.instruction("shl r9, 2");                                           // depth * 4 → number of spaces to emit
    emitter.instruction("xor rsi, rsi");                                        // initialize the space-emission counter
    emitter.label("__rt_json_pretty_emit_indent_loop");
    emitter.instruction("cmp rsi, r9");                                         // have we written every indent space?
    emitter.instruction("jge __rt_json_pretty_emit_indent_save");               // exit the indent loop once finished
    emitter.instruction("mov BYTE PTR [r11], 32");                              // emit a single indent space
    emitter.instruction("add r11, 1");                                          // advance the scratch write pointer past the space
    emitter.instruction("add rsi, 1");                                          // increment the space-emission counter
    emitter.instruction("jmp __rt_json_pretty_emit_indent_loop");               // continue emitting indent spaces
    emitter.label("__rt_json_pretty_emit_indent_save");
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // save the updated scratch write pointer after the indent
    emitter.label("__rt_json_pretty_emit_indent_done");
    emitter.instruction("ret");                                                 // return to the caller within the pretty-printer

    // Finalization: copy the scratch buffer back over the source slice and
    // republish concat_off so the caller sees a stable (ptr, len) pair.
    emitter.label("__rt_json_pretty_finalize");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source pointer (= destination)
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the scratch start pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the final scratch write pointer
    emitter.instruction("mov r9, r11");                                         // r9 = total pretty length (computed below)
    emitter.instruction("sub r9, r10");                                         // r9 = pretty_end - scratch_start
    emitter.instruction("xor rsi, rsi");                                        // initialize the copy index
    emitter.label("__rt_json_pretty_copy_back");
    emitter.instruction("cmp rsi, r9");                                         // have we copied every pretty byte back?
    emitter.instruction("jge __rt_json_pretty_publish");                        // exit the copy loop once finished
    emitter.instruction("movzx r8, BYTE PTR [r10 + rsi]");                      // load the next pretty byte from the scratch area
    emitter.instruction("mov BYTE PTR [rax + rsi], r8b");                       // store it at the original source offset
    emitter.instruction("add rsi, 1");                                          // advance the copy index
    emitter.instruction("jmp __rt_json_pretty_copy_back");                      // continue the copy loop
    emitter.label("__rt_json_pretty_publish");
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer
    emitter.instruction("mov r11, rax");                                        // r11 = source pointer
    emitter.instruction("sub r11, r10");                                        // compute the absolute concat-buffer offset for the source start
    emitter.instruction("add r11, r9");                                         // advance the offset by the pretty length
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r11");              // republish the concat-buffer offset past the pretty output
    emitter.instruction("mov rdx, r9");                                         // rdx = result len (the pretty length)
    // rax already holds the result ptr (= original source ptr).
    emitter.instruction("mov rsp, rbp");                                        // unwind the pretty-printer scratch frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the pretty (ptr, len) pair

    // Skip path: no flag set → return inputs unchanged.
    emitter.label("__rt_json_pretty_skip");
    emitter.instruction("mov rsp, rbp");                                        // unwind the pretty-printer scratch frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the compact slice unchanged
}
