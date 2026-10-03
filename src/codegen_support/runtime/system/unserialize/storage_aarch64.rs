//! Purpose:
//! Emits AArch64 object-property hydration, hash conversion, and serialized-key parsing.
//!
//! Called from:
//! - `super::emit_unserialize()` after the recursive decoder and per-call context helpers.
//!
//! Key details:
//! - Parsed Mixed ownership is transferred into object slots or rebuilt indexed arrays without extra retains.
//! - Every store writes the slot's high word (the string length, or zero), which clears the
//!   uninitialized marker a typed property without a default starts with.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::runtime::data::UNSER_PROPERTY_ASSIGN_PREFIX;
use crate::codegen_support::sentinels::{
    TAGGED_SCALAR_PROPERTY_TAG, TAGGED_SCALAR_TAG_INT, TAGGED_SCALAR_TAG_NULL,
};

/// Emits AArch64 object-property storage and parsed-hash conversion helpers.
pub(super) fn emit_object_storage(emitter: &mut Emitter) {
    // -- __rt_obj_store_prop(x0=obj, x1=key_ptr, x2=key_len, x3=valbox): inject a property --
    // Matches the (mangled) key against the class's serialize property-info table and
    // stores the parsed value into the matching object slot per the property's tag.
    emitter.label_global("__rt_obj_store_prop");
    emitter.instruction("ldr x9, [x0]");                                        // class id from the object header
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_class_serprop_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // property-info table for this class
    emitter.instruction("ldr x11, [x10]");                                      // property count
    emitter.instruction("add x12, x10, #8");                                    // rows start (skip the count word)
    emitter.instruction("mov x13, #0");                                         // row index
    emitter.label("__rt_obj_store_prop_loop");
    emitter.instruction("cmp x13, x11");                                        // scanned every row?
    emitter.instruction("b.ge __rt_obj_store_prop_done");                       // unknown key is ignored
    emitter.instruction("add x14, x12, x13, lsl #5");                           // row = rows + index*32
    emitter.instruction("ldr x4, [x14]");                                       // row mangled key pointer
    emitter.instruction("ldr x5, [x14, #8]");                                   // row mangled key length
    emitter.instruction("cmp x5, x2");                                          // same length as the parsed key?
    emitter.instruction("b.ne __rt_obj_store_prop_next");                       // lengths differ, skip
    emitter.instruction("mov x6, #0");                                          // byte compare cursor
    emitter.label("__rt_obj_store_prop_cmp");
    emitter.instruction("cmp x6, x2");                                          // compared all bytes?
    emitter.instruction("b.ge __rt_obj_store_prop_match");                      // full match
    emitter.instruction("ldrb w7, [x4, x6]");                                   // row key byte
    emitter.instruction("ldrb w8, [x1, x6]");                                   // parsed key byte
    emitter.instruction("cmp w7, w8");                                          // bytes equal?
    emitter.instruction("b.ne __rt_obj_store_prop_next");                       // mismatch, skip this row
    emitter.instruction("add x6, x6, #1");                                      // next byte
    emitter.instruction("b __rt_obj_store_prop_cmp");                           // continue comparing
    emitter.label("__rt_obj_store_prop_match");
    // -- reject a hydrated value whose boxed tag the declared property type does not
    //    accept, raising PHP's TypeError instead of writing a mismatched payload --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_class_serpdiag_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // diagnostic rows for this class
    emitter.instruction("mov x11, #32");                                        // diagnostic row stride
    emitter.instruction("madd x10, x13, x11, x10");                             // diagnostic row = base + index*32
    emitter.instruction("ldr x12, [x10, #16]");                                 // accepted boxed value tags
    emitter.instruction("ldr x11, [x3]");                                       // boxed value tag
    emitter.instruction("mov x15, #1");                                         // probe bit for the boxed tag
    emitter.instruction("lsl x15, x15, x11");                                   // tag bit
    emitter.instruction("tst x12, x15");                                        // is the tag accepted for this property?
    emitter.instruction("b.eq __rt_obj_store_prop_type_error");                 // raise PHP's hydration TypeError
    // -- PHP widens an accepted int into a `float`/`?float` slot; retag the owned box --
    emitter.instruction("ldr x12, [x10, #24]");                                 // widen-int-to-float flag
    emitter.instruction("cbz x12, __rt_obj_store_prop_no_widen");               // most slots keep the parsed type
    emitter.instruction("cmp x11, #0");                                         // is the boxed value an int?
    emitter.instruction("b.ne __rt_obj_store_prop_no_widen");                   // only an int widens
    emitter.instruction("ldr x9, [x3, #8]");                                    // integer payload
    emitter.instruction("scvtf d0, x9");                                        // convert to the PHP float value
    emitter.instruction("fmov x9, d0");                                         // move the double bits into a GPR
    emitter.instruction("str x9, [x3, #8]");                                    // store the widened payload
    emitter.instruction("mov x9, #2");                                          // runtime tag 2 = float
    emitter.instruction("str x9, [x3]");                                        // retag the owned box as a float
    emitter.label("__rt_obj_store_prop_no_widen");
    emitter.instruction("ldr x6, [x14, #16]");                                  // property byte offset
    emitter.instruction("ldr x7, [x14, #24]");                                  // property value tag
    emitter.instruction("add x8, x0, x6");                                      // address of the property slot
    emitter.instruction("cmp x7, #7");                                          // is this a Mixed/untyped slot?
    emitter.instruction("b.eq __rt_obj_store_prop_mixed");                      // store the boxed cell directly
    emitter.instruction("cmp x7, #1");                                          // is this a string slot?
    emitter.instruction("b.eq __rt_obj_store_prop_str");                        // store pointer and length
    emitter.instruction("cmp x7, #4");                                          // is this an indexed-array slot?
    emitter.instruction("b.eq __rt_obj_store_prop_arr");                        // convert the parsed hash to an indexed array
    emitter.instruction(&format!("cmp x7, #{}", TAGGED_SCALAR_PROPERTY_TAG));   // is this an inline tagged-scalar (`?int`) slot?
    emitter.instruction("b.eq __rt_obj_store_prop_tagged");                     // store the payload and its runtime tag inline
    emitter.instruction("ldr x9, [x3, #8]");                                    // typed scalar/object/hash: unbox the low word
    emitter.instruction("str x9, [x8]");                                        // store it inline in the slot
    emitter.instruction("str xzr, [x8, #8]");                                   // the high word marks the typed property initialized
    emitter.instruction("ret");                                                 // property stored
    emitter.label("__rt_obj_store_prop_tagged");
    // Only an int or null reaches this arm: the declared-type check above already raised the
    // TypeError for a string, float, bool or array, so a non-int tag here is the canonical null.
    emitter.instruction("ldr x9, [x3]");                                        // boxed value tag
    emitter.instruction(&format!("cmp x9, #{}", TAGGED_SCALAR_TAG_INT));        // is the boxed value an int?
    emitter.instruction("b.ne __rt_obj_store_prop_tagged_null");                // null or a mismatched type: store the canonical tagged null pair
    emitter.instruction("ldr x9, [x3, #8]");                                    // unbox the integer payload
    emitter.instruction("str x9, [x8]");                                        // payload word of the tagged slot
    emitter.instruction(&format!("mov x9, #{}", TAGGED_SCALAR_TAG_INT));        // runtime tag of a non-null tagged int
    emitter.instruction("str x9, [x8, #8]");                                    // tag word of the tagged slot
    emitter.instruction("ret");                                                 // property stored
    emitter.label("__rt_obj_store_prop_tagged_null");
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x9", crate::codegen_support::NULL_SENTINEL);
    emitter.instruction("str x9, [x8]");                                        // canonical tagged-null payload word
    emitter.instruction(&format!("mov x9, #{}", TAGGED_SCALAR_TAG_NULL));       // runtime tag of PHP null
    emitter.instruction("str x9, [x8, #8]");                                    // tag word of the tagged slot
    emitter.instruction("ret");                                                 // property stored
    emitter.label("__rt_obj_store_prop_arr");
    emitter.instruction("stp x8, x30, [sp, #-16]!");                            // save the slot address and return address
    emitter.instruction("ldr x0, [x3, #8]");                                    // parsed hash pointer (box low word)
    emitter.instruction("bl __rt_hash_to_indexed_array");                       // materialize a native indexed array
    emitter.instruction("ldp x8, x30, [sp], #16");                              // restore the slot address and return address
    emitter.instruction("str x0, [x8]");                                        // store the indexed-array pointer
    emitter.instruction("str xzr, [x8, #8]");                                   // the high word marks the typed property initialized
    emitter.instruction("ret");                                                 // property stored
    emitter.label("__rt_obj_store_prop_str");
    emitter.instruction("ldr x9, [x3, #8]");                                    // string pointer from the box
    emitter.instruction("str x9, [x8]");                                        // store the string pointer
    emitter.instruction("ldr x9, [x3, #16]");                                   // string length from the box
    emitter.instruction("str x9, [x8, #8]");                                    // store the string length
    emitter.instruction("ret");                                                 // property stored
    // A parsed null keeps its boxed tag-8 cell like every other value: the in-band
    // NULL_SENTINEL is not a Mixed cell pointer, and every reader of the slot
    // (`=== null`, var_dump, json_encode) dereferenced it.
    emitter.label("__rt_obj_store_prop_mixed");
    emitter.instruction("str x3, [x8]");                                        // store the boxed Mixed cell pointer
    emitter.instruction("str xzr, [x8, #8]");                                   // the high word marks the typed property initialized
    emitter.instruction("ret");                                                 // property stored
    // -- declared-type mismatch: compose and throw PHP's hydration TypeError --
    emitter.label("__rt_obj_store_prop_type_error");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x4", "_class_serpdiag_ptrs");
    emitter.instruction("ldr x4, [x4, x9, lsl #3]");                            // diagnostic rows for this class
    emitter.instruction("mov x5, #32");                                         // diagnostic row stride
    emitter.instruction("madd x4, x13, x5, x4");                                // diagnostic row for this property
    emitter.instruction("ldr x5, [x4, #8]");                                    // message suffix byte length
    emitter.instruction("ldr x4, [x4]");                                        // message suffix pointer
    emitter.instruction("ldr x1, [x3, #8]");                                    // rejected value payload (object class resolution)
    emitter.instruction("ldr x0, [x3]");                                        // rejected value runtime tag
    crate::codegen_support::abi::emit_symbol_address(emitter, "x2", "_unser_property_assign_prefix");
    emitter.instruction("mov x7, x3");                                          // hand the rejected value box to the helper for release
    emitter.instruction(&format!("mov x3, #{}", UNSER_PROPERTY_ASSIGN_PREFIX.len())); // "Cannot assign " byte length
    emitter.instruction("mov x6, #1");                                          // spell a bool value as true/false
    emitter.instruction("b __rt_unser_throw_type_error");                       // close the context and throw the TypeError
    emitter.label("__rt_obj_store_prop_next");
    emitter.instruction("add x13, x13, #1");                                    // advance to the next row
    emitter.instruction("b __rt_obj_store_prop_loop");                          // continue scanning
    emitter.label("__rt_obj_store_prop_done");
    emitter.instruction("ret");                                                 // no matching property, ignore the value

    // -- __rt_hash_to_indexed_array(x0=hash) -> x0=indexed array: rebuild a parsed
    // hash (with boxed-Mixed values) as a native value_type-7 indexed array so
    // indexed-array-typed property slots match what property access expects. --
    emitter.label_global("__rt_hash_to_indexed_array");
    emitter.instruction("stp x29, x30, [sp, #-48]!");                           // open the conversion frame
    emitter.instruction("mov x29, sp");                                         // set the frame pointer
    emitter.instruction("stp x19, x20, [sp, #16]");                             // save callee-saved temporaries
    emitter.instruction("str x21, [sp, #32]");                                  // save callee-saved cursor
    emitter.instruction("mov x19, x0");                                         // hash pointer
    emitter.instruction("mov x0, #0");                                          // initial capacity 0
    emitter.instruction("mov x1, #8");                                          // 8-byte element slots
    emitter.instruction("bl __rt_array_new");                                   // allocate an empty indexed array
    emitter.instruction("mov x20, x0");                                         // destination array pointer
    emitter.instruction("mov x21, #0");                                         // hash iteration cursor
    emitter.label("__rt_hash_to_indexed_array_loop");
    emitter.instruction("mov x0, x19");                                         // hash pointer
    emitter.instruction("mov x1, x21");                                         // resume cursor
    emitter.instruction("bl __rt_hash_iter_next_value");                        // x3=value low, x5=value tag, x0=next cursor
    emitter.instruction("cmn x0, #1");                                          // cursor == -1 (iteration done)?
    emitter.instruction("b.eq __rt_hash_to_indexed_array_done");                // stop when exhausted
    emitter.instruction("mov x21, x0");                                         // save the resume cursor
    emitter.instruction("mov x0, x20");                                         // destination array
    emitter.instruction("mov x1, x3");                                          // boxed-Mixed value pointer (parsed-hash value)
    emitter.instruction("bl __rt_array_push_refcounted");                       // append, transferring ownership
    emitter.instruction("mov x20, x0");                                         // array may move on COW growth
    emitter.instruction("b __rt_hash_to_indexed_array_loop");                   // continue iterating
    emitter.label("__rt_hash_to_indexed_array_done");
    emitter.instruction("mov x0, x20");                                         // return the indexed array
    emitter.instruction("ldr x21, [sp, #32]");                                  // restore the cursor register
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore the temporaries
    emitter.instruction("ldp x29, x30, [sp], #48");                             // close the conversion frame
    emitter.instruction("ret");                                                 // return the converted array
}

