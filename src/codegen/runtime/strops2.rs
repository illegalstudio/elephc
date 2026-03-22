use crate::codegen::emit::Emitter;

/// ucwords: uppercase first letter of each word (after whitespace).
/// Input: x1=ptr, x2=len. Output: x1=new_ptr, x2=len.
pub fn emit_ucwords(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ucwords ---");
    emitter.label("__rt_ucwords");
    emitter.instruction("sub sp, sp, #16");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp]");                                 // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // set frame pointer
    emitter.instruction("bl __rt_strcopy");                                     // copy string to mutable concat_buf
    emitter.instruction("cbz x2, __rt_ucwords_done");                           // empty string → nothing to do
    emitter.instruction("mov x9, x1");                                          // cursor pointer
    emitter.instruction("mov x10, x2");                                         // remaining length
    emitter.instruction("mov x11, #1");                                         // word_start flag (1 = next char starts a word)

    emitter.label("__rt_ucwords_loop");
    emitter.instruction("cbz x10, __rt_ucwords_done");                          // no bytes left → done
    emitter.instruction("ldrb w12, [x9]");                                      // load current byte
    // -- check if current char is whitespace --
    emitter.instruction("cmp w12, #32");                                        // space?
    emitter.instruction("b.eq __rt_ucwords_ws");                                // yes → mark next as word start
    emitter.instruction("cmp w12, #9");                                         // tab?
    emitter.instruction("b.eq __rt_ucwords_ws");                                // yes → mark next as word start
    emitter.instruction("cmp w12, #10");                                        // newline?
    emitter.instruction("b.eq __rt_ucwords_ws");                                // yes → mark next as word start
    // -- not whitespace: uppercase if word_start --
    emitter.instruction("cbz x11, __rt_ucwords_next");                          // not word start → skip uppercasing
    emitter.instruction("cmp w12, #97");                                        // check if char >= 'a'
    emitter.instruction("b.lt __rt_ucwords_clear");                             // not lowercase → just clear flag
    emitter.instruction("cmp w12, #122");                                       // check if char <= 'z'
    emitter.instruction("b.gt __rt_ucwords_clear");                             // not lowercase → just clear flag
    emitter.instruction("sub w12, w12, #32");                                   // convert a-z to A-Z
    emitter.instruction("strb w12, [x9]");                                      // store uppercased byte
    emitter.label("__rt_ucwords_clear");
    emitter.instruction("mov x11, #0");                                         // clear word_start flag
    emitter.instruction("b __rt_ucwords_next");                                 // advance to next char

    emitter.label("__rt_ucwords_ws");
    emitter.instruction("mov x11, #1");                                         // set word_start flag for next char

    emitter.label("__rt_ucwords_next");
    emitter.instruction("add x9, x9, #1");                                      // advance cursor
    emitter.instruction("sub x10, x10, #1");                                    // decrement remaining
    emitter.instruction("b __rt_ucwords_loop");                                 // process next byte

    emitter.label("__rt_ucwords_done");
    emitter.instruction("ldp x29, x30, [sp]");                                 // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x1/x2 from strcopy
}

