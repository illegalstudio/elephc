//! Purpose:
//! Emits the `__rt_unserialize_mixed` runtime helper (and its internal cursor-based
//! recursive parser `__rt_unser_at` / key parser `__rt_unser_key`) that parse a PHP
//! `serialize()` wire string into a freshly boxed Mixed value.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//! - The EIR `unserialize()` lowering in `crate::codegen_ir::lower_inst::builtins::system`.
//!
//! Key details:
//! - Recognizes scalars `N;`, `b:0;`/`b:1;`, `i:<int>;`, `d:<float>;` (incl.
//!   `INF`/`-INF`/`NAN`), `s:<bytelen>:"<raw>";`, and arrays `a:n:{<key><val>...}`.
//!   Objects (`O:`) and references (`r:`) are unsupported (yield a null result so the
//!   builtin returns PHP `false`).
//! - Arrays build a hash (`__rt_hash_new` value_type 7) whose values are boxed Mixed
//!   cells stored with per-entry tag 7 — the canonical heterogeneous representation
//!   (see `__rt_array_to_mixed`). Ownership: scalar/value boxes come from
//!   `__rt_mixed_from_value` (persists strings) and are transferred into the hash by
//!   `__rt_hash_set` (which does not incref values); string keys are borrowed and
//!   persisted by `__rt_hash_set`; the finished hash is boxed without an extra incref.
//! - Integer/length digits are parsed inline; floats reuse libc `strtod` (endptr gives
//!   the consumed span; it stops at `;` and natively handles `INF`/`-INF`/`NAN`).

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_unserialize_mixed` plus its internal parser helpers.
///
/// `__rt_unserialize_mixed` input: AArch64 `x1`=ptr, `x2`=len; x86_64 `rax`=ptr,
/// `rdx`=len. Output: AArch64 `x0` / x86_64 `rax` = boxed Mixed pointer, or 0 on a
/// parse error or unsupported wire form (the caller boxes that as PHP `false`).
pub(crate) fn emit_unserialize(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_unserialize_x86_64(emitter);
        return;
    }
    emit_unserialize_aarch64(emitter);
}

/// AArch64 implementation of the unserialize entry, recursive parser, and key parser.
fn emit_unserialize_aarch64(emitter: &mut Emitter) {
    // -- entry wrapper: __rt_unser_at(base=ptr, pos=0, end=len) --
    emitter.blank();
    emitter.comment("--- runtime: unserialize_mixed (serialize() wire -> boxed Mixed) ---");
    emitter.label_global("__rt_unserialize_mixed");
    emitter.instruction("mov x0, x1");                                          // base = source string pointer
    emitter.instruction("mov x2, x2");                                          // end = source string length (already in x2)
    emitter.instruction("mov x1, #0");                                          // start parsing at position 0
    emitter.instruction("b __rt_unser_at");                                     // tail-call the recursive parser (returns box in x0)

    // -- __rt_unser_at(base=x0, pos=x1, end=x2) -> x0=boxed Mixed (0 on fail), x1=newpos --
    emitter.blank();
    emitter.comment("--- runtime: unser_at (recursive serialize() value parser) ---");
    emitter.label_global("__rt_unser_at");
    // [sp+0]=base [8]=pos [16]=end [24]=hash [32]=count [40]=index [48]=key_lo [56]=key_hi [64]=scratch
    emitter.instruction("sub sp, sp, #112");                                    // recursive parser frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the base pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the current position
    emitter.instruction("str x2, [sp, #16]");                                   // save the end position
    emitter.instruction("cmp x1, x2");                                          // is the cursor already at/past the end?
    emitter.instruction("b.ge __rt_unser_at_fail");                             // nothing left to parse
    emitter.instruction("ldrb w9, [x0, x1]");                                   // load the leading type byte
    // -- back-reference? r:N; (object identity) or R:N; (PHP reference) resolves
    //    to a previously parsed value and consumes no new index --
    emitter.instruction("cmp w9, #114");                                        // ASCII 'r'?
    emitter.instruction("b.eq __rt_unser_at_ref");                              // resolve an object back-reference
    emitter.instruction("cmp w9, #82");                                         // ASCII 'R'?
    emitter.instruction("b.eq __rt_unser_at_ref");                              // resolve a PHP reference
    // -- every other value consumes the next pre-order index, mirroring the
    //    counter serialize() used, so r:/R: targets line up by index --
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_unser_count");
    emitter.instruction("ldr x11, [x10]");                                      // current value index
    emitter.instruction("str x11, [sp, #88]");                                  // reserve this value's index
    emitter.instruction("add x11, x11, #1");                                    // advance the registry counter
    emitter.instruction("str x11, [x10]");                                      // publish the advanced counter
    emitter.instruction("cmp w9, #78");                                         // ASCII 'N' (null)?
    emitter.instruction("b.eq __rt_unser_at_null");                             // parse null
    emitter.instruction("cmp w9, #98");                                         // ASCII 'b' (bool)?
    emitter.instruction("b.eq __rt_unser_at_bool");                             // parse bool
    emitter.instruction("cmp w9, #105");                                        // ASCII 'i' (int)?
    emitter.instruction("b.eq __rt_unser_at_int");                              // parse int
    emitter.instruction("cmp w9, #100");                                        // ASCII 'd' (float)?
    emitter.instruction("b.eq __rt_unser_at_float");                            // parse float
    emitter.instruction("cmp w9, #115");                                        // ASCII 's' (string)?
    emitter.instruction("b.eq __rt_unser_at_str");                              // parse string
    emitter.instruction("cmp w9, #97");                                         // ASCII 'a' (array)?
    emitter.instruction("b.eq __rt_unser_at_array");                            // parse array
    emitter.instruction("cmp w9, #79");                                         // ASCII 'O' (object)?
    emitter.instruction("b.eq __rt_unser_at_object");                           // parse object
    emitter.instruction("b __rt_unser_at_fail");                                // unsupported wire form

    // -- null: "N;" --
    emitter.label("__rt_unser_at_null");
    emitter.instruction("mov x0, #8");                                          // value tag = null
    emitter.instruction("mov x1, #0");                                          // null payload low word
    emitter.instruction("mov x2, #0");                                          // null payload high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box the null value
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload position
    emitter.instruction("add x1, x1, #2");                                      // newpos skips "N;"
    emitter.instruction("b __rt_unser_at_ret");                                 // return the box and new position

    // -- bool: "b:0;" / "b:1;" --
    emitter.label("__rt_unser_at_bool");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload base
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload position
    emitter.instruction("add x12, x10, x11");                                   // pointer to the type byte
    emitter.instruction("ldrb w9, [x12, #2]");                                  // load the bool digit at offset 2
    emitter.instruction("sub w9, w9, #48");                                     // ASCII '0'/'1' -> 0/1
    emitter.instruction("and x1, x9, #1");                                      // clamp to a single bool bit
    emitter.instruction("mov x0, #3");                                          // value tag = bool
    emitter.instruction("mov x2, #0");                                          // bool high payload unused
    emitter.instruction("bl __rt_mixed_from_value");                            // box the bool value
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload position
    emitter.instruction("add x1, x1, #4");                                      // newpos skips "b:X;"
    emitter.instruction("b __rt_unser_at_ret");                                 // return the box and new position

    // -- int: "i:" + optional '-' + digits + ";" --
    emitter.label("__rt_unser_at_int");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload base
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload position
    emitter.instruction("add x10, x10, x11");                                   // pointer to the type byte
    emitter.instruction("add x10, x10, #2");                                    // skip "i:" to the first digit
    emitter.instruction("mov x11, #0");                                         // digit accumulator
    emitter.instruction("mov x12, #0");                                         // negative-sign flag
    emitter.instruction("ldrb w9, [x10]");                                      // first numeric byte
    emitter.instruction("cmp w9, #45");                                         // leading '-'?
    emitter.instruction("b.ne __rt_unser_at_int_loop");                         // no sign
    emitter.instruction("mov x12, #1");                                         // record negative sign
    emitter.instruction("add x10, x10, #1");                                    // skip '-'
    emitter.label("__rt_unser_at_int_loop");
    emitter.instruction("ldrb w9, [x10]");                                      // next numeric byte
    emitter.instruction("cmp w9, #48");                                         // below '0'?
    emitter.instruction("b.lt __rt_unser_at_int_done");                         // terminator reached
    emitter.instruction("cmp w9, #57");                                         // above '9'?
    emitter.instruction("b.gt __rt_unser_at_int_done");                         // terminator reached
    emitter.instruction("sub w9, w9, #48");                                     // digit value
    emitter.instruction("mov x13, #10");                                        // decimal base
    emitter.instruction("mul x11, x11, x13");                                   // shift accumulator
    emitter.instruction("add x11, x11, x9");                                    // add digit
    emitter.instruction("add x10, x10, #1");                                    // advance cursor
    emitter.instruction("b __rt_unser_at_int_loop");                            // continue
    emitter.label("__rt_unser_at_int_done");
    emitter.instruction("cbz x12, __rt_unser_at_int_box");                      // not signed
    emitter.instruction("neg x11, x11");                                        // apply sign
    emitter.label("__rt_unser_at_int_box");
    emitter.instruction("str x10, [sp, #64]");                                  // save the cursor (at ';') across the box call
    emitter.instruction("mov x1, x11");                                         // value payload = parsed int
    emitter.instruction("mov x0, #0");                                          // value tag = int
    emitter.instruction("mov x2, #0");                                          // int high payload unused
    emitter.instruction("bl __rt_mixed_from_value");                            // box the int value
    emitter.instruction("ldr x10, [sp, #64]");                                  // reload the cursor
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload base
    emitter.instruction("sub x1, x10, x9");                                     // newpos = cursor - base
    emitter.instruction("add x1, x1, #1");                                      // skip the ';'
    emitter.instruction("b __rt_unser_at_ret");                                 // return the box and new position

    // -- float: "d:" + (INF/-INF/NAN | digits) + ";" --
    emitter.label("__rt_unser_at_float");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload base
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload position
    emitter.instruction("add x0, x10, x11");                                    // pointer to the type byte
    emitter.instruction("add x0, x0, #2");                                      // strtod source = first byte after "d:"
    emitter.instruction("add x1, sp, #64");                                     // strtod endptr = &scratch
    emitter.bl_c("strtod"); // parse the float (stops at ';') -> d0, scratch=endptr
    emitter.instruction("fmov x9, d0");                                         // move the parsed double into a GPR
    emitter.instruction("mov x1, x9");                                          // value payload = float bits
    emitter.instruction("mov x0, #2");                                          // value tag = float
    emitter.instruction("mov x2, #0");                                          // float high payload unused
    emitter.instruction("bl __rt_mixed_from_value");                            // box the float value
    emitter.instruction("ldr x10, [sp, #64]");                                  // reload the strtod endptr
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload base
    emitter.instruction("sub x1, x10, x9");                                     // newpos = endptr - base
    emitter.instruction("add x1, x1, #1");                                      // skip the ';'
    emitter.instruction("b __rt_unser_at_ret");                                 // return the box and new position

    // -- string: "s:" + bytelen + ":\"" + raw + "\";" --
    emitter.label("__rt_unser_at_str");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload base
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload position
    emitter.instruction("add x10, x10, x11");                                   // pointer to the type byte
    emitter.instruction("add x10, x10, #2");                                    // skip "s:" to the length digits
    emitter.instruction("mov x11, #0");                                         // length accumulator
    emitter.label("__rt_unser_at_strlen");
    emitter.instruction("ldrb w9, [x10]");                                      // next length byte
    emitter.instruction("cmp w9, #48");                                         // below '0'?
    emitter.instruction("b.lt __rt_unser_at_strlen_done");                      // ':' terminator reached
    emitter.instruction("cmp w9, #57");                                         // above '9'?
    emitter.instruction("b.gt __rt_unser_at_strlen_done");                      // ':' terminator reached
    emitter.instruction("sub w9, w9, #48");                                     // digit value
    emitter.instruction("mov x13, #10");                                        // decimal base
    emitter.instruction("mul x11, x11, x13");                                   // shift accumulator
    emitter.instruction("add x11, x11, x9");                                    // add digit
    emitter.instruction("add x10, x10, #1");                                    // advance cursor
    emitter.instruction("b __rt_unser_at_strlen");                              // continue
    emitter.label("__rt_unser_at_strlen_done");
    emitter.instruction("add x10, x10, #2");                                    // skip ':' and opening '\"' to the raw bytes
    emitter.instruction("add x9, x10, x11");                                    // raw end = raw + len
    emitter.instruction("str x9, [sp, #64]");                                   // save raw end across the box call
    emitter.instruction("mov x1, x10");                                         // string payload pointer = raw bytes
    emitter.instruction("mov x2, x11");                                         // string payload length
    emitter.instruction("mov x0, #1");                                          // value tag = string (mixed_from_value persists it)
    emitter.instruction("bl __rt_mixed_from_value");                            // box an owned copy of the string
    emitter.instruction("ldr x10, [sp, #64]");                                  // reload raw end
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload base
    emitter.instruction("sub x1, x10, x9");                                     // newpos = raw end - base
    emitter.instruction("add x1, x1, #2");                                      // skip closing '\"' and ';'
    emitter.instruction("b __rt_unser_at_ret");                                 // return the box and new position

    // -- array: "a:" + count + ":{" + count*(key value) + "}" --
    emitter.label("__rt_unser_at_array");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload base
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload position
    emitter.instruction("add x10, x10, x11");                                   // pointer to the type byte
    emitter.instruction("add x10, x10, #2");                                    // skip "a:" to the count digits
    emitter.instruction("mov x11, #0");                                         // count accumulator
    emitter.label("__rt_unser_at_count");
    emitter.instruction("ldrb w9, [x10]");                                      // next count byte
    emitter.instruction("cmp w9, #48");                                         // below '0'?
    emitter.instruction("b.lt __rt_unser_at_count_done");                       // ':' terminator reached
    emitter.instruction("cmp w9, #57");                                         // above '9'?
    emitter.instruction("b.gt __rt_unser_at_count_done");                       // ':' terminator reached
    emitter.instruction("sub w9, w9, #48");                                     // digit value
    emitter.instruction("mov x13, #10");                                        // decimal base
    emitter.instruction("mul x11, x11, x13");                                   // shift accumulator
    emitter.instruction("add x11, x11, x9");                                    // add digit
    emitter.instruction("add x10, x10, #1");                                    // advance cursor
    emitter.instruction("b __rt_unser_at_count");                               // continue
    emitter.label("__rt_unser_at_count_done");
    emitter.instruction("str x11, [sp, #32]");                                  // save the entry count
    emitter.instruction("add x10, x10, #2");                                    // skip ':' and '{' to the body
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload base
    emitter.instruction("sub x12, x10, x9");                                    // body position offset
    emitter.instruction("str x12, [sp, #8]");                                   // advance the cursor to the body
    emitter.instruction("mov x0, x11");                                         // hash capacity = entry count
    emitter.instruction("mov x1, #7");                                          // hash value_type = boxed Mixed
    emitter.instruction("bl __rt_hash_new");                                    // allocate the destination hash
    emitter.instruction("str x0, [sp, #24]");                                   // save the hash pointer
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize the entry index
    emitter.label("__rt_unser_at_array_loop");
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the entry index
    emitter.instruction("ldr x3, [sp, #32]");                                   // reload the entry count
    emitter.instruction("cmp x4, x3");                                          // all entries parsed?
    emitter.instruction("b.ge __rt_unser_at_array_close");                      // box the hash when done
    emitter.instruction("ldr x0, [sp, #0]");                                    // base
    emitter.instruction("ldr x1, [sp, #8]");                                    // current position
    emitter.instruction("ldr x2, [sp, #16]");                                   // end
    emitter.instruction("bl __rt_unser_key");                                   // parse the key -> x0=key_lo, x1=key_hi, x2=newpos
    emitter.instruction("str x0, [sp, #48]");                                   // save key_lo
    emitter.instruction("str x1, [sp, #56]");                                   // save key_hi
    emitter.instruction("str x2, [sp, #8]");                                    // advance past the key
    emitter.instruction("ldr x0, [sp, #0]");                                    // base
    emitter.instruction("ldr x1, [sp, #8]");                                    // position after the key
    emitter.instruction("ldr x2, [sp, #16]");                                   // end
    emitter.instruction("bl __rt_unser_at");                                    // recursively parse the value -> x0=box, x1=newpos
    emitter.instruction("str x1, [sp, #8]");                                    // advance past the value
    emitter.instruction("mov x3, x0");                                          // value_lo = parsed value box
    emitter.instruction("ldr x0, [sp, #24]");                                   // hash pointer
    emitter.instruction("ldr x1, [sp, #48]");                                   // key_lo
    emitter.instruction("ldr x2, [sp, #56]");                                   // key_hi (-1 for int keys)
    emitter.instruction("mov x4, #0");                                          // value_hi unused
    emitter.instruction("mov x5, #7");                                          // value tag = boxed Mixed (transfer the box)
    emitter.instruction("bl __rt_hash_set");                                    // insert the entry -> x0 = (possibly new) hash
    emitter.instruction("str x0, [sp, #24]");                                   // save the updated hash pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the entry index
    emitter.instruction("add x4, x4, #1");                                      // advance the entry index
    emitter.instruction("str x4, [sp, #40]");                                   // persist the entry index
    emitter.instruction("b __rt_unser_at_array_loop");                          // continue with the next entry
    emitter.label("__rt_unser_at_array_close");
    emitter.instruction("mov x0, #24");                                         // box the hash: Mixed cell = tag + two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the boxed Mixed cell
    emitter.instruction("mov x9, #5");                                          // heap kind 5 = boxed Mixed cell
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the heap header
    emitter.instruction("mov x9, #5");                                          // value tag 5 = associative array (hash)
    emitter.instruction("str x9, [x0]");                                        // store the value tag
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the hash pointer
    emitter.instruction("str x9, [x0, #8]");                                    // store the hash pointer (ownership transferred, no incref)
    emitter.instruction("str xzr, [x0, #16]");                                  // clear the high payload word
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload position (at the closing '}')
    emitter.instruction("add x1, x1, #1");                                      // newpos skips the '}'
    emitter.instruction("b __rt_unser_at_ret");                                 // return the box and new position

    // -- object: "O:" + namelen + ":\"" + class + "\":" + count + ":{" + count*(key value) + "}" --
    emitter.label("__rt_unser_at_object");
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload base
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload position
    emitter.instruction("add x10, x10, x11");                                   // pointer to the type byte
    emitter.instruction("add x10, x10, #2");                                    // skip "O:" to the class-name length digits
    emitter.instruction("mov x11, #0");                                         // class-name length accumulator
    emitter.label("__rt_unser_at_obj_namelen");
    emitter.instruction("ldrb w9, [x10]");                                      // next length byte
    emitter.instruction("cmp w9, #48");                                         // below '0'?
    emitter.instruction("b.lt __rt_unser_at_obj_namelen_done");                 // ':' terminator reached
    emitter.instruction("cmp w9, #57");                                         // above '9'?
    emitter.instruction("b.gt __rt_unser_at_obj_namelen_done");                 // ':' terminator reached
    emitter.instruction("sub w9, w9, #48");                                     // digit value
    emitter.instruction("mov x13, #10");                                        // decimal base
    emitter.instruction("mul x11, x11, x13");                                   // shift accumulator
    emitter.instruction("add x11, x11, x9");                                    // add digit
    emitter.instruction("add x10, x10, #1");                                    // advance cursor
    emitter.instruction("b __rt_unser_at_obj_namelen");                         // continue
    emitter.label("__rt_unser_at_obj_namelen_done");
    emitter.instruction("add x10, x10, #2");                                    // skip ':' and opening '\"' to the class name bytes
    emitter.instruction("add x12, x10, x11");                                   // class-name end = name + len
    emitter.instruction("str x12, [sp, #64]");                                  // save the class-name end across the call
    emitter.instruction("mov x1, x10");                                         // class-name pointer (new_by_name arg)
    emitter.instruction("mov x2, x11");                                         // class-name length (new_by_name arg)
    emitter.instruction("bl __rt_new_by_name");                                 // instantiate the class by name (0 on unknown class)
    emitter.instruction("cbz x0, __rt_unser_at_fail");                          // unknown class fails the parse
    emitter.instruction("str x0, [sp, #24]");                                   // save the new object pointer
    emitter.instruction("ldr x12, [sp, #64]");                                  // reload the class-name end
    emitter.instruction("add x12, x12, #2");                                    // skip closing '\"' and ':' to the property count
    emitter.instruction("mov x11, #0");                                         // property-count accumulator
    emitter.label("__rt_unser_at_obj_count");
    emitter.instruction("ldrb w9, [x12]");                                      // next count byte
    emitter.instruction("cmp w9, #48");                                         // below '0'?
    emitter.instruction("b.lt __rt_unser_at_obj_count_done");                   // ':' terminator reached
    emitter.instruction("cmp w9, #57");                                         // above '9'?
    emitter.instruction("b.gt __rt_unser_at_obj_count_done");                   // ':' terminator reached
    emitter.instruction("sub w9, w9, #48");                                     // digit value
    emitter.instruction("mov x13, #10");                                        // decimal base
    emitter.instruction("mul x11, x11, x13");                                   // shift accumulator
    emitter.instruction("add x11, x11, x9");                                    // add digit
    emitter.instruction("add x12, x12, #1");                                    // advance cursor
    emitter.instruction("b __rt_unser_at_obj_count");                           // continue
    emitter.label("__rt_unser_at_obj_count_done");
    emitter.instruction("str x11, [sp, #32]");                                  // save the property count
    emitter.instruction("add x12, x12, #2");                                    // skip ':' and '{' to the body
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload base
    emitter.instruction("sub x12, x12, x9");                                    // body position offset
    emitter.instruction("str x12, [sp, #8]");                                   // advance the cursor to the body
    // -- __unserialize magic: parse the body into an assoc array, then call
    //    __unserialize($this, $data) instead of injecting properties by name --
    emitter.instruction("ldr x9, [sp, #24]");                                   // object pointer
    emitter.instruction("ldr x9, [x9]");                                        // class id from the object header
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_class_unserialize_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // __unserialize method symbol (0 if none)
    emitter.instruction("cbz x10, __rt_unser_obj_default");                     // no __unserialize → inject properties by name
    emitter.instruction("str x10, [sp, #72]");                                  // park the __unserialize target across the body parse
    emitter.instruction("ldr x0, [sp, #32]");                                   // entry count = hash capacity hint
    emitter.instruction("mov x1, #7");                                          // hash value_type = boxed Mixed
    emitter.instruction("bl __rt_hash_new");                                    // allocate the $data hash
    emitter.instruction("str x0, [sp, #80]");                                   // save the $data hash pointer
    emitter.instruction("str xzr, [sp, #40]");                                  // entry index = 0
    emitter.label("__rt_unser_obj_data_loop");
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the entry index
    emitter.instruction("ldr x3, [sp, #32]");                                   // reload the entry count
    emitter.instruction("cmp x4, x3");                                          // all entries parsed?
    emitter.instruction("b.ge __rt_unser_obj_data_done");                       // call __unserialize when done
    emitter.instruction("ldr x0, [sp, #0]");                                    // base
    emitter.instruction("ldr x1, [sp, #8]");                                    // current position
    emitter.instruction("ldr x2, [sp, #16]");                                   // end
    emitter.instruction("bl __rt_unser_key");                                   // parse the key -> x0=key_lo, x1=key_hi, x2=newpos
    emitter.instruction("str x0, [sp, #48]");                                   // save key_lo
    emitter.instruction("str x1, [sp, #56]");                                   // save key_hi
    emitter.instruction("str x2, [sp, #8]");                                    // advance past the key
    emitter.instruction("ldr x0, [sp, #0]");                                    // base
    emitter.instruction("ldr x1, [sp, #8]");                                    // position after the key
    emitter.instruction("ldr x2, [sp, #16]");                                   // end
    emitter.instruction("bl __rt_unser_at");                                    // recursively parse the value -> x0=box, x1=newpos
    emitter.instruction("str x1, [sp, #8]");                                    // advance past the value
    emitter.instruction("mov x3, x0");                                          // value_lo = parsed value box
    emitter.instruction("ldr x0, [sp, #80]");                                   // $data hash pointer
    emitter.instruction("ldr x1, [sp, #48]");                                   // key_lo
    emitter.instruction("ldr x2, [sp, #56]");                                   // key_hi (-1 for int keys)
    emitter.instruction("mov x4, #0");                                          // value_hi unused
    emitter.instruction("mov x5, #7");                                          // value tag = boxed Mixed (transfer the box)
    emitter.instruction("bl __rt_hash_set");                                    // insert the entry -> x0 = (possibly new) hash
    emitter.instruction("str x0, [sp, #80]");                                   // save the updated $data hash pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the entry index
    emitter.instruction("add x4, x4, #1");                                      // advance the entry index
    emitter.instruction("str x4, [sp, #40]");                                   // persist the entry index
    emitter.instruction("b __rt_unser_obj_data_loop");                          // continue with the next entry
    emitter.label("__rt_unser_obj_data_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // $this receiver = first argument
    emitter.instruction("ldr x1, [sp, #80]");                                   // $data assoc array (bare hash) = second argument
    emitter.instruction("ldr x10, [sp, #72]");                                  // reload the __unserialize target
    emitter.instruction("blr x10");                                             // call __unserialize($this, $data)
    emitter.instruction("b __rt_unser_at_obj_box");                             // box the object (position is at the closing '}')
    emitter.label("__rt_unser_obj_default");
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize the property index
    emitter.label("__rt_unser_at_obj_loop");
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the property index
    emitter.instruction("ldr x3, [sp, #32]");                                   // reload the property count
    emitter.instruction("cmp x4, x3");                                          // all properties parsed?
    emitter.instruction("b.ge __rt_unser_at_obj_close");                        // box the object when done
    emitter.instruction("ldr x0, [sp, #0]");                                    // base
    emitter.instruction("ldr x1, [sp, #8]");                                    // current position
    emitter.instruction("ldr x2, [sp, #16]");                                   // end
    emitter.instruction("bl __rt_unser_key");                                   // parse the mangled key -> x0=key_ptr, x1=key_len, x2=newpos
    emitter.instruction("str x0, [sp, #48]");                                   // save the key pointer
    emitter.instruction("str x1, [sp, #56]");                                   // save the key length
    emitter.instruction("str x2, [sp, #8]");                                    // advance past the key
    emitter.instruction("ldr x0, [sp, #0]");                                    // base
    emitter.instruction("ldr x1, [sp, #8]");                                    // position after the key
    emitter.instruction("ldr x2, [sp, #16]");                                   // end
    emitter.instruction("bl __rt_unser_at");                                    // recursively parse the value -> x0=box, x1=newpos
    emitter.instruction("str x1, [sp, #8]");                                    // advance past the value
    emitter.instruction("mov x3, x0");                                          // value box
    emitter.instruction("ldr x0, [sp, #24]");                                   // object pointer
    emitter.instruction("ldr x1, [sp, #48]");                                   // key pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // key length
    emitter.instruction("bl __rt_obj_store_prop");                              // store the value into the matching property slot
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the property index
    emitter.instruction("add x4, x4, #1");                                      // advance the property index
    emitter.instruction("str x4, [sp, #40]");                                   // persist the property index
    emitter.instruction("b __rt_unser_at_obj_loop");                            // continue with the next property
    emitter.label("__rt_unser_at_obj_close");
    // -- __wakeup magic: after default property injection, call __wakeup($this) --
    emitter.instruction("ldr x9, [sp, #24]");                                   // object pointer
    emitter.instruction("ldr x9, [x9]");                                        // class id from the object header
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_class_wakeup_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // __wakeup method symbol (0 if none)
    emitter.instruction("cbz x10, __rt_unser_at_obj_box");                      // no __wakeup → box the object directly
    emitter.instruction("ldr x0, [sp, #24]");                                   // $this receiver
    emitter.instruction("blr x10");                                             // call __wakeup($this)
    emitter.label("__rt_unser_at_obj_box");
    emitter.instruction("mov x0, #24");                                         // box the object: Mixed cell = tag + two payload words
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the boxed Mixed cell
    emitter.instruction("mov x9, #5");                                          // heap kind 5 = boxed Mixed cell
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the heap header
    emitter.instruction("mov x9, #6");                                          // value tag 6 = object
    emitter.instruction("str x9, [x0]");                                        // store the value tag
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the object pointer
    emitter.instruction("str x9, [x0, #8]");                                    // store the object pointer (ownership transferred)
    emitter.instruction("str xzr, [x0, #16]");                                  // clear the high payload word
    // -- register this object box so a later r:<index>; resolves to the same
    //    object (its index was reserved before its properties were parsed) --
    emitter.instruction("ldr x9, [sp, #88]");                                   // reserved value index for this object
    emitter.instruction("mov x10, #65536");                                     // value-registry capacity
    emitter.instruction("cmp x9, x10");                                         // is the registry full?
    emitter.instruction("b.ge __rt_unser_obj_box_noreg");                       // overflow → skip registration
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_unser_values");
    emitter.instruction("str x0, [x10, x9, lsl #3]");                           // values[index] = this object box
    emitter.label("__rt_unser_obj_box_noreg");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload position (at the closing '}')
    emitter.instruction("add x1, x1, #1");                                      // newpos skips the '}'
    emitter.instruction("b __rt_unser_at_ret");                                 // return the box and new position

    // -- failure: null box, position unchanged --
    emitter.label("__rt_unser_at_fail");
    emitter.instruction("mov x0, #0");                                          // null result signals parse failure
    emitter.instruction("ldr x1, [sp, #8]");                                    // newpos = unchanged position

    emitter.label("__rt_unser_at_ret");
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // deallocate the parser frame
    emitter.instruction("ret");                                                 // return x0=box, x1=newpos

    // -- back-reference: r:N; / R:N; -> a fresh box aliasing the Nth parsed value.
    //    N is 1-based (PHP's value index); objects are retained so refcounts stay
    //    balanced. An out-of-range or never-registered index yields null. --
    emitter.label("__rt_unser_at_ref");
    emitter.instruction("ldr x10, [sp, #0]");                                   // base
    emitter.instruction("ldr x11, [sp, #8]");                                   // position
    emitter.instruction("add x10, x10, x11");                                   // pointer to the leading 'r'/'R'
    emitter.instruction("add x10, x10, #2");                                    // skip the marker and ':'
    emitter.instruction("mov x11, #0");                                         // index accumulator
    emitter.label("__rt_unser_at_ref_loop");
    emitter.instruction("ldrb w9, [x10]");                                      // next byte
    emitter.instruction("cmp w9, #48");                                         // below '0'?
    emitter.instruction("b.lt __rt_unser_at_ref_done");                         // terminator reached
    emitter.instruction("cmp w9, #57");                                         // above '9'?
    emitter.instruction("b.gt __rt_unser_at_ref_done");                         // terminator reached
    emitter.instruction("sub w9, w9, #48");                                     // digit value
    emitter.instruction("mov x13, #10");                                        // decimal base
    emitter.instruction("mul x11, x11, x13");                                   // shift the accumulator
    emitter.instruction("add x11, x11, x9");                                    // add the digit
    emitter.instruction("add x10, x10, #1");                                    // advance the cursor
    emitter.instruction("b __rt_unser_at_ref_loop");                            // continue
    emitter.label("__rt_unser_at_ref_done");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload base
    emitter.instruction("sub x12, x10, x9");                                    // offset of the ';'
    emitter.instruction("add x12, x12, #1");                                    // newpos skips the ';'
    emitter.instruction("str x12, [sp, #8]");                                   // save the new position
    emitter.instruction("cbz x11, __rt_unser_at_ref_fail");                     // index 0 is invalid
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_unser_count");
    emitter.instruction("ldr x9, [x9]");                                        // number of registered values
    emitter.instruction("cmp x11, x9");                                         // index beyond what was parsed?
    emitter.instruction("b.gt __rt_unser_at_ref_fail");                         // out of range → null
    emitter.instruction("sub x12, x11, #1");                                    // 0-based registry slot
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_unser_values");
    emitter.instruction("ldr x13, [x9, x12, lsl #3]");                          // the registered value box (0 if none)
    emitter.instruction("cbz x13, __rt_unser_at_ref_fail");                     // nothing registered (e.g. a cycle) → null
    emitter.instruction("str x13, [sp, #64]");                                  // save the source box across the alloc
    emitter.instruction("mov x0, #24");                                         // a fresh boxed Mixed cell
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate it
    emitter.instruction("ldr x13, [sp, #64]");                                  // reload the source box
    emitter.instruction("ldur x9, [x13, #-8]");                                 // source heap header
    emitter.instruction("str x9, [x0, #-8]");                                   // copy the heap header
    emitter.instruction("ldr x9, [x13]");                                       // source value tag
    emitter.instruction("str x9, [x0]");                                        // copy the value tag
    emitter.instruction("ldr x10, [x13, #8]");                                  // source low payload (object pointer)
    emitter.instruction("str x10, [x0, #8]");                                   // copy the low payload
    emitter.instruction("ldr x11, [x13, #16]");                                 // source high payload
    emitter.instruction("str x11, [x0, #16]");                                  // copy the high payload
    emitter.instruction("cmp x9, #6");                                          // does the alias point at an object?
    emitter.instruction("b.ne __rt_unser_at_ref_boxed");                        // non-objects need no retain
    emitter.instruction("str x0, [sp, #64]");                                   // save the fresh box across the retain
    emitter.instruction("mov x0, x10");                                         // object pointer
    emitter.instruction("bl __rt_incref");                                      // retain the shared object
    emitter.instruction("ldr x0, [sp, #64]");                                   // reload the fresh box
    emitter.label("__rt_unser_at_ref_boxed");
    emitter.instruction("ldr x1, [sp, #8]");                                    // newpos past the ';'
    emitter.instruction("b __rt_unser_at_ret");                                 // return the aliasing box
    emitter.label("__rt_unser_at_ref_fail");
    emitter.instruction("mov x0, #0");                                          // unresolved reference → null
    emitter.instruction("ldr x1, [sp, #8]");                                    // newpos past the ';'
    emitter.instruction("b __rt_unser_at_ret");                                 // return the null result

    // -- __rt_unserialize_begin: reset the per-call value registry --
    emitter.blank();
    emitter.comment("--- runtime: unserialize_begin (reset reference registry) ---");
    emitter.label_global("__rt_unserialize_begin");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_unser_count");
    emitter.instruction("str xzr, [x9]");                                       // value registry count = 0
    emitter.instruction("ret");                                                 // registry is reset

    // -- __rt_obj_store_prop(x0=obj, x1=key_ptr, x2=key_len, x3=valbox): inject a property --
    // Matches the (mangled) key against the class's serialize property-info table and
    // stores the parsed value into the matching object slot per the property's tag.
    emitter.label_global("__rt_obj_store_prop");
    emitter.instruction("ldr x9, [x0]");                                        // class id from the object header
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_class_serprop_ptrs");
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
    emitter.instruction("ldr x6, [x14, #16]");                                  // property byte offset
    emitter.instruction("ldr x7, [x14, #24]");                                  // property value tag
    emitter.instruction("add x8, x0, x6");                                      // address of the property slot
    emitter.instruction("cmp x7, #7");                                          // is this a Mixed/untyped slot?
    emitter.instruction("b.eq __rt_obj_store_prop_mixed");                      // store the boxed cell directly
    emitter.instruction("cmp x7, #1");                                          // is this a string slot?
    emitter.instruction("b.eq __rt_obj_store_prop_str");                        // store pointer and length
    emitter.instruction("cmp x7, #4");                                          // is this an indexed-array slot?
    emitter.instruction("b.eq __rt_obj_store_prop_arr");                        // convert the parsed hash to an indexed array
    emitter.instruction("ldr x9, [x3, #8]");                                    // typed scalar/object/hash: unbox the low word
    emitter.instruction("str x9, [x8]");                                        // store it inline in the slot
    emitter.instruction("ret");                                                 // property stored
    emitter.label("__rt_obj_store_prop_arr");
    emitter.instruction("stp x8, x30, [sp, #-16]!");                            // save the slot address and return address
    emitter.instruction("ldr x0, [x3, #8]");                                    // parsed hash pointer (box low word)
    emitter.instruction("bl __rt_hash_to_indexed_array");                       // materialize a native indexed array
    emitter.instruction("ldp x8, x30, [sp], #16");                              // restore the slot address and return address
    emitter.instruction("str x0, [x8]");                                        // store the indexed-array pointer
    emitter.instruction("ret");                                                 // property stored
    emitter.label("__rt_obj_store_prop_str");
    emitter.instruction("ldr x9, [x3, #8]");                                    // string pointer from the box
    emitter.instruction("str x9, [x8]");                                        // store the string pointer
    emitter.instruction("ldr x9, [x3, #16]");                                   // string length from the box
    emitter.instruction("str x9, [x8, #8]");                                    // store the string length
    emitter.instruction("ret");                                                 // property stored
    emitter.label("__rt_obj_store_prop_mixed");
    emitter.instruction("ldr x9, [x3]");                                        // boxed value tag
    emitter.instruction("cmp x9, #8");                                          // is the boxed value null?
    emitter.instruction("b.eq __rt_obj_store_prop_mixed_null");                 // store the null sentinel
    emitter.instruction("str x3, [x8]");                                        // store the boxed Mixed cell pointer
    emitter.instruction("ret");                                                 // property stored
    emitter.label("__rt_obj_store_prop_mixed_null");
    crate::codegen::abi::emit_load_int_immediate(emitter, "x9", crate::codegen::NULL_SENTINEL);
    emitter.instruction("str x9, [x8]");                                        // store the in-band null sentinel
    emitter.instruction("str xzr, [x8, #8]");                                   // clear the high word
    emitter.instruction("ret");                                                 // property stored
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
    emitter.instruction("bl __rt_hash_iter_next");                              // x3=value low, x5=value tag, x0=next cursor
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

    emit_unser_key_aarch64(emitter);
}

/// Emits the AArch64 leaf key parser `__rt_unser_key`.
///
/// Input: `x0`=base, `x1`=pos, `x2`=end. Output: `x0`=key_lo (int value or string
/// pointer), `x1`=key_hi (-1 for an integer key, else the string byte length), `x2`=newpos.
/// String key pointers are borrowed into the source buffer; `__rt_hash_set` persists them.
fn emit_unser_key_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unser_key (serialize() array key parser, leaf) ---");
    emitter.label_global("__rt_unser_key");
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
}

/// x86_64 implementation of the unserialize entry, recursive parser, and key parser.
fn emit_unserialize_x86_64(emitter: &mut Emitter) {
    // -- entry wrapper: __rt_unser_at(base=ptr, pos=0, end=len) --
    emitter.blank();
    emitter.comment("--- runtime: unserialize_mixed (serialize() wire -> boxed Mixed) ---");
    emitter.label_global("__rt_unserialize_mixed");
    emitter.instruction("mov rdi, rax");                                        // base = source string pointer
    emitter.instruction("xor rsi, rsi");                                        // start parsing at position 0
    emitter.instruction("jmp __rt_unser_at");                                   // tail-call the recursive parser (rdx already = len = end)

    // -- __rt_unser_at(base=rdi, pos=rsi, end=rdx) -> rax=box (0 fail), rdx=newpos --
    emitter.blank();
    emitter.comment("--- runtime: unser_at (recursive serialize() value parser) ---");
    emitter.label_global("__rt_unser_at");
    // [rbp-8]=base [16]=pos [24]=end [32]=hash [40]=count [48]=index [56]=key_lo [64]=key_hi [72]=scratch
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 96");                                         // recursive parser frame (with a reference-index slot)
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the base pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the current position
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the end position
    emitter.instruction("cmp rsi, rdx");                                        // is the cursor already at/past the end?
    emitter.instruction("jge __rt_unser_at_fail");                              // nothing left to parse
    emitter.instruction("movzx r9d, BYTE PTR [rdi + rsi]");                     // load the leading type byte
    // -- back-reference? r:N; / R:N; resolves to a previously parsed value and
    //    consumes no new index --
    emitter.instruction("cmp r9d, 114");                                        // ASCII 'r'?
    emitter.instruction("je __rt_unser_at_ref");                                // resolve an object back-reference
    emitter.instruction("cmp r9d, 82");                                         // ASCII 'R'?
    emitter.instruction("je __rt_unser_at_ref");                                // resolve a PHP reference
    // -- every other value consumes the next pre-order index, mirroring serialize() --
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_unser_count");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // current value index
    emitter.instruction("mov QWORD PTR [rbp - 88], r11");                       // reserve this value's index
    emitter.instruction("add r11, 1");                                          // advance the registry counter
    emitter.instruction("mov QWORD PTR [r10], r11");                            // publish the advanced counter
    emitter.instruction("cmp r9d, 78");                                         // ASCII 'N' (null)?
    emitter.instruction("je __rt_unser_at_null");                               // parse null
    emitter.instruction("cmp r9d, 98");                                         // ASCII 'b' (bool)?
    emitter.instruction("je __rt_unser_at_bool");                               // parse bool
    emitter.instruction("cmp r9d, 105");                                        // ASCII 'i' (int)?
    emitter.instruction("je __rt_unser_at_int");                                // parse int
    emitter.instruction("cmp r9d, 100");                                        // ASCII 'd' (float)?
    emitter.instruction("je __rt_unser_at_float");                              // parse float
    emitter.instruction("cmp r9d, 115");                                        // ASCII 's' (string)?
    emitter.instruction("je __rt_unser_at_str");                                // parse string
    emitter.instruction("cmp r9d, 97");                                         // ASCII 'a' (array)?
    emitter.instruction("je __rt_unser_at_array");                              // parse array
    emitter.instruction("cmp r9d, 79");                                         // ASCII 'O' (object)?
    emitter.instruction("je __rt_unser_at_object");                             // parse object
    emitter.instruction("jmp __rt_unser_at_fail");                              // unsupported wire form

    // -- null: "N;" --
    emitter.label("__rt_unser_at_null");
    emitter.instruction("mov rax, 8");                                          // value tag = null
    emitter.instruction("mov rdi, 0");                                          // null payload low word
    emitter.instruction("mov rsi, 0");                                          // null payload high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the null value
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload position
    emitter.instruction("add rdx, 2");                                          // newpos skips "N;"
    emitter.instruction("jmp __rt_unser_at_ret");                               // return box and new position

    // -- bool: "b:0;" / "b:1;" --
    emitter.label("__rt_unser_at_bool");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload base
    emitter.instruction("add r10, QWORD PTR [rbp - 16]");                       // pointer to the type byte
    emitter.instruction("movzx r9d, BYTE PTR [r10 + 2]");                       // load the bool digit at offset 2
    emitter.instruction("sub r9d, 48");                                         // ASCII '0'/'1' -> 0/1
    emitter.instruction("and r9, 1");                                           // clamp to a single bool bit
    emitter.instruction("mov rdi, r9");                                         // value payload = bool bit
    emitter.instruction("mov rax, 3");                                          // value tag = bool
    emitter.instruction("mov rsi, 0");                                          // bool high payload unused
    emitter.instruction("call __rt_mixed_from_value");                          // box the bool value
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload position
    emitter.instruction("add rdx, 4");                                          // newpos skips "b:X;"
    emitter.instruction("jmp __rt_unser_at_ret");                               // return box and new position

    // -- int: "i:" + optional '-' + digits + ";" --
    emitter.label("__rt_unser_at_int");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload base
    emitter.instruction("add r10, QWORD PTR [rbp - 16]");                       // pointer to the type byte
    emitter.instruction("add r10, 2");                                          // skip "i:" to the first digit
    emitter.instruction("xor r11, r11");                                        // digit accumulator
    emitter.instruction("xor r8, r8");                                          // negative-sign flag
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // first numeric byte
    emitter.instruction("cmp r9d, 45");                                         // leading '-'?
    emitter.instruction("jne __rt_unser_at_int_loop");                          // no sign
    emitter.instruction("mov r8, 1");                                           // record negative sign
    emitter.instruction("add r10, 1");                                          // skip '-'
    emitter.label("__rt_unser_at_int_loop");
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // next numeric byte
    emitter.instruction("cmp r9d, 48");                                         // below '0'?
    emitter.instruction("jl __rt_unser_at_int_done");                           // terminator reached
    emitter.instruction("cmp r9d, 57");                                         // above '9'?
    emitter.instruction("jg __rt_unser_at_int_done");                           // terminator reached
    emitter.instruction("sub r9d, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift accumulator
    emitter.instruction("add r11, r9");                                         // add digit
    emitter.instruction("add r10, 1");                                          // advance cursor
    emitter.instruction("jmp __rt_unser_at_int_loop");                          // continue
    emitter.label("__rt_unser_at_int_done");
    emitter.instruction("test r8, r8");                                         // signed?
    emitter.instruction("jz __rt_unser_at_int_box");                            // not signed
    emitter.instruction("neg r11");                                             // apply sign
    emitter.label("__rt_unser_at_int_box");
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // save the cursor (at ';') across the box call
    emitter.instruction("mov rdi, r11");                                        // value payload = parsed int
    emitter.instruction("mov rax, 0");                                          // value tag = int
    emitter.instruction("mov rsi, 0");                                          // int high payload unused
    emitter.instruction("call __rt_mixed_from_value");                          // box the int value
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the cursor
    emitter.instruction("sub r10, QWORD PTR [rbp - 8]");                        // newpos = cursor - base
    emitter.instruction("add r10, 1");                                          // skip the ';'
    emitter.instruction("mov rdx, r10");                                        // newpos
    emitter.instruction("jmp __rt_unser_at_ret");                               // return box and new position

    // -- float: "d:" + (INF/-INF/NAN | digits) + ";" --
    emitter.label("__rt_unser_at_float");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload base
    emitter.instruction("add rdi, QWORD PTR [rbp - 16]");                       // pointer to the type byte
    emitter.instruction("add rdi, 2");                                          // strtod source = first byte after "d:"
    emitter.instruction("lea rsi, [rbp - 72]");                                 // strtod endptr = &scratch
    emitter.instruction("call strtod");                                         // parse the float (stops at ';') -> xmm0, scratch=endptr
    emitter.instruction("movq r9, xmm0");                                       // move the parsed double into a GPR
    emitter.instruction("mov rdi, r9");                                         // value payload = float bits
    emitter.instruction("mov rax, 2");                                          // value tag = float
    emitter.instruction("mov rsi, 0");                                          // float high payload unused
    emitter.instruction("call __rt_mixed_from_value");                          // box the float value
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the strtod endptr
    emitter.instruction("sub r10, QWORD PTR [rbp - 8]");                        // newpos = endptr - base
    emitter.instruction("add r10, 1");                                          // skip the ';'
    emitter.instruction("mov rdx, r10");                                        // newpos
    emitter.instruction("jmp __rt_unser_at_ret");                               // return box and new position

    // -- string: "s:" + bytelen + ":\"" + raw + "\";" --
    emitter.label("__rt_unser_at_str");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload base
    emitter.instruction("add r10, QWORD PTR [rbp - 16]");                       // pointer to the type byte
    emitter.instruction("add r10, 2");                                          // skip "s:" to the length digits
    emitter.instruction("xor r11, r11");                                        // length accumulator
    emitter.label("__rt_unser_at_strlen");
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // next length byte
    emitter.instruction("cmp r9d, 48");                                         // below '0'?
    emitter.instruction("jl __rt_unser_at_strlen_done");                        // ':' terminator reached
    emitter.instruction("cmp r9d, 57");                                         // above '9'?
    emitter.instruction("jg __rt_unser_at_strlen_done");                        // ':' terminator reached
    emitter.instruction("sub r9d, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift accumulator
    emitter.instruction("add r11, r9");                                         // add digit
    emitter.instruction("add r10, 1");                                          // advance cursor
    emitter.instruction("jmp __rt_unser_at_strlen");                            // continue
    emitter.label("__rt_unser_at_strlen_done");
    emitter.instruction("add r10, 2");                                          // skip ':' and opening '\"' to the raw bytes
    emitter.instruction("mov r8, r10");                                         // raw end accumulator = raw start
    emitter.instruction("add r8, r11");                                         // raw end = raw + len
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                        // save raw end across the box call
    emitter.instruction("mov rdi, r10");                                        // string payload pointer = raw bytes
    emitter.instruction("mov rsi, r11");                                        // string payload length
    emitter.instruction("mov rax, 1");                                          // value tag = string (mixed_from_value persists it)
    emitter.instruction("call __rt_mixed_from_value");                          // box an owned copy of the string
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload raw end
    emitter.instruction("sub r10, QWORD PTR [rbp - 8]");                        // newpos = raw end - base
    emitter.instruction("add r10, 2");                                          // skip closing '\"' and ';'
    emitter.instruction("mov rdx, r10");                                        // newpos
    emitter.instruction("jmp __rt_unser_at_ret");                               // return box and new position

    // -- array: "a:" + count + ":{" + count*(key value) + "}" --
    emitter.label("__rt_unser_at_array");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload base
    emitter.instruction("add r10, QWORD PTR [rbp - 16]");                       // pointer to the type byte
    emitter.instruction("add r10, 2");                                          // skip "a:" to the count digits
    emitter.instruction("xor r11, r11");                                        // count accumulator
    emitter.label("__rt_unser_at_count");
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // next count byte
    emitter.instruction("cmp r9d, 48");                                         // below '0'?
    emitter.instruction("jl __rt_unser_at_count_done");                         // ':' terminator reached
    emitter.instruction("cmp r9d, 57");                                         // above '9'?
    emitter.instruction("jg __rt_unser_at_count_done");                         // ':' terminator reached
    emitter.instruction("sub r9d, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift accumulator
    emitter.instruction("add r11, r9");                                         // add digit
    emitter.instruction("add r10, 1");                                          // advance cursor
    emitter.instruction("jmp __rt_unser_at_count");                             // continue
    emitter.label("__rt_unser_at_count_done");
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the entry count
    emitter.instruction("add r10, 2");                                          // skip ':' and '{' to the body
    emitter.instruction("sub r10, QWORD PTR [rbp - 8]");                        // body position offset
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // advance the cursor to the body
    emitter.instruction("mov rdi, r11");                                        // hash capacity = entry count
    emitter.instruction("mov rsi, 7");                                          // hash value_type = boxed Mixed
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination hash
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the entry index
    emitter.label("__rt_unser_at_array_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the entry index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 40]");                       // all entries parsed?
    emitter.instruction("jge __rt_unser_at_array_close");                       // box the hash when done
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // current position
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // end
    emitter.instruction("call __rt_unser_key");                                 // parse the key -> rax=key_lo, rdx=key_hi, rcx=newpos
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save key_lo
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save key_hi
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // advance past the key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // position after the key
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // end
    emitter.instruction("call __rt_unser_at");                                  // recursively parse the value -> rax=box, rdx=newpos
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // advance past the value
    emitter.instruction("mov rcx, rax");                                        // value_lo = parsed value box
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // key_hi (-1 for int keys)
    emitter.instruction("mov r8, 0");                                           // value_hi unused
    emitter.instruction("mov r9, 7");                                           // value tag = boxed Mixed (transfer the box)
    emitter.instruction("call __rt_hash_set");                                  // insert the entry -> rax = (possibly new) hash
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the updated hash pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the entry index
    emitter.instruction("add rcx, 1");                                          // advance the entry index
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the entry index
    emitter.instruction("jmp __rt_unser_at_array_loop");                        // continue with the next entry
    emitter.label("__rt_unser_at_array_close");
    emitter.instruction("mov rax, 24");                                         // box the hash: Mixed cell = tag + two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate the boxed Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the hash pointer
    emitter.instruction("mov QWORD PTR [rax - 8], 5");                          // stamp the heap header (boxed Mixed kind)
    emitter.instruction("mov QWORD PTR [rax], 5");                              // value tag 5 = associative array (hash)
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the hash pointer (ownership transferred)
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // clear the high payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload position (at the closing '}')
    emitter.instruction("add rdx, 1");                                          // newpos skips the '}'
    emitter.instruction("jmp __rt_unser_at_ret");                               // return box and new position

    // -- object: "O:" + namelen + ":\"" + class + "\":" + count + ":{" + count*(key value) + "}" --
    emitter.label("__rt_unser_at_object");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload base
    emitter.instruction("add r10, QWORD PTR [rbp - 16]");                       // pointer to the type byte
    emitter.instruction("add r10, 2");                                          // skip "O:" to the class-name length digits
    emitter.instruction("xor r11, r11");                                        // class-name length accumulator
    emitter.label("__rt_unser_at_obj_namelen");
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // next length byte
    emitter.instruction("cmp r9d, 48");                                         // below '0'?
    emitter.instruction("jl __rt_unser_at_obj_namelen_done");                   // ':' terminator reached
    emitter.instruction("cmp r9d, 57");                                         // above '9'?
    emitter.instruction("jg __rt_unser_at_obj_namelen_done");                   // ':' terminator reached
    emitter.instruction("sub r9d, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift accumulator
    emitter.instruction("add r11, r9");                                         // add digit
    emitter.instruction("add r10, 1");                                          // advance cursor
    emitter.instruction("jmp __rt_unser_at_obj_namelen");                       // continue
    emitter.label("__rt_unser_at_obj_namelen_done");
    emitter.instruction("add r10, 2");                                          // skip ':' and opening '\"' to the class name bytes
    emitter.instruction("mov r8, r10");                                         // class-name end accumulator = name start
    emitter.instruction("add r8, r11");                                         // class-name end = name + len
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                        // save the class-name end across the call
    emitter.instruction("mov rax, r10");                                        // class-name pointer (new_by_name arg)
    emitter.instruction("mov rdx, r11");                                        // class-name length (new_by_name arg)
    emitter.instruction("call __rt_new_by_name");                               // instantiate the class by name (0 on unknown class)
    emitter.instruction("test rax, rax");                                       // unknown class?
    emitter.instruction("jz __rt_unser_at_fail");                               // unknown class fails the parse
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the new object pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the class-name end
    emitter.instruction("add r10, 2");                                          // skip closing '\"' and ':' to the property count
    emitter.instruction("xor r11, r11");                                        // property-count accumulator
    emitter.label("__rt_unser_at_obj_count");
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // next count byte
    emitter.instruction("cmp r9d, 48");                                         // below '0'?
    emitter.instruction("jl __rt_unser_at_obj_count_done");                     // ':' terminator reached
    emitter.instruction("cmp r9d, 57");                                         // above '9'?
    emitter.instruction("jg __rt_unser_at_obj_count_done");                     // ':' terminator reached
    emitter.instruction("sub r9d, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift accumulator
    emitter.instruction("add r11, r9");                                         // add digit
    emitter.instruction("add r10, 1");                                          // advance cursor
    emitter.instruction("jmp __rt_unser_at_obj_count");                         // continue
    emitter.label("__rt_unser_at_obj_count_done");
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the property count
    emitter.instruction("add r10, 2");                                          // skip ':' and '{' to the body
    emitter.instruction("sub r10, QWORD PTR [rbp - 8]");                        // body position offset
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // advance the cursor to the body
    // -- __unserialize magic: parse the body into an assoc array, then call
    //    __unserialize($this, $data) instead of injecting properties by name --
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // object pointer
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // class id from the object header
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_class_unserialize_ptrs");
    emitter.instruction("mov r10, QWORD PTR [r11 + rax*8]");                    // __unserialize method symbol (0 if none)
    emitter.instruction("test r10, r10");                                       // does the class define __unserialize?
    emitter.instruction("jz __rt_unser_obj_default");                           // no → inject properties by name
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // park the __unserialize target
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // entry count = hash capacity hint
    emitter.instruction("mov rsi, 7");                                          // hash value_type = boxed Mixed
    emitter.instruction("call __rt_hash_new");                                  // allocate the $data hash
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the $data hash pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // entry index = 0
    emitter.label("__rt_unser_obj_data_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the entry index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 40]");                       // all entries parsed?
    emitter.instruction("jge __rt_unser_obj_data_done");                        // call __unserialize when done
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // current position
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // end
    emitter.instruction("call __rt_unser_key");                                 // parse the key -> rax=key_lo, rdx=key_hi, rcx=newpos
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save key_lo
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save key_hi
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // advance past the key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // position after the key
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // end
    emitter.instruction("call __rt_unser_at");                                  // recursively parse the value -> rax=box, rdx=newpos
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // advance past the value
    emitter.instruction("mov rcx, rax");                                        // value_lo = parsed value box
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // $data hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // key_lo
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // key_hi (-1 for int keys)
    emitter.instruction("mov r8, 0");                                           // value_hi unused
    emitter.instruction("mov r9, 7");                                           // value tag = boxed Mixed (transfer the box)
    emitter.instruction("call __rt_hash_set");                                  // insert the entry -> rax = (possibly new) hash
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the updated $data hash pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the entry index
    emitter.instruction("add rcx, 1");                                          // advance the entry index
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the entry index
    emitter.instruction("jmp __rt_unser_obj_data_loop");                        // continue with the next entry
    emitter.label("__rt_unser_obj_data_done");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // $this receiver = first argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                       // $data assoc array (bare hash) = second argument
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the __unserialize target
    emitter.instruction("call r10");                                            // call __unserialize($this, $data)
    emitter.instruction("jmp __rt_unser_at_obj_box");                           // box the object (position is at the closing '}')
    emitter.label("__rt_unser_obj_default");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the property index
    emitter.label("__rt_unser_at_obj_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the property index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 40]");                       // all properties parsed?
    emitter.instruction("jge __rt_unser_at_obj_close");                         // box the object when done
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // current position
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // end
    emitter.instruction("call __rt_unser_key");                                 // parse the mangled key -> rax=key_ptr, rdx=key_len, rcx=newpos
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the key pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save the key length
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // advance past the key
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // base
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // position after the key
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // end
    emitter.instruction("call __rt_unser_at");                                  // recursively parse the value -> rax=box, rdx=newpos
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // advance past the value
    emitter.instruction("mov rcx, rax");                                        // value box
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // object pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // key length
    emitter.instruction("call __rt_obj_store_prop");                            // store the value into the matching property slot
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the property index
    emitter.instruction("add rcx, 1");                                          // advance the property index
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the property index
    emitter.instruction("jmp __rt_unser_at_obj_loop");                          // continue with the next property
    emitter.label("__rt_unser_at_obj_close");
    // -- __wakeup magic: after default property injection, call __wakeup($this) --
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // object pointer
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // class id from the object header
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_class_wakeup_ptrs");
    emitter.instruction("mov r10, QWORD PTR [r11 + rax*8]");                    // __wakeup method symbol (0 if none)
    emitter.instruction("test r10, r10");                                       // does the class define __wakeup?
    emitter.instruction("jz __rt_unser_at_obj_box");                            // no → box the object directly
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // $this receiver
    emitter.instruction("call r10");                                            // call __wakeup($this)
    emitter.label("__rt_unser_at_obj_box");
    emitter.instruction("mov rax, 24");                                         // box the object: Mixed cell = tag + two payload words
    emitter.instruction("call __rt_heap_alloc");                                // allocate the boxed Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the object pointer
    emitter.instruction("mov QWORD PTR [rax - 8], 5");                          // stamp the heap header (boxed Mixed kind)
    emitter.instruction("mov QWORD PTR [rax], 6");                              // value tag 6 = object
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the object pointer (ownership transferred)
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // clear the high payload word
    // -- register this object box so a later r:<index>; resolves to the same object --
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // reserved value index for this object
    emitter.instruction("cmp r10, 65536");                                      // is the value registry full?
    emitter.instruction("jge __rt_unser_obj_box_noreg");                        // overflow → skip registration
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_unser_values");
    emitter.instruction("mov QWORD PTR [r11 + r10*8], rax");                    // values[index] = this object box
    emitter.label("__rt_unser_obj_box_noreg");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload position (at the closing '}')
    emitter.instruction("add rdx, 1");                                          // newpos skips the '}'
    emitter.instruction("jmp __rt_unser_at_ret");                               // return box and new position

    // -- failure: null box, position unchanged --
    emitter.label("__rt_unser_at_fail");
    emitter.instruction("xor eax, eax");                                        // null result signals parse failure
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // newpos = unchanged position

    emitter.label("__rt_unser_at_ret");
    emitter.instruction("add rsp, 96");                                         // deallocate the parser frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return rax=box, rdx=newpos

    // -- back-reference: r:N; / R:N; -> a fresh box aliasing the Nth parsed value
    //    (1-based); objects are retained. Out-of-range/unregistered index -> null. --
    emitter.label("__rt_unser_at_ref");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // base
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // position
    emitter.instruction("add r10, r11");                                        // pointer to the leading 'r'/'R'
    emitter.instruction("add r10, 2");                                          // skip the marker and ':'
    emitter.instruction("xor r11, r11");                                        // index accumulator
    emitter.label("__rt_unser_at_ref_loop");
    emitter.instruction("movzx r9d, BYTE PTR [r10]");                           // next byte
    emitter.instruction("cmp r9d, 48");                                         // below '0'?
    emitter.instruction("jl __rt_unser_at_ref_done");                           // terminator reached
    emitter.instruction("cmp r9d, 57");                                         // above '9'?
    emitter.instruction("jg __rt_unser_at_ref_done");                           // terminator reached
    emitter.instruction("sub r9d, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift the accumulator
    emitter.instruction("add r11, r9");                                         // add the digit
    emitter.instruction("add r10, 1");                                          // advance the cursor
    emitter.instruction("jmp __rt_unser_at_ref_loop");                          // continue
    emitter.label("__rt_unser_at_ref_done");
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload base
    emitter.instruction("sub r10, r9");                                         // offset of the ';'
    emitter.instruction("add r10, 1");                                          // newpos skips the ';'
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the new position
    emitter.instruction("test r11, r11");                                       // index 0 is invalid
    emitter.instruction("jz __rt_unser_at_ref_fail");                           // bail to null
    crate::codegen::abi::emit_symbol_address(emitter, "r9", "_unser_count");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // number of registered values
    emitter.instruction("cmp r11, r9");                                         // index beyond what was parsed?
    emitter.instruction("jg __rt_unser_at_ref_fail");                           // out of range → null
    emitter.instruction("sub r11, 1");                                          // 0-based registry slot
    crate::codegen::abi::emit_symbol_address(emitter, "r9", "_unser_values");
    emitter.instruction("mov r9, QWORD PTR [r9 + r11*8]");                      // the registered value box (0 if none)
    emitter.instruction("test r9, r9");                                         // nothing registered (e.g. a cycle)?
    emitter.instruction("jz __rt_unser_at_ref_fail");                           // → null
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // save the source box across the alloc
    emitter.instruction("mov rax, 24");                                         // a fresh boxed Mixed cell
    emitter.instruction("call __rt_heap_alloc");                                // allocate it
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the source box
    emitter.instruction("mov r10, QWORD PTR [r9 - 8]");                         // source heap header
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // copy the heap header
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // source value tag
    emitter.instruction("mov QWORD PTR [rax], r10");                            // copy the value tag
    emitter.instruction("mov r10, QWORD PTR [r9 + 8]");                         // source low payload (object pointer)
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // copy the low payload
    emitter.instruction("mov r10, QWORD PTR [r9 + 16]");                        // source high payload
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // copy the high payload
    emitter.instruction("cmp QWORD PTR [rax], 6");                              // does the alias point at an object?
    emitter.instruction("jne __rt_unser_at_ref_boxed");                         // non-objects need no retain
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the fresh box across the retain
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // object pointer
    emitter.instruction("call __rt_incref");                                    // retain the shared object
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the fresh box
    emitter.label("__rt_unser_at_ref_boxed");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // newpos past the ';'
    emitter.instruction("jmp __rt_unser_at_ret");                               // return the aliasing box
    emitter.label("__rt_unser_at_ref_fail");
    emitter.instruction("xor eax, eax");                                        // unresolved reference → null
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // newpos past the ';'
    emitter.instruction("jmp __rt_unser_at_ret");                               // return the null result

    // -- __rt_unserialize_begin: reset the per-call value registry --
    emitter.blank();
    emitter.comment("--- runtime: unserialize_begin (reset reference registry) ---");
    emitter.label_global("__rt_unserialize_begin");
    crate::codegen::abi::emit_symbol_address(emitter, "r9", "_unser_count");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // value registry count = 0
    emitter.instruction("ret");                                                 // registry is reset

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
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_class_serprop_ptrs");
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
    emitter.instruction("mov rax, QWORD PTR [rcx + 8]");                        // typed scalar/object/hash: unbox the low word
    emitter.instruction("mov QWORD PTR [r10], rax");                            // store it inline in the slot
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
    crate::codegen::abi::emit_load_int_immediate(emitter, "r11", crate::codegen::NULL_SENTINEL);
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

    emit_unser_key_x86_64(emitter);
}

/// Emits the x86_64 leaf key parser `__rt_unser_key`.
///
/// Input: `rdi`=base, `rsi`=pos, `rdx`=end. Output: `rax`=key_lo, `rdx`=key_hi (-1 for
/// an integer key, else the string byte length), `rcx`=newpos. String key pointers are
/// borrowed into the source buffer; `__rt_hash_set` persists them.
fn emit_unser_key_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unser_key (serialize() array key parser, leaf) ---");
    emitter.label_global("__rt_unser_key");
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
}
