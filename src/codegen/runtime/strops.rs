use crate::codegen::emit::Emitter;

/// strcopy: copy a string to concat_buf (for in-place modification).
/// Input:  x1=ptr, x2=len
/// Output: x1=new_ptr (in concat_buf), x2=len (unchanged)
pub fn emit_strcopy(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strcopy ---");
    emitter.label("__rt_strcopy");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- get concat_buf write position --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve exact address of offset variable
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset into concat_buf
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load page address of concat buffer
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve exact buffer base address
    emitter.instruction("add x9, x7, x8");                                      // compute destination: buf + offset

    // -- copy bytes from source to concat_buf --
    emitter.instruction("mov x10, x9");                                         // save destination start pointer
    emitter.instruction("mov x11, x2");                                         // copy length as loop counter
    emitter.label("__rt_strcopy_loop");
    emitter.instruction("cbz x11, __rt_strcopy_done");                          // if no bytes remain, done copying
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance source ptr
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to dest, advance dest ptr
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_strcopy_loop");                                 // continue copying

    // -- update concat_off and return new pointer --
    emitter.label("__rt_strcopy_done");
    emitter.instruction("add x8, x8, x2");                                      // advance offset by bytes copied
    emitter.instruction("str x8, [x6]");                                        // store updated offset to _concat_off
    emitter.instruction("mov x1, x10");                                         // return new pointer (start of copy)
    // x2 unchanged

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// strtolower: copy string to concat_buf, lowercasing A-Z.
/// Input:  x1=ptr, x2=len
/// Output: x1=new_ptr, x2=len
pub fn emit_strtolower(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtolower ---");
    emitter.label("__rt_strtolower");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- get concat_buf write position --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve exact address of offset variable
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load page address of concat buffer
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve exact buffer base address
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("mov x10, x9");                                         // save destination start for return value
    emitter.instruction("mov x11, x2");                                         // copy length as loop counter

    // -- copy bytes, converting uppercase to lowercase --
    emitter.label("__rt_strtolower_loop");
    emitter.instruction("cbz x11, __rt_strtolower_done");                       // if no bytes remain, done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance ptr
    emitter.instruction("cmp w12, #65");                                        // compare with 'A' (0x41)
    emitter.instruction("b.lt __rt_strtolower_store");                          // if below 'A', store unchanged
    emitter.instruction("cmp w12, #90");                                        // compare with 'Z' (0x5A)
    emitter.instruction("b.gt __rt_strtolower_store");                          // if above 'Z', store unchanged
    emitter.instruction("add w12, w12, #32");                                   // convert A-Z to a-z by adding 32
    emitter.label("__rt_strtolower_store");
    emitter.instruction("strb w12, [x9], #1");                                  // store (possibly lowered) byte, advance dest
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining count
    emitter.instruction("b __rt_strtolower_loop");                              // continue processing next byte

    // -- update concat_off and return --
    emitter.label("__rt_strtolower_done");
    emitter.instruction("add x8, x8, x2");                                      // advance offset by string length
    emitter.instruction("str x8, [x6]");                                        // store updated offset to _concat_off
    emitter.instruction("mov x1, x10");                                         // return new pointer (start of lowered copy)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// strtoupper: copy string to concat_buf, uppercasing a-z.
pub fn emit_strtoupper(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strtoupper ---");
    emitter.label("__rt_strtoupper");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- get concat_buf write position --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve exact address of offset variable
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load page address of concat buffer
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve exact buffer base address
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("mov x10, x9");                                         // save destination start for return value
    emitter.instruction("mov x11, x2");                                         // copy length as loop counter

    // -- copy bytes, converting lowercase to uppercase --
    emitter.label("__rt_strtoupper_loop");
    emitter.instruction("cbz x11, __rt_strtoupper_done");                       // if no bytes remain, done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance ptr
    emitter.instruction("cmp w12, #97");                                        // compare with 'a' (0x61)
    emitter.instruction("b.lt __rt_strtoupper_store");                          // if below 'a', store unchanged
    emitter.instruction("cmp w12, #122");                                       // compare with 'z' (0x7A)
    emitter.instruction("b.gt __rt_strtoupper_store");                          // if above 'z', store unchanged
    emitter.instruction("sub w12, w12, #32");                                   // convert a-z to A-Z by subtracting 32
    emitter.label("__rt_strtoupper_store");
    emitter.instruction("strb w12, [x9], #1");                                  // store (possibly uppered) byte, advance dest
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining count
    emitter.instruction("b __rt_strtoupper_loop");                              // continue processing next byte

    // -- update concat_off and return --
    emitter.label("__rt_strtoupper_done");
    emitter.instruction("add x8, x8, x2");                                      // advance offset by string length
    emitter.instruction("str x8, [x6]");                                        // store updated offset to _concat_off
    emitter.instruction("mov x1, x10");                                         // return new pointer (start of uppered copy)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// trim: strip whitespace from both ends. Returns adjusted ptr+len (no copy needed).
pub fn emit_trim(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: trim ---");
    // ltrim first, then rtrim
    emitter.label("__rt_trim");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- delegate to ltrim then rtrim --
    emitter.instruction("bl __rt_ltrim");                                       // strip leading whitespace (adjusts x1, x2)
    emitter.instruction("bl __rt_rtrim");                                       // strip trailing whitespace (adjusts x2)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// ltrim: strip whitespace from left. Adjusts x1 and x2.
pub fn emit_ltrim(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ltrim ---");
    emitter.label("__rt_ltrim");
    emitter.label("__rt_ltrim_loop");
    emitter.instruction("cbz x2, __rt_ltrim_done");                             // if string is empty, nothing to trim
    emitter.instruction("ldrb w9, [x1]");                                       // peek at first byte without advancing
    emitter.instruction("cmp w9, #32");                                         // check for space (0x20)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if space, skip it
    emitter.instruction("cmp w9, #9");                                          // check for tab (0x09)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if tab, skip it
    emitter.instruction("cmp w9, #10");                                         // check for newline (0x0A)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if newline, skip it
    emitter.instruction("cmp w9, #13");                                         // check for carriage return (0x0D)
    emitter.instruction("b.eq __rt_ltrim_skip");                                // if CR, skip it
    emitter.instruction("b __rt_ltrim_done");                                   // non-whitespace found, stop trimming

    // -- advance past whitespace character --
    emitter.label("__rt_ltrim_skip");
    emitter.instruction("add x1, x1, #1");                                      // advance string pointer past whitespace
    emitter.instruction("sub x2, x2, #1");                                      // decrement string length
    emitter.instruction("b __rt_ltrim_loop");                                   // check next character

    emitter.label("__rt_ltrim_done");
    emitter.instruction("ret");                                                 // return with adjusted x1 and x2
}

/// rtrim: strip whitespace from right. Adjusts x2.
pub fn emit_rtrim(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: rtrim ---");
    emitter.label("__rt_rtrim");
    emitter.label("__rt_rtrim_loop");
    emitter.instruction("cbz x2, __rt_rtrim_done");                             // if string is empty, nothing to trim
    emitter.instruction("sub x9, x2, #1");                                      // compute index of last character
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load last byte of string
    emitter.instruction("cmp w10, #32");                                        // check for space (0x20)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if space, strip it
    emitter.instruction("cmp w10, #9");                                         // check for tab (0x09)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if tab, strip it
    emitter.instruction("cmp w10, #10");                                        // check for newline (0x0A)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if newline, strip it
    emitter.instruction("cmp w10, #13");                                        // check for carriage return (0x0D)
    emitter.instruction("b.eq __rt_rtrim_strip");                               // if CR, strip it
    emitter.instruction("b __rt_rtrim_done");                                   // non-whitespace found, stop trimming

    // -- shrink length to strip trailing whitespace --
    emitter.label("__rt_rtrim_strip");
    emitter.instruction("sub x2, x2, #1");                                      // reduce length by 1 (removes last char)
    emitter.instruction("b __rt_rtrim_loop");                                   // check new last character

    emitter.label("__rt_rtrim_done");
    emitter.instruction("ret");                                                 // return with adjusted x2
}

/// strpos: find needle in haystack. Returns position in x0, or -1 if not found.
/// Input: x1=haystack_ptr, x2=haystack_len, x3=needle_ptr, x4=needle_len
/// Output: x0 = position (or -1)
pub fn emit_strpos(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strpos ---");
    emitter.label("__rt_strpos");

    // -- edge cases --
    emitter.instruction("cbz x4, __rt_strpos_empty");                           // empty needle always matches at position 0
    emitter.instruction("cmp x4, x2");                                          // compare needle length with haystack length
    emitter.instruction("b.gt __rt_strpos_notfound");                           // needle longer than haystack, can't match
    emitter.instruction("mov x5, #0");                                          // initialize search position to 0

    // -- outer loop: try matching needle at each position --
    emitter.label("__rt_strpos_outer");
    emitter.instruction("sub x9, x2, x4");                                      // last valid start = haystack_len - needle_len
    emitter.instruction("cmp x5, x9");                                          // check if position exceeds last valid start
    emitter.instruction("b.gt __rt_strpos_notfound");                           // past end, needle not found

    // -- inner loop: compare needle bytes at current position --
    emitter.instruction("mov x6, #0");                                          // needle comparison index = 0
    emitter.label("__rt_strpos_inner");
    emitter.instruction("cmp x6, x4");                                          // check if all needle bytes matched
    emitter.instruction("b.ge __rt_strpos_found");                              // all matched, found at position x5
    emitter.instruction("add x7, x5, x6");                                      // compute haystack index = pos + needle_idx
    emitter.instruction("ldrb w8, [x1, x7]");                                   // load haystack byte at computed index
    emitter.instruction("ldrb w9, [x3, x6]");                                   // load needle byte at current index
    emitter.instruction("cmp w8, w9");                                          // compare haystack and needle bytes
    emitter.instruction("b.ne __rt_strpos_next");                               // mismatch, try next position
    emitter.instruction("add x6, x6, #1");                                      // advance needle index
    emitter.instruction("b __rt_strpos_inner");                                 // continue comparing

    // -- advance to next haystack position --
    emitter.label("__rt_strpos_next");
    emitter.instruction("add x5, x5, #1");                                      // increment search position
    emitter.instruction("b __rt_strpos_outer");                                 // retry from new position

    // -- return results --
    emitter.label("__rt_strpos_found");
    emitter.instruction("mov x0, x5");                                          // return match position
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_strpos_empty");
    emitter.instruction("mov x0, #0");                                          // empty needle found at position 0
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_strpos_notfound");
    emitter.instruction("mov x0, #-1");                                         // return -1 (not found)
    emitter.instruction("ret");                                                 // return to caller
}

/// strrpos: find last occurrence of needle. Returns position or -1.
pub fn emit_strrpos(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strrpos ---");
    emitter.label("__rt_strrpos");

    // -- edge cases --
    emitter.instruction("cbz x4, __rt_strrpos_empty");                          // empty needle returns last position
    emitter.instruction("cmp x4, x2");                                          // compare needle length with haystack length
    emitter.instruction("b.gt __rt_strrpos_notfound");                          // needle longer than haystack, can't match
    emitter.instruction("sub x5, x2, x4");                                      // start searching from rightmost valid position

    // -- outer loop: try matching needle from right to left --
    emitter.label("__rt_strrpos_outer");
    emitter.instruction("mov x6, #0");                                          // reset needle comparison index
    emitter.label("__rt_strrpos_inner");
    emitter.instruction("cmp x6, x4");                                          // check if all needle bytes matched
    emitter.instruction("b.ge __rt_strrpos_found");                             // all matched, found at position x5
    emitter.instruction("add x7, x5, x6");                                      // compute haystack index = pos + needle_idx
    emitter.instruction("ldrb w8, [x1, x7]");                                   // load haystack byte at computed index
    emitter.instruction("ldrb w9, [x3, x6]");                                   // load needle byte at current index
    emitter.instruction("cmp w8, w9");                                          // compare haystack and needle bytes
    emitter.instruction("b.ne __rt_strrpos_prev");                              // mismatch, try previous position
    emitter.instruction("add x6, x6, #1");                                      // advance needle index
    emitter.instruction("b __rt_strrpos_inner");                                // continue comparing

    // -- move to previous position (searching right to left) --
    emitter.label("__rt_strrpos_prev");
    emitter.instruction("cbz x5, __rt_strrpos_notfound");                       // if at position 0, nowhere left to search
    emitter.instruction("sub x5, x5, #1");                                      // decrement search position
    emitter.instruction("b __rt_strrpos_outer");                                // retry from new position

    // -- return results --
    emitter.label("__rt_strrpos_found");
    emitter.instruction("mov x0, x5");                                          // return last match position
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_strrpos_empty");
    emitter.instruction("sub x0, x2, #0");                                      // empty needle returns haystack length
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_strrpos_notfound");
    emitter.instruction("mov x0, #-1");                                         // return -1 (not found)
    emitter.instruction("ret");                                                 // return to caller
}

/// str_repeat: repeat a string N times into concat_buf.
/// Input: x1=ptr, x2=len, x3=times
/// Output: x1=result_ptr, x2=result_len
pub fn emit_str_repeat(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_repeat ---");
    emitter.label("__rt_str_repeat");

    // -- set up stack frame (48 bytes) --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save source pointer and length
    emitter.instruction("str x3, [sp, #16]");                                   // save repetition count

    // -- get concat_buf write position --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve exact address of offset variable
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load page address of concat buffer
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve exact buffer base address
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("str x9, [sp, #24]");                                   // save result start pointer

    // -- outer loop: repeat N times --
    emitter.instruction("mov x10, x3");                                         // initialize repetition counter
    emitter.label("__rt_str_repeat_loop");
    emitter.instruction("cbz x10, __rt_str_repeat_done");                       // if counter is 0, done repeating
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload source pointer and length
    emitter.instruction("mov x11, x2");                                         // copy length as inner loop counter

    // -- inner loop: copy one instance of the string --
    emitter.label("__rt_str_repeat_copy");
    emitter.instruction("cbz x11, __rt_str_repeat_next");                       // if no bytes remain, move to next repetition
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance src ptr
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to dest, advance dest ptr
    emitter.instruction("sub x11, x11, #1");                                    // decrement inner byte counter
    emitter.instruction("b __rt_str_repeat_copy");                              // continue copying bytes
    emitter.label("__rt_str_repeat_next");
    emitter.instruction("sub x10, x10, #1");                                    // decrement repetition counter
    emitter.instruction("b __rt_str_repeat_loop");                              // continue to next repetition

    // -- finalize: compute result length and update concat_off --
    emitter.label("__rt_str_repeat_done");
    emitter.instruction("ldr x1, [sp, #24]");                                   // load result start pointer
    emitter.instruction("sub x2, x9, x1");                                      // result length = dest_end - dest_start
    emitter.instruction("ldr x8, [x6]");                                        // reload current concat_off
    emitter.instruction("add x8, x8, x2");                                      // advance offset by result length
    emitter.instruction("str x8, [x6]");                                        // store updated concat_off

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// strrev: reverse a string into concat_buf.
pub fn emit_strrev(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strrev ---");
    emitter.label("__rt_strrev");

    // -- get concat_buf write position --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve exact address of offset variable
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load page address of concat buffer
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve exact buffer base address
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("mov x10, x9");                                         // save destination start for return value
    emitter.instruction("add x11, x1, x2");                                     // x11 = pointer to end of source string
    emitter.instruction("mov x12, x2");                                         // copy length as loop counter

    // -- copy bytes in reverse order (last-to-first) --
    emitter.label("__rt_strrev_loop");
    emitter.instruction("cbz x12, __rt_strrev_done");                           // if no bytes remain, done reversing
    emitter.instruction("sub x11, x11, #1");                                    // move source pointer backward (from end)
    emitter.instruction("ldrb w13, [x11]");                                     // load byte from current source position
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest (forward order), advance dest
    emitter.instruction("sub x12, x12, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_strrev_loop");                                  // continue reversing

    // -- update concat_off and return --
    emitter.label("__rt_strrev_done");
    emitter.instruction("add x8, x8, x2");                                      // advance offset by string length
    emitter.instruction("str x8, [x6]");                                        // store updated offset to _concat_off
    emitter.instruction("mov x1, x10");                                         // return pointer to reversed string
    // x2 unchanged
    emitter.instruction("ret");                                                 // return to caller
}

/// chr: convert int to single-character string.
/// Input: x0 = char code
/// Output: x1 = ptr, x2 = 1
pub fn emit_chr(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: chr ---");
    emitter.label("__rt_chr");

    // -- get concat_buf write position --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve exact address of offset variable
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load page address of concat buffer
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve exact buffer base address

    // -- store single character --
    emitter.instruction("add x1, x7, x8");                                      // compute write position, set as return ptr
    emitter.instruction("strb w0, [x1]");                                       // store the character byte at that position
    emitter.instruction("add x8, x8, #1");                                      // advance offset by 1 byte
    emitter.instruction("str x8, [x6]");                                        // store updated offset to _concat_off
    emitter.instruction("mov x2, #1");                                          // return length = 1 (single character)
    emitter.instruction("ret");                                                 // return to caller
}

/// strcmp: compare two strings lexicographically.
/// Input: x1/x2 = str_a, x3/x4 = str_b
/// Output: x0 = <0, 0, or >0
pub fn emit_strcmp(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strcmp ---");
    emitter.label("__rt_strcmp");

    // -- determine minimum length for comparison --
    emitter.instruction("cmp x2, x4");                                          // compare lengths of both strings
    emitter.instruction("csel x5, x2, x4, lt");                                 // x5 = min(len_a, len_b)
    emitter.instruction("mov x6, #0");                                          // initialize byte index to 0

    // -- compare bytes up to minimum length --
    emitter.label("__rt_strcmp_loop");
    emitter.instruction("cmp x6, x5");                                          // check if we've compared all min-length bytes
    emitter.instruction("b.ge __rt_strcmp_len");                                // if done, compare by string lengths
    emitter.instruction("ldrb w7, [x1, x6]");                                   // load byte from string A at index
    emitter.instruction("ldrb w8, [x3, x6]");                                   // load byte from string B at index
    emitter.instruction("cmp w7, w8");                                          // compare the two bytes
    emitter.instruction("b.ne __rt_strcmp_diff");                               // if different, return their difference
    emitter.instruction("add x6, x6, #1");                                      // advance to next byte
    emitter.instruction("b __rt_strcmp_loop");                                  // continue comparing

    // -- bytes differ: return difference --
    emitter.label("__rt_strcmp_diff");
    emitter.instruction("sub x0, x7, x8");                                      // return char_a - char_b (negative, 0, or positive)
    emitter.instruction("ret");                                                 // return to caller

    // -- all shared bytes equal: compare by length --
    emitter.label("__rt_strcmp_len");
    emitter.instruction("sub x0, x2, x4");                                      // return len_a - len_b as tiebreaker
    emitter.instruction("ret");                                                 // return to caller
}

/// strcasecmp: case-insensitive string comparison.
pub fn emit_strcasecmp(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strcasecmp ---");
    emitter.label("__rt_strcasecmp");

    // -- determine minimum length for comparison --
    emitter.instruction("cmp x2, x4");                                          // compare lengths of both strings
    emitter.instruction("csel x5, x2, x4, lt");                                 // x5 = min(len_a, len_b)
    emitter.instruction("mov x6, #0");                                          // initialize byte index to 0

    // -- compare bytes (case-insensitive) --
    emitter.label("__rt_strcasecmp_loop");
    emitter.instruction("cmp x6, x5");                                          // check if we've compared all min-length bytes
    emitter.instruction("b.ge __rt_strcasecmp_len");                            // if done, compare by string lengths
    emitter.instruction("ldrb w7, [x1, x6]");                                   // load byte from string A at index
    emitter.instruction("ldrb w8, [x3, x6]");                                   // load byte from string B at index

    // -- convert byte A to lowercase if uppercase --
    emitter.instruction("cmp w7, #65");                                         // compare with 'A'
    emitter.instruction("b.lt __rt_strcasecmp_b");                              // if below 'A', skip conversion
    emitter.instruction("cmp w7, #90");                                         // compare with 'Z'
    emitter.instruction("b.gt __rt_strcasecmp_b");                              // if above 'Z', skip conversion
    emitter.instruction("add w7, w7, #32");                                     // convert A-Z to a-z

    // -- convert byte B to lowercase if uppercase --
    emitter.label("__rt_strcasecmp_b");
    emitter.instruction("cmp w8, #65");                                         // compare with 'A'
    emitter.instruction("b.lt __rt_strcasecmp_cmp");                            // if below 'A', skip conversion
    emitter.instruction("cmp w8, #90");                                         // compare with 'Z'
    emitter.instruction("b.gt __rt_strcasecmp_cmp");                            // if above 'Z', skip conversion
    emitter.instruction("add w8, w8, #32");                                     // convert A-Z to a-z

    // -- compare lowered bytes --
    emitter.label("__rt_strcasecmp_cmp");
    emitter.instruction("cmp w7, w8");                                          // compare the two lowered bytes
    emitter.instruction("b.ne __rt_strcasecmp_diff");                           // if different, return their difference
    emitter.instruction("add x6, x6, #1");                                      // advance to next byte
    emitter.instruction("b __rt_strcasecmp_loop");                              // continue comparing

    // -- bytes differ: return difference --
    emitter.label("__rt_strcasecmp_diff");
    emitter.instruction("sub x0, x7, x8");                                      // return lowered_a - lowered_b
    emitter.instruction("ret");                                                 // return to caller

    // -- all shared bytes equal: compare by length --
    emitter.label("__rt_strcasecmp_len");
    emitter.instruction("sub x0, x2, x4");                                      // return len_a - len_b as tiebreaker
    emitter.instruction("ret");                                                 // return to caller
}

/// str_starts_with: check if haystack starts with needle.
/// Input: x1/x2=haystack, x3/x4=needle
/// Output: x0 = 1 if starts with, 0 otherwise
pub fn emit_str_starts_with(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_starts_with ---");
    emitter.label("__rt_str_starts_with");

    // -- check if needle fits in haystack --
    emitter.instruction("cmp x4, x2");                                          // compare needle length with haystack length
    emitter.instruction("b.gt __rt_str_starts_with_no");                        // needle longer than haystack, can't match
    emitter.instruction("mov x5, #0");                                          // initialize comparison index

    // -- compare prefix bytes --
    emitter.label("__rt_str_starts_with_loop");
    emitter.instruction("cmp x5, x4");                                          // check if all needle bytes compared
    emitter.instruction("b.ge __rt_str_starts_with_yes");                       // all matched, haystack starts with needle
    emitter.instruction("ldrb w6, [x1, x5]");                                   // load haystack byte at index
    emitter.instruction("ldrb w7, [x3, x5]");                                   // load needle byte at index
    emitter.instruction("cmp w6, w7");                                          // compare the two bytes
    emitter.instruction("b.ne __rt_str_starts_with_no");                        // mismatch, does not start with needle
    emitter.instruction("add x5, x5, #1");                                      // advance to next byte
    emitter.instruction("b __rt_str_starts_with_loop");                         // continue comparing

    // -- return results --
    emitter.label("__rt_str_starts_with_yes");
    emitter.instruction("mov x0, #1");                                          // return 1 (true: starts with needle)
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_str_starts_with_no");
    emitter.instruction("mov x0, #0");                                          // return 0 (false: does not start with)
    emitter.instruction("ret");                                                 // return to caller
}

/// str_ends_with: check if haystack ends with needle.
pub fn emit_str_ends_with(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_ends_with ---");
    emitter.label("__rt_str_ends_with");

    // -- check if needle fits in haystack --
    emitter.instruction("cmp x4, x2");                                          // compare needle length with haystack length
    emitter.instruction("b.gt __rt_str_ends_with_no");                          // needle longer than haystack, can't match
    emitter.instruction("sub x5, x2, x4");                                      // compute offset where suffix starts
    emitter.instruction("mov x6, #0");                                          // initialize comparison index

    // -- compare suffix bytes --
    emitter.label("__rt_str_ends_with_loop");
    emitter.instruction("cmp x6, x4");                                          // check if all needle bytes compared
    emitter.instruction("b.ge __rt_str_ends_with_yes");                         // all matched, haystack ends with needle
    emitter.instruction("add x7, x5, x6");                                      // compute haystack index = offset + idx
    emitter.instruction("ldrb w8, [x1, x7]");                                   // load haystack byte at suffix position
    emitter.instruction("ldrb w9, [x3, x6]");                                   // load needle byte at index
    emitter.instruction("cmp w8, w9");                                          // compare the two bytes
    emitter.instruction("b.ne __rt_str_ends_with_no");                          // mismatch, does not end with needle
    emitter.instruction("add x6, x6, #1");                                      // advance to next byte
    emitter.instruction("b __rt_str_ends_with_loop");                           // continue comparing

    // -- return results --
    emitter.label("__rt_str_ends_with_yes");
    emitter.instruction("mov x0, #1");                                          // return 1 (true: ends with needle)
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_str_ends_with_no");
    emitter.instruction("mov x0, #0");                                          // return 0 (false: does not end with)
    emitter.instruction("ret");                                                 // return to caller
}

/// str_replace: replace all occurrences of search with replace in subject.
/// Input: x1/x2=search, x3/x4=replace, x5/x6=subject
/// Output: x1=result_ptr, x2=result_len (in concat_buf)
pub fn emit_str_replace(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_replace ---");
    emitter.label("__rt_str_replace");

    // -- set up stack frame (80 bytes) --
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer

    // -- save input arguments to stack --
    emitter.instruction("stp x1, x2, [sp]");                                    // save search string ptr and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save replacement string ptr and length
    emitter.instruction("stp x5, x6, [sp, #32]");                               // save subject string ptr and length

    // -- get concat_buf destination --
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve exact address of offset variable
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                          // load page address of concat buffer
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                   // resolve exact buffer base address
    emitter.instruction("add x12, x11, x10");                                   // compute destination pointer
    emitter.instruction("str x12, [sp, #48]");                                  // save result start pointer
    emitter.instruction("str x9, [sp, #56]");                                   // save offset variable address

    // -- initialize subject scan index --
    emitter.instruction("mov x13, #0");                                         // subject index = 0

    // -- main loop: scan subject for search string --
    emitter.label("__rt_str_replace_loop");
    emitter.instruction("ldp x5, x6, [sp, #32]");                               // reload subject ptr and length
    emitter.instruction("cmp x13, x6");                                         // check if past end of subject
    emitter.instruction("b.ge __rt_str_replace_done");                          // if done, finalize result

    // -- check if search string matches at current position --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload search ptr and length
    emitter.instruction("cbz x2, __rt_str_replace_copy_byte");                  // empty search = never matches, copy byte
    emitter.instruction("sub x14, x6, x13");                                    // remaining = subject_len - current_pos
    emitter.instruction("cmp x2, x14");                                         // check if search fits in remaining
    emitter.instruction("b.gt __rt_str_replace_copy_byte");                     // search longer than remaining, copy byte

    // -- compare search string at current position --
    emitter.instruction("mov x15, #0");                                         // match comparison index = 0
    emitter.label("__rt_str_replace_match");
    emitter.instruction("cmp x15, x2");                                         // check if all search bytes matched
    emitter.instruction("b.ge __rt_str_replace_found");                         // full match found
    emitter.instruction("add x16, x13, x15");                                   // compute subject index = pos + match_idx
    emitter.instruction("ldrb w17, [x5, x16]");                                 // load subject byte at computed index
    emitter.instruction("ldrb w18, [x1, x15]");                                 // load search byte at match index
    emitter.instruction("cmp w17, w18");                                        // compare subject and search bytes
    emitter.instruction("b.ne __rt_str_replace_copy_byte");                     // mismatch, just copy current byte
    emitter.instruction("add x15, x15, #1");                                    // advance match index
    emitter.instruction("b __rt_str_replace_match");                            // continue matching

    // -- match found: copy replacement string --
    emitter.label("__rt_str_replace_found");
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload replacement ptr and length
    emitter.instruction("mov x15, #0");                                         // replacement copy index = 0
    emitter.label("__rt_str_replace_rep_copy");
    emitter.instruction("cmp x15, x4");                                         // check if all replacement bytes copied
    emitter.instruction("b.ge __rt_str_replace_rep_done");                      // done copying replacement
    emitter.instruction("ldrb w17, [x3, x15]");                                 // load replacement byte at index
    emitter.instruction("strb w17, [x12], #1");                                 // store to dest, advance dest ptr
    emitter.instruction("add x15, x15, #1");                                    // advance replacement index
    emitter.instruction("b __rt_str_replace_rep_copy");                         // continue copying replacement
    emitter.label("__rt_str_replace_rep_done");
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload search ptr and length
    emitter.instruction("add x13, x13, x2");                                    // skip past matched search in subject
    emitter.instruction("b __rt_str_replace_loop");                             // continue scanning subject

    // -- no match: copy single byte from subject --
    emitter.label("__rt_str_replace_copy_byte");
    emitter.instruction("ldp x5, x6, [sp, #32]");                               // reload subject ptr and length
    emitter.instruction("ldrb w17, [x5, x13]");                                 // load subject byte at current position
    emitter.instruction("strb w17, [x12], #1");                                 // store to dest, advance dest ptr
    emitter.instruction("add x13, x13, #1");                                    // advance subject index by 1
    emitter.instruction("b __rt_str_replace_loop");                             // continue scanning

    // -- finalize: compute result length and update concat_off --
    emitter.label("__rt_str_replace_done");
    emitter.instruction("ldr x1, [sp, #48]");                                   // load result start pointer
    emitter.instruction("sub x2, x12, x1");                                     // result length = dest_end - dest_start
    emitter.instruction("ldr x9, [sp, #56]");                                   // load offset variable address
    emitter.instruction("ldr x10, [x9]");                                       // load current concat_off
    emitter.instruction("add x10, x10, x2");                                    // advance offset by result length
    emitter.instruction("str x10, [x9]");                                       // store updated concat_off

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// explode: split string by delimiter into array of strings.
/// Input: x1/x2=delimiter, x3/x4=string
/// Output: x0 = array pointer
pub fn emit_explode(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: explode ---");
    emitter.label("__rt_explode");

    // -- set up stack frame (80 bytes) --
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save delimiter ptr and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save input string ptr and length

    // -- create a new string array --
    emitter.instruction("mov x0, #16");                                         // initial array capacity = 16 elements
    emitter.instruction("mov x1, #16");                                         // element size = 16 bytes (ptr + len)
    emitter.instruction("bl __rt_array_new");                                   // call array constructor, returns array in x0
    emitter.instruction("str x0, [sp, #32]");                                   // save array pointer on stack

    // -- initialize scan state --
    emitter.instruction("mov x13, #0");                                         // current scan position = 0
    emitter.instruction("str x13, [sp, #40]");                                  // save current scan position
    emitter.instruction("str x13, [sp, #48]");                                  // segment start = 0

    // -- main loop: scan for delimiter occurrences --
    emitter.label("__rt_explode_loop");
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload string ptr and length
    emitter.instruction("ldr x13, [sp, #40]");                                  // reload current scan position
    emitter.instruction("cmp x13, x4");                                         // check if past end of string
    emitter.instruction("b.ge __rt_explode_last");                              // if done, push final segment

    // -- check if delimiter fits at current position --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload delimiter ptr and length
    emitter.instruction("sub x14, x4, x13");                                    // remaining = string_len - scan_pos
    emitter.instruction("cmp x2, x14");                                         // check if delimiter fits in remaining
    emitter.instruction("b.gt __rt_explode_last");                              // delimiter longer than remaining, done

    // -- compare delimiter at current position --
    emitter.instruction("mov x15, #0");                                         // delimiter comparison index = 0
    emitter.label("__rt_explode_cmp");
    emitter.instruction("cmp x15, x2");                                         // check if all delimiter bytes matched
    emitter.instruction("b.ge __rt_explode_match");                             // full match, delimiter found
    emitter.instruction("add x16, x13, x15");                                   // compute string index = scan_pos + cmp_idx
    emitter.instruction("ldrb w17, [x3, x16]");                                 // load string byte at computed index
    emitter.instruction("ldrb w18, [x1, x15]");                                 // load delimiter byte at cmp index
    emitter.instruction("cmp w17, w18");                                        // compare string and delimiter bytes
    emitter.instruction("b.ne __rt_explode_advance");                           // mismatch, advance by 1
    emitter.instruction("add x15, x15, #1");                                    // advance delimiter index
    emitter.instruction("b __rt_explode_cmp");                                  // continue comparing

    // -- no match: advance scan position by 1 --
    emitter.label("__rt_explode_advance");
    emitter.instruction("add x13, x13, #1");                                    // move scan position forward by 1
    emitter.instruction("str x13, [sp, #40]");                                  // save updated scan position
    emitter.instruction("b __rt_explode_loop");                                 // continue scanning

    // -- delimiter found: push segment before it to array --
    emitter.label("__rt_explode_match");
    emitter.instruction("ldr x0, [sp, #32]");                                   // load array pointer
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload string ptr and length
    emitter.instruction("ldr x16, [sp, #48]");                                  // load segment start position
    emitter.instruction("add x1, x3, x16");                                     // segment ptr = string + segment_start
    emitter.instruction("sub x2, x13, x16");                                    // segment len = scan_pos - segment_start
    emitter.instruction("bl __rt_array_push_str");                              // push segment string to array

    // -- advance past delimiter, update segment start --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload delimiter ptr and length
    emitter.instruction("ldr x13, [sp, #40]");                                  // reload scan position
    emitter.instruction("add x13, x13, x2");                                    // skip past delimiter
    emitter.instruction("str x13, [sp, #40]");                                  // save new scan position
    emitter.instruction("str x13, [sp, #48]");                                  // update segment start to after delimiter
    emitter.instruction("b __rt_explode_loop");                                 // continue scanning

    // -- push final segment (from last delimiter to end of string) --
    emitter.label("__rt_explode_last");
    emitter.instruction("ldr x0, [sp, #32]");                                   // load array pointer
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload string ptr and length
    emitter.instruction("ldr x16, [sp, #48]");                                  // load segment start position
    emitter.instruction("add x1, x3, x16");                                     // segment ptr = string + segment_start
    emitter.instruction("sub x2, x4, x16");                                     // segment len = string_len - segment_start
    emitter.instruction("bl __rt_array_push_str");                              // push final segment to array

    // -- return array and restore frame --
    emitter.instruction("ldr x0, [sp, #32]");                                   // return array pointer in x0
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// implode: join array elements with glue string.
/// Input: x1/x2=glue, x3=array_ptr
/// Output: x1=result_ptr, x2=result_len
pub fn emit_implode(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: implode ---");
    emitter.label("__rt_implode");

    // -- set up stack frame (64 bytes) --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save glue string ptr and length
    emitter.instruction("str x3, [sp, #16]");                                   // save array pointer

    // -- get concat_buf write position --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve exact address of offset variable
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load page address of concat buffer
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve exact buffer base address
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("str x9, [sp, #24]");                                   // save result start pointer
    emitter.instruction("str x6, [sp, #32]");                                   // save offset variable address

    // -- load array length and initialize index --
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload array pointer
    emitter.instruction("ldr x10, [x3]");                                       // load array element count
    emitter.instruction("mov x11, #0");                                         // initialize element index = 0

    // -- main loop: join elements with glue --
    emitter.label("__rt_implode_loop");
    emitter.instruction("cmp x11, x10");                                        // check if all elements processed
    emitter.instruction("b.ge __rt_implode_done");                              // if done, finalize result

    // -- insert glue before element (skip for first element) --
    emitter.instruction("cbz x11, __rt_implode_elem");                          // skip glue before first element
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload glue ptr and length
    emitter.instruction("mov x12, x2");                                         // copy glue length as counter
    emitter.label("__rt_implode_glue");
    emitter.instruction("cbz x12, __rt_implode_elem");                          // if no glue bytes remain, copy element
    emitter.instruction("ldrb w13, [x1], #1");                                  // load glue byte, advance glue ptr
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest, advance dest ptr
    emitter.instruction("sub x12, x12, #1");                                    // decrement glue byte counter
    emitter.instruction("b __rt_implode_glue");                                 // continue copying glue

    // -- copy current array element --
    emitter.label("__rt_implode_elem");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload array pointer
    emitter.instruction("lsl x12, x11, #4");                                    // compute byte offset: index * 16
    emitter.instruction("add x12, x3, x12");                                    // add to array base
    emitter.instruction("add x12, x12, #24");                                   // skip 24-byte array header
    emitter.instruction("ldr x1, [x12]");                                       // load element string pointer
    emitter.instruction("ldr x2, [x12, #8]");                                   // load element string length

    // -- copy element bytes to output --
    emitter.instruction("mov x12, x2");                                         // copy element length as counter
    emitter.label("__rt_implode_copy");
    emitter.instruction("cbz x12, __rt_implode_next");                          // if no bytes remain, move to next element
    emitter.instruction("ldrb w13, [x1], #1");                                  // load element byte, advance src ptr
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest, advance dest ptr
    emitter.instruction("sub x12, x12, #1");                                    // decrement byte counter
    emitter.instruction("b __rt_implode_copy");                                 // continue copying element

    // -- advance to next element --
    emitter.label("__rt_implode_next");
    emitter.instruction("add x11, x11, #1");                                    // increment element index
    emitter.instruction("b __rt_implode_loop");                                 // process next element

    // -- finalize: compute result length and update concat_off --
    emitter.label("__rt_implode_done");
    emitter.instruction("ldr x1, [sp, #24]");                                   // load result start pointer
    emitter.instruction("sub x2, x9, x1");                                      // result length = dest_end - dest_start
    emitter.instruction("ldr x6, [sp, #32]");                                   // load offset variable address
    emitter.instruction("ldr x8, [x6]");                                        // load current concat_off
    emitter.instruction("add x8, x8, x2");                                      // advance offset by result length
    emitter.instruction("str x8, [x6]");                                        // store updated concat_off

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
