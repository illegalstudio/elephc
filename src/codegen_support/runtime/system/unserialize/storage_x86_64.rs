//! Purpose:
//! Emits x86_64 object-property hydration, hash conversion, and serialized-key parsing.
//!
//! Called from:
//! - `super::emit_unserialize()` after the recursive decoder and per-call context helpers.
//!
//! Key details:
//! - Manually boxed values preserve heap markers while parsed ownership moves into final storage.
//! - Every store writes the slot's high word (the string length, or zero), which clears the
//!   uninitialized marker a typed property without a default starts with.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::runtime::data::UNSER_PROPERTY_ASSIGN_PREFIX;
use crate::codegen_support::sentinels::{
    SERPROP_FALSE_ONLY_BIT, TAGGED_SCALAR_PROPERTY_TAG, TAGGED_SCALAR_TAG_INT,
    TAGGED_SCALAR_TAG_NULL,
};

/// Emits x86_64 object-property storage and parsed-hash conversion helpers.
pub(super) fn emit_object_storage(emitter: &mut Emitter) {
    // -- __rt_obj_store_prop(rdi=obj, rsi=key_ptr, rdx=key_len, rcx=valbox): inject a property --
    emitter.label_global("__rt_obj_store_prop");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the store frame
    emitter.instruction("sub rsp, 96");                                         // reserve frame slots (class id spill included)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the object pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the key pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the key length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the value box
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                        // save the object's owning box for release on a hydration TypeError
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // class id from the object header
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
    // -- reject a hydrated value whose boxed tag the declared property type does not accept --
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // row index
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // class id from the object header
    emitter.instruction("lea r10, [rip + _class_serpdiag_ptrs]");               // diagnostic pointer table
    emitter.instruction("mov r10, QWORD PTR [r10 + r9 * 8]");                   // diagnostic rows for this class
    emitter.instruction("imul rax, rax, 32");                                   // diagnostic row stride
    emitter.instruction("add r10, rax");                                        // diagnostic row = base + index*32
    emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                       // accepted boxed value tags
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // boxed value
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // boxed value tag
    emitter.instruction("mov rcx, rax");                                        // variable shift count = boxed tag
    emitter.instruction("mov r8, 1");                                           // probe bit for the boxed tag
    emitter.instruction("shl r8, cl");                                          // tag bit
    emitter.instruction("test r11, r8");                                        // is the tag accepted for this property?
    emitter.instruction("jnz __rt_obj_store_prop_accepted");                    // a directly accepted tag needs no payload check
    emitter.instruction(&format!("test r11, {}", SERPROP_FALSE_ONLY_BIT));      // does the declared type admit a `false` member?
    emitter.instruction("jz __rt_obj_store_prop_type_error");                   // no false member: raise PHP's hydration TypeError
    emitter.instruction("cmp rax, 3");                                          // is the boxed value a bool?
    emitter.instruction("jne __rt_obj_store_prop_type_error");                  // only a bool can satisfy a declared `false`
    emitter.instruction("cmp QWORD PTR [r9 + 8], 0");                           // boxed payload (0 = false, 1 = true)
    emitter.instruction("jne __rt_obj_store_prop_type_error");                  // `true` is not a `false`
    emitter.label("__rt_obj_store_prop_accepted");
    // -- PHP widens an accepted int into a `float`/`?float` slot; retag the owned box --
    emitter.instruction("cmp QWORD PTR [r10 + 24], 0");                         // widen-int-to-float flag
    emitter.instruction("je __rt_obj_store_prop_no_widen");                     // most slots keep the parsed type
    emitter.instruction("test rax, rax");                                       // is the boxed value an int?
    emitter.instruction("jnz __rt_obj_store_prop_no_widen");                    // only an int widens
    emitter.instruction("mov r8, QWORD PTR [r9 + 8]");                          // integer payload
    emitter.instruction("cvtsi2sd xmm0, r8");                                   // convert to the PHP float value
    emitter.instruction("movq r8, xmm0");                                       // move the double bits into a GPR
    emitter.instruction("mov QWORD PTR [r9 + 8], r8");                          // store the widened payload
    emitter.instruction("mov QWORD PTR [r9], 2");                               // retag the owned box as a float
    emitter.label("__rt_obj_store_prop_no_widen");
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
    emitter.instruction(&format!("cmp r9, {}", TAGGED_SCALAR_PROPERTY_TAG));    // is this an inline tagged-scalar (`?int`) slot?
    emitter.instruction("je __rt_obj_store_prop_tagged");                       // store the payload and its runtime tag inline
    emitter.instruction("mov rax, QWORD PTR [rcx + 8]");                        // typed scalar/object/hash: unbox the low word
    emitter.instruction("mov QWORD PTR [r10], rax");                            // store it inline in the slot
    emitter.instruction("mov QWORD PTR [r10 + 8], 0");                          // the high word marks the typed property initialized
    emitter.instruction("jmp __rt_obj_store_prop_ret");                         // property stored
    emitter.label("__rt_obj_store_prop_tagged");
    // Only an int or null reaches this arm; the declared-type check above already raised the
    // TypeError for a mismatch. See the AArch64 variant.
    emitter.instruction(&format!("cmp QWORD PTR [rcx], {}", TAGGED_SCALAR_TAG_INT)); // is the boxed value an int?
    emitter.instruction("jne __rt_obj_store_prop_tagged_null");                 // null or a mismatched type: store the canonical tagged null pair
    emitter.instruction("mov rax, QWORD PTR [rcx + 8]");                        // unbox the integer payload
    emitter.instruction("mov QWORD PTR [r10], rax");                            // payload word of the tagged slot
    emitter.instruction(&format!("mov QWORD PTR [r10 + 8], {}", TAGGED_SCALAR_TAG_INT)); // tag word: a non-null tagged int
    emitter.instruction("jmp __rt_obj_store_prop_ret");                         // property stored
    emitter.label("__rt_obj_store_prop_tagged_null");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "rax", crate::codegen_support::NULL_SENTINEL);
    emitter.instruction("mov QWORD PTR [r10], rax");                            // canonical tagged-null payload word
    emitter.instruction(&format!("mov QWORD PTR [r10 + 8], {}", TAGGED_SCALAR_TAG_NULL)); // tag word: PHP null
    emitter.instruction("jmp __rt_obj_store_prop_ret");                         // property stored
    emitter.label("__rt_obj_store_prop_arr");
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // save the property byte offset across the call
    emitter.instruction("mov rdi, QWORD PTR [rcx + 8]");                        // parsed hash pointer (box low word)
    emitter.instruction("call __rt_hash_to_indexed_array");                     // materialize a native indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // object pointer
    emitter.instruction("add r10, QWORD PTR [rbp - 64]");                       // slot = object + byte offset
    emitter.instruction("mov QWORD PTR [r10], rax");                            // store the indexed-array pointer
    emitter.instruction("mov QWORD PTR [r10 + 8], 0");                          // the high word marks the typed property initialized
    emitter.instruction("jmp __rt_obj_store_prop_ret");                         // property stored
    emitter.label("__rt_obj_store_prop_str");
    emitter.instruction("mov rax, QWORD PTR [rcx + 8]");                        // string pointer from the box
    emitter.instruction("mov QWORD PTR [r10], rax");                            // store the string pointer
    emitter.instruction("mov rax, QWORD PTR [rcx + 16]");                       // string length from the box
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // store the string length
    emitter.instruction("jmp __rt_obj_store_prop_ret");                         // property stored
    // A parsed null keeps its boxed tag-8 cell like every other value: the in-band
    // NULL_SENTINEL is not a Mixed cell pointer, and every reader of the slot
    // (`=== null`, var_dump, json_encode) dereferenced it.
    emitter.label("__rt_obj_store_prop_mixed");
    emitter.instruction("mov QWORD PTR [r10], rcx");                            // store the boxed Mixed cell pointer
    emitter.instruction("mov QWORD PTR [r10 + 8], 0");                          // the high word marks the typed property initialized
    emitter.instruction("jmp __rt_obj_store_prop_ret");                         // property stored
    // -- declared-type mismatch: compose and throw PHP's hydration TypeError --
    emitter.label("__rt_obj_store_prop_type_error");
    // The owning Mixed box owns the object, so releasing it can free the very object whose header
    // names the diagnostic's declaring class. Read the class id FIRST, exactly as the AArch64 twin
    // saves `x9` before its release; reading it afterwards yielded class id 0 (the first class in
    // the table) and named the wrong class in the TypeError.
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // object pointer
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // class id from the object header
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // keep the class id across the owning-box release
    // The decoder published this object's owning Mixed box before parsing its body, so a
    // hydration TypeError must release it here: the throw unwinds past `__rt_unser_obj_fail_x`,
    // the only other place that drops the box, and `__rt_unserialize_end` never owns it.
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // partially hydrated object's owning Mixed box
    emitter.instruction("test rax, rax");                                       // was an owning box supplied?
    emitter.instruction("jz __rt_obj_store_prop_type_error_released_x");        // no owning box: nothing to release
    emitter.instruction("call __rt_decref_mixed");                              // release the box and its object ownership (pointer in rax)
    emitter.label("__rt_obj_store_prop_type_error_released_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // rejected value box
    emitter.instruction("mov rdi, QWORD PTR [r9 + 8]");                         // rejected value payload (object class resolution)
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // rejected value runtime tag
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // class id, read before the owning box was released
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // row index
    emitter.instruction("lea r11, [rip + _class_serpdiag_ptrs]");               // diagnostic pointer table
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // diagnostic rows for this class
    emitter.instruction("imul r9, r9, 32");                                     // diagnostic row stride
    emitter.instruction("add r11, r9");                                         // diagnostic row for this property
    emitter.instruction("mov r8, QWORD PTR [r11]");                             // message suffix pointer
    emitter.instruction("mov r9, QWORD PTR [r11 + 8]");                         // message suffix byte length
    emitter.instruction("lea rsi, [rip + _unser_property_assign_prefix]");      // "Cannot assign " prefix
    emitter.instruction(&format!("mov rdx, {}", UNSER_PROPERTY_ASSIGN_PREFIX.len())); // prefix byte length
    emitter.instruction("mov r10, 1");                                          // spell a bool value as true/false
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // hand the rejected value box to the helper for release
    emitter.instruction("add rsp, 96");                                         // drop the store frame so the error helper sees a normal entry
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("jmp __rt_unser_throw_type_error");                     // close the context and throw the TypeError
    emitter.label("__rt_obj_store_prop_next");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the row index
    emitter.instruction("add rax, 1");                                          // advance to the next row
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // persist the row index
    emitter.instruction("jmp __rt_obj_store_prop_loop");                        // continue scanning
    emitter.label("__rt_obj_store_prop_done");
    emitter.label("__rt_obj_store_prop_ret");
    emitter.instruction("add rsp, 96");                                         // deallocate the store frame
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
    emitter.instruction("call __rt_hash_iter_next_value");                      // rcx=value low, rax=next cursor
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// x86_64 rejects a hydrated value whose boxed tag the declared property type does not
    /// accept, routing it through the catchable unserialize TypeError helper instead of
    /// writing the mismatched payload.
    #[test]
    fn store_prop_rejects_a_mismatched_typed_property_on_x86_64() {
        let mut emitter = Emitter::new(Target::parse("linux-x86_64").unwrap());
        emit_object_storage(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("_class_serpdiag_ptrs"), "{asm}");
        assert!(asm.contains("__rt_obj_store_prop_type_error"), "{asm}");
        assert!(asm.contains("__rt_unser_throw_type_error"), "{asm}");
        assert!(asm.contains("_unser_property_assign_prefix"), "{asm}");
        // The helper reads r8 as the suffix pointer and r9 as its length, so the store path
        // must not swap them (a swap is invisible on aarch64 but corrupts the message here).
        let suffix_ptr = asm
            .find("mov r8, QWORD PTR [r11]")
            .expect("the suffix pointer must load into r8");
        let suffix_len = asm
            .find("mov r9, QWORD PTR [r11 + 8]")
            .expect("the suffix length must load into r9");
        assert!(
            suffix_ptr < suffix_len,
            "the store path passes the suffix pointer/length in the helper's order:\n{asm}"
        );
        // An accepted int into a float slot is widened, not reinterpreted.
        assert!(asm.contains("cvtsi2sd xmm0, r8"), "{asm}");
    }
}