/// str_ireplace: case-insensitive str_replace.
/// Input: x1/x2=search, x3/x4=replace, x5/x6=subject. Output: x1/x2=result.
pub fn emit_str_ireplace(emitter: &mut Emitter) {
    // Same as str_replace but uses case-insensitive comparison.
    emitter.blank();
    emitter.comment("--- runtime: str_ireplace ---");
    emitter.label("__rt_str_ireplace");
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                   // save search ptr/len
    emitter.instruction("stp x3, x4, [sp, #16]");                              // save replace ptr/len
    emitter.instruction("stp x5, x6, [sp, #32]");                              // save subject ptr/len

    // -- get concat_buf destination --
    emitter.instruction("adrp x9, _concat_off@PAGE");                          // load concat offset page
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                         // load concat buffer page
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                  // resolve address
    emitter.instruction("add x12, x11, x10");                                  // destination pointer
    emitter.instruction("str x12, [sp, #48]");                                  // save result start
    emitter.instruction("str x9, [sp, #56]");                                   // save offset variable ptr
    emitter.instruction("mov x13, #0");                                         // subject scan index

    emitter.label("__rt_sirepl_loop");
    emitter.instruction("ldp x5, x6, [sp, #32]");                              // reload subject
    emitter.instruction("cmp x13, x6");                                         // check if past end
    emitter.instruction("b.ge __rt_sirepl_done");                               // done scanning

    // -- case-insensitive match check --
    emitter.instruction("ldp x1, x2, [sp]");                                   // reload search
    emitter.instruction("cbz x2, __rt_sirepl_copy_byte");                      // empty search → no match
    emitter.instruction("sub x14, x6, x13");                                    // remaining in subject
    emitter.instruction("cmp x2, x14");                                         // search longer than remaining?
    emitter.instruction("b.gt __rt_sirepl_copy_byte");                          // yes → can't match

    emitter.instruction("mov x15, #0");                                         // match index
    emitter.label("__rt_sirepl_cmp");
    emitter.instruction("cmp x15, x2");                                         // compared all search chars?
    emitter.instruction("b.ge __rt_sirepl_found");                              // full match found

    emitter.instruction("add x16, x13, x15");                                  // subject position
    emitter.instruction("ldrb w17, [x5, x16]");                                // load subject byte
    emitter.instruction("ldrb w18, [x1, x15]");                                // load search byte
    // -- tolower both for comparison --
    emitter.instruction("cmp w17, #65");                                        // subject byte >= 'A'?
    emitter.instruction("b.lt 1f");                                             // skip if not
    emitter.instruction("cmp w17, #90");                                        // subject byte <= 'Z'?
    emitter.instruction("b.gt 1f");                                             // skip if not
    emitter.instruction("add w17, w17, #32");                                   // tolower subject byte
    emitter.raw("1:");
    emitter.instruction("cmp w18, #65");                                        // search byte >= 'A'?
    emitter.instruction("b.lt 2f");                                             // skip if not
    emitter.instruction("cmp w18, #90");                                        // search byte <= 'Z'?
    emitter.instruction("b.gt 2f");                                             // skip if not
    emitter.instruction("add w18, w18, #32");                                   // tolower search byte
    emitter.raw("2:");
    emitter.instruction("cmp w17, w18");                                        // compare lowered bytes
    emitter.instruction("b.ne __rt_sirepl_copy_byte");                          // mismatch → not a match
    emitter.instruction("add x15, x15, #1");                                    // advance match index
    emitter.instruction("b __rt_sirepl_cmp");                                   // continue matching

    emitter.label("__rt_sirepl_found");
    // -- copy replacement --
    emitter.instruction("ldp x3, x4, [sp, #16]");                              // reload replace
    emitter.instruction("mov x15, #0");                                         // replace copy index
    emitter.label("__rt_sirepl_rep");
    emitter.instruction("cmp x15, x4");                                         // all replacement bytes copied?
    emitter.instruction("b.ge __rt_sirepl_rep_done");                           // yes → advance past search
    emitter.instruction("ldrb w17, [x3, x15]");                                // load replacement byte
    emitter.instruction("strb w17, [x12], #1");                                // store to output, advance dest
    emitter.instruction("add x15, x15, #1");                                   // next replacement byte
    emitter.instruction("b __rt_sirepl_rep");                                   // continue
    emitter.label("__rt_sirepl_rep_done");
    emitter.instruction("ldp x1, x2, [sp]");                                   // reload search length
    emitter.instruction("add x13, x13, x2");                                    // skip past matched search in subject
    emitter.instruction("b __rt_sirepl_loop");                                  // continue scanning

    emitter.label("__rt_sirepl_copy_byte");
    emitter.instruction("ldp x5, x6, [sp, #32]");                              // reload subject
    emitter.instruction("ldrb w17, [x5, x13]");                                // load subject byte
    emitter.instruction("strb w17, [x12], #1");                                // copy to output
    emitter.instruction("add x13, x13, #1");                                   // advance subject index
    emitter.instruction("b __rt_sirepl_loop");                                  // continue

    emitter.label("__rt_sirepl_done");
    emitter.instruction("ldr x1, [sp, #48]");                                  // result start
    emitter.instruction("sub x2, x12, x1");                                     // result length
    emitter.instruction("ldr x9, [sp, #56]");                                  // offset variable ptr
    emitter.instruction("ldr x10, [x9]");                                       // current offset
    emitter.instruction("add x10, x10, x2");                                    // advance by result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset
    emitter.instruction("ldp x29, x30, [sp, #64]");                            // restore frame
    emitter.instruction("add sp, sp, #80");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}

