//! Purpose:
//! Emits the AArch64 public entry boundary and recursive serialized-value decoder.
//!
//! Called from:
//! - `super::emit_unserialize()` around the AArch64 preflight validator.
//!
//! Key details:
//! - The entry owns throw-safe cleanup; the decoder assumes the allocation-free preflight succeeded.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};

/// Emits the AArch64 public unserialize entry and its exception-cleanup boundary.
pub(super) fn emit_entry(emitter: &mut Emitter) {
    let boundary_bytes = TRY_HANDLER_SLOT_SIZE + 32;
    let frame_link_offset = boundary_bytes - 16;
    let source_offset = TRY_HANDLER_SLOT_SIZE;
    let source_len_offset = source_offset + 8;

    // -- entry wrapper: protect begin/end cleanup across hydration-hook throws --
    emitter.blank();
    emitter.comment("--- runtime: unserialize_mixed (serialize() wire -> boxed Mixed) ---");
    emitter.label_global("__rt_unserialize_mixed");
    emitter.instruction(&format!("sub sp, sp, #{}", boundary_bytes));           // reserve a complete handler record plus input/result spills
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", frame_link_offset)); // preserve the caller frame and return address across setjmp
    emitter.instruction(&format!("add x29, sp, #{}", frame_link_offset));       // establish the protected unserialize wrapper frame
    emitter.instruction(&format!("str x1, [sp, #{}]", source_offset));          // preserve source pointer across setjmp
    emitter.instruction(&format!("str x2, [sp, #{}]", source_len_offset));      // preserve source length across setjmp
    crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction("str x10, [sp]");                                       // handler.next = previous exception-handler head
    crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
    emitter.instruction("str x10, [sp, #8]");                                   // preserve the activation frame that survives this boundary
    crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!("str x10, [sp, #{}]", TRY_HANDLER_DIAG_DEPTH_OFFSET)); // snapshot diagnostic suppression across longjmp
    emitter.instruction("mov x10, sp");                                         // compute this wrapper's exception-handler record address
    crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("add x0, sp, #{}", TRY_HANDLER_JMP_BUF_OFFSET)); // pass this boundary's opaque jmp_buf to setjmp
    emitter.bl_c("setjmp"); // catch Throwable control flow escaping hydration hooks
    emitter.instruction("cbnz x0, __rt_unserialize_mixed_throw");               // longjmp resumes here so runtime state can be cleaned first
    emitter.instruction(&format!("ldr x0, [sp, #{}]", source_offset));          // base = preserved source string pointer
    emitter.instruction("mov x1, #0");                                          // start parsing at position 0
    emitter.instruction(&format!("ldr x2, [sp, #{}]", source_len_offset));      // end = preserved source string length
    emitter.instruction("mov x3, #0");                                          // preflight starts at recursive depth zero
    emitter.instruction("bl __rt_unser_validate_at");                           // reject truncated/overflowing grammar before allocating or running hooks
    emitter.instruction("cbz x0, __rt_unserialize_mixed_invalid");              // malformed input returns PHP false through the normal end path
    emitter.instruction(&format!("ldr x0, [sp, #{}]", source_offset));          // reload base after the validator's caller-clobbered registers
    emitter.instruction("mov x1, #0");                                          // parse the already validated value from the beginning
    emitter.instruction(&format!("ldr x2, [sp, #{}]", source_len_offset));      // restore the validated source extent
    emitter.instruction("bl __rt_unser_at");                                    // parse while the cleanup boundary is active
    emitter.instruction("b __rt_unserialize_mixed_parsed");                     // share exception-boundary teardown with validation failures
    emitter.label("__rt_unserialize_mixed_invalid");
    emitter.instruction("mov x0, #0");                                          // null result signals a bounded parse failure
    emitter.label("__rt_unserialize_mixed_parsed");
    emitter.instruction(&format!("str x0, [sp, #{}]", source_offset));          // preserve the parsed box while popping the boundary
    emitter.instruction("ldr x10, [sp]");                                       // reload the previous exception-handler head
    crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("ldr x10, [sp, #{}]", TRY_HANDLER_DIAG_DEPTH_OFFSET)); // reload diagnostic suppression after the protected parse
    crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!("ldr x0, [sp, #{}]", source_offset));          // recover the parsed Mixed result
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame_link_offset)); // restore the caller frame and return address
    emitter.instruction(&format!("add sp, sp, #{}", boundary_bytes));           // release the exception boundary frame
    emitter.instruction("ret");                                                 // return the parsed box to the lowering's normal end path
    emitter.label("__rt_unserialize_mixed_throw");
    emitter.instruction("ldr x10, [sp]");                                       // reload the handler preceding this internal boundary
    crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("ldr x10, [sp, #{}]", TRY_HANDLER_DIAG_DEPTH_OFFSET)); // restore diagnostic suppression skipped by longjmp
    crate::codegen_support::abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction("mov x0, #0");                                          // end cleanup ignores the placeholder parse result on throw
    emitter.instruction("bl __rt_unserialize_end");                             // release policy/context state before propagating the Throwable
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame_link_offset)); // restore the caller frame before rethrowing
    emitter.instruction(&format!("add sp, sp, #{}", boundary_bytes));           // discard the protected parser stack through its boundary
    emitter.instruction("b __rt_throw_current");                                // resume propagation at the caller's exception handler
}

