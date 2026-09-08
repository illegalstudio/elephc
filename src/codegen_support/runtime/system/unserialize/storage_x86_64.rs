//! Purpose:
//! Emits x86_64 object-property hydration, hash conversion, and serialized-key parsing.
//!
//! Called from:
//! - `super::emit_unserialize()` after the recursive decoder and per-call context helpers.
//!
//! Key details:
//! - Manually boxed values preserve heap markers while parsed ownership moves into final storage.

use crate::codegen_support::emit::Emitter;

/// Emits x86_64 object-property storage and parsed-hash conversion helpers.
pub(super) fn emit_object_storage(emitter: &mut Emitter) {
    // -- __rt_obj_store_prop(rdi=obj, rsi=key_ptr, rdx=key_len, rcx=valbox): inject a property --
    emitter.label_global("__rt_obj_store_prop");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the store frame
    emitter.instruction("sub rsp, 64");                                         // reserve frame slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the object pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the key pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the key length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the value box
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // class id from the object header
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_stdclass_class_id");
    emitter.instruction("cmp rax, QWORD PTR [r10]");                           // does the receiver use dynamic property storage?
    emitter.instruction("jne __rt_obj_store_prop_declared");                   // fixed-layout objects use the generated property table
    emitter.instruction("call __rt_stdclass_set");                             // original args still carry object, key pair, and boxed value
    emitter.instruction("jmp __rt_obj_store_prop_ret");                        // dynamic property stored
    emitter.label("__rt_obj_store_prop_declared");
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_class_serprop_ptrs");
    emitter.instruction("shl rax, 3");                                          // class_id * 8 (pointer stride)
    emitter.instruction("add r10, rax");                                        // slot = base + class_id*8
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // property-info table for this class
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the property-info table
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // property count
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the property count
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // row index = 0
    emitter.label("__rt_obj_store_prop_loop");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the row index
    emitter.instruction("cmp rax, QWORD PTR [rbp - 48]");                       // scanned every row?
    emitter.instruction("jge __rt_obj_store_prop_done");                        // unknown key is ignored
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // property-info table
    emitter.instruction("shl rax, 5");                                          // index * 32 (row stride)
    emitter.instruction("add rax, r10");                                        // table + index*32
    emitter.instruction("add rax, 8");                                          // skip the count word to the row
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the row pointer
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // row mangled key pointer
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // row mangled key length
    emitter.instruction("cmp rdx, QWORD PTR [rbp - 24]");                       // same length as the parsed key?
    emitter.instruction("jne __rt_obj_store_prop_next");                        // lengths differ, skip
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // parsed key pointer
    emitter.instruction("xor r8, r8");                                          // byte compare cursor
    emitter.label("__rt_obj_store_prop_cmp");
    emitter.instruction("cmp r8, rdx");                                         // compared all bytes?
    emitter.instruction("jge __rt_obj_store_prop_match");                       // full match
    emitter.instruction("mov al, BYTE PTR [r9 + r8]");                          // row key byte
    emitter.instruction("mov cl, BYTE PTR [rsi + r8]");                         // parsed key byte
    emitter.instruction("cmp al, cl");                                          // bytes equal?
    emitter.instruction("jne __rt_obj_store_prop_next");                        // mismatch, skip this row
    emitter.instruction("add r8, 1");                                           // next byte
    emitter.instruction("jmp __rt_obj_store_prop_cmp");                         // continue comparing
    emitter.label("__rt_obj_store_prop_match");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the row pointer
    emitter.instruction("mov r8, QWORD PTR [rax + 16]");                        // property byte offset
    emitter.instruction("mov r9, QWORD PTR [rax + 24]");                        // property value tag
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // object pointer
    emitter.instruction("add r10, r8");                                         // address of the property slot
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // value box
    emitter.instruction("cmp r9, 7");                                           // is this a Mixed/untyped slot?
    emitter.instruction("je __rt_obj_store_prop_mixed");                        // store the boxed cell directly
    emitter.instruction("cmp r9, 1");                                           // is this a string slot?
    emitter.instruction("je __rt_obj_store_prop_str");                          // store pointer and length
    emitter.instruction("cmp r9, 4");                                           // is this an indexed-array slot?
    emitter.instruction("je __rt_obj_store_prop_arr");                          // convert the parsed hash to an indexed array
    emitter.instruction("cmp r9, 11");                                          // is this an inline TaggedScalar slot?
    emitter.instruction("je __rt_obj_store_prop_tagged_scalar");                // restore payload plus the parsed runtime tag
    emitter.instruction("mov rax, QWORD PTR [rcx + 8]");                        // typed scalar/object/hash: unbox the low word
    emitter.instruction("mov QWORD PTR [r10], rax");                            // store it inline in the slot
    emitter.instruction("jmp __rt_obj_store_prop_ret");                         // property stored
    emitter.label("__rt_obj_store_prop_tagged_scalar");
    emitter.instruction("mov rax, QWORD PTR [rcx + 8]");                        // load the scalar payload from the parsed Mixed cell
    emitter.instruction("mov QWORD PTR [r10], rax");                            // restore the inline payload word
    emitter.instruction("mov rax, QWORD PTR [rcx]");                            // load the parsed int/null runtime tag
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // restore the inline runtime-tag word
    emitter.instruction("jmp __rt_obj_store_prop_ret");                         // property stored
    emitter.label("__rt_obj_store_prop_arr");
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // save the property byte offset across the call
    emitter.instruction("mov rdi, QWORD PTR [rcx + 8]");                        // parsed hash pointer (box low word)
    emitter.instruction("call __rt_hash_to_indexed_array");                     // materialize a native indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // object pointer
    emitter.instruction("add r10, QWORD PTR [rbp - 64]");                       // slot = object + byte offset
    emitter.instruction("mov QWORD PTR [r10], rax");                            // store the indexed-array pointer
    emitter.instruction("jmp __rt_obj_store_prop_ret");                         // property stored
    emitter.label("__rt_obj_store_prop_str");
    emitter.instruction("mov rax, QWORD PTR [rcx + 8]");                        // string pointer from the box
    emitter.instruction("mov QWORD PTR [r10], rax");                            // store the string pointer
    emitter.instruction("mov rax, QWORD PTR [rcx + 16]");                       // string length from the box
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // store the string length
    emitter.instruction("jmp __rt_obj_store_prop_ret");                         // property stored
    emitter.label("__rt_obj_store_prop_mixed");
    emitter.instruction("mov rax, QWORD PTR [rcx]");                            // boxed value tag
    emitter.instruction("cmp rax, 8");                                          // is the boxed value null?
    emitter.instruction("je __rt_obj_store_prop_mixed_null");                   // store the null sentinel
    emitter.instruction("mov QWORD PTR [r10], rcx");                            // store the boxed Mixed cell pointer
    emitter.instruction("jmp __rt_obj_store_prop_ret");                         // property stored
    emitter.label("__rt_obj_store_prop_mixed_null");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "r11", crate::codegen_support::NULL_SENTINEL);
    emitter.instruction("mov QWORD PTR [r10], r11");                            // store the in-band null sentinel
    emitter.instruction("mov QWORD PTR [r10 + 8], 0");                          // clear the high word
    emitter.instruction("jmp __rt_obj_store_prop_ret");                         // property stored
    emitter.label("__rt_obj_store_prop_next");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the row index
    emitter.instruction("add rax, 1");                                          // advance to the next row
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // persist the row index
    emitter.instruction("jmp __rt_obj_store_prop_loop");                        // continue scanning
    emitter.label("__rt_obj_store_prop_done");
    emitter.label("__rt_obj_store_prop_ret");
    emitter.instruction("add rsp, 64");                                         // deallocate the store frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller

    // -- __rt_hash_to_indexed_array(rdi=hash) -> rax=indexed array: rebuild a parsed
    // hash (boxed-Mixed values) as a native value_type-7 indexed array. --
    emitter.label_global("__rt_hash_to_indexed_array");
    emitter.instruction("push rbp");                                            // open the conversion frame
    emitter.instruction("mov rbp, rsp");                                        // set the frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve callee-saved spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rbx");                        // save rbx
    emitter.instruction("mov QWORD PTR [rbp - 16], r12");                       // save r12
    emitter.instruction("mov QWORD PTR [rbp - 24], r13");                       // save r13
    emitter.instruction("mov rbx, rdi");                                        // hash pointer
    emitter.instruction("mov rdi, 0");                                          // initial capacity 0
    emitter.instruction("mov rsi, 8");                                          // 8-byte element slots
    emitter.instruction("call __rt_array_new");                                 // allocate an empty indexed array
    emitter.instruction("mov r12, rax");                                        // destination array pointer
    emitter.instruction("xor r13, r13");                                        // hash iteration cursor
    emitter.label("__rt_hash_to_indexed_array_loop");
    emitter.instruction("mov rdi, rbx");                                        // hash pointer
    emitter.instruction("mov rsi, r13");                                        // resume cursor
    emitter.instruction("call __rt_hash_iter_next");                            // rcx=value low, rax=next cursor
    emitter.instruction("cmp rax, -1");                                         // iteration done?
    emitter.instruction("je __rt_hash_to_indexed_array_done");                  // stop when exhausted
    emitter.instruction("mov r13, rax");                                        // save the resume cursor
    emitter.instruction("mov rdi, r12");                                        // destination array
    emitter.instruction("mov rsi, rcx");                                        // boxed-Mixed value pointer (parsed-hash value)
    emitter.instruction("call __rt_array_push_refcounted");                     // append, transferring ownership
    emitter.instruction("mov r12, rax");                                        // array may move on COW growth
    emitter.instruction("jmp __rt_hash_to_indexed_array_loop");                 // continue iterating
    emitter.label("__rt_hash_to_indexed_array_done");
    emitter.instruction("mov rax, r12");                                        // return the indexed array
    emitter.instruction("mov rbx, QWORD PTR [rbp - 8]");                        // restore rbx
    emitter.instruction("mov r12, QWORD PTR [rbp - 16]");                       // restore r12
    emitter.instruction("mov r13, QWORD PTR [rbp - 24]");                       // restore r13
    emitter.instruction("add rsp, 32");                                         // close the conversion frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the converted array
}