/// substr_replace: replace portion of string.
/// Input: x1/x2=subject, x3/x4=replacement, x0=offset, x7=length (-1=to end).
/// Output: x1/x2=result.
pub fn emit_substr_replace(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: substr_replace ---");
    emitter.label("__rt_substr_replace");
    emitter.instruction("sub sp, sp, #16");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp]");                                 // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // set frame pointer

    // -- clamp offset --
    emitter.instruction("cmp x0, #0");                                          // check if offset is negative
    emitter.instruction("b.ge 1f");                                             // skip if non-negative
    emitter.instruction("add x0, x2, x0");                                      // offset = len + offset
    emitter.instruction("cmp x0, #0");                                          // clamp to 0
    emitter.instruction("csel x0, xzr, x0, lt");                               // if still negative, use 0
    emitter.raw("1:");
    emitter.instruction("cmp x0, x2");                                          // clamp offset to string length
    emitter.instruction("csel x0, x2, x0, gt");                                // min(offset, len)

    // -- compute replace length --
    emitter.instruction("cmn x7, #1");                                          // check if length == -1 (sentinel)
    emitter.instruction("b.ne 2f");                                             // if not sentinel, use given length
    emitter.instruction("sub x7, x2, x0");                                      // length = remaining from offset
    emitter.raw("2:");
    emitter.instruction("cmp x7, #0");                                          // clamp negative length to 0
    emitter.instruction("csel x7, xzr, x7, lt");                               // max(0, length)
    emitter.instruction("add x8, x0, x7");                                      // end = offset + length
    emitter.instruction("cmp x8, x2");                                          // clamp end to string length
    emitter.instruction("csel x8, x2, x8, gt");                                // min(end, len)

    // -- build result: prefix + replacement + suffix --
    emitter.instruction("adrp x9, _concat_off@PAGE");                          // load concat offset page
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                         // load concat buffer page
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                  // resolve address
    emitter.instruction("add x12, x11, x10");                                  // destination pointer
    emitter.instruction("mov x13, x12");                                        // save result start

    // -- copy prefix: subject[0..offset] --
    emitter.instruction("mov x14, #0");                                         // copy index
    emitter.label("__rt_subrepl_pre");
    emitter.instruction("cmp x14, x0");                                         // copied offset bytes?
    emitter.instruction("b.ge __rt_subrepl_mid");                               // yes → copy replacement
    emitter.instruction("ldrb w15, [x1, x14]");                                // load prefix byte
    emitter.instruction("strb w15, [x12], #1");                                // store and advance
    emitter.instruction("add x14, x14, #1");                                   // next byte
    emitter.instruction("b __rt_subrepl_pre");                                  // continue

    // -- copy replacement --
    emitter.label("__rt_subrepl_mid");
    emitter.instruction("mov x14, #0");                                         // replacement copy index
    emitter.label("__rt_subrepl_rep");
    emitter.instruction("cmp x14, x4");                                         // all replacement bytes copied?
    emitter.instruction("b.ge __rt_subrepl_suf");                               // yes → copy suffix
    emitter.instruction("ldrb w15, [x3, x14]");                                // load replacement byte
    emitter.instruction("strb w15, [x12], #1");                                // store and advance
    emitter.instruction("add x14, x14, #1");                                   // next byte
    emitter.instruction("b __rt_subrepl_rep");                                  // continue

    // -- copy suffix: subject[end..len] --
    emitter.label("__rt_subrepl_suf");
    emitter.instruction("mov x14, x8");                                         // start from end position
    emitter.label("__rt_subrepl_suf_loop");
    emitter.instruction("cmp x14, x2");                                         // past end of subject?
    emitter.instruction("b.ge __rt_subrepl_done");                              // yes → done
    emitter.instruction("ldrb w15, [x1, x14]");                                // load suffix byte
    emitter.instruction("strb w15, [x12], #1");                                // store and advance
    emitter.instruction("add x14, x14, #1");                                   // next byte
    emitter.instruction("b __rt_subrepl_suf_loop");                             // continue

    emitter.label("__rt_subrepl_done");
    emitter.instruction("mov x1, x13");                                         // result pointer
    emitter.instruction("sub x2, x12, x13");                                    // result length
    emitter.instruction("ldr x10, [x9]");                                       // reload current offset
    emitter.instruction("add x10, x10, x2");                                    // advance by result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset
    emitter.instruction("ldp x29, x30, [sp]");                                 // restore frame
    emitter.instruction("add sp, sp, #16");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}

