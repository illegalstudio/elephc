//! Purpose:
//! Emits the `__rt_json_encode_array_dynamic`, `__rt_json_arr_dyn_loop` runtime helper assembly for json encode array dynamic.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - JSON encoders are emitted formatter state machines; escaping, type tags, and buffer growth are observable PHP behavior.
//! - Output goes through a `__rt_strbuf_*` descriptor in the frame (`[sp+48]` / `[rbp-96]`) that
//!   starts in the scratch tail and grows onto the heap. Each element reserves room first and
//!   appends its child slice through `__rt_json_buf_append`, which also releases a grown child.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::abi;
use crate::codegen_support::sentinels::POINTER_ARRAY_VALUE_TYPE;

/// Emits `__rt_json_encode_array_dynamic`: encodes a PHP indexed array to a JSON array literal.
///
/// Pipeline: recursion-depth check → snapshot JSON_FORCE_OBJECT → emit opening `[` or `{`
/// → iterate elements emitting commas and pretty-printing → dispatch each element through the
/// value_type tag to the appropriate JSON helper → recurse into nested arrays/objects → emit closing
/// delimiter → publish the scratch offset and return the encoded slice.
///
/// Input:  x0 = array pointer (PhpArray)
/// Output: x1 = result ptr, x2 = result len
pub(crate) fn emit_json_encode_array_dynamic(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_array_dynamic_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_encode_array_dynamic ---");
    emitter.label_global("__rt_json_encode_array_dynamic");

    // -- hand a run-time promoted array to the object-form encoder --
    // A statically `Array(...)` value can hold HASH storage at run time:
    // `__rt_array_set_mixed_key` promotes the destination when a key does not fit the packed
    // layout, and the static type does not move with it. Encoding that as a JSON array reads
    // hash internals. The probe is the uniform heap-kind byte, the same one
    // `__rt_array_edge_key` dispatches on; `__rt_json_encode_assoc` takes the container in x0
    // and returns the slice in x1/x2 exactly as this entry does, so it is a tail jump — and it
    // is the encoder that renders `[5 => 3]` the way PHP does, `{"5":3}` rather than `[3]`.
    // The conditional branch skips OVER an unconditional one because AArch64 cannot name an
    // external label in a conditional branch.
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the uniform heap-kind header word
    emitter.instruction("and x9, x9, #0xff");                                   // isolate the low-byte heap kind
    emitter.instruction("cmp x9, #3");                                          // kind 3 = associative hash storage
    emitter.instruction("b.ne __rt_json_arr_dyn_packed");                       // packed storage encodes inline below
    emitter.instruction("b __rt_json_encode_assoc");                            // hash storage is the object-form encoder's job
    emitter.label("__rt_json_arr_dyn_packed");
    super::json_integer_tree::emit_json_integer_tree_entry(
        emitter,
        "__rt_json_arr_dyn_integer_tree",
    );

    emitter.instruction("sub sp, sp, #112");                                    // allocate stack space for array metadata and element scratch values
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the helper stack frame
    emitter.instruction("str x19, [sp, #88]");                                  // preserve caller x19 before caching active JSON flags
    emitter.instruction("str x0, [sp, #0]");                                    // save the source array pointer

    // -- enter the recursion-depth check so JSON_ERROR_DEPTH fires when
    //    the user-supplied $depth limit is exceeded --
    emitter.instruction("bl __rt_json_depth_enter");                            // increment _json_active_depth and throw on overflow when requested

    // -- snapshot JSON_FORCE_OBJECT once: when set, the array encodes as a
    //    JSON object whose keys are the integer indexes "0", "1", ... --
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x19, [x9]");                                       // cache the active flag bitmask for the whole indexed-array encode

    // -- open the output buffer (descriptor at [sp+48]) at the scratch tail --
    emitter.instruction("add x0, sp, #48");                                     // address of the output-buffer descriptor
    emitter.instruction("bl __rt_strbuf_init");                                 // open the buffer at the current scratch offset
    emitter.instruction("add x0, sp, #48");                                     // address of the output-buffer descriptor
    emitter.instruction("mov x1, #1");                                          // room for the opening byte
    emitter.instruction("bl __rt_json_buf_ensure");                             // x11 = cursor with headroom reserved

    // -- write opening bracket or brace, depending on JSON_FORCE_OBJECT --
    emitter.instruction("and x12, x19, #16");                                   // isolate JSON_FORCE_OBJECT from the cached flag bitmask
    emitter.instruction("mov w13, #91");                                        // ASCII '[' (default for indexed arrays)
    emitter.instruction("cbz x12, __rt_json_arr_dyn_open_emit");                // skip the brace override when the flag is clear
    emitter.instruction("mov w13, #123");                                       // ASCII '{' (force-object form opens with a brace)
    emitter.label("__rt_json_arr_dyn_open_emit");
    emitter.instruction("strb w13, [x11]");                                     // emit the chosen opening byte
    emitter.instruction("add x11, x11, #1");                                    // advance past the opening byte
    emitter.instruction("str x11, [sp, #56]");                                  // persist the updated write pointer
    emitter.instruction("bl __rt_json_pretty_push");                            // enter one pretty-print indentation level after the container opens

    // -- cache array length and packed value_type tag --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // load the current array length
    emitter.instruction("str x3, [sp, #24]");                                   // save the array length for the element loop
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the packed array kind word
    emitter.instruction("lsr x9, x9, #8");                                      // move the packed value_type tag into the low bits
    emitter.instruction("and x9, x9, #0x7f");                                   // isolate the packed value_type tag
    emitter.instruction("str x9, [sp, #32]");                                   // save the array element value_type tag
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize the loop index to zero

    emitter.label("__rt_json_arr_dyn_loop");
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload the array length
    emitter.instruction("cmp x4, x3");                                          // have we encoded every element?
    emitter.instruction("b.ge __rt_json_arr_dyn_close");                        // finish once the loop index reaches the array length

    // -- reserve room for the separator, the indentation, and an inline key prefix --
    emitter.instruction("add x0, sp, #48");                                     // address of the output-buffer descriptor
    emitter.instruction("mov x1, #1");                                          // room for the comma separator
    emitter.instruction("bl __rt_json_buf_ensure");                             // grow first: every byte of this element prefix fits after it

    // -- emit comma separators between elements --
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index after the reservation
    emitter.instruction("cbz x4, __rt_json_arr_dyn_elem");                      // skip the comma before the first element
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload the current write pointer
    emitter.instruction("mov w12, #44");                                        // ASCII ','
    emitter.instruction("strb w12, [x11]");                                     // write the comma separator
    emitter.instruction("add x11, x11, #1");                                    // advance past the comma
    emitter.instruction("str x11, [sp, #56]");                                  // persist the updated write pointer

    emitter.label("__rt_json_arr_dyn_elem");
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload write pos before optional pretty indentation
    emitter.instruction("bl __rt_json_pretty_line");                            // append newline and indentation for this element/key when pretty-printing
    emitter.instruction("str x11, [sp, #56]");                                  // save the write pos after any pretty indentation
    // -- when JSON_FORCE_OBJECT is set, prefix every element with `"<idx>":` --
    emitter.instruction("and x12, x19, #16");                                   // isolate JSON_FORCE_OBJECT from the cached flag bitmask
    emitter.instruction("cbz x12, __rt_json_arr_dyn_elem_no_key");              // skip the key prefix when the flag is clear
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload the current concat-buffer write pointer
    emitter.instruction("mov w13, #34");                                        // ASCII '"'
    emitter.instruction("strb w13, [x11]");                                     // emit the opening quote of the synthetic key
    emitter.instruction("add x11, x11, #1");                                    // advance past the opening quote
    emitter.instruction("str x11, [sp, #56]");                                  // persist the updated write pointer
    // Sync concat_off so __rt_itoa formats the index digits just past the quote.
    emitter.instruction("add x0, sp, #48");                                     // address of the output-buffer descriptor
    emitter.instruction("bl __rt_strbuf_sync");                                 // point itoa's scratch past the cursor (or at the heap buffer's opening offset)
    emitter.instruction("ldr x0, [sp, #40]");                                   // load the loop index as the integer key value
    emitter.instruction("bl __rt_itoa");                                        // format the index as decimal digits at concat_off
    // __rt_itoa returns x1=ptr, x2=len of the formatted slice — append it at the
    // running write position so the slice is contiguous with the prefix.
    emitter.instruction("add x0, sp, #48");                                     // address of the output-buffer descriptor
    emitter.instruction("bl __rt_json_buf_append");                             // copy the digits to the cursor; x11 = advanced cursor
    emitter.instruction("mov w13, #34");                                        // ASCII '"'
    emitter.instruction("strb w13, [x11]");                                     // emit the closing quote of the synthetic key
    emitter.instruction("mov w13, #58");                                        // ASCII ':'
    emitter.instruction("strb w13, [x11, #1]");                                 // emit the colon between key and value
    emitter.instruction("add x11, x11, #2");                                    // advance the running write pointer past `":`
    emitter.instruction("bl __rt_json_pretty_colon_space");                     // append the pretty-print key/value space when requested
    emitter.instruction("str x11, [sp, #56]");                                  // persist the updated write pointer
    emitter.label("__rt_json_arr_dyn_elem_no_key");

    // -- update concat_off so nested encoders append from the current write position --
    emitter.instruction("add x0, sp, #48");                                     // address of the output-buffer descriptor
    emitter.instruction("bl __rt_strbuf_sync");                                 // nested encoders must not format over the existing JSON prefix

    // -- dispatch on the array value_type tag --
    emitter.instruction("ldr x12, [sp, #32]");                                  // reload the packed array value_type tag
    emitter.instruction("cmp x12, #0");                                         // does this array store ints?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_int");                    // ints encode through itoa
    emitter.instruction("cmp x12, #1");                                         // does this array store strings?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_str");                    // strings encode through the JSON string helper
    emitter.instruction("cmp x12, #2");                                         // does this array store floats?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_float");                  // floats encode through ftoa
    emitter.instruction("cmp x12, #3");                                         // does this array store bools?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_bool");                   // bools encode through the JSON bool helper
    emitter.instruction("cmp x12, #4");                                         // does this array store nested indexed arrays?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_array");                  // nested arrays encode recursively
    emitter.instruction("cmp x12, #5");                                         // does this array store nested associative arrays?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_assoc");                  // nested hashes encode through the assoc helper
    emitter.instruction("cmp x12, #6");                                         // does this array store object instances?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_object");                 // objects encode through the object descriptor walker
    emitter.instruction("cmp x12, #7");                                         // does this array store boxed mixed payloads?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_mixed");                  // boxed mixed payloads encode through the mixed helper
    emitter.instruction(&format!("cmp x12, #{}", POINTER_ARRAY_VALUE_TYPE));    // does this array store raw ?ptr<T> words?
    emitter.instruction("b.eq __rt_json_arr_dyn_value_pointer");                // pointer words encode as null or an integer address
    emitter.instruction("b __rt_json_arr_dyn_value_null");                      // null and unsupported payloads encode as JSON null

    emitter.label("__rt_json_arr_dyn_value_pointer");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                            // load the raw pointer word
    emitter.instruction("cbz x0, __rt_json_arr_dyn_value_null");                // a null pointer word encodes as JSON null
    emitter.instruction("bl __rt_itoa");                                        // encode a live address as decimal digits
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_int");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                            // load the integer element payload
    emitter.instruction("bl __rt_itoa");                                        // encode the integer element as decimal digits
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_str");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x5, x4, x4");                                      // compute index * 2 for the ptr/len pair
    emitter.instruction("add x5, x5, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x1, [x0, x5, lsl #3]");                            // load the string pointer
    emitter.instruction("add x5, x5, #1");                                      // advance to the string length slot
    emitter.instruction("ldr x2, [x0, x5, lsl #3]");                            // load the string length
    emitter.instruction("bl __rt_json_encode_str");                             // encode the string element with JSON escaping
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_float");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x9, [x0, x4, lsl #3]");                            // load the float bits from the 8-byte array slot
    emitter.instruction("fmov d0, x9");                                         // move the float bits into the FP register file
    emitter.instruction("bl __rt_json_encode_float");                           // encode the float element, rejecting Inf/NaN per JSON semantics
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_bool");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                            // load the bool payload from the 8-byte array slot
    emitter.instruction("bl __rt_json_encode_bool");                            // encode the bool element as true/false
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_array");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                            // load the nested array pointer from the 8-byte array slot
    emitter.instruction("bl __rt_json_encode_array_dynamic");                   // encode the nested indexed array recursively
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded nested array into concat_buf

    emitter.label("__rt_json_arr_dyn_value_assoc");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                            // load the nested associative-array pointer from the 8-byte array slot
    emitter.instruction("bl __rt_json_encode_assoc");                           // encode the nested associative array recursively
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded nested hash into concat_buf

    emitter.label("__rt_json_arr_dyn_value_object");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                            // load the object pointer from the 8-byte array slot
    emitter.instruction("bl __rt_json_encode_object");                          // encode the object via the per-class JSON descriptor walker
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded object into concat_buf

    emitter.label("__rt_json_arr_dyn_value_mixed");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source array pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #3");                                      // skip the 24-byte array header
    emitter.instruction("ldr x0, [x0, x4, lsl #3]");                            // load the boxed mixed pointer from the 8-byte array slot
    emitter.instruction("bl __rt_json_encode_mixed");                           // encode the boxed mixed payload recursively
    emitter.instruction("b __rt_json_arr_dyn_copy");                            // copy the encoded mixed payload into concat_buf

    emitter.label("__rt_json_arr_dyn_value_null");
    emitter.instruction("bl __rt_json_encode_null");                            // encode null/unsupported payloads as JSON null

    emitter.label("__rt_json_arr_dyn_copy");
    emitter.instruction("add x0, sp, #48");                                     // address of the output-buffer descriptor
    emitter.instruction("bl __rt_json_buf_append");                             // copy the encoded element to the cursor, growing first, and release a transient child buffer

    emitter.label("__rt_json_arr_dyn_next");
    emitter.instruction("ldr x4, [sp, #40]");                                   // reload the loop index
    emitter.instruction("add x4, x4, #1");                                      // advance to the next array element
    emitter.instruction("str x4, [sp, #40]");                                   // persist the updated loop index
    emitter.instruction("b __rt_json_arr_dyn_loop");                            // continue encoding the remaining elements

    emitter.label("__rt_json_arr_dyn_close");
    emitter.instruction("add x0, sp, #48");                                     // address of the output-buffer descriptor
    emitter.instruction("mov x1, #1");                                          // room for the closing byte
    emitter.instruction("bl __rt_json_buf_ensure");                             // x11 = cursor with headroom for the closing line
    emitter.instruction("bl __rt_json_pretty_pop");                             // leave the container indentation level before closing it
    emitter.instruction("ldr x3, [sp, #24]");                                   // reload array length to decide whether closing needs its own line
    emitter.instruction("cbz x3, __rt_json_arr_dyn_close_choose");              // empty containers stay compact even under JSON_PRETTY_PRINT
    emitter.instruction("bl __rt_json_pretty_line");                            // append the closing-line indentation for non-empty pretty containers
    emitter.label("__rt_json_arr_dyn_close_choose");
    emitter.instruction("and x12, x19, #16");                                   // isolate JSON_FORCE_OBJECT from the cached flag bitmask
    emitter.instruction("mov w13, #93");                                        // ASCII ']' (default for indexed arrays)
    emitter.instruction("cbz x12, __rt_json_arr_dyn_close_emit");               // skip the brace override when the flag is clear
    emitter.instruction("mov w13, #125");                                       // ASCII '}' (force-object form closes with a brace)
    emitter.label("__rt_json_arr_dyn_close_emit");
    emitter.instruction("strb w13, [x11]");                                     // write the chosen closing byte
    emitter.instruction("add x11, x11, #1");                                    // advance past the closing byte
    emitter.instruction("str x11, [sp, #56]");                                  // checkpoint the write pointer across the depth-exit helper call
    emitter.instruction("bl __rt_json_depth_exit");                             // decrement _json_active_depth so a sibling encoder can re-enter cleanly
    emitter.instruction("add x0, sp, #48");                                     // address of the output-buffer descriptor
    emitter.instruction("bl __rt_strbuf_finish");                               // x1/x2 = encoded array, _concat_off published
    emitter.instruction("ldr x19, [sp, #88]");                                  // restore caller x19 after using it as the flag cache
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the helper stack frame
    emitter.instruction("ret");                                                 // return the encoded JSON slice in x1/x2
}

