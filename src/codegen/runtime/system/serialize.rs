//! Purpose:
//! Emits the `__rt_serialize_value` / `__rt_serialize_mixed` runtime helpers that
//! format a runtime value into PHP `serialize()` wire form, appended to `_concat_buf`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//! - The EIR `serialize()` lowering in `crate::codegen_ir::lower_inst::builtins::system`.
//!
//! Key details:
//! - Output is the exact PHP wire format: `N;`, `b:0;`/`b:1;`, `i:<int>;`,
//!   `d:<shortest-round-trip>;` (`d:INF;`/`d:-INF;`/`d:NAN;` for non-finite),
//!   `s:<bytelen>:"<raw>";`, and (added in a later increment) `a:<n>:{...}`.
//! - Float digits reuse `__rt_json_ftoa` (serialize_precision = -1, the same
//!   shortest-round-trip formatter PHP uses for serialize floats), passing the
//!   uppercase `'E'` exponent marker so exponential floats match PHP's
//!   `serialize`/`var_export` layout (`d:1.0E+20;`), not `json_encode`'s `'e'`.
//! - All helpers append at the current `_concat_off`, advance it past the bytes
//!   written, and return the slice pointer/length in the string result registers
//!   (`x1`/`x2` on AArch64, `rax`/`rdx` on x86_64).

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_serialize_value`, the tag-dispatching serializer, and the
/// `__rt_serialize_mixed` wrapper that unpacks a boxed Mixed cell first.
///
/// `__rt_serialize_value` inputs: AArch64 `x0`=value_tag, `x1`=value_lo,
/// `x2`=value_hi; x86_64 `rdi`=value_tag, `rsi`=value_lo, `rdx`=value_hi.
/// `__rt_serialize_mixed` input: AArch64 `x0` / x86_64 `rax` = boxed Mixed pointer
/// (a null pointer serializes as `N;`).
/// Both return the serialized slice pointer/length in the string result registers.
pub(crate) fn emit_serialize(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_serialize_x86_64(emitter);
        return;
    }
    emit_serialize_aarch64(emitter);
}

/// AArch64 implementation of `__rt_serialize_mixed` and `__rt_serialize_value`.
fn emit_serialize_aarch64(emitter: &mut Emitter) {
    use crate::codegen::abi::emit_symbol_address;

    emitter.blank();
    emitter.comment("--- runtime: serialize_mixed (unbox a Mixed cell, then serialize) ---");
    emitter.label_global("__rt_serialize_mixed");
    emitter.instruction("cbz x0, __rt_serialize_mixed_null");                   // a null Mixed pointer serializes as PHP null
    crate::codegen::abi::emit_load_int_immediate(emitter, "x9", crate::codegen::NULL_SENTINEL);
    emitter.instruction("cmp x0, x9");                                          // is this the in-band null sentinel?
    emitter.instruction("b.eq __rt_serialize_mixed_null");                      // the null sentinel serializes as PHP null
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed runtime value tag
    emitter.instruction("ldr x10, [x0, #8]");                                   // load the boxed low payload word
    emitter.instruction("ldr x2, [x0, #16]");                                   // value_hi argument = boxed high payload word
    emitter.instruction("mov x1, x10");                                         // value_lo argument = boxed low payload word
    emitter.instruction("mov x0, x9");                                          // value_tag argument = boxed runtime tag
    emitter.instruction("b __rt_serialize_value");                              // tail-call the shared tag-dispatching serializer
    emitter.label("__rt_serialize_mixed_null");
    emitter.instruction("mov x0, #8");                                          // synthesize a null value tag for the empty box
    emitter.instruction("mov x1, #0");                                          // null payload low word
    emitter.instruction("mov x2, #0");                                          // null payload high word
    emitter.instruction("b __rt_serialize_value");                              // serialize the synthesized null value

    emitter.blank();
    emitter.comment("--- runtime: serialize_value (tag/lo/hi -> serialize() wire bytes) ---");
    emitter.label_global("__rt_serialize_value");

    // -- set up stack frame --
    // [sp+0]=output start, [sp+8]=write pos, [sp+16]=tag, [sp+24]=lo, [sp+32]=hi
    emitter.instruction("sub sp, sp, #64");                                     // allocate the serialize scratch frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the new frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the value tag across helper calls
    emitter.instruction("str x1, [sp, #24]");                                   // save the low payload word across helper calls
    emitter.instruction("str x2, [sp, #32]");                                   // save the high payload word across helper calls

    // -- compute the current concat_buf write position --
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat-buffer offset
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // compute the absolute write pointer
    emitter.instruction("str x11, [sp, #0]");                                   // save the serialized-slice start pointer
    emitter.instruction("str x11, [sp, #8]");                                   // save the running write pointer

    // -- reference counter: every serialized value consumes the next index, so a
    //    later repeated object can be emitted as r:<index>. Skip objects (tag 6),
    //    which register their own index in the dedup path, and nested-Mixed boxes
    //    (tag 7), where the inner value is counted instead. Array keys flow through
    //    here too and undo this increment right after they are emitted. --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the value tag
    emitter.instruction("cmp x0, #6");                                          // object? counted in the dedup path
    emitter.instruction("b.eq __rt_serialize_value_counted");                   // skip the generic increment
    emitter.instruction("cmp x0, #7");                                          // nested-Mixed box? inner value is counted
    emitter.instruction("b.eq __rt_serialize_value_counted");                   // skip the generic increment
    emit_symbol_address(emitter, "x9", "_ser_value_counter");
    emitter.instruction("ldr x10, [x9]");                                       // load the running value counter
    emitter.instruction("add x10, x10, #1");                                    // this value takes the next index
    emitter.instruction("str x10, [x9]");                                       // publish the advanced counter
    emitter.label("__rt_serialize_value_counted");

    // -- dispatch on the runtime value tag --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the value tag for dispatch
    emitter.instruction("cmp x0, #0");                                          // is the value an integer?
    emitter.instruction("b.eq __rt_serialize_int");                             // serialize integers as i:<n>;
    emitter.instruction("cmp x0, #1");                                          // is the value a string?
    emitter.instruction("b.eq __rt_serialize_str");                             // serialize strings as s:<len>:"...";
    emitter.instruction("cmp x0, #2");                                          // is the value a float?
    emitter.instruction("b.eq __rt_serialize_float");                           // serialize floats as d:<repr>;
    emitter.instruction("cmp x0, #3");                                          // is the value a bool?
    emitter.instruction("b.eq __rt_serialize_bool");                            // serialize bools as b:0;/b:1;
    emitter.instruction("cmp x0, #4");                                          // is the value an indexed array?
    emitter.instruction("b.eq __rt_serialize_arr_indexed");                     // serialize indexed arrays as a:n:{...}
    emitter.instruction("cmp x0, #5");                                          // is the value an associative array?
    emitter.instruction("b.eq __rt_serialize_arr_hash");                        // serialize hashes as a:n:{...}
    emitter.instruction("cmp x0, #6");                                          // is the value an object?
    emitter.instruction("b.eq __rt_serialize_arr_object");                      // serialize objects as O:len:"Class":n:{...}
    emitter.instruction("cmp x0, #7");                                          // is the value a boxed nested Mixed?
    emitter.instruction("b.eq __rt_serialize_nested_mixed");                    // unbox and re-dispatch
    emitter.instruction("cmp x0, #8");                                          // is the value null?
    emitter.instruction("b.eq __rt_serialize_null");                            // serialize null as N;
    // Tag 10 (callables) is not serializable here and degrades to null.
    emitter.instruction("b __rt_serialize_null");                               // unsupported tags serialize as null

    // -- indexed array / hash / nested mixed: delegate, then resume finalize --
    emitter.label("__rt_serialize_arr_indexed");
    emitter.instruction("ldr x0, [sp, #24]");                                   // array pointer = saved low payload word
    emitter.instruction("bl __rt_serialize_indexed_array");                     // append a:n:{...} for the indexed array
    emitter.instruction("b __rt_serialize_after_container");                    // recompute the write pointer and finish
    emitter.label("__rt_serialize_arr_hash");
    emitter.instruction("ldr x0, [sp, #24]");                                   // hash pointer = saved low payload word
    emitter.instruction("bl __rt_serialize_hash");                              // append a:n:{...} for the associative array
    emitter.instruction("b __rt_serialize_after_container");                    // recompute the write pointer and finish
    emitter.label("__rt_serialize_arr_object");
    emitter.instruction("ldr x0, [sp, #24]");                                   // object pointer = saved low payload word
    emitter.instruction("bl __rt_serialize_obj_ref");                           // dedup: emit r:<idx>; if seen, else register -> x0=1 if emitted
    emitter.instruction("cbnz x0, __rt_serialize_after_container");             // back-reference emitted → finalize
    emitter.instruction("ldr x0, [sp, #24]");                                   // object pointer = saved low payload word
    emitter.instruction("bl __rt_serialize_object");                            // append O:len:\"Class\":n:{...} for the object
    emitter.instruction("b __rt_serialize_after_container");                    // recompute the write pointer and finish
    emitter.label("__rt_serialize_nested_mixed");
    emitter.instruction("ldr x0, [sp, #24]");                                   // inner Mixed pointer = saved low payload word
    emitter.instruction("bl __rt_serialize_mixed");                             // unbox and serialize the nested value
    emitter.label("__rt_serialize_after_container");
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // reload the offset advanced by the container serializer
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // recompute the running write pointer
    emitter.instruction("str x11, [sp, #8]");                                   // persist the write pointer for the finalizer
    emitter.instruction("b __rt_serialize_done");                               // finish the serialized value

    // -- null: "N;" --
    emitter.label("__rt_serialize_null");
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the running write pointer
    emitter.instruction("mov w12, #78");                                        // ASCII 'N'
    emitter.instruction("strb w12, [x11]");                                     // write the null type marker
    emitter.instruction("mov w12, #59");                                        // ASCII ';'
    emitter.instruction("strb w12, [x11, #1]");                                 // write the terminating semicolon
    emitter.instruction("add x11, x11, #2");                                    // advance past "N;"
    emitter.instruction("str x11, [sp, #8]");                                   // save the updated write pointer
    emitter.instruction("b __rt_serialize_done");                               // finish the serialized value

    // -- bool: "b:0;" / "b:1;" --
    emitter.label("__rt_serialize_bool");
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the running write pointer
    emitter.instruction("mov w12, #98");                                        // ASCII 'b'
    emitter.instruction("strb w12, [x11]");                                     // write the bool type marker
    emitter.instruction("mov w12, #58");                                        // ASCII ':'
    emitter.instruction("strb w12, [x11, #1]");                                 // write the marker separator
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the bool payload
    emitter.instruction("mov w12, #48");                                        // ASCII '0' for false
    emitter.instruction("mov w13, #49");                                        // ASCII '1' for true
    emitter.instruction("cmp x0, #0");                                          // is the bool payload truthy?
    emitter.instruction("csel w12, w13, w12, ne");                              // select '1' when non-zero, else '0'
    emitter.instruction("strb w12, [x11, #2]");                                 // write the chosen bool digit
    emitter.instruction("mov w12, #59");                                        // ASCII ';'
    emitter.instruction("strb w12, [x11, #3]");                                 // write the terminating semicolon
    emitter.instruction("add x11, x11, #4");                                    // advance past "b:X;"
    emitter.instruction("str x11, [sp, #8]");                                   // save the updated write pointer
    emitter.instruction("b __rt_serialize_done");                               // finish the serialized value

    // -- int: "i:" + digits + ";" --
    emitter.label("__rt_serialize_int");
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the running write pointer
    emitter.instruction("mov w12, #105");                                       // ASCII 'i'
    emitter.instruction("strb w12, [x11]");                                     // write the integer type marker
    emitter.instruction("mov w12, #58");                                        // ASCII ':'
    emitter.instruction("strb w12, [x11, #1]");                                 // write the marker separator
    emitter.instruction("add x11, x11, #2");                                    // advance past "i:"
    emitter.instruction("str x11, [sp, #8]");                                   // save the write pointer before formatting digits
    emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x12, x11, x10");                                   // compute the absolute offset for the digit scratch
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x12, [x9]");                                       // point itoa scratch at the current write position
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload the integer payload
    emitter.instruction("bl __rt_itoa");                                        // format the integer -> x1=digit ptr, x2=digit len
    emit_serialize_copy_run_aarch64(emitter, "__rt_serialize_int"); // copy the digits to the write pos
    emitter.instruction("mov w12, #59");                                        // ASCII ';'
    emitter.instruction("strb w12, [x11]");                                     // write the terminating semicolon
    emitter.instruction("add x11, x11, #1");                                    // advance past the semicolon
    emitter.instruction("str x11, [sp, #8]");                                   // save the updated write pointer
    emitter.instruction("b __rt_serialize_done");                               // finish the serialized value

    // -- string: "s:" + bytelen + ":\"" + raw bytes + "\";" --
    emitter.label("__rt_serialize_str");
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the running write pointer
    emitter.instruction("mov w12, #115");                                       // ASCII 's'
    emitter.instruction("strb w12, [x11]");                                     // write the string type marker
    emitter.instruction("mov w12, #58");                                        // ASCII ':'
    emitter.instruction("strb w12, [x11, #1]");                                 // write the marker separator
    emitter.instruction("add x11, x11, #2");                                    // advance past "s:"
    emitter.instruction("str x11, [sp, #8]");                                   // save the write pointer before formatting the length
    emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x12, x11, x10");                                   // compute the absolute offset for the length scratch
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x12, [x9]");                                       // point itoa scratch at the current write position
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the byte length from the high payload word
    emitter.instruction("bl __rt_itoa");                                        // format the byte length -> x1=digit ptr, x2=digit len
    emit_serialize_copy_run_aarch64(emitter, "__rt_serialize_strlen"); // copy the length digits
    emitter.instruction("mov w12, #58");                                        // ASCII ':'
    emitter.instruction("strb w12, [x11]");                                     // write the length/value separator
    emitter.instruction("mov w12, #34");                                        // ASCII '"'
    emitter.instruction("strb w12, [x11, #1]");                                 // write the opening quote
    emitter.instruction("add x11, x11, #2");                                    // advance past the ":\"" separator
    // -- copy the raw string bytes verbatim (serialize does not escape) --
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the source string pointer
    emitter.instruction("ldr x2, [sp, #32]");                                   // reload the source string length
    emitter.instruction("mov x10, #0");                                         // initialize the byte-copy index
    emitter.label("__rt_serialize_str_copy");
    emitter.instruction("cmp x10, x2");                                         // have all source bytes been copied?
    emitter.instruction("b.ge __rt_serialize_str_copy_done");                   // exit once every byte is copied
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load the next source byte
    emitter.instruction("strb w12, [x11, x10]");                                // store it at the running write position
    emitter.instruction("add x10, x10, #1");                                    // advance the byte-copy index
    emitter.instruction("b __rt_serialize_str_copy");                           // continue copying source bytes
    emitter.label("__rt_serialize_str_copy_done");
    emitter.instruction("add x11, x11, x2");                                    // advance the write pointer past the raw bytes
    emitter.instruction("mov w12, #34");                                        // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                     // write the closing quote
    emitter.instruction("mov w12, #59");                                        // ASCII ';'
    emitter.instruction("strb w12, [x11, #1]");                                 // write the terminating semicolon
    emitter.instruction("add x11, x11, #2");                                    // advance past the closing "\";"
    emitter.instruction("str x11, [sp, #8]");                                   // save the updated write pointer
    emitter.instruction("b __rt_serialize_done");                               // finish the serialized value

    // -- float: "d:" + (INF/-INF/NAN | shortest round-trip) + ";" --
    emitter.label("__rt_serialize_float");
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the running write pointer
    emitter.instruction("mov w12, #100");                                       // ASCII 'd'
    emitter.instruction("strb w12, [x11]");                                     // write the float type marker
    emitter.instruction("mov w12, #58");                                        // ASCII ':'
    emitter.instruction("strb w12, [x11, #1]");                                 // write the marker separator
    emitter.instruction("add x11, x11, #2");                                    // advance past "d:"
    emitter.instruction("str x11, [sp, #8]");                                   // save the write pointer before formatting digits
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the raw float bit pattern
    emitter.instruction("lsr x10, x9, #52");                                    // shift the exponent field into the low bits
    emitter.instruction("and x10, x10, #0x7ff");                                // isolate the 11-bit exponent
    emitter.instruction("cmp x10, #0x7ff");                                     // is the exponent all ones (Inf/NaN)?
    emitter.instruction("b.ne __rt_serialize_float_finite");                    // finite floats use the shortest formatter
    emitter.instruction("lsl x10, x9, #12");                                    // drop the sign+exponent to test the mantissa
    emitter.instruction("cbnz x10, __rt_serialize_float_nan");                  // non-zero mantissa means NaN
    emitter.instruction("tbnz x9, #63, __rt_serialize_float_neginf");           // negative sign means -INF
    // +INF: "INF"
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the write pointer for the literal
    emitter.instruction("mov w12, #73");                                        // ASCII 'I'
    emitter.instruction("strb w12, [x11]");                                     // write 'I'
    emitter.instruction("mov w12, #78");                                        // ASCII 'N'
    emitter.instruction("strb w12, [x11, #1]");                                 // write 'N'
    emitter.instruction("mov w12, #70");                                        // ASCII 'F'
    emitter.instruction("strb w12, [x11, #2]");                                 // write 'F'
    emitter.instruction("add x11, x11, #3");                                    // advance past "INF"
    emitter.instruction("str x11, [sp, #8]");                                   // save the write pointer
    emitter.instruction("b __rt_serialize_float_semi");                         // append the terminating semicolon
    emitter.label("__rt_serialize_float_neginf");
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the write pointer for the literal
    emitter.instruction("mov w12, #45");                                        // ASCII '-'
    emitter.instruction("strb w12, [x11]");                                     // write the negative sign
    emitter.instruction("mov w12, #73");                                        // ASCII 'I'
    emitter.instruction("strb w12, [x11, #1]");                                 // write 'I'
    emitter.instruction("mov w12, #78");                                        // ASCII 'N'
    emitter.instruction("strb w12, [x11, #2]");                                 // write 'N'
    emitter.instruction("mov w12, #70");                                        // ASCII 'F'
    emitter.instruction("strb w12, [x11, #3]");                                 // write 'F'
    emitter.instruction("add x11, x11, #4");                                    // advance past "-INF"
    emitter.instruction("str x11, [sp, #8]");                                   // save the write pointer
    emitter.instruction("b __rt_serialize_float_semi");                         // append the terminating semicolon
    emitter.label("__rt_serialize_float_nan");
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the write pointer for the literal
    emitter.instruction("mov w12, #78");                                        // ASCII 'N'
    emitter.instruction("strb w12, [x11]");                                     // write 'N'
    emitter.instruction("mov w12, #65");                                        // ASCII 'A'
    emitter.instruction("strb w12, [x11, #1]");                                 // write 'A'
    emitter.instruction("mov w12, #78");                                        // ASCII 'N'
    emitter.instruction("strb w12, [x11, #2]");                                 // write 'N'
    emitter.instruction("add x11, x11, #3");                                    // advance past "NAN"
    emitter.instruction("str x11, [sp, #8]");                                   // save the write pointer
    emitter.instruction("b __rt_serialize_float_semi");                         // append the terminating semicolon
    emitter.label("__rt_serialize_float_finite");
    emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x12, x11, x10");                                   // compute the absolute offset for the float digits
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x12, [x9]");                                       // point the float formatter at the current write position
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the raw float bit pattern
    emitter.instruction("fmov d0, x9");                                         // move the bits into the FP argument register
    emitter.instruction("mov w0, #69");                                         // exponent marker 'E' (serialize uppercase layout)
    emitter.instruction("bl __rt_json_ftoa");                                   // append the shortest round-trip digits in place
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // reload the offset advanced by the formatter
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // recompute the write pointer after the digits
    emitter.instruction("str x11, [sp, #8]");                                   // save the write pointer
    emitter.label("__rt_serialize_float_semi");
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the write pointer for the semicolon
    emitter.instruction("mov w12, #59");                                        // ASCII ';'
    emitter.instruction("strb w12, [x11]");                                     // write the terminating semicolon
    emitter.instruction("add x11, x11, #1");                                    // advance past the semicolon
    emitter.instruction("str x11, [sp, #8]");                                   // save the updated write pointer
    emitter.instruction("b __rt_serialize_done");                               // finish the serialized value

    // -- finalize: update _concat_off and return the slice pointer/length --
    emitter.label("__rt_serialize_done");
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the final write pointer
    emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x12, x11, x10");                                   // compute the absolute end offset
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x12, [x9]");                                       // publish the advanced concat-buffer offset
    emitter.instruction("ldr x1, [sp, #0]");                                    // result pointer = serialized-slice start
    emitter.instruction("sub x2, x11, x1");                                     // result length = end pointer - start pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate the serialize scratch frame
    emitter.instruction("ret");                                                 // return the serialized slice in x1/x2

    // -- __rt_serialize_uint: append a u64's decimal digits at _concat_off --
    emitter.blank();
    emitter.comment("--- runtime: serialize_uint (append decimal digits, no prefix) ---");
    emitter.label_global("__rt_serialize_uint");
    emitter.instruction("sub sp, sp, #32");                                     // small frame for the digit helper
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the value across the itoa call
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current write offset
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x12, x11, x10");                                   // compute the write target pointer
    emitter.instruction("str x12, [sp, #8]");                                   // save the write target across the itoa call
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the value to format
    emitter.instruction("bl __rt_itoa");                                        // format digits into scratch -> x1=ptr, x2=len
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the write target pointer
    emitter.instruction("mov x10, #0");                                         // initialize the digit-copy index
    emitter.label("__rt_serialize_uint_copy");
    emitter.instruction("cmp x10, x2");                                         // copied every digit?
    emitter.instruction("b.ge __rt_serialize_uint_done");                       // exit once all digits are copied
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load the next digit byte
    emitter.instruction("strb w12, [x11, x10]");                                // store it at the write target
    emitter.instruction("add x10, x10, #1");                                    // advance the digit-copy index
    emitter.instruction("b __rt_serialize_uint_copy");                          // continue copying digits
    emitter.label("__rt_serialize_uint_done");
    emitter.instruction("add x11, x11, x2");                                    // advance the write pointer past the digits
    emit_symbol_address(emitter, "x9", "_concat_off");
    emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x12, x11, x10");                                   // compute the new absolute offset
    emitter.instruction("str x12, [x9]");                                       // publish the advanced offset
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the digit-helper frame
    emitter.instruction("ret");                                                 // return with digits appended

    // -- __rt_serialize_indexed_array: append a:n:{ i:K;<v>... } for a tag-4 array --
    emitter.blank();
    emitter.comment("--- runtime: serialize_indexed_array (PHP a:n:{...} for indexed arrays) ---");
    emitter.label_global("__rt_serialize_indexed_array");
    emit_append_literal_aarch64(emitter, &[b'a', b':'], "the array prefix");
    emitter.instruction("b __rt_serialize_indexed_body");                       // emit the shared <count>:{...} body (x0 still = array)
    // __rt_serialize_indexed_body: append <count>:{ i:K;<v>... } WITHOUT the
    // leading "a:", so object bodies (O:...:<count>:{...}) can reuse it.
    emitter.label_global("__rt_serialize_indexed_body");
    // [sp+0]=array, [sp+8]=len, [sp+16]=value_type, [sp+24]=index
    emitter.instruction("sub sp, sp, #96");                                     // frame for the indexed-array serializer
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the array pointer
    emitter.instruction("ldr x1, [x0]");                                        // load the element count from the header
    emitter.instruction("str x1, [sp, #8]");                                    // save the element count
    emitter.instruction("ldur x2, [x0, #-8]");                                  // load the packed array kind word
    emitter.instruction("lsr x2, x2, #8");                                      // shift the value_type field into the low bits
    emitter.instruction("and x2, x2, #0x7f");                                   // isolate the 7-bit value_type
    emitter.instruction("str x2, [sp, #16]");                                   // save the array value_type
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the element count
    emitter.instruction("bl __rt_serialize_uint");                              // append the element count digits
    emit_append_literal_aarch64(emitter, &[b':', b'{'], "the array body open");
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize the element index
    emitter.label("__rt_serialize_idx_loop");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the element index
    emitter.instruction("ldr x3, [sp, #8]");                                    // reload the element count
    emitter.instruction("cmp x4, x3");                                          // have all elements been serialized?
    emitter.instruction("b.ge __rt_serialize_idx_close");                       // close the container when done
    emitter.instruction("ldr x1, [sp, #24]");                                   // integer key = element index
    emitter.instruction("mov x0, #0");                                          // key value tag = int
    emitter.instruction("mov x2, #0");                                          // key high payload word unused
    emitter.instruction("bl __rt_serialize_value");                             // append the i:<index>; key
    emit_uncount_key_aarch64(emitter);                                          // keys do not consume a reference index
    emitter.instruction("ldr x6, [sp, #16]");                                   // reload the array value_type
    emitter.instruction("ldr x7, [sp, #0]");                                    // reload the array pointer
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the element index
    emitter.instruction("cmp x6, #1");                                          // is the element type a string (16-byte slots)?
    emitter.instruction("b.eq __rt_serialize_idx_str");                         // strings load a ptr/len pair
    emitter.instruction("add x8, x4, #3");                                      // skip the 24-byte (3-word) array header
    emitter.instruction("ldr x1, [x7, x8, lsl #3]");                            // load the 8-byte element payload
    emitter.instruction("cmp x6, #7");                                          // is the element a boxed Mixed payload?
    emitter.instruction("b.eq __rt_serialize_idx_mixed");                       // boxed mixed elements unbox via serialize_mixed
    emitter.instruction("mov x0, x6");                                          // value tag = array value_type
    emitter.instruction("mov x2, #0");                                          // value high payload word unused for 8-byte slots
    emitter.instruction("bl __rt_serialize_value");                             // serialize the scalar/array element
    emitter.instruction("b __rt_serialize_idx_next");                           // advance to the next element
    emitter.label("__rt_serialize_idx_str");
    emitter.instruction("add x8, x4, x4");                                      // index * 2 for the ptr/len pair
    emitter.instruction("add x8, x8, #3");                                      // skip the 24-byte (3-word) array header
    emitter.instruction("ldr x1, [x7, x8, lsl #3]");                            // load the string pointer
    emitter.instruction("add x8, x8, #1");                                      // advance to the length slot
    emitter.instruction("ldr x2, [x7, x8, lsl #3]");                            // load the string length
    emitter.instruction("mov x0, #1");                                          // value tag = string
    emitter.instruction("bl __rt_serialize_value");                             // serialize the string element
    emitter.instruction("b __rt_serialize_idx_next");                           // advance to the next element
    emitter.label("__rt_serialize_idx_mixed");
    emitter.instruction("mov x0, x1");                                          // boxed Mixed pointer argument
    emitter.instruction("bl __rt_serialize_mixed");                             // unbox and serialize the element
    emitter.label("__rt_serialize_idx_next");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the element index
    emitter.instruction("add x4, x4, #1");                                      // advance to the next element
    emitter.instruction("str x4, [sp, #24]");                                   // persist the element index
    emitter.instruction("b __rt_serialize_idx_loop");                           // continue the element loop
    emitter.label("__rt_serialize_idx_close");
    emit_append_literal_aarch64(emitter, &[b'}'], "the array body close");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the indexed-array frame
    emitter.instruction("ret");                                                 // return with the indexed array appended

    // -- __rt_serialize_hash: append a:n:{ <key><value>... } for a tag-5 hash --
    emitter.blank();
    emitter.comment("--- runtime: serialize_hash (PHP a:n:{...} for associative arrays) ---");
    emitter.label_global("__rt_serialize_hash");
    emit_append_literal_aarch64(emitter, &[b'a', b':'], "the array prefix");
    emitter.instruction("b __rt_serialize_hash_body");                          // emit the shared <count>:{...} body (x0 still = hash)
    // __rt_serialize_hash_body: append <count>:{ <key><value>... } WITHOUT the
    // leading "a:", so object bodies (O:...:<count>:{...}) can reuse it.
    emitter.label_global("__rt_serialize_hash_body");
    // [0]=hash [8]=count [16]=cursor [24]=written [32]=key [40]=keylen [48]=vlo [56]=vhi [64]=vtag
    emitter.instruction("sub sp, sp, #96");                                     // frame for the hash serializer
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the hash pointer
    emitter.instruction("bl __rt_hash_count");                                  // count entries -> x0
    emitter.instruction("str x0, [sp, #8]");                                    // save the entry count
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the entry count
    emitter.instruction("bl __rt_serialize_uint");                              // append the entry count digits
    emit_append_literal_aarch64(emitter, &[b':', b'{'], "the array body open");
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize the iterator cursor
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize the written-entry count
    emitter.label("__rt_serialize_hash_loop");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the written-entry count
    emitter.instruction("ldr x3, [sp, #8]");                                    // reload the total entry count
    emitter.instruction("cmp x4, x3");                                          // have all entries been serialized?
    emitter.instruction("b.ge __rt_serialize_hash_close");                      // close the container when done
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the hash pointer
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // next entry -> x0..x5
    emitter.instruction("str x0, [sp, #16]");                                   // save the advanced cursor
    emitter.instruction("str x1, [sp, #32]");                                   // save the key pointer / integer key
    emitter.instruction("str x2, [sp, #40]");                                   // save the key length (-1 for int keys)
    emitter.instruction("str x3, [sp, #48]");                                   // save the value low payload word
    emitter.instruction("str x4, [sp, #56]");                                   // save the value high payload word
    emitter.instruction("str x5, [sp, #64]");                                   // save the value tag
    emitter.instruction("ldr x2, [sp, #40]");                                   // reload the key length
    emitter.instruction("cmn x2, #1");                                          // is this an integer key (length == -1)?
    emitter.instruction("b.eq __rt_serialize_hash_key_int");                    // integer keys serialize as i:K;
    emitter.instruction("ldr x1, [sp, #32]");                                   // string key pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // string key length
    emitter.instruction("mov x0, #1");                                          // key value tag = string
    emitter.instruction("bl __rt_serialize_value");                             // append the s:len:"key"; key
    emitter.instruction("b __rt_serialize_hash_key_done");                      // continue to the value
    emitter.label("__rt_serialize_hash_key_int");
    emitter.instruction("ldr x1, [sp, #32]");                                   // integer key payload
    emitter.instruction("mov x0, #0");                                          // key value tag = int
    emitter.instruction("mov x2, #0");                                          // key high payload word unused
    emitter.instruction("bl __rt_serialize_value");                             // append the i:K; key
    emitter.label("__rt_serialize_hash_key_done");
    emit_uncount_key_aarch64(emitter);                                          // keys do not consume a reference index
    emitter.instruction("ldr x0, [sp, #64]");                                   // reload the value tag
    emitter.instruction("ldr x1, [sp, #48]");                                   // reload the value low payload word
    emitter.instruction("ldr x2, [sp, #56]");                                   // reload the value high payload word
    emitter.instruction("bl __rt_serialize_value");                             // append the serialized value
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload the written-entry count
    emitter.instruction("add x4, x4, #1");                                      // count this entry as written
    emitter.instruction("str x4, [sp, #24]");                                   // persist the written-entry count
    emitter.instruction("b __rt_serialize_hash_loop");                          // continue the entry loop
    emitter.label("__rt_serialize_hash_close");
    emit_append_literal_aarch64(emitter, &[b'}'], "the array body close");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the hash frame
    emitter.instruction("ret");                                                 // return with the hash appended

    // -- __rt_concat_append: append x1 raw bytes from x0 into _concat_buf at _concat_off --
    emitter.label_global("__rt_concat_append");
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // current write offset
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // write target pointer
    emitter.instruction("mov x12, #0");                                         // byte cursor = 0
    emitter.label("__rt_concat_append_loop");
    emitter.instruction("cmp x12, x1");                                         // copied the whole run?
    emitter.instruction("b.ge __rt_concat_append_done");                        // exit when the run is copied
    emitter.instruction("ldrb w13, [x0, x12]");                                 // load a source byte
    emitter.instruction("strb w13, [x11, x12]");                                // store it into the buffer
    emitter.instruction("add x12, x12, #1");                                    // advance the byte cursor
    emitter.instruction("b __rt_concat_append_loop");                           // continue copying
    emitter.label("__rt_concat_append_done");
    emitter.instruction("add x10, x10, x1");                                    // advance the write offset by the run
    emitter.instruction("str x10, [x9]");                                       // persist the new write offset
    emitter.instruction("ret");                                                 // return with the bytes appended

    // -- __rt_serialize_pstr: append a serialized string s:len:"bytes"; (x0=ptr, x1=len) --
    emitter.label_global("__rt_serialize_pstr");
    emitter.instruction("sub sp, sp, #32");                                     // allocate the pstr frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set the frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the string pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the string length
    emit_append_literal_aarch64(emitter, &[b's', b':'], "the string prefix");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the string length
    emitter.instruction("bl __rt_serialize_uint");                              // append the length digits
    emit_append_literal_aarch64(emitter, &[b':', b'"'], "the string open quote");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the string pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the string length
    emitter.instruction("bl __rt_concat_append");                               // copy the raw string bytes
    emit_append_literal_aarch64(emitter, &[b'"', b';'], "the string close");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the pstr frame
    emitter.instruction("ret");                                                 // return with the string appended

    // -- __rt_serialize_object: append O:len:"Class":n:{<key><val>...} (x0=object ptr) --
    emitter.label_global("__rt_serialize_object");
    emitter.instruction("sub sp, sp, #96");                                     // allocate the object frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set the frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer
    emitter.instruction("ldr x1, [x0]");                                        // load the class id from the object header
    emitter.instruction("str x1, [sp, #8]");                                    // save the class id
    emit_symbol_address(emitter, "x9", "_class_name_entries");
    emitter.instruction("add x10, x9, x1, lsl #4");                             // entry = base + class_id*16
    emitter.instruction("ldr x11, [x10]");                                      // class name pointer
    emitter.instruction("ldr x12, [x10, #8]");                                  // class name length
    emitter.instruction("str x11, [sp, #40]");                                  // save the class name pointer
    emitter.instruction("str x12, [sp, #48]");                                  // save the class name length
    emit_append_literal_aarch64(emitter, &[b'O', b':'], "the object prefix");
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload the class name length
    emitter.instruction("bl __rt_serialize_uint");                              // append the class name length digits
    emit_append_literal_aarch64(emitter, &[b':', b'"'], "the class name open quote");
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the class name pointer
    emitter.instruction("ldr x1, [sp, #48]");                                   // reload the class name length
    emitter.instruction("bl __rt_concat_append");                               // copy the class name bytes
    emit_append_literal_aarch64(emitter, &[b'"', b':'], "the class name close");
    // -- __serialize magic: when the class defines __serialize(), the object body
    //    is the returned array's <count>:{key;val;...} pairs (not the raw props) --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the class id
    emit_symbol_address(emitter, "x9", "_class_serialize_ptrs");
    emitter.instruction("ldr x10, [x9, x1, lsl #3]");                           // __serialize method symbol (0 if none)
    emitter.instruction("cbz x10, __rt_serialize_object_sleep");                // no __serialize → try __sleep, else property walk
    emitter.instruction("str x10, [sp, #16]");                                  // park the __serialize target across the call
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // capture the write offset just after the O:...:\"name\": prefix
    emitter.instruction("str x10, [sp, #24]");                                  // save it so any method scratch can be rewound away
    emitter.instruction("ldr x0, [sp, #0]");                                    // $this receiver for the method call
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the __serialize target
    emitter.instruction("blr x10");                                             // call __serialize($this) -> x0 = array (bare pointer)
    emitter.instruction("str x0, [sp, #32]");                                   // save the returned array pointer
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the saved post-prefix offset
    emitter.instruction("str x10, [x9]");                                       // rewind, discarding any concat scratch the method left
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the returned array pointer
    emitter.instruction("ldur x9, [x0, #-8]");                                  // load its heap kind word
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the heap kind (2=indexed, 3=hash)
    emitter.instruction("cmp x9, #3");                                          // is the returned array a hash?
    emitter.instruction("b.eq __rt_serialize_object_ser_hash");                 // hashes use the hash body emitter
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the indexed array pointer
    emitter.instruction("bl __rt_serialize_indexed_body");                      // append <count>:{ i:K;<v>... }
    emitter.instruction("b __rt_serialize_object_magic_done");                  // finish the object
    emitter.label("__rt_serialize_object_ser_hash");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the hash pointer
    emitter.instruction("bl __rt_serialize_hash_body");                         // append <count>:{ <key><val>... }
    emitter.label("__rt_serialize_object_magic_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the object frame
    emitter.instruction("ret");                                                 // return with the object appended
    // -- __sleep magic: when the class defines __sleep(), serialize only the
    //    named properties (in __sleep's order) using their mangled keys --
    emitter.label("__rt_serialize_object_sleep");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the class id
    emit_symbol_address(emitter, "x9", "_class_sleep_ptrs");
    emitter.instruction("ldr x10, [x9, x1, lsl #3]");                           // __sleep method symbol (0 if none)
    emitter.instruction("cbz x10, __rt_serialize_object_default");              // no __sleep → walk every property
    emitter.instruction("str x10, [sp, #16]");                                  // park the __sleep target across the call
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // capture the post-prefix write offset
    emitter.instruction("str x10, [sp, #24]");                                  // save it for the scratch rewind
    emitter.instruction("ldr x0, [sp, #0]");                                    // $this receiver
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the __sleep target
    emitter.instruction("blr x10");                                             // call __sleep($this) -> x0 = names array (indexed)
    emitter.instruction("str x0, [sp, #32]");                                   // save the names array pointer
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the saved post-prefix offset
    emitter.instruction("str x10, [x9]");                                       // rewind away any method scratch
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload the names array pointer
    emitter.instruction("ldr x0, [x0]");                                        // names count = indexed-array header word
    emitter.instruction("str x0, [sp, #24]");                                   // save the names count
    emitter.instruction("bl __rt_serialize_uint");                              // append the property count digits
    emit_append_literal_aarch64(emitter, &[b':', b'{'], "the object body open");
    emitter.instruction("str xzr, [sp, #40]");                                  // name index = 0
    emitter.label("__rt_serialize_object_sleep_loop");
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the name index
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload the names count
    emitter.instruction("cmp x4, x3");                                          // emitted every named property?
    emitter.instruction("b.ge __rt_serialize_object_sleep_done");               // close the body when done
    emitter.instruction("ldr x7, [sp, #32]");                                   // reload the names array pointer
    emitter.instruction("add x8, x4, x4");                                      // index * 2 for the string ptr/len pair
    emitter.instruction("add x8, x8, #3");                                      // skip the 24-byte (3-word) header
    emitter.instruction("ldr x1, [x7, x8, lsl #3]");                            // name string pointer
    emitter.instruction("add x8, x8, #1");                                      // advance to the length slot
    emitter.instruction("ldr x2, [x7, x8, lsl #3]");                            // name string length
    emitter.instruction("ldr x0, [sp, #0]");                                    // object pointer
    emitter.instruction("bl __rt_serialize_named_prop");                        // emit mangled-key + value for the named property
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the name index
    emitter.instruction("add x4, x4, #1");                                      // advance to the next name
    emitter.instruction("str x4, [sp, #40]");                                   // persist the name index
    emitter.instruction("b __rt_serialize_object_sleep_loop");                  // continue the name loop
    emitter.label("__rt_serialize_object_sleep_done");
    emit_append_literal_aarch64(emitter, &[b'}'], "the object body close");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the object frame
    emitter.instruction("ret");                                                 // return with the object appended

    emitter.label("__rt_serialize_object_default");
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the class id
    emit_symbol_address(emitter, "x9", "_class_serprop_ptrs");
    emitter.instruction("ldr x13, [x9, x1, lsl #3]");                           // property-info table pointer
    emitter.instruction("str x13, [sp, #16]");                                  // save the property-info pointer
    emitter.instruction("ldr x0, [x13]");                                       // load the property count
    emitter.instruction("str x0, [sp, #24]");                                   // save the property count
    emitter.instruction("bl __rt_serialize_uint");                              // append the property count digits
    emit_append_literal_aarch64(emitter, &[b':', b'{'], "the object body open");
    emitter.instruction("str xzr, [sp, #32]");                                  // property cursor = 0
    emitter.label("__rt_serialize_object_loop");
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload the property cursor
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload the property count
    emitter.instruction("cmp x4, x3");                                          // processed every property?
    emitter.instruction("b.ge __rt_serialize_object_close");                    // close the body when done
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the property-info pointer
    emitter.instruction("add x14, x13, #8");                                    // skip the count to the first row
    emitter.instruction("add x14, x14, x4, lsl #5");                            // row = rows + cursor*32
    emitter.instruction("ldr x0, [x14]");                                       // mangled key pointer
    emitter.instruction("ldr x1, [x14, #8]");                                   // mangled key length
    emitter.instruction("bl __rt_serialize_pstr");                              // append the mangled property key
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload the property cursor
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the property-info pointer
    emitter.instruction("add x14, x13, #8");                                    // skip the count to the first row
    emitter.instruction("add x14, x14, x4, lsl #5");                            // row = rows + cursor*32
    emitter.instruction("ldr x9, [x14, #16]");                                  // property byte offset
    emitter.instruction("ldr x10, [x14, #24]");                                 // property value tag
    emitter.instruction("ldr x7, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("add x7, x7, x9");                                      // address of the property slot
    emitter.instruction("ldr x1, [x7]");                                        // value low payload word
    emitter.instruction("ldr x2, [x7, #8]");                                    // value high payload word
    emitter.instruction("mov x0, x10");                                         // value tag
    emitter.instruction("bl __rt_serialize_value");                             // append the serialized property value
    emitter.instruction("ldr x4, [sp, #32]");                                   // reload the property cursor
    emitter.instruction("add x4, x4, #1");                                      // advance to the next property
    emitter.instruction("str x4, [sp, #32]");                                   // persist the cursor
    emitter.instruction("b __rt_serialize_object_loop");                        // continue the property loop
    emitter.label("__rt_serialize_object_close");
    emit_append_literal_aarch64(emitter, &[b'}'], "the object body close");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the object frame
    emitter.instruction("ret");                                                 // return with the object appended

    // -- __rt_serialize_named_prop(x0=obj, x1=name_ptr, x2=name_len): used by the
    //    __sleep path. Finds the property whose unmangled (short) name matches and
    //    appends its PHP-mangled key followed by its serialized value. The short
    //    name is the mangled key's suffix after its last NUL (public keys have no
    //    NUL, so the short name is the whole key). Unknown names are skipped. --
    emitter.blank();
    emitter.comment("--- runtime: serialize_named_prop (emit one __sleep-named property) ---");
    emitter.label_global("__rt_serialize_named_prop");
    // [0]=obj [8]=name_ptr [16]=name_len [24]=serprop [32]=count [40]=row index
    emitter.instruction("sub sp, sp, #80");                                     // frame for the named-property emitter
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the target name pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the target name length
    emitter.instruction("ldr x9, [x0]");                                        // class id from the object header
    emit_symbol_address(emitter, "x10", "_class_serprop_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // property-info table for this class
    emitter.instruction("str x10, [sp, #24]");                                  // save the property-info pointer
    emitter.instruction("ldr x11, [x10]");                                      // property count
    emitter.instruction("str x11, [sp, #32]");                                  // save the property count
    emitter.instruction("str xzr, [sp, #40]");                                  // row index = 0
    emitter.label("__rt_serialize_named_prop_loop");
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the row index
    emitter.instruction("ldr x3, [sp, #32]");                                   // reload the property count
    emitter.instruction("cmp x4, x3");                                          // scanned every row?
    emitter.instruction("b.ge __rt_serialize_named_prop_done");                 // unknown name → emit nothing
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the property-info pointer
    emitter.instruction("add x14, x10, #8");                                    // skip the count word to the rows
    emitter.instruction("add x14, x14, x4, lsl #5");                            // row = rows + index*32
    emitter.instruction("ldr x5, [x14]");                                       // mangled key pointer
    emitter.instruction("ldr x6, [x14, #8]");                                   // mangled key length
    // -- compute the short-name offset: index just past the last NUL byte --
    emitter.instruction("mov x7, #0");                                          // scan cursor
    emitter.instruction("mov x12, #0");                                         // short-name offset (no NUL → whole key)
    emitter.label("__rt_serialize_named_prop_scan");
    emitter.instruction("cmp x7, x6");                                          // scanned the whole key?
    emitter.instruction("b.ge __rt_serialize_named_prop_scan_done");            // stop at the key end
    emitter.instruction("ldrb w13, [x5, x7]");                                  // load the next key byte
    emitter.instruction("cbnz w13, __rt_serialize_named_prop_scan_next");       // non-NUL bytes do not move the short-name start
    emitter.instruction("add x12, x7, #1");                                     // short name starts just after this NUL (last wins)
    emitter.label("__rt_serialize_named_prop_scan_next");
    emitter.instruction("add x7, x7, #1");                                      // advance the scan cursor
    emitter.instruction("b __rt_serialize_named_prop_scan");                    // continue scanning
    emitter.label("__rt_serialize_named_prop_scan_done");
    emitter.instruction("add x15, x5, x12");                                    // short name pointer
    emitter.instruction("sub x16, x6, x12");                                    // short name length
    emitter.instruction("ldr x2, [sp, #16]");                                   // target name length
    emitter.instruction("cmp x16, x2");                                         // same length as the target name?
    emitter.instruction("b.ne __rt_serialize_named_prop_next");                 // lengths differ → not this row
    emitter.instruction("ldr x1, [sp, #8]");                                    // target name pointer
    emitter.instruction("mov x7, #0");                                          // byte-compare cursor
    emitter.label("__rt_serialize_named_prop_cmp");
    emitter.instruction("cmp x7, x2");                                          // compared all bytes?
    emitter.instruction("b.ge __rt_serialize_named_prop_match");                // full match
    emitter.instruction("ldrb w13, [x15, x7]");                                 // short-name byte
    emitter.instruction("ldrb w17, [x1, x7]");                                  // target-name byte
    emitter.instruction("cmp w13, w17");                                        // bytes equal?
    emitter.instruction("b.ne __rt_serialize_named_prop_next");                 // mismatch → not this row
    emitter.instruction("add x7, x7, #1");                                      // next byte
    emitter.instruction("b __rt_serialize_named_prop_cmp");                     // continue comparing
    emitter.label("__rt_serialize_named_prop_match");
    emitter.instruction("mov x0, x5");                                          // mangled key pointer
    emitter.instruction("mov x1, x6");                                          // mangled key length
    emitter.instruction("bl __rt_serialize_pstr");                              // append the s:len:"\0*\0name"; key
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the property-info pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the row index
    emitter.instruction("add x14, x10, #8");                                    // skip the count word to the rows
    emitter.instruction("add x14, x14, x4, lsl #5");                            // row = rows + index*32
    emitter.instruction("ldr x9, [x14, #16]");                                  // property byte offset
    emitter.instruction("ldr x10, [x14, #24]");                                 // property value tag
    emitter.instruction("ldr x7, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("add x7, x7, x9");                                      // address of the property slot
    emitter.instruction("ldr x1, [x7]");                                        // value low payload word
    emitter.instruction("ldr x2, [x7, #8]");                                    // value high payload word
    emitter.instruction("mov x0, x10");                                         // value tag
    emitter.instruction("bl __rt_serialize_value");                             // append the serialized property value
    emitter.instruction("b __rt_serialize_named_prop_done");                    // stop after the matching property
    emitter.label("__rt_serialize_named_prop_next");
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the row index
    emitter.instruction("add x4, x4, #1");                                      // advance to the next row
    emitter.instruction("str x4, [sp, #40]");                                   // persist the row index
    emitter.instruction("b __rt_serialize_named_prop_loop");                    // continue scanning
    emitter.label("__rt_serialize_named_prop_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate the named-property frame
    emitter.instruction("ret");                                                 // return with the named property appended

    // -- __rt_serialize_begin: reset the reference state for a new top-level
    //    serialize() call (value counter and the seen-objects map) --
    emitter.blank();
    emitter.comment("--- runtime: serialize_begin (reset reference tracking) ---");
    emitter.label_global("__rt_serialize_begin");
    emit_symbol_address(emitter, "x9", "_ser_value_counter");
    emitter.instruction("str xzr, [x9]");                                       // value counter = 0
    emit_symbol_address(emitter, "x9", "_ser_obj_count");
    emitter.instruction("str xzr, [x9]");                                       // seen-objects count = 0
    emitter.instruction("ret");                                                 // reference tracking is reset

    // -- __rt_serialize_obj_ref(x0=obj): dedup objects by identity. If the object
    //    pointer was already serialized, append r:<index>; and return 1 so the
    //    caller skips re-serializing it. Otherwise assign it the next reference
    //    index, register it (until the map is full), and return 0. --
    emitter.blank();
    emitter.comment("--- runtime: serialize_obj_ref (object identity back-reference) ---");
    emitter.label_global("__rt_serialize_obj_ref");
    emitter.instruction("sub sp, sp, #32");                                     // frame for the dedup helper
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer
    emit_symbol_address(emitter, "x9", "_ser_obj_count");
    emitter.instruction("ldr x9, [x9]");                                        // number of registered objects
    emitter.instruction("mov x10, #0");                                         // scan index = 0
    emitter.label("__rt_serialize_obj_ref_scan");
    emitter.instruction("cmp x10, x9");                                         // scanned every registered object?
    emitter.instruction("b.ge __rt_serialize_obj_ref_new");                     // not seen → register a new index
    emit_symbol_address(emitter, "x11", "_ser_obj_ptrs");
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // registered object pointer
    emitter.instruction("ldr x13, [sp, #0]");                                   // this object pointer
    emitter.instruction("cmp x12, x13");                                        // same identity?
    emitter.instruction("b.eq __rt_serialize_obj_ref_found");                   // already serialized → emit a back-reference
    emitter.instruction("add x10, x10, #1");                                    // next registered object
    emitter.instruction("b __rt_serialize_obj_ref_scan");                       // continue scanning
    emitter.label("__rt_serialize_obj_ref_found");
    emit_symbol_address(emitter, "x11", "_ser_obj_idxs");
    emitter.instruction("ldr x0, [x11, x10, lsl #3]");                          // the recorded reference index
    emitter.instruction("str x0, [sp, #8]");                                    // save it across the literal appends
    emit_append_literal_aarch64(emitter, &[b'r', b':'], "the back-reference prefix");
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the reference index
    emitter.instruction("bl __rt_serialize_uint");                              // append the index digits
    emit_append_literal_aarch64(emitter, &[b';'], "the back-reference terminator");
    emitter.instruction("mov x0, #1");                                          // signal: r:<index>; emitted
    emitter.instruction("b __rt_serialize_obj_ref_ret");                        // return to the caller
    emitter.label("__rt_serialize_obj_ref_new");
    emit_symbol_address(emitter, "x9", "_ser_value_counter");
    emitter.instruction("ldr x10, [x9]");                                       // load the running value counter
    emitter.instruction("add x10, x10, #1");                                    // this object takes the next index
    emitter.instruction("str x10, [x9]");                                       // publish the advanced counter
    emit_symbol_address(emitter, "x11", "_ser_obj_count");
    emitter.instruction("ldr x12, [x11]");                                      // current registered-object count
    emitter.instruction("mov x13, #65536");                                     // object-map capacity
    emitter.instruction("cmp x12, x13");                                        // is the map full?
    emitter.instruction("b.ge __rt_serialize_obj_ref_done");                    // full → keep counting but stop deduping
    emit_symbol_address(emitter, "x13", "_ser_obj_ptrs");
    emitter.instruction("ldr x14, [sp, #0]");                                   // object pointer
    emitter.instruction("str x14, [x13, x12, lsl #3]");                         // record the object pointer
    emit_symbol_address(emitter, "x13", "_ser_obj_idxs");
    emitter.instruction("str x10, [x13, x12, lsl #3]");                         // record its reference index
    emitter.instruction("add x12, x12, #1");                                    // grow the map
    emitter.instruction("str x12, [x11]");                                      // publish the new count
    emitter.label("__rt_serialize_obj_ref_done");
    emitter.instruction("mov x0, #0");                                          // signal: registered, serialize the object
    emitter.label("__rt_serialize_obj_ref_ret");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the dedup frame
    emitter.instruction("ret");                                                 // return the back-reference flag in x0
}

/// Emits the AArch64 sequence that decrements `_ser_value_counter` by one,
/// undoing the generic per-value increment for an array key (keys do not consume
/// a PHP reference index). Clobbers x9/x10.
fn emit_uncount_key_aarch64(emitter: &mut Emitter) {
    use crate::codegen::abi::emit_symbol_address;
    emit_symbol_address(emitter, "x9", "_ser_value_counter");
    emitter.instruction("ldr x10, [x9]");                                       // load the running value counter
    emitter.instruction("sub x10, x10, #1");                                    // a key does not consume an index
    emitter.instruction("str x10, [x9]");                                       // publish the corrected counter
}

/// Emits an AArch64 sequence that appends fixed literal bytes at the current
/// `_concat_off`, advancing the offset (clobbers x9-x12).
fn emit_append_literal_aarch64(emitter: &mut Emitter, bytes: &[u8], what: &str) {
    use crate::codegen::abi::emit_symbol_address;
    emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current write offset
    emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // compute the write target pointer
    for (i, b) in bytes.iter().enumerate() {
        emitter.instruction(&format!("mov w12, #{}", b));                       // literal byte for {what}
        emitter.instruction(&format!("strb w12, [x11, #{}]", i));               // write the literal byte
    }
    emitter.instruction(&format!("add x10, x10, #{}", bytes.len()));            // advance the offset
    emitter.instruction("str x10, [x9]");                                       // publish the advanced offset
    let _ = what;
}

/// Emits the AArch64 digit-copy run shared by the integer and string-length paths.
///
/// On entry `x1`/`x2` describe an itoa-produced digit slice and `[sp+8]` holds the
/// running write pointer; on exit the digits are copied to the write position and
/// `x11` points just past them (the caller persists `x11` to `[sp+8]`).
fn emit_serialize_copy_run_aarch64(emitter: &mut Emitter, prefix: &str) {
    let loop_label = format!("{}_digit_copy", prefix);
    let done_label = format!("{}_digit_done", prefix);
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the running write pointer
    emitter.instruction("mov x10, #0");                                         // initialize the digit-copy index
    emitter.label(&loop_label);
    emitter.instruction("cmp x10, x2");                                         // have all digits been copied?
    emitter.instruction(&format!("b.ge {}", done_label));                       // exit once every digit is copied
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load the next digit byte
    emitter.instruction("strb w12, [x11, x10]");                                // store it at the running write position
    emitter.instruction("add x10, x10, #1");                                    // advance the digit-copy index
    emitter.instruction(&format!("b {}", loop_label));                          // continue copying digit bytes
    emitter.label(&done_label);
    emitter.instruction("add x11, x11, x2");                                    // advance the write pointer past the digits
}

/// x86_64 implementation of `__rt_serialize_mixed` and `__rt_serialize_value`.
fn emit_serialize_x86_64(emitter: &mut Emitter) {
    use crate::codegen::abi::emit_symbol_address;

    emitter.blank();
    emitter.comment("--- runtime: serialize_mixed (unbox a Mixed cell, then serialize) ---");
    emitter.label_global("__rt_serialize_mixed");
    emitter.instruction("test rax, rax");                                       // a null Mixed pointer serializes as PHP null
    emitter.instruction("jz __rt_serialize_mixed_null");                        // branch to the synthesized-null path
    crate::codegen::abi::emit_load_int_immediate(emitter, "r11", crate::codegen::NULL_SENTINEL);
    emitter.instruction("cmp rax, r11");                                        // is this the in-band null sentinel?
    emitter.instruction("je __rt_serialize_mixed_null");                        // the null sentinel serializes as PHP null
    emitter.instruction("mov rdi, QWORD PTR [rax]");                            // value_tag argument = boxed runtime tag
    emitter.instruction("mov rdx, QWORD PTR [rax + 16]");                       // value_hi argument = boxed high payload word
    emitter.instruction("mov rsi, QWORD PTR [rax + 8]");                        // value_lo argument = boxed low payload word
    emitter.instruction("jmp __rt_serialize_value");                            // tail-call the shared tag-dispatching serializer
    emitter.label("__rt_serialize_mixed_null");
    emitter.instruction("mov rdi, 8");                                          // synthesize a null value tag for the empty box
    emitter.instruction("mov rsi, 0");                                          // null payload low word
    emitter.instruction("mov rdx, 0");                                          // null payload high word
    emitter.instruction("jmp __rt_serialize_value");                            // serialize the synthesized null value

    emitter.blank();
    emitter.comment("--- runtime: serialize_value (tag/lo/hi -> serialize() wire bytes) ---");
    emitter.label_global("__rt_serialize_value");

    // -- set up stack frame --
    // [rbp-8]=output start, [rbp-16]=write pos, [rbp-24]=tag, [rbp-32]=lo, [rbp-40]=hi
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 48");                                         // allocate the serialize scratch frame
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save the value tag across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the low payload word across helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // save the high payload word across helper calls

    // -- compute the current concat_buf write position --
    emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the current concat-buffer offset
    emit_symbol_address(emitter, "r11", "_concat_buf");
    emitter.instruction("add r11, r10");                                        // compute the absolute write pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], r11");                        // save the serialized-slice start pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the running write pointer

    // -- reference counter: every serialized value consumes the next index (see the
    //    AArch64 path). Skip objects (tag 6, counted in the dedup path) and nested
    //    Mixed boxes (tag 7). Array keys flow through here and undo this afterwards. --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the value tag
    emitter.instruction("cmp rdi, 6");                                          // object? counted in the dedup path
    emitter.instruction("je __rt_serialize_value_counted");                     // skip the generic increment
    emitter.instruction("cmp rdi, 7");                                          // nested-Mixed box? inner value is counted
    emitter.instruction("je __rt_serialize_value_counted");                     // skip the generic increment
    emit_symbol_address(emitter, "r9", "_ser_value_counter");
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the running value counter
    emitter.instruction("add rax, 1");                                          // this value takes the next index
    emitter.instruction("mov QWORD PTR [r9], rax");                             // publish the advanced counter
    emitter.label("__rt_serialize_value_counted");

    // -- dispatch on the runtime value tag --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the value tag for dispatch
    emitter.instruction("cmp rdi, 0");                                          // is the value an integer?
    emitter.instruction("je __rt_serialize_int");                               // serialize integers as i:<n>;
    emitter.instruction("cmp rdi, 1");                                          // is the value a string?
    emitter.instruction("je __rt_serialize_str");                               // serialize strings as s:<len>:"...";
    emitter.instruction("cmp rdi, 2");                                          // is the value a float?
    emitter.instruction("je __rt_serialize_float");                             // serialize floats as d:<repr>;
    emitter.instruction("cmp rdi, 3");                                          // is the value a bool?
    emitter.instruction("je __rt_serialize_bool");                              // serialize bools as b:0;/b:1;
    emitter.instruction("cmp rdi, 4");                                          // is the value an indexed array?
    emitter.instruction("je __rt_serialize_arr_indexed");                       // serialize indexed arrays as a:n:{...}
    emitter.instruction("cmp rdi, 5");                                          // is the value an associative array?
    emitter.instruction("je __rt_serialize_arr_hash");                          // serialize hashes as a:n:{...}
    emitter.instruction("cmp rdi, 6");                                          // is the value an object?
    emitter.instruction("je __rt_serialize_arr_object");                        // serialize objects as O:len:"Class":n:{...}
    emitter.instruction("cmp rdi, 7");                                          // is the value a boxed nested Mixed?
    emitter.instruction("je __rt_serialize_nested_mixed");                      // unbox and re-dispatch
    emitter.instruction("cmp rdi, 8");                                          // is the value null?
    emitter.instruction("je __rt_serialize_null");                              // serialize null as N;
    // Tag 10 (callables) is not serializable here and degrades to null.
    emitter.instruction("jmp __rt_serialize_null");                             // unsupported tags serialize as null

    // -- indexed array / hash / nested mixed: delegate, then resume finalize --
    emitter.label("__rt_serialize_arr_indexed");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // array pointer = saved low payload word
    emitter.instruction("call __rt_serialize_indexed_array");                   // append a:n:{...} for the indexed array
    emitter.instruction("jmp __rt_serialize_after_container");                  // recompute the write pointer and finish
    emitter.label("__rt_serialize_arr_hash");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // hash pointer = saved low payload word
    emitter.instruction("call __rt_serialize_hash");                            // append a:n:{...} for the associative array
    emitter.instruction("jmp __rt_serialize_after_container");                  // recompute the write pointer and finish
    emitter.label("__rt_serialize_arr_object");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // object pointer = saved low payload word
    emitter.instruction("call __rt_serialize_obj_ref");                         // dedup: emit r:<idx>; if seen, else register -> rax=1 if emitted
    emitter.instruction("test rax, rax");                                       // was a back-reference emitted?
    emitter.instruction("jnz __rt_serialize_after_container");                  // yes → finalize the value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // object pointer = saved low payload word
    emitter.instruction("call __rt_serialize_object");                          // append O:len:\"Class\":n:{...} for the object
    emitter.instruction("jmp __rt_serialize_after_container");                  // recompute the write pointer and finish
    emitter.label("__rt_serialize_nested_mixed");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // inner Mixed pointer = saved low payload word
    emitter.instruction("call __rt_serialize_mixed");                           // unbox and serialize the nested value
    emitter.label("__rt_serialize_after_container");
    emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // reload the offset advanced by the container serializer
    emit_symbol_address(emitter, "r11", "_concat_buf");
    emitter.instruction("add r11, r10");                                        // recompute the running write pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // persist the write pointer for the finalizer
    emitter.instruction("jmp __rt_serialize_done");                             // finish the serialized value

    // -- null: "N;" --
    emitter.label("__rt_serialize_null");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the running write pointer
    emitter.instruction("mov BYTE PTR [r11], 78");                              // ASCII 'N'
    emitter.instruction("mov BYTE PTR [r11 + 1], 59");                          // ASCII ';'
    emitter.instruction("add r11, 2");                                          // advance past "N;"
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the updated write pointer
    emitter.instruction("jmp __rt_serialize_done");                             // finish the serialized value

    // -- bool: "b:0;" / "b:1;" --
    emitter.label("__rt_serialize_bool");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the running write pointer
    emitter.instruction("mov BYTE PTR [r11], 98");                              // ASCII 'b'
    emitter.instruction("mov BYTE PTR [r11 + 1], 58");                          // ASCII ':'
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the bool payload
    emitter.instruction("mov edx, 48");                                         // ASCII '0' for false
    emitter.instruction("mov ecx, 49");                                         // ASCII '1' for true
    emitter.instruction("test rax, rax");                                       // is the bool payload truthy?
    emitter.instruction("cmovne edx, ecx");                                     // select '1' when non-zero, else '0'
    emitter.instruction("mov BYTE PTR [r11 + 2], dl");                          // write the chosen bool digit
    emitter.instruction("mov BYTE PTR [r11 + 3], 59");                          // ASCII ';'
    emitter.instruction("add r11, 4");                                          // advance past "b:X;"
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the updated write pointer
    emitter.instruction("jmp __rt_serialize_done");                             // finish the serialized value

    // -- int: "i:" + digits + ";" --
    emitter.label("__rt_serialize_int");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the running write pointer
    emitter.instruction("mov BYTE PTR [r11], 105");                             // ASCII 'i'
    emitter.instruction("mov BYTE PTR [r11 + 1], 58");                          // ASCII ':'
    emitter.instruction("add r11, 2");                                          // advance past "i:"
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the write pointer before formatting digits
    emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("mov rcx, r11");                                        // copy the write pointer for the offset computation
    emitter.instruction("sub rcx, r10");                                        // compute the absolute offset for the digit scratch
    emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov QWORD PTR [r10], rcx");                            // point itoa scratch at the current write position
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the integer payload
    emitter.instruction("call __rt_itoa");                                      // format the integer -> rax=digit ptr, rdx=digit len
    emit_serialize_copy_run_x86_64(emitter, "__rt_serialize_int"); // copy the digits to the write pos
    emitter.instruction("mov BYTE PTR [r11], 59");                              // ASCII ';'
    emitter.instruction("add r11, 1");                                          // advance past the semicolon
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the updated write pointer
    emitter.instruction("jmp __rt_serialize_done");                             // finish the serialized value

    // -- string: "s:" + bytelen + ":\"" + raw bytes + "\";" --
    emitter.label("__rt_serialize_str");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the running write pointer
    emitter.instruction("mov BYTE PTR [r11], 115");                             // ASCII 's'
    emitter.instruction("mov BYTE PTR [r11 + 1], 58");                          // ASCII ':'
    emitter.instruction("add r11, 2");                                          // advance past "s:"
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the write pointer before formatting length
    emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("mov rcx, r11");                                        // copy the write pointer for the offset computation
    emitter.instruction("sub rcx, r10");                                        // compute the absolute offset for the length scratch
    emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov QWORD PTR [r10], rcx");                            // point itoa scratch at the current write position
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the byte length from the high payload word
    emitter.instruction("call __rt_itoa");                                      // format the byte length -> rax=digit ptr, rdx=digit len
    emit_serialize_copy_run_x86_64(emitter, "__rt_serialize_strlen"); // copy the length digits
    emitter.instruction("mov BYTE PTR [r11], 58");                              // ASCII ':'
    emitter.instruction("mov BYTE PTR [r11 + 1], 34");                          // ASCII '"'
    emitter.instruction("add r11, 2");                                          // advance past the ":\"" separator
    // -- copy the raw string bytes verbatim (serialize does not escape) --
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the source string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // reload the source string length
    emitter.instruction("xor rcx, rcx");                                        // initialize the byte-copy index
    emitter.label("__rt_serialize_str_copy");
    emitter.instruction("cmp rcx, rdx");                                        // have all source bytes been copied?
    emitter.instruction("jae __rt_serialize_str_copy_done");                    // exit once every byte is copied
    emitter.instruction("mov al, BYTE PTR [rsi + rcx]");                        // load the next source byte
    emitter.instruction("mov BYTE PTR [r11 + rcx], al");                        // store it at the running write position
    emitter.instruction("add rcx, 1");                                          // advance the byte-copy index
    emitter.instruction("jmp __rt_serialize_str_copy");                         // continue copying source bytes
    emitter.label("__rt_serialize_str_copy_done");
    emitter.instruction("add r11, rdx");                                        // advance the write pointer past the raw bytes
    emitter.instruction("mov BYTE PTR [r11], 34");                              // ASCII '"'
    emitter.instruction("mov BYTE PTR [r11 + 1], 59");                          // ASCII ';'
    emitter.instruction("add r11, 2");                                          // advance past the closing "\";"
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the updated write pointer
    emitter.instruction("jmp __rt_serialize_done");                             // finish the serialized value

    // -- float: "d:" + (INF/-INF/NAN | shortest round-trip) + ";" --
    emitter.label("__rt_serialize_float");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the running write pointer
    emitter.instruction("mov BYTE PTR [r11], 100");                             // ASCII 'd'
    emitter.instruction("mov BYTE PTR [r11 + 1], 58");                          // ASCII ':'
    emitter.instruction("add r11, 2");                                          // advance past "d:"
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the write pointer before formatting digits
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the raw float bit pattern
    emitter.instruction("mov r10, r9");                                         // copy the bits for exponent extraction
    emitter.instruction("shr r10, 52");                                         // shift the exponent field into the low bits
    emitter.instruction("and r10, 0x7ff");                                      // isolate the 11-bit exponent
    emitter.instruction("cmp r10, 0x7ff");                                      // is the exponent all ones (Inf/NaN)?
    emitter.instruction("jne __rt_serialize_float_finite");                     // finite floats use the shortest formatter
    emitter.instruction("mov r10, r9");                                         // copy the bits for mantissa testing
    emitter.instruction("shl r10, 12");                                         // drop the sign+exponent to test the mantissa
    emitter.instruction("jnz __rt_serialize_float_nan");                        // non-zero mantissa means NaN
    emitter.instruction("bt r9, 63");                                           // test the float sign bit
    emitter.instruction("jc __rt_serialize_float_neginf");                      // negative sign means -INF
    // +INF: "INF"
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the write pointer for the literal
    emitter.instruction("mov BYTE PTR [r11], 73");                              // ASCII 'I'
    emitter.instruction("mov BYTE PTR [r11 + 1], 78");                          // ASCII 'N'
    emitter.instruction("mov BYTE PTR [r11 + 2], 70");                          // ASCII 'F'
    emitter.instruction("add r11, 3");                                          // advance past "INF"
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the write pointer
    emitter.instruction("jmp __rt_serialize_float_semi");                       // append the terminating semicolon
    emitter.label("__rt_serialize_float_neginf");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the write pointer for the literal
    emitter.instruction("mov BYTE PTR [r11], 45");                              // ASCII '-'
    emitter.instruction("mov BYTE PTR [r11 + 1], 73");                          // ASCII 'I'
    emitter.instruction("mov BYTE PTR [r11 + 2], 78");                          // ASCII 'N'
    emitter.instruction("mov BYTE PTR [r11 + 3], 70");                          // ASCII 'F'
    emitter.instruction("add r11, 4");                                          // advance past "-INF"
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the write pointer
    emitter.instruction("jmp __rt_serialize_float_semi");                       // append the terminating semicolon
    emitter.label("__rt_serialize_float_nan");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the write pointer for the literal
    emitter.instruction("mov BYTE PTR [r11], 78");                              // ASCII 'N'
    emitter.instruction("mov BYTE PTR [r11 + 1], 65");                          // ASCII 'A'
    emitter.instruction("mov BYTE PTR [r11 + 2], 78");                          // ASCII 'N'
    emitter.instruction("add r11, 3");                                          // advance past "NAN"
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the write pointer
    emitter.instruction("jmp __rt_serialize_float_semi");                       // append the terminating semicolon
    emitter.label("__rt_serialize_float_finite");
    emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("mov rcx, r11");                                        // copy the write pointer for the offset computation
    emitter.instruction("sub rcx, r10");                                        // compute the absolute offset for the float digits
    emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov QWORD PTR [r10], rcx");                            // point the float formatter at the write position
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the raw float bit pattern
    emitter.instruction("movq xmm0, r9");                                       // move the bits into the FP argument register
    emitter.instruction("mov edi, 69");                                         // exponent marker 'E' (serialize uppercase layout)
    emitter.instruction("call __rt_json_ftoa");                                 // append the shortest round-trip digits in place
    emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // reload the offset advanced by the formatter
    emit_symbol_address(emitter, "r11", "_concat_buf");
    emitter.instruction("add r11, r10");                                        // recompute the write pointer after the digits
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the write pointer
    emitter.label("__rt_serialize_float_semi");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the write pointer for the semicolon
    emitter.instruction("mov BYTE PTR [r11], 59");                              // ASCII ';'
    emitter.instruction("add r11, 1");                                          // advance past the semicolon
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the updated write pointer
    emitter.instruction("jmp __rt_serialize_done");                             // finish the serialized value

    // -- finalize: update _concat_off and return the slice pointer/length --
    emitter.label("__rt_serialize_done");
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the final write pointer
    emit_symbol_address(emitter, "r10", "_concat_buf");
    emitter.instruction("mov rcx, r11");                                        // copy the end pointer for the offset computation
    emitter.instruction("sub rcx, r10");                                        // compute the absolute end offset
    emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov QWORD PTR [r10], rcx");                            // publish the advanced concat-buffer offset
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // result pointer = serialized-slice start
    emitter.instruction("mov rdx, r11");                                        // copy the end pointer for the length computation
    emitter.instruction("sub rdx, rax");                                        // result length = end pointer - start pointer
    emitter.instruction("add rsp, 48");                                         // deallocate the serialize scratch frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the serialized slice in rax/rdx

    // -- __rt_serialize_uint: append a u64's decimal digits at _concat_off --
    emitter.blank();
    emitter.comment("--- runtime: serialize_uint (append decimal digits, no prefix) ---");
    emitter.label_global("__rt_serialize_uint");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // small frame for the digit helper
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the value across the itoa call
    emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the current write offset
    emit_symbol_address(emitter, "r11", "_concat_buf");
    emitter.instruction("add r11, r10");                                        // compute the write target pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the write target across the itoa call
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the value to format
    emitter.instruction("call __rt_itoa");                                      // format digits into scratch -> rax=ptr, rdx=len
    emitter.instruction("mov rsi, rax");                                        // digit source pointer
    emitter.instruction("mov r8, rdx");                                         // digit count
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the write target pointer
    emitter.instruction("xor rcx, rcx");                                        // initialize the digit-copy index
    emitter.label("__rt_serialize_uint_copy");
    emitter.instruction("cmp rcx, r8");                                         // copied every digit?
    emitter.instruction("jae __rt_serialize_uint_done");                        // exit once all digits are copied
    emitter.instruction("mov al, BYTE PTR [rsi + rcx]");                        // load the next digit byte
    emitter.instruction("mov BYTE PTR [r11 + rcx], al");                        // store it at the write target
    emitter.instruction("add rcx, 1");                                          // advance the digit-copy index
    emitter.instruction("jmp __rt_serialize_uint_copy");                        // continue copying digits
    emitter.label("__rt_serialize_uint_done");
    emitter.instruction("add r11, r8");                                         // advance the write pointer past the digits
    emit_symbol_address(emitter, "r9", "_concat_buf");
    emitter.instruction("mov rcx, r11");                                        // copy the end pointer
    emitter.instruction("sub rcx, r9");                                         // compute the new absolute offset
    emit_symbol_address(emitter, "r9", "_concat_off");
    emitter.instruction("mov QWORD PTR [r9], rcx");                             // publish the advanced offset
    emitter.instruction("add rsp, 32");                                         // deallocate the digit-helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with digits appended

    // -- __rt_serialize_indexed_array: append a:n:{ i:K;<v>... } for a tag-4 array --
    emitter.blank();
    emitter.comment("--- runtime: serialize_indexed_array (PHP a:n:{...} for indexed arrays) ---");
    emitter.label_global("__rt_serialize_indexed_array");
    emit_append_literal_x86_64(emitter, &[b'a', b':'], "the array prefix");
    emitter.instruction("jmp __rt_serialize_indexed_body");                     // emit the shared <count>:{...} body (rax still = array)
    // __rt_serialize_indexed_body: append <count>:{ i:K;<v>... } WITHOUT the
    // leading "a:", so object bodies (O:...:<count>:{...}) can reuse it.
    emitter.label_global("__rt_serialize_indexed_body");
    // [rbp-8]=array, [rbp-16]=len, [rbp-24]=value_type, [rbp-32]=index
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // frame for the indexed-array serializer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the array pointer
    emitter.instruction("mov rcx, QWORD PTR [rax]");                            // load the element count from the header
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // save the element count
    emitter.instruction("mov rcx, QWORD PTR [rax - 8]");                        // load the packed array kind word
    emitter.instruction("shr rcx, 8");                                          // shift the value_type field into the low bits
    emitter.instruction("and rcx, 0x7f");                                       // isolate the 7-bit value_type
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the array value_type
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the element count
    emitter.instruction("call __rt_serialize_uint");                            // append the element count digits
    emit_append_literal_x86_64(emitter, &[b':', b'{'], "the array body open");
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the element index
    emitter.label("__rt_serialize_idx_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the element index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // have all elements been serialized?
    emitter.instruction("jae __rt_serialize_idx_close");                        // close the container when done
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // integer key = element index
    emitter.instruction("mov rdi, 0");                                          // key value tag = int
    emitter.instruction("mov rdx, 0");                                          // key high payload word unused
    emitter.instruction("call __rt_serialize_value");                           // append the i:<index>; key
    emit_uncount_key_x86_64(emitter);                                           // keys do not consume a reference index
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the array value_type
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the array pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the element index
    emitter.instruction("cmp r9, 1");                                           // is the element type a string (16-byte slots)?
    emitter.instruction("je __rt_serialize_idx_str");                           // strings load a ptr/len pair
    emitter.instruction("add rcx, 3");                                          // skip the 24-byte (3-word) array header
    emitter.instruction("mov rsi, QWORD PTR [rsi + rcx*8]");                    // load the 8-byte element payload
    emitter.instruction("cmp r9, 7");                                           // is the element a boxed Mixed payload?
    emitter.instruction("je __rt_serialize_idx_mixed");                         // boxed mixed elements unbox via serialize_mixed
    emitter.instruction("mov rdi, r9");                                         // value tag = array value_type
    emitter.instruction("mov rdx, 0");                                          // value high payload word unused for 8-byte slots
    emitter.instruction("call __rt_serialize_value");                           // serialize the scalar/array element
    emitter.instruction("jmp __rt_serialize_idx_next");                         // advance to the next element
    emitter.label("__rt_serialize_idx_str");
    emitter.instruction("lea rcx, [rcx + rcx]");                                // index * 2 for the ptr/len pair
    emitter.instruction("add rcx, 3");                                          // skip the 24-byte (3-word) array header
    emitter.instruction("mov rdi, QWORD PTR [rsi + rcx*8]");                    // load the string pointer
    emitter.instruction("add rcx, 1");                                          // advance to the length slot
    emitter.instruction("mov rdx, QWORD PTR [rsi + rcx*8]");                    // load the string length
    emitter.instruction("mov rsi, rdi");                                        // value_lo = string pointer
    emitter.instruction("mov rdi, 1");                                          // value tag = string
    emitter.instruction("call __rt_serialize_value");                           // serialize the string element
    emitter.instruction("jmp __rt_serialize_idx_next");                         // advance to the next element
    emitter.label("__rt_serialize_idx_mixed");
    emitter.instruction("mov rax, rsi");                                        // boxed Mixed pointer argument
    emitter.instruction("call __rt_serialize_mixed");                           // unbox and serialize the element
    emitter.label("__rt_serialize_idx_next");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the element index
    emitter.instruction("add rcx, 1");                                          // advance to the next element
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the element index
    emitter.instruction("jmp __rt_serialize_idx_loop");                         // continue the element loop
    emitter.label("__rt_serialize_idx_close");
    emit_append_literal_x86_64(emitter, &[b'}'], "the array body close");
    emitter.instruction("add rsp, 32");                                         // deallocate the indexed-array frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with the indexed array appended

    // -- __rt_serialize_hash: append a:n:{ <key><value>... } for a tag-5 hash --
    emitter.blank();
    emitter.comment("--- runtime: serialize_hash (PHP a:n:{...} for associative arrays) ---");
    emitter.label_global("__rt_serialize_hash");
    emit_append_literal_x86_64(emitter, &[b'a', b':'], "the array prefix");
    emitter.instruction("jmp __rt_serialize_hash_body");                        // emit the shared <count>:{...} body (rax still = hash)
    // __rt_serialize_hash_body: append <count>:{ <key><value>... } WITHOUT the
    // leading "a:", so object bodies (O:...:<count>:{...}) can reuse it.
    emitter.label_global("__rt_serialize_hash_body");
    // [rbp-8]=hash [16]=count [24]=cursor [32]=written [40]=key [48]=keylen [56]=vlo [64]=vhi [72]=vtag
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 80");                                         // frame for the hash serializer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the hash pointer
    emitter.instruction("mov rdi, rax");                                        // hash count argument
    emitter.instruction("call __rt_hash_count");                                // count entries -> rax
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the entry count
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the entry count
    emitter.instruction("call __rt_serialize_uint");                            // append the entry count digits
    emit_append_literal_x86_64(emitter, &[b':', b'{'], "the array body open");
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the iterator cursor
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the written-entry count
    emitter.label("__rt_serialize_hash_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the written-entry count
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // have all entries been serialized?
    emitter.instruction("jae __rt_serialize_hash_close");                       // close the container when done
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the hash pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // next entry -> rax,rdi,rdx,rcx,r8,r9
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the advanced cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save the key pointer / integer key
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the key length (-1 for int keys)
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save the value low payload word
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // save the value high payload word
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // save the value tag
    emitter.instruction("cmp rdx, -1");                                         // is this an integer key (length == -1)?
    emitter.instruction("je __rt_serialize_hash_key_int");                      // integer keys serialize as i:K;
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // string key pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // string key length
    emitter.instruction("mov rdi, 1");                                          // key value tag = string
    emitter.instruction("call __rt_serialize_value");                           // append the s:len:"key"; key
    emitter.instruction("jmp __rt_serialize_hash_key_done");                    // continue to the value
    emitter.label("__rt_serialize_hash_key_int");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // integer key payload
    emitter.instruction("mov rdi, 0");                                          // key value tag = int
    emitter.instruction("mov rdx, 0");                                          // key high payload word unused
    emitter.instruction("call __rt_serialize_value");                           // append the i:K; key
    emitter.label("__rt_serialize_hash_key_done");
    emit_uncount_key_x86_64(emitter);                                           // keys do not consume a reference index
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // reload the value tag
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the value low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 64]");                       // reload the value high payload word
    emitter.instruction("call __rt_serialize_value");                           // append the serialized value
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the written-entry count
    emitter.instruction("add rcx, 1");                                          // count this entry as written
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the written-entry count
    emitter.instruction("jmp __rt_serialize_hash_loop");                        // continue the entry loop
    emitter.label("__rt_serialize_hash_close");
    emit_append_literal_x86_64(emitter, &[b'}'], "the array body close");
    emitter.instruction("add rsp, 80");                                         // deallocate the hash frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with the hash appended

    // -- __rt_concat_append: append rsi raw bytes from rdi into _concat_buf at _concat_off --
    emitter.label_global("__rt_concat_append");
    emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // current write offset
    emit_symbol_address(emitter, "r11", "_concat_buf");
    emitter.instruction("add r11, r10");                                        // write target pointer
    emitter.instruction("xor rcx, rcx");                                        // byte cursor = 0
    emitter.label("__rt_concat_append_loop");
    emitter.instruction("cmp rcx, rsi");                                        // copied the whole run?
    emitter.instruction("jae __rt_concat_append_done");                         // exit when the run is copied
    emitter.instruction("mov al, BYTE PTR [rdi + rcx]");                        // load a source byte
    emitter.instruction("mov BYTE PTR [r11 + rcx], al");                        // store it into the buffer
    emitter.instruction("add rcx, 1");                                          // advance the byte cursor
    emitter.instruction("jmp __rt_concat_append_loop");                         // continue copying
    emitter.label("__rt_concat_append_done");
    emitter.instruction("add r10, rsi");                                        // advance the write offset by the run
    emit_symbol_address(emitter, "r9", "_concat_off");
    emitter.instruction("mov QWORD PTR [r9], r10");                             // persist the new write offset
    emitter.instruction("ret");                                                 // return with the bytes appended

    // -- __rt_serialize_pstr: append a serialized string s:len:"bytes"; (rdi=ptr, rsi=len) --
    emitter.label_global("__rt_serialize_pstr");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the pstr frame
    emitter.instruction("sub rsp, 16");                                         // reserve frame slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the string length
    emit_append_literal_x86_64(emitter, &[b's', b':'], "the string prefix");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the string length
    emitter.instruction("call __rt_serialize_uint");                            // append the length digits
    emit_append_literal_x86_64(emitter, &[b':', b'"'], "the string open quote");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the string length
    emitter.instruction("call __rt_concat_append");                             // copy the raw string bytes
    emit_append_literal_x86_64(emitter, &[b'"', b';'], "the string close");
    emitter.instruction("add rsp, 16");                                         // deallocate the pstr frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with the string appended

    // -- __rt_serialize_object: append O:len:"Class":n:{<key><val>...} (rdi=object ptr) --
    emitter.label_global("__rt_serialize_object");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the object frame
    emitter.instruction("sub rsp, 64");                                         // reserve frame slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the object pointer
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the class id from the object header
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the class id
    emit_symbol_address(emitter, "r10", "_class_name_entries");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // class id
    emitter.instruction("shl rcx, 4");                                          // class_id * 16 (entry stride)
    emitter.instruction("add r10, rcx");                                        // entry = base + class_id*16
    emitter.instruction("mov rdx, QWORD PTR [r10]");                            // class name pointer
    emitter.instruction("mov r8, QWORD PTR [r10 + 8]");                         // class name length
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the class name pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // save the class name length
    emit_append_literal_x86_64(emitter, &[b'O', b':'], "the object prefix");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the class name length
    emitter.instruction("call __rt_serialize_uint");                            // append the class name length digits
    emit_append_literal_x86_64(emitter, &[b':', b'"'], "the class name open quote");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the class name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // reload the class name length
    emitter.instruction("call __rt_concat_append");                             // copy the class name bytes
    emit_append_literal_x86_64(emitter, &[b'"', b':'], "the class name close");
    // -- __serialize magic: object body = returned array's <count>:{key;val;...} --
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the class id
    emit_symbol_address(emitter, "r10", "_class_serialize_ptrs");
    emitter.instruction("mov r10, QWORD PTR [r10 + rax*8]");                    // __serialize method symbol (0 if none)
    emitter.instruction("test r10, r10");                                       // does the class define __serialize?
    emitter.instruction("jz __rt_serialize_object_sleep");                      // no → try __sleep, else property walk
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // park the __serialize target across the call
    emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // capture the write offset after the O:...:\"name\": prefix
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save it so method scratch can be rewound away
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // $this receiver for the method call
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the __serialize target
    emitter.instruction("call r10");                                            // __serialize($this) -> rax = array (bare pointer)
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the returned array pointer
    emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the saved post-prefix offset
    emitter.instruction("mov QWORD PTR [r10], rax");                            // rewind, discarding any concat scratch the method left
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the returned array pointer
    emitter.instruction("mov rcx, QWORD PTR [rax - 8]");                        // load its heap kind word
    emitter.instruction("and rcx, 0xff");                                       // isolate the heap kind (2=indexed, 3=hash)
    emitter.instruction("cmp rcx, 3");                                          // is the returned array a hash?
    emitter.instruction("je __rt_serialize_object_ser_hash");                   // hashes use the hash body emitter
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the indexed array pointer
    emitter.instruction("call __rt_serialize_indexed_body");                    // append <count>:{ i:K;<v>... }
    emitter.instruction("jmp __rt_serialize_object_magic_done");                // finish the object
    emitter.label("__rt_serialize_object_ser_hash");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the hash pointer
    emitter.instruction("call __rt_serialize_hash_body");                       // append <count>:{ <key><val>... }
    emitter.label("__rt_serialize_object_magic_done");
    emitter.instruction("add rsp, 64");                                         // deallocate the object frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with the object appended
    // -- __sleep magic: serialize only the named properties using mangled keys --
    emitter.label("__rt_serialize_object_sleep");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the class id
    emit_symbol_address(emitter, "r10", "_class_sleep_ptrs");
    emitter.instruction("mov r10, QWORD PTR [r10 + rax*8]");                    // __sleep method symbol (0 if none)
    emitter.instruction("test r10, r10");                                       // does the class define __sleep?
    emitter.instruction("jz __rt_serialize_object_default");                    // no → walk every property
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // park the __sleep target across the call
    emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // capture the post-prefix write offset
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save it for the scratch rewind
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // $this receiver
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the __sleep target
    emitter.instruction("call r10");                                            // __sleep($this) -> rax = names array (indexed)
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the names array pointer
    emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // reload the saved post-prefix offset
    emitter.instruction("mov QWORD PTR [r10], rax");                            // rewind away any method scratch
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the names array pointer
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // names count = indexed-array header word
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the names count
    emitter.instruction("call __rt_serialize_uint");                            // append the property count digits
    emit_append_literal_x86_64(emitter, &[b':', b'{'], "the object body open");
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // name index = 0
    emitter.label("__rt_serialize_object_sleep_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the name index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // emitted every named property?
    emitter.instruction("jae __rt_serialize_object_sleep_done");                // close the body when done
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the names array pointer
    emitter.instruction("lea rcx, [rcx + rcx]");                                // index * 2 for the string ptr/len pair
    emitter.instruction("add rcx, 3");                                          // skip the 24-byte (3-word) header
    emitter.instruction("mov r8, QWORD PTR [rsi + rcx*8]");                     // name string pointer
    emitter.instruction("add rcx, 1");                                          // advance to the length slot
    emitter.instruction("mov r9, QWORD PTR [rsi + rcx*8]");                     // name string length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // object pointer
    emitter.instruction("mov rsi, r8");                                         // name string pointer argument
    emitter.instruction("mov rdx, r9");                                         // name string length argument
    emitter.instruction("call __rt_serialize_named_prop");                      // emit mangled-key + value for the named property
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the name index
    emitter.instruction("add rcx, 1");                                          // advance to the next name
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // persist the name index
    emitter.instruction("jmp __rt_serialize_object_sleep_loop");                // continue the name loop
    emitter.label("__rt_serialize_object_sleep_done");
    emit_append_literal_x86_64(emitter, &[b'}'], "the object body close");
    emitter.instruction("add rsp, 64");                                         // deallocate the object frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with the object appended

    emitter.label("__rt_serialize_object_default");
    emit_symbol_address(emitter, "r10", "_class_serprop_ptrs");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // class id
    emitter.instruction("shl rcx, 3");                                          // class_id * 8 (pointer stride)
    emitter.instruction("add r10, rcx");                                        // slot = base + class_id*8
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // property-info table pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the property-info pointer
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // load the property count
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the property count
    emitter.instruction("call __rt_serialize_uint");                            // append the property count digits
    emit_append_literal_x86_64(emitter, &[b':', b'{'], "the object body open");
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // property cursor = 0
    emitter.label("__rt_serialize_object_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the property cursor
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // processed every property?
    emitter.instruction("jae __rt_serialize_object_close");                     // close the body when done
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the property-info pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the property cursor
    emitter.instruction("shl rax, 5");                                          // cursor * 32 (row stride)
    emitter.instruction("add r11, rax");                                        // advance to the row
    emitter.instruction("add r11, 8");                                          // skip the count word to the row fields
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // mangled key pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                        // mangled key length
    emitter.instruction("call __rt_serialize_pstr");                            // append the mangled property key
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the property-info pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the property cursor
    emitter.instruction("shl rax, 5");                                          // cursor * 32 (row stride)
    emitter.instruction("add r11, rax");                                        // advance to the row
    emitter.instruction("add r11, 8");                                          // skip the count word to the row fields
    emitter.instruction("mov r8, QWORD PTR [r11 + 16]");                        // property byte offset
    emitter.instruction("mov r9, QWORD PTR [r11 + 24]");                        // property value tag
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("add rcx, r8");                                         // address of the property slot
    emitter.instruction("mov rsi, QWORD PTR [rcx]");                            // value low payload word
    emitter.instruction("mov rdx, QWORD PTR [rcx + 8]");                        // value high payload word
    emitter.instruction("mov rdi, r9");                                         // value tag
    emitter.instruction("call __rt_serialize_value");                           // append the serialized property value
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the property cursor
    emitter.instruction("add rcx, 1");                                          // advance to the next property
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // persist the cursor
    emitter.instruction("jmp __rt_serialize_object_loop");                      // continue the property loop
    emitter.label("__rt_serialize_object_close");
    emit_append_literal_x86_64(emitter, &[b'}'], "the object body close");
    emitter.instruction("add rsp, 64");                                         // deallocate the object frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with the object appended

    // -- __rt_serialize_named_prop(rdi=obj, rsi=name_ptr, rdx=name_len): used by
    //    the __sleep path. Finds the property whose unmangled (short) name matches
    //    and appends its PHP-mangled key followed by its serialized value. The
    //    short name is the mangled key's suffix after its last NUL. --
    emitter.blank();
    emitter.comment("--- runtime: serialize_named_prop (emit one __sleep-named property) ---");
    emitter.label_global("__rt_serialize_named_prop");
    // [rbp-8]=obj [16]=name_ptr [24]=name_len [32]=serprop [40]=count [48]=row index
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the named-property frame
    emitter.instruction("sub rsp, 64");                                         // reserve frame slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the object pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the target name pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the target name length
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // class id from the object header
    emit_symbol_address(emitter, "r10", "_class_serprop_ptrs");
    emitter.instruction("mov r10, QWORD PTR [r10 + rax*8]");                    // property-info table for this class
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the property-info pointer
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // property count
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the property count
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // row index = 0
    emitter.label("__rt_serialize_named_prop_loop");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the row index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 40]");                       // scanned every row?
    emitter.instruction("jae __rt_serialize_named_prop_done");                  // unknown name → emit nothing
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the property-info pointer
    emitter.instruction("add r10, 8");                                          // skip the count word to the rows
    emitter.instruction("mov rax, rcx");                                        // row index
    emitter.instruction("shl rax, 5");                                          // index * 32 (row stride)
    emitter.instruction("add r10, rax");                                        // r10 = row pointer
    emitter.instruction("mov r8, QWORD PTR [r10]");                             // mangled key pointer
    emitter.instruction("mov r9, QWORD PTR [r10 + 8]");                         // mangled key length
    // -- compute the short-name offset: index just past the last NUL byte --
    emitter.instruction("xor rcx, rcx");                                        // scan cursor
    emitter.instruction("xor r11, r11");                                        // short-name offset (no NUL → whole key)
    emitter.label("__rt_serialize_named_prop_scan");
    emitter.instruction("cmp rcx, r9");                                         // scanned the whole key?
    emitter.instruction("jae __rt_serialize_named_prop_scan_done");             // stop at the key end
    emitter.instruction("mov al, BYTE PTR [r8 + rcx]");                         // load the next key byte
    emitter.instruction("test al, al");                                         // is it a NUL?
    emitter.instruction("jne __rt_serialize_named_prop_scan_next");             // non-NUL bytes do not move the short-name start
    emitter.instruction("lea r11, [rcx + 1]");                                  // short name starts just after this NUL (last wins)
    emitter.label("__rt_serialize_named_prop_scan_next");
    emitter.instruction("add rcx, 1");                                          // advance the scan cursor
    emitter.instruction("jmp __rt_serialize_named_prop_scan");                  // continue scanning
    emitter.label("__rt_serialize_named_prop_scan_done");
    emitter.instruction("mov rsi, r8");                                         // short name pointer = key + offset
    emitter.instruction("add rsi, r11");                                        // apply the short-name offset
    emitter.instruction("mov rdx, r9");                                         // short name length = key len - offset
    emitter.instruction("sub rdx, r11");                                        // apply the short-name offset
    emitter.instruction("cmp rdx, QWORD PTR [rbp - 24]");                       // same length as the target name?
    emitter.instruction("jne __rt_serialize_named_prop_next");                  // lengths differ → not this row
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // target name pointer
    emitter.instruction("xor rcx, rcx");                                        // byte-compare cursor
    emitter.label("__rt_serialize_named_prop_cmp");
    emitter.instruction("cmp rcx, rdx");                                        // compared all bytes?
    emitter.instruction("jae __rt_serialize_named_prop_match");                 // full match
    emitter.instruction("mov al, BYTE PTR [rsi + rcx]");                        // short-name byte
    emitter.instruction("mov r10b, BYTE PTR [rdi + rcx]");                      // target-name byte
    emitter.instruction("cmp al, r10b");                                        // bytes equal?
    emitter.instruction("jne __rt_serialize_named_prop_next");                  // mismatch → not this row
    emitter.instruction("add rcx, 1");                                          // next byte
    emitter.instruction("jmp __rt_serialize_named_prop_cmp");                   // continue comparing
    emitter.label("__rt_serialize_named_prop_match");
    emitter.instruction("mov rdi, r8");                                         // mangled key pointer
    emitter.instruction("mov rsi, r9");                                         // mangled key length
    emitter.instruction("call __rt_serialize_pstr");                            // append the s:len:"\0*\0name"; key
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the property-info pointer
    emitter.instruction("add r10, 8");                                          // skip the count word to the rows
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // reload the row index
    emitter.instruction("shl rax, 5");                                          // index * 32 (row stride)
    emitter.instruction("add r10, rax");                                        // r10 = row pointer
    emitter.instruction("mov r8, QWORD PTR [r10 + 16]");                        // property byte offset
    emitter.instruction("mov r9, QWORD PTR [r10 + 24]");                        // property value tag
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // reload the object pointer
    emitter.instruction("add rcx, r8");                                         // address of the property slot
    emitter.instruction("mov rsi, QWORD PTR [rcx]");                            // value low payload word
    emitter.instruction("mov rdx, QWORD PTR [rcx + 8]");                        // value high payload word
    emitter.instruction("mov rdi, r9");                                         // value tag
    emitter.instruction("call __rt_serialize_value");                           // append the serialized property value
    emitter.instruction("jmp __rt_serialize_named_prop_done");                  // stop after the matching property
    emitter.label("__rt_serialize_named_prop_next");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the row index
    emitter.instruction("add rcx, 1");                                          // advance to the next row
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the row index
    emitter.instruction("jmp __rt_serialize_named_prop_loop");                  // continue scanning
    emitter.label("__rt_serialize_named_prop_done");
    emitter.instruction("add rsp, 64");                                         // deallocate the named-property frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with the named property appended

    // -- __rt_serialize_begin: reset the reference state for a new top-level
    //    serialize() call (value counter and the seen-objects map) --
    emitter.blank();
    emitter.comment("--- runtime: serialize_begin (reset reference tracking) ---");
    emitter.label_global("__rt_serialize_begin");
    emit_symbol_address(emitter, "r9", "_ser_value_counter");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // value counter = 0
    emit_symbol_address(emitter, "r9", "_ser_obj_count");
    emitter.instruction("mov QWORD PTR [r9], 0");                               // seen-objects count = 0
    emitter.instruction("ret");                                                 // reference tracking is reset

    // -- __rt_serialize_obj_ref(rdi=obj): dedup objects by identity. If the object
    //    pointer was already serialized, append r:<index>; and return 1 so the
    //    caller skips re-serializing it. Otherwise assign the next reference index,
    //    register it (until the map is full), and return 0. --
    emitter.blank();
    emitter.comment("--- runtime: serialize_obj_ref (object identity back-reference) ---");
    emitter.label_global("__rt_serialize_obj_ref");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the dedup frame
    emitter.instruction("sub rsp, 32");                                         // reserve frame slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the object pointer
    emit_symbol_address(emitter, "r9", "_ser_obj_count");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // number of registered objects
    emitter.instruction("xor r10, r10");                                        // scan index = 0
    emitter.label("__rt_serialize_obj_ref_scan");
    emitter.instruction("cmp r10, r9");                                         // scanned every registered object?
    emitter.instruction("jge __rt_serialize_obj_ref_new");                      // not seen → register a new index
    emit_symbol_address(emitter, "r11", "_ser_obj_ptrs");
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // registered object pointer
    emitter.instruction("cmp rax, QWORD PTR [rbp - 8]");                        // same identity?
    emitter.instruction("je __rt_serialize_obj_ref_found");                     // already serialized → emit a back-reference
    emitter.instruction("add r10, 1");                                          // next registered object
    emitter.instruction("jmp __rt_serialize_obj_ref_scan");                     // continue scanning
    emitter.label("__rt_serialize_obj_ref_found");
    emit_symbol_address(emitter, "r11", "_ser_obj_idxs");
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // the recorded reference index
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save it across the literal appends
    emit_append_literal_x86_64(emitter, &[b'r', b':'], "the back-reference prefix");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the reference index
    emitter.instruction("call __rt_serialize_uint");                            // append the index digits (value in rax)
    emit_append_literal_x86_64(emitter, &[b';'], "the back-reference terminator");
    emitter.instruction("mov rax, 1");                                          // signal: r:<index>; emitted
    emitter.instruction("jmp __rt_serialize_obj_ref_ret");                      // return to the caller
    emitter.label("__rt_serialize_obj_ref_new");
    emit_symbol_address(emitter, "r9", "_ser_value_counter");
    emitter.instruction("mov rax, QWORD PTR [r9]");                             // load the running value counter
    emitter.instruction("add rax, 1");                                          // this object takes the next index
    emitter.instruction("mov QWORD PTR [r9], rax");                             // publish the advanced counter
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the assigned index
    emit_symbol_address(emitter, "r9", "_ser_obj_count");
    emitter.instruction("mov rcx, QWORD PTR [r9]");                             // current registered-object count
    emitter.instruction("cmp rcx, 65536");                                      // is the object map full?
    emitter.instruction("jge __rt_serialize_obj_ref_done");                     // full → keep counting but stop deduping
    emit_symbol_address(emitter, "r11", "_ser_obj_ptrs");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // object pointer
    emitter.instruction("mov QWORD PTR [r11 + rcx*8], rax");                    // record the object pointer
    emit_symbol_address(emitter, "r11", "_ser_obj_idxs");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // assigned index
    emitter.instruction("mov QWORD PTR [r11 + rcx*8], rax");                    // record its reference index
    emit_symbol_address(emitter, "r9", "_ser_obj_count");
    emitter.instruction("add rcx, 1");                                          // grow the map
    emitter.instruction("mov QWORD PTR [r9], rcx");                             // publish the new count
    emitter.label("__rt_serialize_obj_ref_done");
    emitter.instruction("mov rax, 0");                                          // signal: registered, serialize the object
    emitter.label("__rt_serialize_obj_ref_ret");
    emitter.instruction("add rsp, 32");                                         // deallocate the dedup frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the back-reference flag in rax
}

/// Emits the x86_64 sequence that decrements `_ser_value_counter` by one, undoing
/// the generic per-value increment for an array key (keys do not consume a PHP
/// reference index). Clobbers r10/rax.
fn emit_uncount_key_x86_64(emitter: &mut Emitter) {
    use crate::codegen::abi::emit_symbol_address;
    emit_symbol_address(emitter, "r10", "_ser_value_counter");
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the running value counter
    emitter.instruction("sub rax, 1");                                          // a key does not consume an index
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the corrected counter
}

/// Emits an x86_64 sequence that appends fixed literal bytes at the current
/// `_concat_off`, advancing the offset (clobbers r9-r11).
fn emit_append_literal_x86_64(emitter: &mut Emitter, bytes: &[u8], what: &str) {
    use crate::codegen::abi::emit_symbol_address;
    emit_symbol_address(emitter, "r10", "_concat_off");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the current write offset
    emit_symbol_address(emitter, "r11", "_concat_buf");
    emitter.instruction("add r11, r10");                                        // compute the write target pointer
    for (i, b) in bytes.iter().enumerate() {
        emitter.instruction(&format!("mov BYTE PTR [r11 + {}], {}", i, b));     // literal byte for {what}
    }
    emitter.instruction(&format!("add r10, {}", bytes.len()));                  // advance the offset
    emit_symbol_address(emitter, "r9", "_concat_off");
    emitter.instruction("mov QWORD PTR [r9], r10");                             // publish the advanced offset
    let _ = what;
}

/// Emits the x86_64 digit-copy run shared by the integer and string-length paths.
///
/// On entry `rax`/`rdx` describe an itoa-produced digit slice and `[rbp-16]` holds
/// the running write pointer; on exit the digits are copied to the write position
/// and `r11` points just past them (the caller persists `r11` to `[rbp-16]`).
fn emit_serialize_copy_run_x86_64(emitter: &mut Emitter, prefix: &str) {
    let loop_label = format!("{}_digit_copy", prefix);
    let done_label = format!("{}_digit_done", prefix);
    emitter.instruction("mov rsi, rax");                                        // remember the digit source pointer
    emitter.instruction("mov r8, rdx");                                         // remember the digit count for the copy bound
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the running write pointer
    emitter.instruction("xor rcx, rcx");                                        // initialize the digit-copy index
    emitter.label(&loop_label);
    emitter.instruction("cmp rcx, r8");                                         // have all digits been copied?
    emitter.instruction(&format!("jae {}", done_label));                        // exit once every digit is copied
    emitter.instruction("mov al, BYTE PTR [rsi + rcx]");                        // load the next digit byte
    emitter.instruction("mov BYTE PTR [r11 + rcx], al");                        // store it at the running write position
    emitter.instruction("add rcx, 1");                                          // advance the digit-copy index
    emitter.instruction(&format!("jmp {}", loop_label));                        // continue copying digit bytes
    emitter.label(&done_label);
    emitter.instruction("add r11, r8");                                         // advance the write pointer past the digits
}