/// str_pad: pad a string to a target length.
/// Input: x1/x2=input, x3/x4=pad_str, x5=target_len, x7=pad_type (0=left, 1=right, 2=both).
/// Output: x1/x2=result.
pub fn emit_str_pad(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_pad ---");
    emitter.label("__rt_str_pad");
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                   // save input string
    emitter.instruction("stp x3, x4, [sp, #16]");                              // save pad string
    emitter.instruction("str x5, [sp, #32]");                                   // save target length
    emitter.instruction("str x7, [sp, #40]");                                   // save pad type

    // -- if input already >= target, return as-is --
    emitter.instruction("cmp x2, x5");                                          // compare input len with target
    emitter.instruction("b.ge __rt_str_pad_noop");                              // already long enough → return copy

    // -- set up concat_buf destination --
    emitter.instruction("adrp x9, _concat_off@PAGE");                          // load concat offset page
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                         // load concat buffer page
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                  // resolve address
    emitter.instruction("add x12, x11, x10");                                  // destination pointer
    emitter.instruction("mov x13, x12");                                        // save result start

    emitter.instruction("sub x14, x5, x2");                                     // pad_needed = target - input_len
    emitter.instruction("ldr x7, [sp, #40]");                                  // reload pad_type

    // -- compute left_pad and right_pad amounts --
    emitter.instruction("cmp x7, #0");                                          // STR_PAD_LEFT?
    emitter.instruction("b.eq __rt_str_pad_left_all");                          // all padding on left
    emitter.instruction("cmp x7, #2");                                          // STR_PAD_BOTH?
    emitter.instruction("b.eq __rt_str_pad_both");                              // split padding
    // -- STR_PAD_RIGHT (default): all padding on right --
    emitter.instruction("mov x15, #0");                                         // left_pad = 0
    emitter.instruction("mov x16, x14");                                        // right_pad = all
    emitter.instruction("b __rt_str_pad_emit");                                 // start emitting

    emitter.label("__rt_str_pad_left_all");
    emitter.instruction("mov x15, x14");                                        // left_pad = all
    emitter.instruction("mov x16, #0");                                         // right_pad = 0
    emitter.instruction("b __rt_str_pad_emit");                                 // start emitting

    emitter.label("__rt_str_pad_both");
    emitter.instruction("lsr x15, x14, #1");                                    // left_pad = pad_needed / 2
    emitter.instruction("sub x16, x14, x15");                                   // right_pad = pad_needed - left_pad
    // fall through to emit

    // -- emit: left_pad chars, then input, then right_pad chars --
    emitter.label("__rt_str_pad_emit");
    // left padding
    emitter.instruction("mov x17, x15");                                        // left pad counter
    emitter.instruction("mov x18, #0");                                         // pad string index
    emitter.label("__rt_str_pad_lp");
    emitter.instruction("cbz x17, __rt_str_pad_input");                         // left padding done → copy input
    emitter.instruction("ldp x3, x4, [sp, #16]");                              // reload pad string
    emitter.instruction("ldrb w0, [x3, x18]");                                 // load pad char at index
    emitter.instruction("strb w0, [x12], #1");                                 // write to output
    emitter.instruction("sub x17, x17, #1");                                    // decrement left pad remaining
    emitter.instruction("add x18, x18, #1");                                   // advance pad index
    emitter.instruction("cmp x18, x4");                                         // wrap around if past pad string
    emitter.instruction("csel x18, xzr, x18, ge");                             // reset to 0 if >= pad_len
    emitter.instruction("b __rt_str_pad_lp");                                   // continue

    // copy input
    emitter.label("__rt_str_pad_input");
    emitter.instruction("ldp x1, x2, [sp]");                                   // reload input string
    emitter.instruction("mov x17, x2");                                         // input copy counter
    emitter.label("__rt_str_pad_inp_loop");
    emitter.instruction("cbz x17, __rt_str_pad_rp");                            // input done → right padding
    emitter.instruction("ldrb w0, [x1], #1");                                  // load input byte
    emitter.instruction("strb w0, [x12], #1");                                 // write to output
    emitter.instruction("sub x17, x17, #1");                                    // decrement
    emitter.instruction("b __rt_str_pad_inp_loop");                             // continue

    // right padding
    emitter.label("__rt_str_pad_rp");
    emitter.instruction("mov x17, x16");                                        // right pad counter
    emitter.instruction("mov x18, #0");                                         // pad string index
    emitter.label("__rt_str_pad_rp_loop");
    emitter.instruction("cbz x17, __rt_str_pad_done");                          // right padding done
    emitter.instruction("ldp x3, x4, [sp, #16]");                              // reload pad string
    emitter.instruction("ldrb w0, [x3, x18]");                                 // load pad char
    emitter.instruction("strb w0, [x12], #1");                                 // write to output
    emitter.instruction("sub x17, x17, #1");                                    // decrement
    emitter.instruction("add x18, x18, #1");                                   // advance pad index
    emitter.instruction("cmp x18, x4");                                         // wrap around
    emitter.instruction("csel x18, xzr, x18, ge");                             // reset to 0
    emitter.instruction("b __rt_str_pad_rp_loop");                              // continue

    emitter.label("__rt_str_pad_done");
    emitter.instruction("mov x1, x13");                                         // result pointer
    emitter.instruction("sub x2, x12, x13");                                    // result length
    emitter.instruction("adrp x9, _concat_off@PAGE");                          // update concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x2");                                    // advance by result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset
    emitter.instruction("ldp x29, x30, [sp, #48]");                            // restore frame
    emitter.instruction("add sp, sp, #64");                                     // deallocate
    emitter.instruction("ret");                                                 // return

    emitter.label("__rt_str_pad_noop");
    emitter.instruction("bl __rt_strcopy");                                     // copy input as-is
    emitter.instruction("ldp x29, x30, [sp, #48]");                            // restore frame
    emitter.instruction("add sp, sp, #64");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}