/// Emits the x86_64 Linux variant of `__rt_json_encode_array_dynamic`.
///
/// Identical in behavior to the ARM64 emitter but uses the x86_64 System V ABI.
/// Preserves r15 as the flag cache register and uses rbp-based frame offsets for locals.
/// Stack layout differs (a 96-byte frame with the output-buffer descriptor at `[rbp - 96]`, vs the
/// 112-byte ARM64 frame with it at `[sp + 48]`).
fn emit_json_encode_array_dynamic_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_array_dynamic ---");
    emitter.label_global("__rt_json_encode_array_dynamic");

    // -- hand a run-time promoted array to the object-form encoder --
    // Same runtime-promotion probe as the AArch64 path; `__rt_json_encode_assoc` takes the
    // container in RAX and returns rax/rdx exactly as this entry does. NOTE the register:
    // every json encoder here carries the array in RAX, not rdi.
    emitter.instruction("mov r11, QWORD PTR [rax - 8]");                        // load the uniform heap-kind header word
    emitter.instruction("and r11, 0xff");                                       // isolate the low-byte heap kind
    emitter.instruction("cmp r11, 3");                                          // kind 3 = associative hash storage
    emitter.instruction("jne __rt_json_arr_dyn_packed");                        // packed storage encodes inline below
    emitter.instruction("jmp __rt_json_encode_assoc");                          // hash storage is the object-form encoder's job
    emitter.label("__rt_json_arr_dyn_packed");
    super::json_integer_tree::emit_json_integer_tree_entry(
        emitter,
        "__rt_json_arr_dyn_integer_tree_x86",
    );

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving JSON-array scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for array metadata and concat-buffer cursors
    emitter.instruction("sub rsp, 96");                                         // reserve local slots plus the output-buffer descriptor at [rbp - 96] (a 16-byte multiple keeps every call SysV-aligned)
    emitter.instruction("mov QWORD PTR [rbp - 64], r15");                       // preserve caller r15 before caching active JSON flags
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source array pointer across nested JSON helper calls

    // Enter the recursion-depth check before any output is produced.
    emitter.instruction("call __rt_json_depth_enter");                          // increment _json_active_depth and throw on overflow when requested

    // Cache active flags in r15 so JSON_FORCE_OBJECT checks survive nested
    // encoder calls without reloading the global flag slot.
    abi::emit_load_symbol_to_reg(emitter, "r15", "_json_active_flags", 0);      // cache the active flag bitmask for the whole indexed-array encode
    emitter.instruction("lea rdi, [rbp - 96]");                                 // address of the output-buffer descriptor
    emitter.instruction("call __rt_strbuf_init");                               // open the buffer at the current scratch offset
    emitter.instruction("lea rdi, [rbp - 96]");                                 // address of the output-buffer descriptor
    emitter.instruction("mov esi, 1");                                          // room for the opening byte
    emitter.instruction("call __rt_json_buf_ensure");                           // r11 = cursor with headroom reserved
    // Choose the opening byte based on JSON_FORCE_OBJECT.
    emitter.instruction("mov r10, r15");                                        // copy cached flags before isolating JSON_FORCE_OBJECT
    emitter.instruction("and r10, 16");                                         // isolate JSON_FORCE_OBJECT (bit 16)
    emitter.instruction("mov rcx, 91");                                         // ASCII '[' (default opening for indexed arrays)
    emitter.instruction("test r10, r10");                                       // is the force-object flag clear?
    emitter.instruction("jz __rt_json_arr_dyn_open_emit_x");                    // skip the brace override on the default path
    emitter.instruction("mov rcx, 123");                                        // ASCII '{' (force-object form opens with a brace)
    emitter.label("__rt_json_arr_dyn_open_emit_x");
    emitter.instruction("mov BYTE PTR [r11], cl");                              // emit the chosen opening byte
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the opening byte
    emitter.instruction("mov QWORD PTR [rbp - 88], r11");                       // persist the updated write pointer before entering the element loop
    emitter.instruction("call __rt_json_pretty_push");                          // enter one pretty-print indentation level after the container opens
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source array pointer (depth_enter clobbered rax)
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the indexed-array length from the first field of the array header
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the array length across nested JSON helper calls
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the packed array kind word so the value_type tag can drive JSON dispatch
    emitter.instruction("shr r10, 8");                                          // move the packed array value_type tag into the low bits for x86_64 dispatch
    emitter.instruction("and r10, 0x7f");                                       // isolate the packed array value_type tag without the persistent COW flag
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save the packed array value_type tag across nested JSON helper calls
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the indexed-array element loop counter to zero

    emitter.label("__rt_json_arr_dyn_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index at the top of the JSON loop
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // have we already encoded every indexed-array element?
    emitter.instruction("jae __rt_json_arr_dyn_close");                         // finish by writing the closing bracket once the loop index reaches the array length
    emitter.instruction("lea rdi, [rbp - 96]");                                 // address of the output-buffer descriptor
    emitter.instruction("mov esi, 1");                                          // room for the comma separator
    emitter.instruction("call __rt_json_buf_ensure");                           // grow first: every byte of this element prefix fits after it
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // is this the first indexed-array element in the JSON output?
    emitter.instruction("jz __rt_json_arr_dyn_elem");                           // skip the comma separator before the first encoded element
    emitter.instruction("mov r11, QWORD PTR [rbp - 88]");                       // reload the current concat-buffer write pointer before appending a comma separator
    emitter.instruction("mov BYTE PTR [r11], 44");                              // write the JSON comma separator between encoded array elements
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the comma separator
    emitter.instruction("mov QWORD PTR [rbp - 88], r11");                       // persist the updated write pointer after appending the comma separator

    emitter.label("__rt_json_arr_dyn_elem");
    emitter.instruction("mov r11, QWORD PTR [rbp - 88]");                       // reload write pos before optional pretty indentation
    emitter.instruction("call __rt_json_pretty_line");                          // append newline and indentation for this element/key when pretty-printing
    emitter.instruction("mov QWORD PTR [rbp - 88], r11");                       // save the write pos after any pretty indentation
    // -- when JSON_FORCE_OBJECT is set, prefix every element with `"<idx>":` --
    emitter.instruction("mov rcx, r15");                                        // copy cached flags before isolating JSON_FORCE_OBJECT
    emitter.instruction("and rcx, 16");                                         // isolate JSON_FORCE_OBJECT (bit 16)
    emitter.instruction("test rcx, rcx");                                       // is the flag clear?
    emitter.instruction("jz __rt_json_arr_dyn_elem_no_key_x");                  // skip the key prefix when the flag is clear
    emitter.instruction("mov r11, QWORD PTR [rbp - 88]");                       // reload the running write pointer
    emitter.instruction("mov BYTE PTR [r11], 34");                              // emit the opening quote of the synthetic key
    emitter.instruction("add r11, 1");                                          // advance past the opening quote
    emitter.instruction("mov QWORD PTR [rbp - 88], r11");                       // persist the updated write pointer
    // Sync concat_off so __rt_itoa formats the index digits just past the quote.
    emitter.instruction("lea rdi, [rbp - 96]");                                 // address of the output-buffer descriptor
    emitter.instruction("call __rt_strbuf_sync");                               // point itoa's scratch past the cursor (or at the heap buffer's opening offset)
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // load the loop index as the integer key value
    emitter.instruction("call __rt_itoa");                                      // format the index as decimal digits
    // __rt_itoa returns rax=ptr, rdx=len of the formatted slice — append it at the
    // running write position so the slice is contiguous with the prefix.
    emitter.instruction("mov rsi, rax");                                        // pass the digit slice pointer to the append helper (rdx = length)
    emitter.instruction("lea rdi, [rbp - 96]");                                 // address of the output-buffer descriptor
    emitter.instruction("call __rt_json_buf_append");                           // copy the digits to the cursor; r11 = advanced cursor
    emitter.instruction("mov BYTE PTR [r11], 34");                              // emit the closing quote of the synthetic key
    emitter.instruction("mov BYTE PTR [r11 + 1], 58");                          // emit the colon between key and value
    emitter.instruction("add r11, 2");                                          // advance the running write pointer past `":`
    emitter.instruction("call __rt_json_pretty_colon_space");                   // append the pretty-print key/value space when requested
    emitter.instruction("mov QWORD PTR [rbp - 88], r11");                       // persist the updated write pointer
    emitter.label("__rt_json_arr_dyn_elem_no_key_x");

    emitter.instruction("lea rdi, [rbp - 96]");                                 // address of the output-buffer descriptor
    emitter.instruction("call __rt_strbuf_sync");                               // nested JSON helpers must not format over the existing prefix
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the packed indexed-array value_type tag for runtime JSON dispatch
    emitter.instruction("cmp r10, 0");                                          // does this indexed array store integers?
    emitter.instruction("je __rt_json_arr_dyn_value_int");                      // encode integer elements through the decimal integer helper
    emitter.instruction("cmp r10, 1");                                          // does this indexed array store strings?
    emitter.instruction("je __rt_json_arr_dyn_value_str");                      // encode string elements through the JSON string helper
    emitter.instruction("cmp r10, 2");                                          // does this indexed array store floats?
    emitter.instruction("je __rt_json_arr_dyn_value_float");                    // encode float elements through the decimal float helper
    emitter.instruction("cmp r10, 3");                                          // does this indexed array store bools?
    emitter.instruction("je __rt_json_arr_dyn_value_bool");                     // encode bool elements through the JSON bool helper
    emitter.instruction("cmp r10, 4");                                          // does this indexed array store nested indexed arrays?
    emitter.instruction("je __rt_json_arr_dyn_value_array");                    // encode nested indexed arrays recursively
    emitter.instruction("cmp r10, 5");                                          // does this indexed array store nested associative arrays?
    emitter.instruction("je __rt_json_arr_dyn_value_assoc");                    // encode nested associative arrays recursively
    emitter.instruction("cmp r10, 6");                                          // does this indexed array store object instances?
    emitter.instruction("je __rt_json_arr_dyn_value_object");                   // encode objects through the object descriptor walker
    emitter.instruction("cmp r10, 7");                                          // does this indexed array store boxed mixed payloads?
    emitter.instruction("je __rt_json_arr_dyn_value_mixed");                    // encode boxed mixed payloads through the mixed JSON helper
    emitter.instruction(&format!("cmp r10, {}", POINTER_ARRAY_VALUE_TYPE));     // does this indexed array store raw ?ptr<T> words?
    emitter.instruction("je __rt_json_arr_dyn_value_pointer");                  // pointer words encode as null or an integer address
    emitter.instruction("jmp __rt_json_arr_dyn_value_null");                    // unsupported payloads currently degrade to JSON null

    emitter.label("__rt_json_arr_dyn_value_pointer");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // load the raw pointer word
    emitter.instruction("test rax, rax");                                       // is this the null pointer word?
    emitter.instruction("jz __rt_json_arr_dyn_value_null");                     // a null pointer word encodes as JSON null
    emitter.instruction("call __rt_itoa");                                      // encode a live address as a decimal JSON slice
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_int");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the integer element payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the payload slot address
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header to land on the first payload slot
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // load the integer element payload from the indexed-array storage slot
    emitter.instruction("call __rt_itoa");                                      // encode the integer element as a decimal JSON slice
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_str");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the ptr/len pair slots
    emitter.instruction("mov rcx, r10");                                        // copy the current indexed-array element index before scaling it into a ptr/len slot pair
    emitter.instruction("add rcx, rcx");                                        // compute index * 2 because string arrays store pointer/length pairs
    emitter.instruction("add rcx, 3");                                          // skip the 24-byte indexed-array header to land on the first ptr/len slot pair
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the string ptr/len pair
    emitter.instruction("mov rax, QWORD PTR [r10 + rcx * 8]");                  // load the string pointer from the indexed-array ptr/len storage pair
    emitter.instruction("add rcx, 1");                                          // advance from the string pointer slot to the paired string length slot
    emitter.instruction("mov rdx, QWORD PTR [r10 + rcx * 8]");                  // load the string length from the indexed-array ptr/len storage pair
    emitter.instruction("call __rt_json_encode_str");                           // encode the string element with JSON escaping and quotes
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_float");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the float payload bits
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the float slot address
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header to land on the first payload slot
    emitter.instruction("mov r10, QWORD PTR [rax + r10 * 8]");                  // load the raw float bit-pattern from the indexed-array storage slot
    emitter.instruction("movq xmm0, r10");                                      // move the raw float bit-pattern into the x86_64 floating-point argument register
    emitter.instruction("call __rt_json_encode_float");                         // encode the float element, rejecting Inf/NaN per JSON semantics
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_bool");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the bool payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the bool slot address
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header to land on the first payload slot
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // load the bool payload from the indexed-array storage slot
    emitter.instruction("call __rt_json_encode_bool");                          // encode the bool element as the JSON literals true/false
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_array");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the nested indexed-array payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the nested-array slot address
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header to land on the first payload slot
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // load the nested indexed-array pointer from the indexed-array storage slot
    emitter.instruction("call __rt_json_encode_array_dynamic");                 // encode the nested indexed-array recursively into a JSON slice
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded nested JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_assoc");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the nested associative-array payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the nested-hash slot address
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header to land on the first payload slot
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // load the nested associative-array pointer from the indexed-array storage slot
    emitter.instruction("call __rt_json_encode_assoc");                         // encode the nested associative array recursively into a JSON slice
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded nested JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_object");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the object payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the object slot address
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header to land on the first payload slot
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // load the object pointer from the indexed-array storage slot
    emitter.instruction("call __rt_json_encode_object");                        // encode the object via the per-class JSON descriptor walker
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded nested JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_mixed");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before loading the boxed mixed payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current indexed-array element index before computing the mixed payload slot address
    emitter.instruction("add r10, 3");                                          // skip the 24-byte indexed-array header to land on the first payload slot
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // load the boxed mixed pointer from the indexed-array storage slot
    emitter.instruction("call __rt_json_encode_mixed");                         // encode the boxed mixed payload recursively into a JSON slice
    emitter.instruction("jmp __rt_json_arr_dyn_copy");                          // copy the encoded nested JSON element into concat_buf

    emitter.label("__rt_json_arr_dyn_value_null");
    emitter.instruction("call __rt_json_encode_null");                          // encode null or unsupported payload families as the JSON null literal

    emitter.label("__rt_json_arr_dyn_copy");
    emitter.instruction("mov rsi, rax");                                        // pass the encoded element pointer to the append helper (rdx = length)
    emitter.instruction("lea rdi, [rbp - 96]");                                 // address of the output-buffer descriptor
    emitter.instruction("call __rt_json_buf_append");                           // copy the encoded element to the cursor, growing first, and release a transient child buffer

    emitter.label("__rt_json_arr_dyn_next");
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // advance the indexed-array element loop counter to the next payload slot
    emitter.instruction("jmp __rt_json_arr_dyn_loop");                          // continue encoding the remaining indexed-array elements

    emitter.label("__rt_json_arr_dyn_close");
    emitter.instruction("lea rdi, [rbp - 96]");                                 // address of the output-buffer descriptor
    emitter.instruction("mov esi, 1");                                          // room for the closing byte
    emitter.instruction("call __rt_json_buf_ensure");                           // r11 = cursor with headroom for the closing line
    emitter.instruction("call __rt_json_pretty_pop");                           // leave the container indentation level before closing it
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // did the array contain any elements?
    emitter.instruction("je __rt_json_arr_dyn_close_choose_x");                 // empty containers stay compact even under JSON_PRETTY_PRINT
    emitter.instruction("call __rt_json_pretty_line");                          // append the closing-line indentation for non-empty pretty containers
    emitter.label("__rt_json_arr_dyn_close_choose_x");
    emitter.instruction("mov rcx, r15");                                        // copy cached flags before isolating JSON_FORCE_OBJECT
    emitter.instruction("and rcx, 16");                                         // isolate JSON_FORCE_OBJECT (bit 16)
    emitter.instruction("mov rax, 93");                                         // ASCII ']' (default for indexed arrays)
    emitter.instruction("test rcx, rcx");                                       // is the force-object flag clear?
    emitter.instruction("jz __rt_json_arr_dyn_close_emit_x");                   // skip the brace override on the default path
    emitter.instruction("mov rax, 125");                                        // ASCII '}' (force-object form closes with a brace)
    emitter.label("__rt_json_arr_dyn_close_emit_x");
    emitter.instruction("mov BYTE PTR [r11], al");                              // emit the chosen closing byte
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the closing byte
    emitter.instruction("mov QWORD PTR [rbp - 88], r11");                       // checkpoint the write pointer across the depth-exit helper call
    emitter.instruction("call __rt_json_depth_exit");                           // decrement _json_active_depth so a sibling encoder can re-enter cleanly
    emitter.instruction("lea rdi, [rbp - 96]");                                 // address of the output-buffer descriptor
    emitter.instruction("call __rt_strbuf_finish");                             // rax/rdx = encoded array, _concat_off published
    emitter.instruction("mov r15, QWORD PTR [rbp - 64]");                       // restore caller r15 after using it as the flag cache
    emitter.instruction("add rsp, 96");                                         // release the local JSON-array scratch frame before returning to generated code
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code
    emitter.instruction("ret");                                                 // return the encoded JSON array slice in the x86_64 string result registers
}