/// Emits the x86_64 leaf key parser `__rt_unser_key`.
///
/// Input: `rdi`=base, `rsi`=pos, `rdx`=end. Output: `rax`=key_lo, `rdx`=key_hi (-1 for
/// an integer key, else the string byte length), `rcx`=newpos. String key pointers are
/// borrowed into the source buffer; `__rt_hash_set` persists them.
pub(super) fn emit_key(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unser_key (serialize() array key parser, leaf) ---");
    emitter.label_global("__rt_unser_key");
    emitter.instruction("cmp rsi, rdx");                                        // require a key type byte before loading it
    emitter.instruction("jae __rt_unser_key_fail_x");                           // return a sentinel cursor for a truncated key
    emitter.instruction("movzx r9d, BYTE PTR [rdi + rsi]");                     // load the key type byte
    emitter.instruction("cmp r9d, 105");                                        // ASCII 'i' (integer key)?
    emitter.instruction("je __rt_unser_key_int");                               // parse an integer key
    // -- string key: "s:" + bytelen + ":\"" + raw + "\";" --
    emitter.instruction("mov r10, rdi");                                        // base copy for cursor math
    emitter.instruction("add r10, rsi");                                        // pointer to the type byte
    emitter.instruction("add r10, 2");                                          // skip "s:" to the length digits
    emitter.instruction("xor r11, r11");                                        // length accumulator
    emitter.label("__rt_unser_key_strlen");
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // next length byte
    emitter.instruction("cmp r9d, 48");                                         // below '0'?
    emitter.instruction("jl __rt_unser_key_strlen_done");                       // ':' terminator reached
    emitter.instruction("cmp r9d, 57");                                         // above '9'?
    emitter.instruction("jg __rt_unser_key_strlen_done");                       // ':' terminator reached
    emitter.instruction("sub r9d, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift accumulator
    emitter.instruction("add r11, r9");                                         // add digit
    emitter.instruction("add r10, 1");                                          // advance cursor
    emitter.instruction("jmp __rt_unser_key_strlen");                           // continue
    emitter.label("__rt_unser_key_strlen_done");
    emitter.instruction("add r10, 2");                                          // skip ':' and opening '\"' to the raw bytes
    emitter.instruction("mov r8, r10");                                         // raw end accumulator = raw start
    emitter.instruction("add r8, r11");                                         // raw end = raw + len
    emitter.instruction("add r8, 2");                                           // skip closing '\"' and ';'
    emitter.instruction("sub r8, rdi");                                         // newpos = (raw end + 2) - base
    emitter.instruction("mov rcx, r8");                                         // key newpos
    emitter.instruction("mov rdx, r11");                                        // key_hi = string byte length
    emitter.instruction("mov rax, r10");                                        // key_lo = borrowed raw string pointer
    emitter.instruction("ret");                                                 // return the string key
    // -- integer key: "i:" + optional '-' + digits + ";" --
    emitter.label("__rt_unser_key_int");
    emitter.instruction("mov r10, rdi");                                        // base copy for cursor math
    emitter.instruction("add r10, rsi");                                        // pointer to the type byte
    emitter.instruction("add r10, 2");                                          // skip "i:" to the first digit
    emitter.instruction("xor r11, r11");                                        // digit accumulator
    emitter.instruction("xor r8, r8");                                          // negative-sign flag
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // first numeric byte
    emitter.instruction("cmp r9d, 45");                                         // leading '-'?
    emitter.instruction("jne __rt_unser_key_int_loop");                         // no sign
    emitter.instruction("mov r8, 1");                                           // record negative sign
    emitter.instruction("add r10, 1");                                          // skip '-'
    emitter.label("__rt_unser_key_int_loop");
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // next numeric byte
    emitter.instruction("cmp r9d, 48");                                         // below '0'?
    emitter.instruction("jl __rt_unser_key_int_done");                          // ';' terminator reached
    emitter.instruction("cmp r9d, 57");                                         // above '9'?
    emitter.instruction("jg __rt_unser_key_int_done");                          // ';' terminator reached
    emitter.instruction("sub r9d, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift accumulator
    emitter.instruction("add r11, r9");                                         // add digit
    emitter.instruction("add r10, 1");                                          // advance cursor
    emitter.instruction("jmp __rt_unser_key_int_loop");                         // continue
    emitter.label("__rt_unser_key_int_done");
    emitter.instruction("test r8, r8");                                         // signed?
    emitter.instruction("jz __rt_unser_key_int_pos");                           // not signed
    emitter.instruction("neg r11");                                             // apply sign
    emitter.label("__rt_unser_key_int_pos");
    emitter.instruction("mov rcx, r10");                                        // cursor copy
    emitter.instruction("sub rcx, rdi");                                        // newpos = cursor - base
    emitter.instruction("add rcx, 1");                                          // skip the ';'
    emitter.instruction("mov rax, r11");                                        // key_lo = integer key value
    emitter.instruction("mov rdx, -1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("ret");                                                 // return the integer key
    emitter.label("__rt_unser_key_fail_x");
    emitter.instruction("lea rcx, [rdx + 1]");                                  // end+1 is an impossible valid cursor
    emitter.instruction("xor eax, eax");                                        // clear key payload on failure
    emitter.instruction("xor edx, edx");                                        // clear key metadata on failure
    emitter.instruction("ret");                                                 // caller/preflight rejects the sentinel
}