/// Emits the AArch64 leaf key parser `__rt_unser_key`.
///
/// Input: `x0`=base, `x1`=pos, `x2`=end. Output: `x0`=key_lo (int value or string
/// pointer), `x1`=key_hi (-1 for an integer key, else the string byte length), `x2`=newpos.
/// String key pointers are borrowed into the source buffer; `__rt_hash_set` persists them.
pub(super) fn emit_key(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unser_key (serialize() array key parser, leaf) ---");
    emitter.label_global("__rt_unser_key");
    emitter.instruction("cmp x1, x2");                                          // require a key type byte before loading it
    emitter.instruction("b.hs __rt_unser_key_fail");                            // return a sentinel cursor for a truncated key
    emitter.instruction("ldrb w9, [x0, x1]");                                   // load the key type byte
    emitter.instruction("cmp w9, #105");                                        // ASCII 'i' (integer key)?
    emitter.instruction("b.eq __rt_unser_key_int");                             // parse an integer key
    // -- string key: "s:" + bytelen + ":\"" + raw + "\";" --
    emitter.instruction("add x10, x0, x1");                                     // pointer to the type byte
    emitter.instruction("add x10, x10, #2");                                    // skip "s:" to the length digits
    emitter.instruction("mov x11, #0");                                         // length accumulator
    emitter.label("__rt_unser_key_strlen");
    emitter.instruction("ldrb w9, [x10]");                                      // next length byte
    emitter.instruction("cmp w9, #48");                                         // below '0'?
    emitter.instruction("b.lt __rt_unser_key_strlen_done");                     // ':' terminator reached
    emitter.instruction("cmp w9, #57");                                         // above '9'?
    emitter.instruction("b.gt __rt_unser_key_strlen_done");                     // ':' terminator reached
    emitter.instruction("sub w9, w9, #48");                                     // digit value
    emitter.instruction("mov x12, #10");                                        // decimal base
    emitter.instruction("mul x11, x11, x12");                                   // shift accumulator
    emitter.instruction("add x11, x11, x9");                                    // add digit
    emitter.instruction("add x10, x10, #1");                                    // advance cursor
    emitter.instruction("b __rt_unser_key_strlen");                             // continue
    emitter.label("__rt_unser_key_strlen_done");
    emitter.instruction("add x10, x10, #2");                                    // skip ':' and opening '\"' to the raw bytes
    emitter.instruction("add x12, x10, x11");                                   // raw end = raw + len
    emitter.instruction("add x12, x12, #2");                                    // skip closing '\"' and ';'
    emitter.instruction("sub x2, x12, x0");                                     // newpos = (raw end + 2) - base
    emitter.instruction("mov x1, x11");                                         // key_hi = string byte length
    emitter.instruction("mov x0, x10");                                         // key_lo = borrowed raw string pointer
    emitter.instruction("ret");                                                 // return the string key
    // -- integer key: "i:" + optional '-' + digits + ";" --
    emitter.label("__rt_unser_key_int");
    emitter.instruction("add x10, x0, x1");                                     // pointer to the type byte
    emitter.instruction("add x10, x10, #2");                                    // skip "i:" to the first digit
    emitter.instruction("mov x11, #0");                                         // digit accumulator
    emitter.instruction("mov x13, #0");                                         // negative-sign flag
    emitter.instruction("ldrb w9, [x10]");                                      // first numeric byte
    emitter.instruction("cmp w9, #45");                                         // leading '-'?
    emitter.instruction("b.ne __rt_unser_key_int_loop");                        // no sign
    emitter.instruction("mov x13, #1");                                         // record negative sign
    emitter.instruction("add x10, x10, #1");                                    // skip '-'
    emitter.label("__rt_unser_key_int_loop");
    emitter.instruction("ldrb w9, [x10]");                                      // next numeric byte
    emitter.instruction("cmp w9, #48");                                         // below '0'?
    emitter.instruction("b.lt __rt_unser_key_int_done");                        // ';' terminator reached
    emitter.instruction("cmp w9, #57");                                         // above '9'?
    emitter.instruction("b.gt __rt_unser_key_int_done");                        // ';' terminator reached
    emitter.instruction("sub w9, w9, #48");                                     // digit value
    emitter.instruction("mov x12, #10");                                        // decimal base
    emitter.instruction("mul x11, x11, x12");                                   // shift accumulator
    emitter.instruction("add x11, x11, x9");                                    // add digit
    emitter.instruction("add x10, x10, #1");                                    // advance cursor
    emitter.instruction("b __rt_unser_key_int_loop");                           // continue
    emitter.label("__rt_unser_key_int_done");
    emitter.instruction("cbz x13, __rt_unser_key_int_pos");                     // not signed
    emitter.instruction("neg x11, x11");                                        // apply sign
    emitter.label("__rt_unser_key_int_pos");
    emitter.instruction("sub x2, x10, x0");                                     // newpos = cursor - base
    emitter.instruction("add x2, x2, #1");                                      // skip the ';'
    emitter.instruction("mov x0, x11");                                         // key_lo = integer key value
    emitter.instruction("mov x1, #-1");                                         // key_hi = -1 marks an integer key
    emitter.instruction("ret");                                                 // return the integer key
    emitter.label("__rt_unser_key_fail");
    emitter.instruction("mov x0, #0");                                          // clear key payload on failure
    emitter.instruction("mov x1, #0");                                          // clear key metadata on failure
    emitter.instruction("add x2, x2, #1");                                      // end+1 is an impossible valid cursor
    emitter.instruction("ret");                                                 // caller/preflight rejects the sentinel
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every AArch64 target rejects a hydrated value whose boxed tag the declared property
    /// type does not accept, routing it through the catchable unserialize TypeError helper
    /// instead of writing the mismatched payload.
    #[test]
    fn store_prop_rejects_a_mismatched_typed_property_on_every_aarch64_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_object_storage(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("_class_serpdiag_ptrs"), "{name}: {asm}");
            assert!(asm.contains("__rt_obj_store_prop_type_error"), "{name}: {asm}");
            assert!(asm.contains("__rt_unser_throw_type_error"), "{name}: {asm}");
            assert!(asm.contains("_unser_property_assign_prefix"), "{name}: {asm}");
            // An accepted int into a float slot is widened, not reinterpreted.
            assert!(asm.contains("scvtf d0, x9"), "{name}: {asm}");
        }
    }
}