/// str_split: split string into array of chunks.
/// Input: x1/x2=string, x3=chunk_length. Output: x0=array pointer.
pub fn emit_str_split(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_split ---");
    emitter.label("__rt_str_split");
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                   // save string ptr/len
    emitter.instruction("str x3, [sp, #16]");                                   // save chunk length

    // -- create array --
    emitter.instruction("mov x0, #16");                                         // initial capacity
    emitter.instruction("mov x1, #16");                                         // elem_size = 16 (str ptr+len)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #24]");                                   // save array pointer
    emitter.instruction("str xzr, [sp, #32]");                                  // current position = 0

    emitter.label("__rt_str_split_loop");
    emitter.instruction("ldr x4, [sp, #32]");                                  // load current position
    emitter.instruction("ldp x1, x2, [sp]");                                   // reload string ptr/len
    emitter.instruction("cmp x4, x2");                                          // past end of string?
    emitter.instruction("b.ge __rt_str_split_done");                            // yes → done

    // -- compute this chunk's actual length --
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload chunk length
    emitter.instruction("sub x5, x2, x4");                                      // remaining = len - pos
    emitter.instruction("cmp x5, x3");                                          // remaining vs chunk_length
    emitter.instruction("csel x5, x3, x5, gt");                                // chunk = min(remaining, chunk_length)

    // -- push chunk as string element --
    emitter.instruction("ldr x0, [sp, #24]");                                  // reload array pointer
    emitter.instruction("add x1, x1, x4");                                      // x1 = base + current position
    emitter.instruction("mov x2, x5");                                          // x2 = chunk length
    emitter.instruction("bl __rt_array_push_str");                              // push chunk onto array

    // -- advance position by chunk length --
    emitter.instruction("ldr x4, [sp, #32]");                                  // reload position
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload chunk length
    emitter.instruction("add x4, x4, x3");                                      // position += chunk_length
    emitter.instruction("str x4, [sp, #32]");                                   // save updated position
    emitter.instruction("b __rt_str_split_loop");                               // continue

    emitter.label("__rt_str_split_done");
    emitter.instruction("ldr x0, [sp, #24]");                                  // return array pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                            // restore frame
    emitter.instruction("add sp, sp, #64");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}

