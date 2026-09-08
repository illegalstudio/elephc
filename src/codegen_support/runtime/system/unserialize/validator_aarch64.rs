//! Purpose:
//! Emits the AArch64 allocation-free grammar preflight for serialized input.
//!
//! Called from:
//! - `super::emit_unserialize()` after the public entry wrapper and before the mutating decoder.
//!
//! Key details:
//! - Every cursor, delimiter, length, and recursive child is bounded before allocation or hooks.

use crate::codegen_support::emit::Emitter;

/// Emits the AArch64 allocation-free grammar preflight used before decoding.
///
/// The validator recursively proves every cursor advance and delimiter before the
/// mutating parser can allocate hashes/objects or invoke hydration hooks.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: bounded unserialize grammar preflight ---");

    // uint(base=x0, pos=x1, end=x2, delimiter=w3) -> x0=ok, x1=value, x2=delimiter position
    emitter.label_global("__rt_unser_validate_uint");
    emitter.instruction("add x9, x0, x1");                                      // absolute digit cursor
    emitter.instruction("add x10, x0, x2");                                     // absolute source end
    emitter.instruction("mov x11, #0");                                         // unsigned accumulator
    emitter.instruction("mov x12, #0");                                         // parsed digit count
    emitter.instruction("mov x14, #10");                                        // decimal radix
    emitter.label("__rt_unser_validate_uint_loop");
    emitter.instruction("cmp x9, x10");                                         // is another byte available?
    emitter.instruction("b.hs __rt_unser_validate_uint_fail");                  // truncated digit run has no delimiter
    emitter.instruction("ldrb w13, [x9]");                                      // inspect one bounded byte
    emitter.instruction("cmp w13, #48");                                        // below ASCII zero?
    emitter.instruction("b.lo __rt_unser_validate_uint_done");                  // require the requested delimiter below
    emitter.instruction("cmp w13, #57");                                        // above ASCII nine?
    emitter.instruction("b.hi __rt_unser_validate_uint_done");                  // require the requested delimiter below
    emitter.instruction("sub w13, w13, #48");                                   // convert the byte to a digit
    emitter.instruction("umulh x15, x11, x14");                                 // detect overflow in accumulator * 10
    emitter.instruction("cbnz x15, __rt_unser_validate_uint_fail");             // wrapped lengths/counts are invalid
    emitter.instruction("mul x11, x11, x14");                                   // shift the accumulator by one decimal place
    emitter.instruction("adds x11, x11, x13");                                  // append the current digit and expose carry
    emitter.instruction("b.cs __rt_unser_validate_uint_fail");                  // reject addition overflow
    emitter.instruction("add x12, x12, #1");                                    // record one valid digit
    emitter.instruction("add x9, x9, #1");                                      // advance within the proven source extent
    emitter.instruction("b __rt_unser_validate_uint_loop");                     // scan the remaining digits
    emitter.label("__rt_unser_validate_uint_done");
    emitter.instruction("cbz x12, __rt_unser_validate_uint_fail");              // every numeric field needs at least one digit
    emitter.instruction("cmp w13, w3");                                         // did the run end on its grammar delimiter?
    emitter.instruction("b.ne __rt_unser_validate_uint_fail");                  // arbitrary terminators are rejected
    emitter.instruction("sub x2, x9, x0");                                      // return delimiter position as a source offset
    emitter.instruction("mov x1, x11");                                         // return the parsed unsigned value
    emitter.instruction("mov x0, #1");                                          // report success
    emitter.instruction("ret");                                                 // leaf return
    emitter.label("__rt_unser_validate_uint_fail");
    emitter.instruction("mov x0, #0");                                          // report a bounded numeric failure
    emitter.instruction("ret");                                                 // leaf return

    // key(base=x0, pos=x1, end=x2, depth=x3) -> x0=ok, x1=newpos
    emitter.label_global("__rt_unser_validate_key");
    emitter.instruction("cmp x1, x2");                                          // require the key type byte before loading it
    emitter.instruction("b.hs __rt_unser_validate_key_fail");                   // truncated key
    emitter.instruction("ldrb w9, [x0, x1]");                                   // inspect the bounded key type
    emitter.instruction("cmp w9, #105");                                        // integer key?
    emitter.instruction("b.eq __rt_unser_validate_key_dispatch");               // use an assembler-local conditional target
    emitter.instruction("cmp w9, #115");                                        // string key?
    emitter.instruction("b.ne __rt_unser_validate_key_fail");                   // reject every other key marker
    emitter.label("__rt_unser_validate_key_dispatch");
    emitter.instruction("b __rt_unser_validate_at");                            // main validator owns integer/string grammar
    emitter.label("__rt_unser_validate_key_fail");
    emitter.instruction("mov x0, #0");                                          // only integer and string keys are valid
    emitter.instruction("ret");                                                 // leaf failure return

    // at(base=x0, pos=x1, end=x2, depth=x3) -> x0=ok, x1=newpos
    emitter.label_global("__rt_unser_validate_at");
    emitter.instruction("sub sp, sp, #80");                                     // reserve recursive cursor/count state and an aligned frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #64");                                    // establish a stable validator frame
    emitter.instruction("stp x0, x1, [sp]");                                    // save source base and starting position
    emitter.instruction("stp x2, x3, [sp, #16]");                               // save source end and recursion depth
    emitter.instruction("cmp x1, x2");                                          // require a type byte before dispatch
    emitter.instruction("b.hs __rt_unser_validate_at_fail");                    // empty/truncated value
    emitter.instruction("cmp x3, #512");                                        // enforce the same recursion ceiling as the parser
    emitter.instruction("b.hs __rt_unser_validate_at_fail");                    // stop hostile nesting before native-stack exhaustion
    emitter.instruction("ldrb w9, [x0, x1]");                                   // load the bounded type byte
    emitter.instruction("cmp w9, #78");                                         // N
    emitter.instruction("b.eq __rt_unser_validate_null");                       // dispatch the null envelope check
    emitter.instruction("cmp w9, #98");                                         // b
    emitter.instruction("b.eq __rt_unser_validate_bool");                       // dispatch the boolean envelope check
    emitter.instruction("cmp w9, #105");                                        // i
    emitter.instruction("b.eq __rt_unser_validate_int");                        // dispatch the signed-integer check
    emitter.instruction("cmp w9, #100");                                        // d
    emitter.instruction("b.eq __rt_unser_validate_float");                      // dispatch the float payload scan
    emitter.instruction("cmp w9, #115");                                        // s
    emitter.instruction("b.eq __rt_unser_validate_string");                     // dispatch the counted-string check
    emitter.instruction("cmp w9, #97");                                         // a
    emitter.instruction("b.eq __rt_unser_validate_array");                      // dispatch the recursive array walk
    emitter.instruction("cmp w9, #79");                                         // O
    emitter.instruction("b.eq __rt_unser_validate_object");                     // dispatch the recursive object walk
    emitter.instruction("cmp w9, #114");                                        // r
    emitter.instruction("b.eq __rt_unser_validate_ref");                        // r reuses the reference-index check
    emitter.instruction("cmp w9, #82");                                         // R
    emitter.instruction("b.eq __rt_unser_validate_ref");                        // R reuses the reference-index check
    emitter.instruction("b __rt_unser_validate_at_fail");                       // reject unsupported wire markers

    emitter.label("__rt_unser_validate_null");
    emitter.instruction("sub x9, x2, x1");                                      // bytes remaining from N
    emitter.instruction("cmp x9, #2");                                          // N plus semicolon must fit
    emitter.instruction("b.lo __rt_unser_validate_at_fail");                    // truncated null envelope
    emitter.instruction("add x9, x0, x1");                                      // type pointer
    emitter.instruction("ldrb w10, [x9, #1]");                                  // bounded delimiter byte
    emitter.instruction("cmp w10, #59");                                        // semicolon?
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // N must be terminated by ';'
    emitter.instruction("add x1, x1, #2");                                      // skip N;
    emitter.instruction("b __rt_unser_validate_at_ok");                         // null is fully bounded

    emitter.label("__rt_unser_validate_bool");
    emitter.instruction("sub x9, x2, x1");                                      // bytes remaining from b
    emitter.instruction("cmp x9, #4");                                          // exact b:<digit>; envelope
    emitter.instruction("b.lo __rt_unser_validate_at_fail");                    // truncated boolean envelope
    emitter.instruction("add x9, x0, x1");                                      // type pointer
    emitter.instruction("ldrb w10, [x9, #1]");                                  // bounded byte after b
    emitter.instruction("cmp w10, #58");                                        // colon after b
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // boolean marker needs its colon
    emitter.instruction("ldrb w10, [x9, #2]");                                  // bounded truth digit
    emitter.instruction("cmp w10, #48");                                        // false digit
    emitter.instruction("b.eq __rt_unser_validate_bool_delim");                 // '0' encodes false
    emitter.instruction("cmp w10, #49");                                        // true digit
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // only '0' and '1' encode booleans
    emitter.label("__rt_unser_validate_bool_delim");
    emitter.instruction("ldrb w10, [x9, #3]");                                  // bounded terminator byte
    emitter.instruction("cmp w10, #59");                                        // terminating semicolon
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // boolean must end with ';'
    emitter.instruction("add x1, x1, #4");                                      // skip complete boolean
    emitter.instruction("b __rt_unser_validate_at_ok");                         // boolean is fully bounded

    emitter.label("__rt_unser_validate_int");
    emitter.instruction("add x9, x1, #1");                                      // colon position
    emitter.instruction("cmp x9, x2");                                          // colon byte must be in bounds
    emitter.instruction("b.hs __rt_unser_validate_at_fail");                    // truncated integer envelope
    emitter.instruction("ldrb w10, [x0, x9]");                                  // bounded byte after i
    emitter.instruction("cmp w10, #58");                                        // require i:
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // integer marker needs its colon
    emitter.instruction("add x9, x9, #1");                                      // first sign/digit position
    emitter.instruction("cmp x9, x2");                                          // sign/digit byte must be in bounds
    emitter.instruction("b.hs __rt_unser_validate_at_fail");                    // truncated integer payload
    emitter.instruction("ldrb w10, [x0, x9]");                                  // bounded sign/digit byte
    emitter.instruction("cmp w10, #45");                                        // optional minus sign
    emitter.instruction("b.ne __rt_unser_validate_int_digits");                 // unsigned payload: scan digits directly
    emitter.instruction("mov x10, #1");                                         // record a negative integer
    emitter.instruction("str x10, [sp, #56]");                                  // preserve sign across the numeric helper
    emitter.instruction("add x9, x9, #1");                                      // skip minus before unsigned digit scan
    emitter.instruction("b __rt_unser_validate_int_scan");                      // join the shared digit scan
    emitter.label("__rt_unser_validate_int_digits");
    emitter.instruction("str xzr, [sp, #56]");                                  // positive integer
    emitter.label("__rt_unser_validate_int_scan");
    emitter.instruction("ldr x0, [sp]");                                        // base
    emitter.instruction("mov x1, x9");                                          // digit position
    emitter.instruction("ldr x2, [sp, #16]");                                   // end
    emitter.instruction("mov w3, #59");                                         // integer terminator ';'
    emitter.instruction("bl __rt_unser_validate_uint");                         // parse the bounded magnitude
    emitter.instruction("cbz x0, __rt_unser_validate_at_fail");                 // propagate numeric failure
    crate::codegen_support::abi::emit_load_int_immediate(emitter, "x9", i64::MAX);
    emitter.instruction("ldr x10, [sp, #56]");                                  // negative-sign flag
    emitter.instruction("cbz x10, __rt_unser_validate_int_positive");           // positive values use the i64::MAX bound
    emitter.instruction("cmp x1, x9");                                          // negative magnitude at most i64::MAX + 1
    emitter.instruction("b.ls __rt_unser_validate_int_range_ok");               // magnitude within i64::MAX always fits
    emitter.instruction("sub x10, x1, x9");                                     // only one extra magnitude value is representable
    emitter.instruction("cmp x10, #1");                                         // is the excess exactly one?
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // only i64::MIN may exceed i64::MAX
    emitter.instruction("b __rt_unser_validate_int_range_ok");                  // accept the i64::MIN magnitude
    emitter.label("__rt_unser_validate_int_positive");
    emitter.instruction("cmp x1, x9");                                          // positive magnitude at most i64::MAX
    emitter.instruction("b.hi __rt_unser_validate_at_fail");                    // reject values above i64::MAX
    emitter.label("__rt_unser_validate_int_range_ok");
    emitter.instruction("add x1, x2, #1");                                      // skip semicolon
    emitter.instruction("b __rt_unser_validate_at_ok");                         // integer is fully bounded

    emitter.label("__rt_unser_validate_float");
    emitter.instruction("add x9, x1, #1");                                      // colon position
    emitter.instruction("cmp x9, x2");                                          // colon byte must be in bounds
    emitter.instruction("b.hs __rt_unser_validate_at_fail");                    // truncated float envelope
    emitter.instruction("ldrb w10, [x0, x9]");                                  // bounded byte after d
    emitter.instruction("cmp w10, #58");                                        // require d:
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // float marker needs its colon
    emitter.instruction("add x9, x9, #1");                                      // first float byte
    emitter.instruction("mov x11, x9");                                         // remember start to reject empty payloads
    emitter.label("__rt_unser_validate_float_loop");
    emitter.instruction("cmp x9, x2");                                          // bound every byte scanned for ';'
    emitter.instruction("b.hs __rt_unser_validate_at_fail");                    // unterminated float payload
    emitter.instruction("ldrb w10, [x0, x9]");                                  // bounded payload byte
    emitter.instruction("cmp w10, #59");                                        // terminating semicolon?
    emitter.instruction("b.eq __rt_unser_validate_float_done");                 // delimiter found: end the scan
    emitter.instruction("add x9, x9, #1");                                      // step past one payload byte
    emitter.instruction("b __rt_unser_validate_float_loop");                    // keep scanning for ';'
    emitter.label("__rt_unser_validate_float_done");
    emitter.instruction("cmp x9, x11");                                         // require at least one float byte
    emitter.instruction("b.eq __rt_unser_validate_at_fail");                    // empty float payload is invalid
    emitter.instruction("add x1, x9, #1");                                      // skip semicolon
    emitter.instruction("b __rt_unser_validate_at_ok");                         // float is fully bounded

    emitter.label("__rt_unser_validate_string");
    emitter.instruction("add x9, x1, #1");                                      // colon after s
    emitter.instruction("cmp x9, x2");                                          // colon byte must be in bounds
    emitter.instruction("b.hs __rt_unser_validate_at_fail");                    // truncated string envelope
    emitter.instruction("ldrb w10, [x0, x9]");                                  // bounded byte after s
    emitter.instruction("cmp w10, #58");                                        // require s:
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // string marker needs its colon
    emitter.instruction("add x1, x9, #1");                                      // first length digit
    emitter.instruction("mov w3, #58");                                         // length delimiter ':'
    emitter.instruction("bl __rt_unser_validate_uint");                         // parse the declared byte length
    emitter.instruction("cbz x0, __rt_unser_validate_at_fail");                 // propagate numeric failure
    emitter.instruction("mov x11, x1");                                         // preserve byte length
    emitter.instruction("add x9, x2, #1");                                      // opening quote position
    emitter.instruction("ldr x10, [sp, #16]");                                  // end
    emitter.instruction("cmp x9, x10");                                         // quote byte must be in bounds
    emitter.instruction("b.hs __rt_unser_validate_at_fail");                    // truncated before the opening quote
    emitter.instruction("ldr x12, [sp]");                                       // base
    emitter.instruction("ldrb w13, [x12, x9]");                                 // bounded opening-quote byte
    emitter.instruction("cmp w13, #34");                                        // opening quote
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // payload must open with '"'
    emitter.instruction("add x9, x9, #1");                                      // raw payload position
    emitter.instruction("sub x13, x10, x9");                                    // bytes remaining after opening quote
    emitter.instruction("cmp x13, #2");                                         // closing quote plus semicolon
    emitter.instruction("b.lo __rt_unser_validate_at_fail");                    // no room for closing quote and ';'
    emitter.instruction("sub x13, x13, #2");                                    // maximum safe payload length
    emitter.instruction("cmp x11, x13");                                        // declared bytes fit before delimiters?
    emitter.instruction("b.hi __rt_unser_validate_at_fail");                    // declared length overruns the source
    emitter.instruction("add x9, x9, x11");                                     // closing quote position
    emitter.instruction("ldrb w13, [x12, x9]");                                 // bounded closing-quote byte
    emitter.instruction("cmp w13, #34");                                        // closing quote
    emitter.instruction("b.ne __rt_unser_validate_string_close_fail");          // payload must close with '"'
    emitter.instruction("add x9, x9, #1");                                      // terminator position
    emitter.instruction("ldrb w13, [x12, x9]");                                 // bounded terminator byte
    emitter.instruction("cmp w13, #59");                                        // string terminator
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // string must end with ';'
    emitter.instruction("add x1, x9, #1");                                      // position after string
    emitter.instruction("b __rt_unser_validate_at_ok");                         // string is fully bounded
    emitter.label("__rt_unser_validate_string_close_fail");
    emitter.instruction("mov x1, x9");                                         // PHP reports the declared closing-quote position
    emitter.instruction("b __rt_unser_validate_at_fail");                      // return the precise malformed-string offset

    emitter.label("__rt_unser_validate_array");
    emitter.instruction("add x9, x1, #1");                                      // colon after a
    emitter.instruction("cmp x9, x2");                                          // colon byte must be in bounds
    emitter.instruction("b.hs __rt_unser_validate_at_fail");                    // truncated array envelope
    emitter.instruction("ldrb w10, [x0, x9]");                                  // bounded byte after a
    emitter.instruction("cmp w10, #58");                                        // require a:
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // array marker needs its colon
    emitter.instruction("add x1, x9, #1");                                      // first count digit
    emitter.instruction("mov w3, #58");                                         // count delimiter ':'
    emitter.instruction("bl __rt_unser_validate_uint");                         // parse the declared entry count
    emitter.instruction("cbz x0, __rt_unser_validate_at_fail");                 // propagate numeric failure
    emitter.instruction("str x1, [sp, #32]");                                   // save entry count
    emitter.instruction("add x9, x2, #1");                                      // opening brace position
    emitter.instruction("ldr x10, [sp, #16]");                                  // saved source end
    emitter.instruction("cmp x9, x10");                                         // brace byte must be in bounds
    emitter.instruction("b.hs __rt_unser_validate_at_fail");                    // truncated before the opening brace
    emitter.instruction("ldr x11, [sp]");                                       // saved source base
    emitter.instruction("ldrb w12, [x11, x9]");                                 // bounded opening-brace byte
    emitter.instruction("cmp w12, #123");                                       // opening brace
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // array body must open with '{'
    emitter.instruction("add x9, x9, #1");                                      // first key position
    emitter.instruction("str x9, [sp, #40]");                                   // current body position
    emitter.instruction("str xzr, [sp, #48]");                                  // entry index
    emitter.label("__rt_unser_validate_array_loop");
    emitter.instruction("ldr x9, [sp, #48]");                                   // current entry index
    emitter.instruction("ldr x10, [sp, #32]");                                  // declared entry count
    emitter.instruction("cmp x9, x10");                                         // every declared entry proven?
    emitter.instruction("b.hs __rt_unser_validate_container_close");            // then require the closing brace
    emitter.instruction("ldr x0, [sp]");                                        // saved source base
    emitter.instruction("ldr x1, [sp, #40]");                                   // resume at the saved body position
    emitter.instruction("ldr x2, [sp, #16]");                                   // saved source end
    emitter.instruction("ldr x3, [sp, #24]");                                   // saved recursion depth
    emitter.instruction("add x3, x3, #1");                                      // nested key depth
    emitter.instruction("bl __rt_unser_validate_key");                          // validate one entry key
    emitter.instruction("cbz x0, __rt_unser_validate_at_fail");                 // propagate key failure
    emitter.instruction("str x1, [sp, #40]");                                   // position after key
    emitter.instruction("ldr x0, [sp]");                                        // saved source base
    emitter.instruction("ldr x2, [sp, #16]");                                   // saved source end
    emitter.instruction("ldr x3, [sp, #24]");                                   // saved recursion depth
    emitter.instruction("add x3, x3, #1");                                      // nested value depth
    emitter.instruction("bl __rt_unser_validate_at");                           // recursively validate the entry value
    emitter.instruction("cbz x0, __rt_unser_validate_at_fail");                 // propagate value failure
    emitter.instruction("str x1, [sp, #40]");                                   // position after value
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the entry index
    emitter.instruction("add x9, x9, #1");                                      // one entry fully proven
    emitter.instruction("str x9, [sp, #48]");                                   // persist the advanced index
    emitter.instruction("b __rt_unser_validate_array_loop");                    // continue with the next entry

    emitter.label("__rt_unser_validate_object");
    emitter.instruction("add x9, x1, #1");                                      // colon after O
    emitter.instruction("cmp x9, x2");                                          // colon byte must be in bounds
    emitter.instruction("b.hs __rt_unser_validate_at_fail");                    // truncated object envelope
    emitter.instruction("ldrb w10, [x0, x9]");                                  // bounded byte after O
    emitter.instruction("cmp w10, #58");                                        // require O:
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // object marker needs its colon
    emitter.instruction("add x1, x9, #1");                                      // first class-name length digit
    emitter.instruction("mov w3, #58");                                         // length delimiter ':'
    emitter.instruction("bl __rt_unser_validate_uint");                         // parse the class-name byte length
    emitter.instruction("cbz x0, __rt_unser_validate_at_fail");                 // propagate numeric failure
    emitter.instruction("mov x11, x1");                                         // class-name byte length
    emitter.instruction("add x9, x2, #1");                                      // opening quote position
    emitter.instruction("ldr x10, [sp, #16]");                                  // saved source end
    emitter.instruction("cmp x9, x10");                                         // quote byte must be in bounds
    emitter.instruction("b.hs __rt_unser_validate_at_fail");                    // truncated before the opening quote
    emitter.instruction("ldr x12, [sp]");                                       // saved source base
    emitter.instruction("ldrb w13, [x12, x9]");                                 // bounded opening-quote byte
    emitter.instruction("cmp w13, #34");                                        // opening quote
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // class name must open with '"'
    emitter.instruction("add x9, x9, #1");                                      // class-name bytes
    emitter.instruction("sub x13, x10, x9");                                    // bytes remaining after the quote
    emitter.instruction("cmp x13, #2");                                         // closing quote and colon
    emitter.instruction("b.lo __rt_unser_validate_at_fail");                    // no room for closing quote and ':'
    emitter.instruction("sub x13, x13, #2");                                    // maximum safe class-name length
    emitter.instruction("cmp x11, x13");                                        // declared name fits before delimiters?
    emitter.instruction("b.hi __rt_unser_validate_at_fail");                    // declared length overruns the source
    emitter.instruction("mov x14, #0");                                        // class-name control-byte scan cursor
    emitter.label("__rt_unser_validate_object_name_scan");
    emitter.instruction("cmp x14, x11");                                       // scanned the full declared class name?
    emitter.instruction("b.hs __rt_unser_validate_object_name_scan_done");     // continue with the closing quote
    emitter.instruction("add x15, x9, x14");                                   // source offset of this class-name byte
    emitter.instruction("ldrb w16, [x12, x15]");                               // load one already bounded class-name byte
    emitter.instruction("cmp w16, #32");                                       // serialized class names cannot contain ASCII controls
    emitter.instruction("b.lo __rt_unser_validate_object_name_invalid");       // fail at the object's marker like php-src
    emitter.instruction("add x14, x14, #1");                                   // scan the next byte
    emitter.instruction("b __rt_unser_validate_object_name_scan");             // continue the bounded scan
    emitter.label("__rt_unser_validate_object_name_invalid");
    emitter.instruction("ldr x1, [sp, #8]");                                   // report the malformed object's starting offset
    emitter.instruction("b __rt_unser_validate_at_fail");                      // reject the invalid class name
    emitter.label("__rt_unser_validate_object_name_scan_done");
    emitter.instruction("add x9, x9, x11");                                     // closing quote
    emitter.instruction("ldrb w13, [x12, x9]");                                 // bounded closing-quote byte
    emitter.instruction("cmp w13, #34");                                        // closing quote
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // class name must close with '"'
    emitter.instruction("add x9, x9, #1");                                      // colon before property count
    emitter.instruction("ldrb w13, [x12, x9]");                                 // bounded delimiter byte
    emitter.instruction("cmp w13, #58");                                        // colon before the property count
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // property count needs its colon
    emitter.instruction("add x1, x9, #1");                                      // first property-count digit
    emitter.instruction("ldr x0, [sp]");                                        // saved source base
    emitter.instruction("ldr x2, [sp, #16]");                                   // saved source end
    emitter.instruction("mov w3, #58");                                         // count delimiter ':'
    emitter.instruction("bl __rt_unser_validate_uint");                         // parse the declared property count
    emitter.instruction("cbz x0, __rt_unser_validate_at_fail");                 // propagate numeric failure
    emitter.instruction("str x1, [sp, #32]");                                   // save property count
    emitter.instruction("add x9, x2, #1");                                      // opening brace position
    emitter.instruction("ldr x10, [sp, #16]");                                  // saved source end
    emitter.instruction("cmp x9, x10");                                         // brace byte must be in bounds
    emitter.instruction("b.hs __rt_unser_validate_at_fail");                    // truncated before the opening brace
    emitter.instruction("ldr x11, [sp]");                                       // saved source base
    emitter.instruction("ldrb w12, [x11, x9]");                                 // bounded opening-brace byte
    emitter.instruction("cmp w12, #123");                                       // opening brace
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // object body must open with '{'
    emitter.instruction("add x9, x9, #1");                                      // step past the opening brace
    emitter.instruction("str x9, [sp, #40]");                                   // first property key
    emitter.instruction("str xzr, [sp, #48]");                                  // property index
    emitter.label("__rt_unser_validate_object_loop");
    emitter.instruction("ldr x9, [sp, #48]");                                   // current property index
    emitter.instruction("ldr x10, [sp, #32]");                                  // declared property count
    emitter.instruction("cmp x9, x10");                                         // every declared property proven?
    emitter.instruction("b.hs __rt_unser_validate_container_close");            // then require the closing brace
    emitter.instruction("ldr x0, [sp]");                                        // saved source base
    emitter.instruction("ldr x1, [sp, #40]");                                   // resume at the saved body position
    emitter.instruction("ldr x2, [sp, #16]");                                   // saved source end
    emitter.instruction("ldr x3, [sp, #24]");                                   // saved recursion depth
    emitter.instruction("add x3, x3, #1");                                      // nested key depth
    emitter.instruction("bl __rt_unser_validate_key");                          // validate one property key
    emitter.instruction("cbz x0, __rt_unser_validate_at_fail");                 // propagate key failure
    emitter.instruction("str x1, [sp, #40]");                                   // position after key
    emitter.instruction("ldr x0, [sp]");                                        // saved source base
    emitter.instruction("ldr x2, [sp, #16]");                                   // saved source end
    emitter.instruction("ldr x3, [sp, #24]");                                   // saved recursion depth
    emitter.instruction("add x3, x3, #1");                                      // nested value depth
    emitter.instruction("bl __rt_unser_validate_at");                           // recursively validate the property value
    emitter.instruction("cbz x0, __rt_unser_validate_at_fail");                 // propagate value failure
    emitter.instruction("str x1, [sp, #40]");                                   // position after value
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the property index
    emitter.instruction("add x9, x9, #1");                                      // one property fully proven
    emitter.instruction("str x9, [sp, #48]");                                   // persist the advanced index
    emitter.instruction("b __rt_unser_validate_object_loop");                   // continue with the next property

    emitter.label("__rt_unser_validate_container_close");
    emitter.instruction("ldr x1, [sp, #40]");                                   // closing-brace position
    emitter.instruction("ldr x2, [sp, #16]");                                   // saved source end
    emitter.instruction("cmp x1, x2");                                          // require the closing brace byte
    emitter.instruction("b.eq __rt_unser_validate_at_ok");                     // a missing final brace is allocation-safe; let hydration run PHP hooks before failing
    emitter.instruction("b.hi __rt_unser_validate_at_fail");                   // reject a cursor that escaped the bounded source
    emitter.instruction("ldr x9, [sp]");                                        // saved source base
    emitter.instruction("ldrb w10, [x9, x1]");                                  // bounded closing-brace byte
    emitter.instruction("cmp w10, #125");                                       // exact closing brace
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // container must close with '}'
    emitter.instruction("add x1, x1, #1");                                      // return after the complete container
    emitter.instruction("b __rt_unser_validate_at_ok");                         // container is fully bounded

    emitter.label("__rt_unser_validate_ref");
    emitter.instruction("add x9, x1, #1");                                      // colon after r/R
    emitter.instruction("cmp x9, x2");                                          // colon byte must be in bounds
    emitter.instruction("b.hs __rt_unser_validate_at_fail");                    // truncated reference envelope
    emitter.instruction("ldrb w10, [x0, x9]");                                  // bounded byte after r/R
    emitter.instruction("cmp w10, #58");                                        // require r:/R:
    emitter.instruction("b.ne __rt_unser_validate_at_fail");                    // reference marker needs its colon
    emitter.instruction("add x1, x9, #1");                                      // first reference-index digit
    emitter.instruction("mov w3, #59");                                         // index terminator ';'
    emitter.instruction("bl __rt_unser_validate_uint");                         // parse the reference index
    emitter.instruction("cbz x0, __rt_unser_validate_at_fail");                 // propagate numeric failure
    emitter.instruction("cbz x1, __rt_unser_validate_at_fail");                 // reference indices are one-based
    emitter.instruction("add x1, x2, #1");                                      // skip semicolon

    emitter.label("__rt_unser_validate_at_ok");
    emitter.instruction("mov x0, #1");                                          // report a fully bounded value
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore recursive validator frame
    emitter.instruction("add sp, sp, #80");                                     // release local validation state
    emitter.instruction("ret");                                                 // return ok=1 to the recursive caller or entry wrapper
    emitter.label("__rt_unser_validate_at_fail");
    emitter.instruction("mov x0, #0");                                          // report malformed/truncated wire data
    crate::codegen_support::abi::emit_symbol_address(emitter, "x11", "_unser_failure_offset");
    emitter.instruction("ldr x10, [x11]");                                     // first failure cursor recorded by the innermost validator frame
    emitter.instruction("cmn x10, #1");                                        // is the no-failure sentinel still present?
    emitter.instruction("b.ne __rt_unser_validate_at_fail_offset_ready");      // recursive parents must preserve the original failure point
    emitter.instruction("str x1, [x11]");                                      // publish the bounded cursor at the first detected grammar failure
    emitter.instruction("mov x10, x1");                                        // return that same cursor to the caller
    emitter.label("__rt_unser_validate_at_fail_offset_ready");
    emitter.instruction("mov x1, x10");                                        // propagate the first failure offset through parent frames
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore recursive validator frame
    emitter.instruction("add sp, sp, #80");                                     // release local validation state
    emitter.instruction("ret");                                                 // return ok=0 to the recursive caller or entry wrapper
}