/// Emits the AArch64 recursive decoder for one prevalidated serialized value.
pub(super) fn emit_parser(emitter: &mut Emitter) {
    // -- __rt_unser_at(base=x0, pos=x1, end=x2) -> x0=boxed Mixed (0 on fail), x1=newpos --
    emitter.blank();
    emitter.comment("--- runtime: unser_at (recursive serialize() value parser) ---");
    emitter.label_global("__rt_unser_at");
    // [sp+0]=base [8]=pos [16]=end [24]=container [32]=count [40]=index [48]=key_lo [56]=key_hi
    // [sp+64]=scratch [72]=hook/name length [80]=policy/data hash [88]=registry index [96]=object box
    emitter.instruction("sub sp, sp, #128");                                    // recursive parser frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the base pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the current position
    emitter.instruction("str x2, [sp, #16]");                                   // save the end position
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_depth");
    emitter.instruction("ldr x10, [x9]");                                       // load current recursive unserialize depth
    emitter.instruction("add x10, x10, #1");                                    // account for this parser frame
    emitter.instruction("str x10, [x9]");                                       // publish parser depth before consuming wire bytes
    emitter.instruction("cmp x10, #512");                                       // bound recursive frames before native-stack exhaustion
    emitter.instruction("b.le __rt_unser_at_depth_in_budget");                  // keep the conditional branch inside the recursive parser atom
    emitter.instruction("b __rt_unser_depth_fatal");                            // terminate hostile nesting through the shared fatal path
    emitter.label("__rt_unser_at_depth_in_budget");
    emitter.instruction("cmp x1, x2");                                          // is the cursor already at/past the end?
    emitter.instruction("b.ge __rt_unser_at_fail");                             // nothing left to parse
    emitter.instruction("ldrb w9, [x0, x1]");                                   // load the leading type byte
    // Every value consumes the next pre-order index, including r:/R: aliases.
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_unser_count");
    emitter.instruction("ldr x11, [x10]");                                      // current value index
    emitter.instruction("str x11, [sp, #88]");                                  // reserve this value's index
    emitter.instruction("add x11, x11, #1");                                    // advance the registry counter
    emitter.instruction("str x11, [x10]");                                      // publish the advanced counter
    emitter.instruction("sub x12, x11, #1");                                    // recover the reserved zero-based registry slot
    emitter.instruction("mov x13, #65536");                                     // materialize the physical reference-registry capacity
    emitter.instruction("cmp x12, x13");                                        // is the reserved slot inside the fixed registry?
    emitter.instruction("b.hs __rt_unser_at_registry_slot_ready");              // out-of-capacity values remain deliberately unregistered
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_unser_values");
    emitter.instruction("str xzr, [x10, x12, lsl #3]");                         // erase any stale object pointer before parsing this value
    emitter.label("__rt_unser_at_registry_slot_ready");
    emitter.instruction("cmp w9, #114");                                        // ASCII 'r'?
    emitter.instruction("b.eq __rt_unser_at_ref");                              // resolve an object back-reference
    emitter.instruction("cmp w9, #82");                                         // ASCII 'R'?
    emitter.instruction("b.eq __rt_unser_at_ref");                              // resolve a PHP reference
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
    emitter.instruction("ldr x10, [sp, #64]");                                  // bounded conversion end pointer
    emitter.instruction("ldr x11, [sp, #0]");                                   // source base
    emitter.instruction("ldr x12, [sp, #8]");                                   // original value position
    emitter.instruction("add x11, x11, x12");                                   // pointer to the type byte
    emitter.instruction("add x11, x11, #2");                                    // first float payload byte
    emitter.instruction("cmp x10, x11");                                        // did strtod consume at least one byte?
    emitter.instruction("b.eq __rt_unser_at_fail");                             // invalid numeric payload
    emitter.instruction("ldr x12, [sp, #0]");                                   // source base
    emitter.instruction("ldr x13, [sp, #16]");                                  // source end offset
    emitter.instruction("add x12, x12, x13");                                   // absolute source end
    emitter.instruction("cmp x10, x12");                                        // end pointer must still address a delimiter
    emitter.instruction("b.hs __rt_unser_at_fail");                             // reject a conversion escaping the source extent
    emitter.instruction("ldrb w12, [x10]");                                     // conversion terminator byte
    emitter.instruction("cmp w12, #59");                                        // exact semicolon delimiter
    emitter.instruction("b.ne __rt_unser_at_fail");                             // reject partial conversions such as `1x;`
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
    emitter.instruction("ldr x9, [sp, #16]");                                   // source end for key-result validation
    emitter.instruction("cmp x2, x9");                                          // key parser must not escape the validated source
    emitter.instruction("b.hi __rt_unser_at_array_fail");                       // release the partially built hash on failure
    emitter.instruction("str x0, [sp, #48]");                                   // save key_lo
    emitter.instruction("str x1, [sp, #56]");                                   // save key_hi
    emitter.instruction("str x2, [sp, #8]");                                    // advance past the key
    emitter.instruction("ldr x0, [sp, #0]");                                    // base
    emitter.instruction("ldr x1, [sp, #8]");                                    // position after the key
    emitter.instruction("ldr x2, [sp, #16]");                                   // end
    emitter.instruction("bl __rt_unser_at");                                    // recursively parse the value -> x0=box, x1=newpos
    emitter.instruction("cbz x0, __rt_unser_at_array_fail");                    // child failure invalidates the whole array
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
    emitter.instruction("ldr x9, [sp, #8]");                                    // closing-brace position
    emitter.instruction("ldr x10, [sp, #16]");                                  // source end
    emitter.instruction("cmp x9, x10");                                         // require the closing delimiter byte
    emitter.instruction("b.hs __rt_unser_at_array_fail");                       // input ended before the closing '}'
    emitter.instruction("ldr x10, [sp, #0]");                                   // source base
    emitter.instruction("ldrb w11, [x10, x9]");                                 // bounded closing delimiter
    emitter.instruction("cmp w11, #125");                                       // exact `}`
    emitter.instruction("b.ne __rt_unser_at_array_fail");                       // anything but '}' fails the array
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
    emitter.label("__rt_unser_at_array_fail");
    emitter.instruction("ldr x0, [sp, #24]");                                   // partially built hash pointer
    emitter.instruction("bl __rt_hash_free_deep");                              // release keys and transferred boxed values locally
    emitter.instruction("mov x0, #0");                                          // report parse failure
    emitter.instruction("ldr x1, [sp, #8]");                                    // preserve current cursor for the caller
    emitter.instruction("b __rt_unser_at_ret");                                 // only the shared return decrements parser depth

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
    emitter.instruction("str x10, [sp, #40]");                                  // save the class-name start across the policy helper
    emitter.instruction("str x11, [sp, #72]");                                  // save the class-name length across the policy helper
    emitter.instruction("mov x0, x10");                                         // class name pointer for allowed_classes policy
    emitter.instruction("mov x1, x11");                                         // class name length for allowed_classes policy
    emitter.instruction("bl __rt_unserialize_class_allowed");                   // decide whether hydration is permitted
    emitter.instruction("str x0, [sp, #80]");                                   // retain policy result until hook/property dispatch
    emitter.instruction("cbz x0, __rt_unser_obj_incomplete");                   // blocked classes become incomplete objects
    emitter.instruction("ldr x10, [sp, #40]");                                  // reload class-name start after helper call
    emitter.instruction("ldr x11, [sp, #72]");                                  // reload class-name length after helper call
    emitter.instruction("mov x1, x10");                                         // class-name pointer (new_by_name arg)
    emitter.instruction("mov x2, x11");                                         // class-name length (new_by_name arg)
    emitter.instruction("bl __rt_new_by_name");                                 // instantiate the class by name (0 on unknown class)
    emitter.instruction("cbnz x0, __rt_unser_obj_allocated");                   // known classes use their declared layout
    emitter.instruction("str xzr, [sp, #80]");                                  // unknown classes suppress hooks and use opaque properties
    emitter.instruction("b __rt_unser_obj_incomplete");                         // match PHP's __PHP_Incomplete_Class fallback
    emitter.label("__rt_unser_obj_incomplete");
    emitter.instruction("mov x0, #32");                                         // class id, original class name, and opaque property hash
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the incomplete-object payload
    emitter.instruction("mov x9, #4");                                          // heap kind 4 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the object heap header
    emitter.instruction("bl __rt_object_handle_acquire");                       // give the incomplete object a normal PHP handle
    emitter.instruction("mov x9, #-2");                                         // reserved class id for __PHP_Incomplete_Class
    emitter.instruction("str x9, [x0]");                                        // publish synthetic class id
    emitter.instruction("str x0, [sp, #24]");                                   // preserve incomplete object across string persistence
    emitter.instruction("ldr x1, [sp, #40]");                                   // original serialized class-name bytes
    emitter.instruction("ldr x2, [sp, #72]");                                   // original serialized class-name length
    emitter.instruction("bl __rt_str_persist");                                 // own the class name independently of the source wire
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload incomplete object payload
    emitter.instruction("str x1, [x0, #8]");                                    // persisted original class-name pointer
    emitter.instruction("str x2, [x0, #16]");                                   // persisted original class-name length
    emitter.instruction("str xzr, [x0, #24]");                                  // property hash is created after its count is parsed
    emitter.label("__rt_unser_obj_allocated");
    emitter.instruction("str x0, [sp, #24]");                                   // save the new object pointer
    emitter.instruction("mov x0, #24");                                         // allocate the object's stable boxed Mixed cell
    emitter.instruction("bl __rt_heap_alloc");                                  // create the box before decoding any property values
    emitter.instruction("mov x9, #5");                                          // heap kind 5 = boxed Mixed cell
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the box header
    emitter.instruction("mov x9, #6");                                          // value tag 6 = object
    emitter.instruction("str x9, [x0]");                                        // publish the object tag
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the object pointer
    emitter.instruction("str x9, [x0, #8]");                                    // transfer the object ownership into the box
    emitter.instruction("str xzr, [x0, #16]");                                  // clear the high payload word
    emitter.instruction("str x0, [sp, #96]");                                   // retain the stable result box through hooks and parsing
    emitter.instruction("ldr x9, [sp, #88]");                                   // reserved value index for this object
    emitter.instruction("mov x10, #65536");                                     // value-registry capacity
    emitter.instruction("cmp x9, x10");                                         // is the reserved slot inside the registry?
    emitter.instruction("b.hs __rt_unser_obj_registered");                      // overflow values cannot participate in back-references
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_unser_values");
    emitter.instruction("str x0, [x10, x9, lsl #3]");                           // publish before parsing so r: can resolve self-references
    emitter.label("__rt_unser_obj_registered");
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
    emitter.instruction("ldr x9, [sp, #80]");                                   // reload allowed_classes decision
    emitter.instruction("cbz x9, __rt_unser_obj_default");                      // blocked objects must never inspect class hook tables
    // -- __unserialize magic: parse the body into an assoc array, then call
    //    __unserialize($this, $data) instead of injecting properties by name --
    emitter.instruction("ldr x9, [sp, #24]");                                   // object pointer
    emitter.instruction("ldr x9, [x9]");                                        // class id from the object header
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_class_unserialize_ptrs");
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
    emitter.instruction("cbz x0, __rt_unser_obj_data_fail");                    // propagate semantic child-decoder failures safely
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
    emitter.instruction("ldr x11, [sp, #8]");                                   // cursor where the closing brace must appear
    emitter.instruction("ldr x12, [sp, #16]");                                  // serialized input length
    emitter.instruction("cmp x11, x12");                                        // is the closing brace truncated?
    emitter.instruction("b.ge __rt_unser_obj_magic_missing_close");             // end-of-input is php-src's failure offset
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload serialized input base
    emitter.instruction("ldrb w10, [x9, x11]");                                 // inspect the expected closing byte
    emitter.instruction("cmp w10, #125");                                       // ASCII '}'?
    emitter.instruction("b.eq __rt_unser_obj_magic_close_valid");               // complete objects invoke the hook directly
    emitter.label("__rt_unser_obj_magic_missing_close");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_unser_warning_callback");
    emitter.instruction("ldr x10, [x10]");                                      // call-site warning callback, if one was installed
    emitter.instruction("cbz x10, __rt_unser_obj_magic_missing_reported");      // runtime-only callers may have no callback
    emitter.instruction("mov x0, x11");                                         // callback arg 1 = failure offset
    emitter.instruction("mov x1, x12");                                         // callback arg 2 = total input length
    emitter.instruction("blr x10");                                             // warn before invoking __unserialize()
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_warning_emitted");
    emitter.instruction("mov x10, #1");                                         // lowering must not duplicate the warning
    emitter.instruction("str x10, [x9]");                                       // publish the pre-emitted warning flag
    emitter.label("__rt_unser_obj_magic_missing_reported");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_force_failure");
    emitter.instruction("mov x10, #1");                                         // the hook still runs, but the public result is false
    emitter.instruction("str x10, [x9]");                                       // publish the post-hook failure flag
    emitter.label("__rt_unser_obj_magic_close_valid");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the concrete receiver for native date-hook trace metadata
    emitter.instruction("ldr x9, [x9]");                                        // class id indexes the generated trace-owner table
    crate::codegen_support::abi::emit_symbol_address(
        emitter,
        "x10",
        "_class_date_unserialize_trace_entries",
    );
    emitter.instruction("add x10, x10, x9, lsl #4");                            // select this class's (owner ptr, owner len) row
    emitter.instruction("ldp x11, x12, [x10]");                                 // zero owner pointer means an ordinary user hook
    emitter.instruction("cbz x11, __rt_unser_obj_trace_ready");                 // do not mark non-native hooks as internal ext/date calls
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_unser_trace_owner_ptr");
    emitter.instruction("str x11, [x10]");                                      // publish the native implementation class name
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_unser_trace_owner_len");
    emitter.instruction("str x12, [x10]");                                      // publish its byte length
    emitter.instruction("ldr x11, [sp, #0]");                                   // serialized call argument base
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_unser_trace_input_ptr");
    emitter.instruction("str x11, [x10]");                                      // preserve the original wire string for the trace preview
    emitter.instruction("ldr x11, [sp, #16]");                                  // serialized call argument length
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_unser_trace_input_len");
    emitter.instruction("str x11, [x10]");                                      // preserve the wire length for the 15-byte preview bound
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_unser_trace_exception_ptr");
    emitter.instruction("str xzr, [x10]");                                      // the thrown Error identity is captured at the first unwind boundary
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_unser_trace_active");
    emitter.instruction("mov x11, #1");                                         // the next throw originates in an internal ext/date hook
    emitter.instruction("str x11, [x10]");                                      // activate the specialized uncaught trace
    emitter.label("__rt_unser_obj_trace_ready");
    emitter.instruction("ldr x0, [sp, #24]");                                   // $this receiver = first argument
    emitter.instruction("ldr x1, [sp, #80]");                                   // $data assoc array (bare hash) = second argument
    emitter.instruction("ldr x10, [sp, #72]");                                  // reload the __unserialize target
    emitter.instruction("blr x10");                                             // call __unserialize($this, $data)
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_unser_trace_active");
    emitter.instruction("str xzr, [x10]");                                      // a successful native hook no longer owns uncaught-trace state
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_unser_trace_exception_ptr");
    emitter.instruction("str xzr, [x10]");                                      // discard the unused exception identity slot after success
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the concrete receiver for DateInterval-handler detection
    emitter.instruction("ldr x9, [x9]");                                        // load its runtime class id
    crate::codegen_support::abi::emit_symbol_address(
        emitter,
        "x10",
        "_class_dateinterval_unserialize_flags",
    );
    emitter.instruction("ldr x9, [x10, x9, lsl #3]");                           // does this class inherit DateInterval's native restoration hook?
    emitter.instruction("cbz x9, __rt_unser_dateinterval_dynamic_done");        // other date objects have no DateInterval dynamic fields
    emitter.instruction("ldr x0, [sp, #80]");                                   // parsed DateInterval data hash
    emitter.instruction("mov x1, #0");                                          // empty dynamic-property key pointer
    emitter.instruction("mov x2, #0");                                          // empty dynamic-property key length
    emitter.instruction("bl __rt_hash_get");                                    // did the malformed payload contain the empty key?
    emitter.instruction("cbz x0, __rt_unser_dateinterval_dynamic_done");        // no empty custom property means no deprecation
    crate::codegen_support::abi::emit_symbol_address(
        emitter,
        "x10",
        "_unser_dateinterval_dynamic_callback",
    );
    emitter.instruction("ldr x10, [x10]");                                      // call-site deprecation callback
    emitter.instruction("cbz x10, __rt_unser_dateinterval_dynamic_done");       // runtime-only callers may have no callback
    emitter.instruction("blr x10");                                             // emit the dynamic-property deprecation at the call site
    emitter.label("__rt_unser_dateinterval_dynamic_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the concrete object receiver
    emitter.instruction("ldr x1, [sp, #80]");                                   // reload the parsed magic data hash
    emitter.instruction("bl __rt_date_magic_restore_props");                    // restore user-declared date-subclass properties
    emitter.instruction("b __rt_unser_at_obj_box");                             // box the object (position is at the closing '}')
    emitter.label("__rt_unser_obj_default");
    emitter.instruction("ldr x9, [sp, #80]");                                   // blocked objects own an opaque Mixed property hash
    emitter.instruction("cbnz x9, __rt_unser_obj_default_props");               // hydrated objects use their declared property slots
    emitter.instruction("ldr x0, [sp, #32]");                                   // property count is the hash capacity hint
    emitter.instruction("mov x1, #7");                                          // values are boxed Mixed cells
    emitter.instruction("bl __rt_hash_new");                                    // allocate property hash before parsing values
    emitter.instruction("ldr x9, [sp, #24]");                                   // incomplete-object payload
    emitter.instruction("str x0, [x9, #24]");                                   // transfer hash ownership into incomplete object
    emitter.label("__rt_unser_obj_default_props");
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
    emitter.instruction("cbz x0, __rt_unser_obj_fail");                         // never pass a failed child box to property storage
    emitter.instruction("mov x3, x0");                                          // value box
    emitter.instruction("ldr x0, [sp, #24]");                                   // object pointer
    emitter.instruction("ldr x1, [sp, #48]");                                   // key pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // key length
    emitter.instruction("ldr x9, [sp, #80]");                                   // blocked objects keep their wire properties opaque
    emitter.instruction("cbz x9, __rt_unser_obj_store_opaque_prop");            // blocked objects retain the parsed property semantically
    emitter.instruction("bl __rt_obj_store_prop");                              // store the value into the matching property slot
    emitter.instruction("b __rt_unser_obj_skip_prop_store");                    // transferred value now belongs to the hydrated object
    emitter.label("__rt_unser_obj_store_opaque_prop");
    emitter.instruction("ldr x0, [sp, #24]");                                   // incomplete-object payload
    emitter.instruction("ldr x0, [x0, #24]");                                   // opaque property hash
    emitter.instruction("ldr x1, [sp, #48]");                                   // serialized property key pointer
    emitter.instruction("ldr x2, [sp, #56]");                                   // serialized property key length
    emitter.instruction("mov x4, #0");                                          // boxed Mixed values have no high payload word
    emitter.instruction("mov x5, #7");                                          // transfer the parsed box as a Mixed hash value
    emitter.instruction("bl __rt_hash_set");                                    // insert property, preserving key/value ownership and order
    emitter.instruction("ldr x9, [sp, #24]");                                   // incomplete-object payload after a possible hash grow
    emitter.instruction("str x0, [x9, #24]");                                   // retain updated property hash pointer
    emitter.label("__rt_unser_obj_skip_prop_store");
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the property index
    emitter.instruction("add x4, x4, #1");                                      // advance the property index
    emitter.instruction("str x4, [sp, #40]");                                   // persist the property index
    emitter.instruction("b __rt_unser_at_obj_loop");                            // continue with the next property
    emitter.label("__rt_unser_at_obj_close");
    emitter.instruction("ldr x9, [sp, #8]");                                    // closing-brace position after the final property
    emitter.instruction("ldr x10, [sp, #16]");                                  // serialized input end position
    emitter.instruction("cmp x9, x10");                                         // is a closing byte available?
    emitter.instruction("b.hs __rt_unser_obj_fail");                            // truncated default objects are parse failures
    emitter.instruction("ldr x10, [sp, #0]");                                   // serialized input base pointer
    emitter.instruction("ldrb w10, [x10, x9]");                                 // read the required closing byte
    emitter.instruction("cmp w10, #125");                                       // ASCII '}'?
    emitter.instruction("b.ne __rt_unser_obj_fail");                            // malformed default object is a parse failure
    // -- __wakeup magic: after default property injection, call __wakeup($this) --
    emitter.instruction("ldr x9, [sp, #80]");                                   // blocked classes cannot run __wakeup
    emitter.instruction("cbz x9, __rt_unser_at_obj_box");                       // incomplete objects never run class hooks
    emitter.label("__rt_unser_obj_wakeup");
    emitter.instruction("ldr x9, [sp, #24]");                                   // object pointer
    emitter.instruction("ldr x9, [x9]");                                        // class id from the object header
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_class_wakeup_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // __wakeup method symbol (0 if none)
    emitter.instruction("cbz x10, __rt_unser_at_obj_box");                      // no __wakeup → box the object directly
    emitter.instruction("ldr x0, [sp, #24]");                                   // $this receiver
    emitter.instruction("blr x10");                                             // call __wakeup($this)
    emitter.label("__rt_unser_at_obj_box");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_force_failure");
    emitter.instruction("ldr x9, [x9]");                                        // did a malformed magic object force failure?
    emitter.instruction("cbnz x9, __rt_unser_at_force_fail");                   // honor a failure raised by a required magic hook
    emitter.instruction("ldr x0, [sp, #96]");                                   // return the stable box published before body parsing
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload position (at the closing '}')
    emitter.instruction("add x1, x1, #1");                                      // newpos skips the '}'
    emitter.instruction("b __rt_unser_at_ret");                                 // return the box and new position

    emitter.label("__rt_unser_at_force_fail");
    emitter.instruction("mov x0, #0");                                          // required hook effects ran, but result is false
    emitter.instruction("ldr x1, [sp, #8]");                                    // preserve php-src's failure offset
    emitter.instruction("b __rt_unser_at_ret");                                 // return the preserved failure position

    emitter.label("__rt_unser_obj_data_fail");
    emitter.instruction("ldr x0, [sp, #80]");                                   // partially built __unserialize data hash
    emitter.instruction("bl __rt_hash_free_deep");                              // release keys and transferred child boxes
    emitter.instruction("b __rt_unser_obj_fail");                               // release the partially hydrated object too
    emitter.label("__rt_unser_obj_fail");
    emitter.instruction("ldr x9, [sp, #88]");                                   // reserved registry slot for the failed object
    emitter.instruction("mov x10, #65536");                                     // value-registry capacity
    emitter.instruction("cmp x9, x10");                                         // was the object published in the fixed registry?
    emitter.instruction("b.hs __rt_unser_obj_fail_noreg");                      // skip clearing an out-of-capacity slot
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_unser_values");
    emitter.instruction("str xzr, [x10, x9, lsl #3]");                          // remove the failed object from future reference lookup
    emitter.label("__rt_unser_obj_fail_noreg");
    emitter.instruction("ldr x0, [sp, #96]");                                   // partially hydrated object's owning Mixed box
    emitter.instruction("bl __rt_decref_mixed");                                // release the box and its object ownership
    emitter.instruction("mov x0, #0");                                          // report parse failure
    emitter.instruction("ldr x1, [sp, #8]");                                    // preserve the child decoder's current cursor
    emitter.instruction("b __rt_unser_at_ret");                                 // only the shared return decrements parser depth

    // -- failure: null box, position unchanged --
    emitter.label("__rt_unser_at_fail");
    emitter.instruction("mov x0, #0");                                          // null result signals parse failure
    emitter.instruction("ldr x1, [sp, #8]");                                    // newpos = unchanged position

    emitter.label("__rt_unser_at_ret");
    emitter.instruction("cbz x0, __rt_unser_at_ret_done");                      // failed values have nothing to register
    emitter.instruction("ldr x9, [sp, #88]");                                   // reserved value index for this result
    emitter.instruction("mov x10, #65536");                                     // value-registry capacity
    emitter.instruction("cmp x9, x10");                                         // is the reserved result slot in bounds?
    emitter.instruction("b.ge __rt_unser_at_ret_done");                         // skip publishing results beyond registry capacity
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_unser_values");
    emitter.instruction("str x0, [x10, x9, lsl #3]");                           // make every parsed value referenceable
    emitter.label("__rt_unser_at_ret_done");
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_depth");
    emitter.instruction("ldr x10, [x9]");                                       // load parser depth before returning to the caller
    emitter.instruction("sub x10, x10, #1");                                    // release this completed recursive parser frame
    emitter.instruction("str x10, [x9]");                                       // keep sibling parses independent
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // deallocate the parser frame
    emitter.instruction("ret");                                                 // return x0=box, x1=newpos

    emitter.label_shared("__rt_unser_depth_fatal");
    emitter.instruction("mov x0, #2");                                          // stderr file descriptor
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_unser_depth_msg");
    emitter.instruction("mov x2, #48");                                         // complete unserialize-depth fatal diagnostic length
    emitter.syscall(4);                                                          // write the fatal diagnostic without recursing further
    emitter.instruction("mov x0, #1");                                          // non-zero failure status
    emitter.syscall(1);                                                          // terminate the hostile parse immediately

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
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_count");
    emitter.instruction("ldr x9, [x9]");                                        // number of registered values
    emitter.instruction("cmp x11, x9");                                         // index beyond what was parsed?
    emitter.instruction("b.gt __rt_unser_at_ref_fail");                         // out of range → null
    emitter.instruction("sub x12, x11, #1");                                    // 0-based registry slot
    emitter.instruction("mov x10, #65536");                                     // materialize the physical reference-registry capacity
    emitter.instruction("cmp x12, x10");                                        // would this logical reference index exceed the registry?
    emitter.instruction("b.hs __rt_unser_at_ref_fail");                         // fail closed instead of reading beyond the fixed registry
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_unser_values");
    emitter.instruction("ldr x13, [x9, x12, lsl #3]");                          // the registered value box (0 if none)
    emitter.instruction("cbz x13, __rt_unser_at_ref_fail");                     // reserved but unpublished registry slot → null
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
}