/// addslashes: escape single quotes, double quotes, backslashes with backslash.
/// Input: x1/x2=string. Output: x1/x2=result.
pub fn emit_addslashes(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: addslashes ---");
    emitter.label("__rt_addslashes");

    // -- set up concat_buf destination --
    emitter.instruction("adrp x6, _concat_off@PAGE");                          // load concat offset page
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                          // load concat buffer page
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                    // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_addslashes_loop");
    emitter.instruction("cbz x11, __rt_addslashes_done");                       // no bytes left → done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load source byte, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    // -- check if char needs escaping --
    emitter.instruction("cmp w12, #39");                                        // single quote?
    emitter.instruction("b.eq __rt_addslashes_esc");                            // yes → escape it
    emitter.instruction("cmp w12, #34");                                        // double quote?
    emitter.instruction("b.eq __rt_addslashes_esc");                            // yes → escape it
    emitter.instruction("cmp w12, #92");                                        // backslash?
    emitter.instruction("b.eq __rt_addslashes_esc");                            // yes → escape it
    // -- store unescaped byte --
    emitter.instruction("strb w12, [x9], #1");                                  // store byte as-is
    emitter.instruction("b __rt_addslashes_loop");                              // next byte

    emitter.label("__rt_addslashes_esc");
    emitter.instruction("mov w13, #92");                                        // backslash character
    emitter.instruction("strb w13, [x9], #1");                                  // write escape backslash
    emitter.instruction("strb w12, [x9], #1");                                  // write the original char
    emitter.instruction("b __rt_addslashes_loop");                              // next byte

    emitter.label("__rt_addslashes_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance by result length
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}

/// stripslashes: remove escape backslashes.
/// Input: x1/x2=string. Output: x1/x2=result.
pub fn emit_stripslashes(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stripslashes ---");
    emitter.label("__rt_stripslashes");

    emitter.instruction("adrp x6, _concat_off@PAGE");                          // load concat offset page
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                          // load concat buffer page
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                    // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining byte count

    emitter.label("__rt_stripslashes_loop");
    emitter.instruction("cbz x11, __rt_stripslashes_done");                     // done if no bytes left
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte, advance source
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.instruction("cmp w12, #92");                                        // is it a backslash?
    emitter.instruction("b.ne __rt_stripslashes_store");                        // no → store as-is
    // -- backslash: skip it and store the next char --
    emitter.instruction("cbz x11, __rt_stripslashes_store");                    // trailing backslash → store it
    emitter.instruction("ldrb w12, [x1], #1");                                  // load escaped char, advance
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.label("__rt_stripslashes_store");
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to output
    emitter.instruction("b __rt_stripslashes_loop");                            // next byte

    emitter.label("__rt_stripslashes_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}

/// nl2br: insert "<br />\n" before each newline.
/// Input: x1/x2=string. Output: x1/x2=result.
pub fn emit_nl2br(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: nl2br ---");
    emitter.label("__rt_nl2br");

    emitter.instruction("adrp x6, _concat_off@PAGE");                          // load concat offset page
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                          // load concat buffer page
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                    // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining count

    emitter.label("__rt_nl2br_loop");
    emitter.instruction("cbz x11, __rt_nl2br_done");                            // no bytes left → done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte, advance source
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    emitter.instruction("cmp w12, #10");                                        // is it '\n'?
    emitter.instruction("b.ne __rt_nl2br_store");                               // no → store as-is
    // -- insert "<br />" before the newline --
    emitter.instruction("mov w13, #60");                                        // '<'
    emitter.instruction("strb w13, [x9], #1");                                  // write '<'
    emitter.instruction("mov w13, #98");                                        // 'b'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'b'
    emitter.instruction("mov w13, #114");                                       // 'r'
    emitter.instruction("strb w13, [x9], #1");                                  // write 'r'
    emitter.instruction("mov w13, #32");                                        // ' '
    emitter.instruction("strb w13, [x9], #1");                                  // write ' '
    emitter.instruction("mov w13, #47");                                        // '/'
    emitter.instruction("strb w13, [x9], #1");                                  // write '/'
    emitter.instruction("mov w13, #62");                                        // '>'
    emitter.instruction("strb w13, [x9], #1");                                  // write '>'
    emitter.label("__rt_nl2br_store");
    emitter.instruction("strb w12, [x9], #1");                                  // write original byte (including '\n')
    emitter.instruction("b __rt_nl2br_loop");                                   // next byte

    emitter.label("__rt_nl2br_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}

/// wordwrap: wrap text at word boundaries.
/// Input: x1/x2=string, x3=width, x4/x5=break_str. Output: x1/x2=result.
pub fn emit_wordwrap(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: wordwrap ---");
    emitter.label("__rt_wordwrap");
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set frame pointer
    emitter.instruction("stp x4, x5, [sp]");                                   // save break string ptr/len
    emitter.instruction("str x3, [sp, #16]");                                   // save width

    // -- set up concat_buf --
    emitter.instruction("adrp x6, _concat_off@PAGE");                          // load concat offset page
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                          // load concat buffer page
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                    // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("str x9, [sp, #24]");                                   // save result start
    emitter.instruction("mov x10, #0");                                         // current line length

    emitter.label("__rt_wordwrap_loop");
    emitter.instruction("cbz x2, __rt_wordwrap_done");                          // no input left → done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte, advance source
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining

    // -- check for existing newlines (reset counter) --
    emitter.instruction("cmp w12, #10");                                        // is it '\n'?
    emitter.instruction("b.ne __rt_wordwrap_check");                            // no → check width
    emitter.instruction("strb w12, [x9], #1");                                  // store newline
    emitter.instruction("mov x10, #0");                                         // reset line length
    emitter.instruction("b __rt_wordwrap_loop");                                // next byte

    emitter.label("__rt_wordwrap_check");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload width
    emitter.instruction("cmp x10, x3");                                         // line length >= width?
    emitter.instruction("b.lt __rt_wordwrap_store");                            // no → just store char

    // -- insert break string at width boundary --
    emitter.instruction("ldp x4, x5, [sp]");                                   // reload break string
    emitter.instruction("mov x14, #0");                                         // break copy index
    emitter.label("__rt_wordwrap_brk");
    emitter.instruction("cmp x14, x5");                                         // all break chars written?
    emitter.instruction("b.ge __rt_wordwrap_brk_done");                         // yes → continue with char
    emitter.instruction("ldrb w13, [x4, x14]");                                // load break char
    emitter.instruction("strb w13, [x9], #1");                                  // write to output
    emitter.instruction("add x14, x14, #1");                                   // next break char
    emitter.instruction("b __rt_wordwrap_brk");                                 // continue
    emitter.label("__rt_wordwrap_brk_done");
    emitter.instruction("mov x10, #0");                                         // reset line length

    emitter.label("__rt_wordwrap_store");
    emitter.instruction("strb w12, [x9], #1");                                  // store current byte
    emitter.instruction("add x10, x10, #1");                                    // increment line length
    emitter.instruction("b __rt_wordwrap_loop");                                // next byte

    emitter.label("__rt_wordwrap_done");
    emitter.instruction("ldr x1, [sp, #24]");                                  // result pointer
    emitter.instruction("sub x2, x9, x1");                                      // result length
    emitter.instruction("adrp x6, _concat_off@PAGE");                          // update concat offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store
    emitter.instruction("ldp x29, x30, [sp, #32]");                            // restore frame
    emitter.instruction("add sp, sp, #48");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}

/// bin2hex: convert binary string to hex representation.
/// Input: x1/x2=string. Output: x1/x2=result (2x length).
pub fn emit_bin2hex(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: bin2hex ---");
    emitter.label("__rt_bin2hex");

    emitter.instruction("adrp x6, _concat_off@PAGE");                          // load concat offset page
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                          // load concat buffer page
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                    // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining count

    emitter.label("__rt_bin2hex_loop");
    emitter.instruction("cbz x11, __rt_bin2hex_done");                          // done if no bytes left
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte, advance source
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining
    // -- high nibble --
    emitter.instruction("lsr w13, w12, #4");                                    // extract high 4 bits
    emitter.instruction("cmp w13, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_bin2hex_hi_af");                             // yes → use a-f
    emitter.instruction("add w13, w13, #48");                                   // convert 0-9 to '0'-'9'
    emitter.instruction("b __rt_bin2hex_hi_store");                             // store
    emitter.label("__rt_bin2hex_hi_af");
    emitter.instruction("add w13, w13, #87");                                   // convert 10-15 to 'a'-'f'
    emitter.label("__rt_bin2hex_hi_store");
    emitter.instruction("strb w13, [x9], #1");                                  // write high nibble hex char
    // -- low nibble --
    emitter.instruction("and w13, w12, #0xf");                                  // extract low 4 bits
    emitter.instruction("cmp w13, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_bin2hex_lo_af");                             // yes → use a-f
    emitter.instruction("add w13, w13, #48");                                   // convert 0-9 to '0'-'9'
    emitter.instruction("b __rt_bin2hex_lo_store");                             // store
    emitter.label("__rt_bin2hex_lo_af");
    emitter.instruction("add w13, w13, #87");                                   // convert 10-15 to 'a'-'f'
    emitter.label("__rt_bin2hex_lo_store");
    emitter.instruction("strb w13, [x9], #1");                                  // write low nibble hex char
    emitter.instruction("b __rt_bin2hex_loop");                                 // next byte

    emitter.label("__rt_bin2hex_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store updated offset
    emitter.instruction("ret");                                                 // return
}

/// hex2bin: convert hex string to binary.
/// Input: x1/x2=hex_string. Output: x1/x2=result (half length).
pub fn emit_hex2bin(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hex2bin ---");
    emitter.label("__rt_hex2bin");

    emitter.instruction("adrp x6, _concat_off@PAGE");                          // load concat offset page
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                          // load concat buffer page
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                    // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("mov x10, x9");                                         // save result start
    emitter.instruction("mov x11, x2");                                         // remaining hex chars

    emitter.label("__rt_hex2bin_loop");
    emitter.instruction("cmp x11, #2");                                         // need at least 2 hex chars
    emitter.instruction("b.lt __rt_hex2bin_done");                              // not enough → done

    // -- parse high nibble (inline hex digit conversion) --
    emitter.instruction("ldrb w12, [x1], #1");                                  // load first hex char
    emitter.instruction("cmp w12, #57");                                        // <= '9'?
    emitter.instruction("b.le 1f");                                             // yes → numeric
    emitter.instruction("cmp w12, #70");                                        // <= 'F'?
    emitter.instruction("b.le 2f");                                             // yes → uppercase
    emitter.instruction("sub w12, w12, #87");                                   // 'a'-'f' → 10-15
    emitter.instruction("b 3f");                                                // done with high nibble
    emitter.raw("1:");
    emitter.instruction("sub w12, w12, #48");                                   // '0'-'9' → 0-9
    emitter.instruction("b 3f");                                                // done
    emitter.raw("2:");
    emitter.instruction("sub w12, w12, #55");                                   // 'A'-'F' → 10-15
    emitter.raw("3:");
    emitter.instruction("lsl w13, w12, #4");                                    // shift to high nibble

    // -- parse low nibble (inline hex digit conversion) --
    emitter.instruction("ldrb w12, [x1], #1");                                  // load second hex char
    emitter.instruction("cmp w12, #57");                                        // <= '9'?
    emitter.instruction("b.le 4f");                                             // yes → numeric
    emitter.instruction("cmp w12, #70");                                        // <= 'F'?
    emitter.instruction("b.le 5f");                                             // yes → uppercase
    emitter.instruction("sub w12, w12, #87");                                   // 'a'-'f' → 10-15
    emitter.instruction("b 6f");                                                // done with low nibble
    emitter.raw("4:");
    emitter.instruction("sub w12, w12, #48");                                   // '0'-'9' → 0-9
    emitter.instruction("b 6f");                                                // done
    emitter.raw("5:");
    emitter.instruction("sub w12, w12, #55");                                   // 'A'-'F' → 10-15
    emitter.raw("6:");
    emitter.instruction("orr w13, w13, w12");                                   // combine high and low nibbles
    emitter.instruction("strb w13, [x9], #1");                                  // store decoded byte
    emitter.instruction("sub x11, x11, #2");                                    // consumed 2 hex chars
    emitter.instruction("b __rt_hex2bin_loop");                                 // next pair

    emitter.label("__rt_hex2bin_done");
    emitter.instruction("mov x1, x10");                                         // result pointer
    emitter.instruction("sub x2, x9, x10");                                     // result length
    emitter.instruction("ldr x8, [x6]");                                        // reload offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store
    emitter.instruction("ret");                                                 // return
}
