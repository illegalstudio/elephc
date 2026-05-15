//! Purpose:
//! Emits the `__rt_json_encode_assoc`, `__rt_hash_iter_next` runtime helper assembly for json encode assoc.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - JSON encoders are emitted formatter state machines; escaping, type tags, and buffer growth are observable PHP behavior.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_json_encode_assoc: encode an assoc array as JSON '{"key":"value",...}'.
/// Input:  x0 = hash table pointer
/// Output: x1 = result ptr (in concat_buf), x2 = result len
///
/// Uses __rt_hash_iter_next to iterate the hash table entries in insertion order.
/// Hash table iter yields: x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi, x5=val_tag per entry.
pub(crate) fn emit_json_encode_assoc(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_assoc_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_encode_assoc ---");
    emitter.label_global("__rt_json_encode_assoc");

    // -- set up stack frame --
    // Frame layout (128 bytes):
    //   [sp + 0..87]   = existing scratch slots (hash ptr, output, count, etc.)
    //   [sp + 88]      = current entry's value tag (existing)
    //   [sp + 96..111] = saved x29/x30
    //   [sp + 112]     = list_mode flag (1 = emit JSON array form, 0 = object form)
    emitter.instruction("sub sp, sp, #128");                                    // allocate 128 bytes (was 112, +16 for the list_mode flag + alignment padding)
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // set new frame pointer

    // -- save hash table pointer --
    emitter.instruction("str x0, [sp, #0]");                                    // save hash ptr

    // -- detect list-shape: an associative array whose keys form the
    //    sequence 0..count-1 in insertion order encodes as a JSON array
    //    `[...]` (PHP semantics). Skip detection when JSON_FORCE_OBJECT
    //    is set so that flag still wins. --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x9, [x9]");                                        // load the active flag bitmask
    emitter.instruction("tst x9, #16");                                         // is JSON_FORCE_OBJECT (bit 16) set?
    emitter.instruction("b.ne __rt_json_assoc_force_object_mode");              // FORCE_OBJECT wins → emit object form
    emitter.instruction("bl __rt_json_assoc_is_list_shape");                    // x0 = 1 if list-shape, 0 otherwise
    emitter.instruction("str x0, [sp, #112]");                                  // park the list_mode flag for the loop and close
    emitter.instruction("b __rt_json_assoc_after_list_check");                  // continue in the JSON object encoder control path
    emitter.label("__rt_json_assoc_force_object_mode");
    emitter.instruction("str xzr, [sp, #112]");                                 // FORCE_OBJECT path always uses object form

    emitter.label("__rt_json_assoc_after_list_check");
    // -- enter the recursion-depth check so JSON_ERROR_DEPTH fires when
    //    the user-supplied $depth limit is exceeded --
    emitter.instruction("bl __rt_json_depth_enter");                            // increment _json_active_depth and throw on overflow when requested

    // -- get output position in concat_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // output position
    emitter.instruction("str x11, [sp, #8]");                                   // save output start
    emitter.instruction("str x11, [sp, #16]");                                  // save output write pos

    // -- write opening bracket: '[' for list-shape, '{' otherwise --
    emitter.instruction("ldr x12, [sp, #112]");                                 // reload the list_mode flag
    emitter.instruction("cbz x12, __rt_json_assoc_open_obj");                   // 0 → object form
    emitter.instruction("mov w12, #91");                                        // ASCII '['
    emitter.instruction("b __rt_json_assoc_open_emit");                         // continue in the JSON object encoder control path
    emitter.label("__rt_json_assoc_open_obj");
    emitter.instruction("mov w12, #123");                                       // ASCII '{'
    emitter.label("__rt_json_assoc_open_emit");
    emitter.instruction("strb w12, [x11]");                                     // write the opening bracket
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos

    // -- get hash table count --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload hash ptr
    emitter.instruction("bl __rt_hash_count");                                  // get count → x0
    emitter.instruction("str x0, [sp, #24]");                                   // save count
    emitter.instruction("str xzr, [sp, #32]");                                  // iterator cursor = 0 (start from hash header head)
    emitter.instruction("str xzr, [sp, #40]");                                  // items written = 0

    // -- iterate hash table entries --
    emitter.label("__rt_json_assoc_loop");
    emitter.instruction("ldr x4, [sp, #40]");                                   // load items written
    emitter.instruction("ldr x3, [sp, #24]");                                   // load total count
    emitter.instruction("cmp x4, x3");                                          // check if all items written
    emitter.instruction("b.ge __rt_json_assoc_close");                          // done

    // -- get next entry via hash_iter --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload hash ptr
    emitter.instruction("ldr x1, [sp, #32]");                                   // load iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // get entry → x0=next_cursor, x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi
    emitter.instruction("str x0, [sp, #32]");                                   // save next iterator cursor

    // -- save key and value on stack --
    emitter.instruction("str x1, [sp, #48]");                                   // save key ptr
    emitter.instruction("str x2, [sp, #56]");                                   // save key len
    emitter.instruction("str x3, [sp, #64]");                                   // save val_lo
    emitter.instruction("str x4, [sp, #72]");                                   // save val_hi
    emitter.instruction("str x5, [sp, #88]");                                   // save val_tag

    // -- add comma if not first entry --
    emitter.instruction("ldr x5, [sp, #40]");                                   // load items written
    emitter.instruction("cbz x5, __rt_json_assoc_key");                         // skip comma for first
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("mov w12, #44");                                        // ASCII ','
    emitter.instruction("strb w12, [x11]");                                     // write ','
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos

    // -- write key as quoted JSON string (skipped in list mode) --
    // List-shape arrays emit `[...]` form so keys must not be written;
    // jump straight to the value emission. Otherwise, string keys are
    // encoded through __rt_json_encode_str so every JSON escape (control
    // bytes, quotes, backslashes, multibyte UTF-8, the active flag set)
    // is honored on the key. Integer keys are formatted through itoa and
    // copied into place inline because their decimal representation
    // never contains JSON-significant bytes.
    emitter.label("__rt_json_assoc_key");
    emitter.instruction("ldr x12, [sp, #112]");                                 // reload the list_mode flag
    emitter.instruction("cbnz x12, __rt_json_assoc_after_key_prefix");          // list mode skips the key + colon prefix
    emitter.instruction("ldr x1, [sp, #48]");                                   // load key ptr (or integer payload when len = -1)
    emitter.instruction("ldr x2, [sp, #56]");                                   // load key len (or -1 sentinel for integer keys)
    emitter.instruction("cmn x2, #1");                                          // is this an integer key?
    emitter.instruction("b.eq __rt_json_assoc_key_int");                        // branch on the current JSON object encoder condition

    // String key: sync _concat_off and tail-call __rt_json_encode_str.
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the current output write position
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x12, x11, x10");                                   // x12 = absolute concat-buffer offset for the write pos
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x12, [x9]");                                       // sync _concat_off so __rt_json_encode_str appends in place
    emitter.instruction("bl __rt_json_encode_str");                             // writes "<escaped key>" into concat_buf and returns x1=ptr, x2=len
    emitter.instruction("add x11, x1, x2");                                     // advance past the closing quote written by encode_str
    emitter.instruction("b __rt_json_assoc_key_colon");                         // continue in the JSON object encoder control path

    // Integer key: format via itoa (which writes into its own 21-byte
    // scratch area inside concat_buf and advances _concat_off by 21),
    // then copy the digit bytes into the JSON output position bracketed
    // by quotes.
    emitter.label("__rt_json_assoc_key_int");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the current output write position
    emitter.instruction("mov w12, #34");                                        // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                     // opening quote for the integer key
    emitter.instruction("add x11, x11, #1");                                    // advance past the opening quote
    emitter.instruction("str x11, [sp, #80]");                                  // park the JSON write pointer across the itoa call
    emitter.instruction("mov x0, x1");                                          // move the integer key payload into the decimal-formatter input register
    emitter.instruction("bl __rt_itoa");                                        // x1=ptr to digits in itoa's scratch area, x2=digit count
    emitter.instruction("ldr x11, [sp, #80]");                                  // reload the parked JSON write pointer
    emitter.instruction("mov x10, #0");                                         // copy index = 0
    emitter.label("__rt_json_assoc_key_int_copy");
    emitter.instruction("cmp x10, x2");                                         // have we copied every digit byte?
    emitter.instruction("b.ge __rt_json_assoc_key_int_copy_done");              // exit when finished
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load the next digit byte from itoa's output
    emitter.instruction("strb w12, [x11, x10]");                                // write it into the JSON output position
    emitter.instruction("add x10, x10, #1");                                    // advance the copy index
    emitter.instruction("b __rt_json_assoc_key_int_copy");                      // continue copying digit bytes
    emitter.label("__rt_json_assoc_key_int_copy_done");
    emitter.instruction("add x11, x11, x2");                                    // advance the JSON write pointer past the digit run
    emitter.instruction("mov w12, #34");                                        // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                     // closing quote for the integer key
    emitter.instruction("add x11, x11, #1");                                    // advance past the closing quote

    emitter.label("__rt_json_assoc_key_colon");
    // -- write colon --
    emitter.instruction("mov w12, #58");                                        // ASCII ':'
    emitter.instruction("strb w12, [x11]");                                     // write ':'
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos after emitting the JSON key prefix

    emitter.label("__rt_json_assoc_after_key_prefix");
    // -- move concat_off to the current write position so nested encoders append safely --
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the current output write position
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x12, x11, x10");                                   // x12 = absolute concat offset for the current write position
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x12, [x9]");                                       // nested JSON/string encoders append after the existing key prefix

    // -- encode the value according to its per-entry runtime tag --
    emitter.instruction("ldr x12, [sp, #88]");                                  // load the saved per-entry value_tag
    emitter.instruction("cmp x12, #0");                                         // is this value an integer?
    emitter.instruction("b.eq __rt_json_assoc_value_int");                      // encode integers via itoa
    emitter.instruction("cmp x12, #1");                                         // is this value a string?
    emitter.instruction("b.eq __rt_json_assoc_value_str");                      // encode strings with JSON escaping
    emitter.instruction("cmp x12, #2");                                         // is this value a float?
    emitter.instruction("b.eq __rt_json_assoc_value_float");                    // encode floats via ftoa
    emitter.instruction("cmp x12, #3");                                         // is this value a bool?
    emitter.instruction("b.eq __rt_json_assoc_value_bool");                     // encode bools via json_encode_bool
    emitter.instruction("cmp x12, #4");                                         // is this value an indexed array?
    emitter.instruction("b.eq __rt_json_assoc_value_array");                    // encode arrays via the indexed-array JSON helpers
    emitter.instruction("cmp x12, #5");                                         // is this value an associative array?
    emitter.instruction("b.eq __rt_json_assoc_value_assoc");                    // encode nested associative arrays recursively
    emitter.instruction("cmp x12, #6");                                         // is this value an object instance?
    emitter.instruction("b.eq __rt_json_assoc_value_object");                   // encode objects via the descriptor walker
    emitter.instruction("cmp x12, #7");                                         // is this value a boxed mixed payload?
    emitter.instruction("b.eq __rt_json_assoc_value_mixed");                    // encode boxed mixed payloads recursively
    emitter.instruction("cmp x12, #8");                                         // is this value null?
    emitter.instruction("b.eq __rt_json_assoc_value_null");                     // encode null via json_encode_null
    emitter.instruction("b __rt_json_assoc_value_null");                        // unsupported tags currently encode as null

    emitter.label("__rt_json_assoc_value_int");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load integer payload from value_lo
    emitter.instruction("bl __rt_itoa");                                        // encode integer payload as decimal digits
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded value into concat_buf

    emitter.label("__rt_json_assoc_value_str");
    emitter.instruction("ldr x1, [sp, #64]");                                   // load string pointer from value_lo
    emitter.instruction("ldr x2, [sp, #72]");                                   // load string length from value_hi
    emitter.instruction("bl __rt_json_encode_str");                             // encode string payload with JSON escaping and quotes
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded value into concat_buf

    emitter.label("__rt_json_assoc_value_float");
    emitter.instruction("ldr x9, [sp, #64]");                                   // load float bits from value_lo
    emitter.instruction("fmov d0, x9");                                         // move float bits into the FP argument register
    emitter.instruction("bl __rt_json_encode_float");                           // encode float payload, rejecting Inf/NaN per JSON semantics
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded value into concat_buf

    emitter.label("__rt_json_assoc_value_bool");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load bool payload from value_lo
    emitter.instruction("bl __rt_json_encode_bool");                            // encode bool payload as true/false
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded value into concat_buf

    emitter.label("__rt_json_assoc_value_array");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load nested array pointer from value_lo
    emitter.instruction("bl __rt_json_encode_array_dynamic");                   // encode nested indexed arrays through the dynamic array JSON helper
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded nested array into concat_buf

    emitter.label("__rt_json_assoc_value_assoc");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load nested associative array pointer from value_lo
    emitter.instruction("bl __rt_json_encode_assoc");                           // encode the nested associative array recursively
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded nested associative array into concat_buf

    emitter.label("__rt_json_assoc_value_object");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load nested object pointer from value_lo
    emitter.instruction("bl __rt_json_encode_object");                          // encode the object via the descriptor walker
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded object into concat_buf

    emitter.label("__rt_json_assoc_value_mixed");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load boxed mixed pointer from value_lo
    emitter.instruction("bl __rt_json_encode_mixed");                           // encode the boxed mixed payload recursively
    emitter.instruction("b __rt_json_assoc_value_copy");                        // copy the encoded mixed payload into concat_buf

    emitter.label("__rt_json_assoc_value_null");
    emitter.instruction("bl __rt_json_encode_null");                            // encode null or unsupported payloads as JSON null

    emitter.label("__rt_json_assoc_value_copy");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the current concat_buf write position
    emitter.instruction("mov x10, #0");                                         // copy index
    emitter.label("__rt_json_assoc_val_copy");
    emitter.instruction("cmp x10, x2");                                         // check if done
    emitter.instruction("b.ge __rt_json_assoc_val_done");                       // done
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load val byte
    emitter.instruction("strb w12, [x11, x10]");                                // write to output
    emitter.instruction("add x10, x10, #1");                                    // increment
    emitter.instruction("b __rt_json_assoc_val_copy");                          // continue
    emitter.label("__rt_json_assoc_val_done");
    emitter.instruction("add x11, x11, x2");                                    // advance write pos
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos

    // -- increment items written --
    emitter.instruction("ldr x5, [sp, #40]");                                   // load items written
    emitter.instruction("add x5, x5, #1");                                      // increment
    emitter.instruction("str x5, [sp, #40]");                                   // save items written
    emitter.instruction("b __rt_json_assoc_loop");                              // continue loop

    // -- write closing bracket: ']' for list-shape, '}' otherwise --
    emitter.label("__rt_json_assoc_close");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("ldr x10, [sp, #112]");                                 // reload the list_mode flag
    emitter.instruction("cbz x10, __rt_json_assoc_close_obj");                  // 0 → object form
    emitter.instruction("mov w12, #93");                                        // ASCII ']'
    emitter.instruction("b __rt_json_assoc_close_emit");                        // continue in the JSON object encoder control path
    emitter.label("__rt_json_assoc_close_obj");
    emitter.instruction("mov w12, #125");                                       // ASCII '}'
    emitter.label("__rt_json_assoc_close_emit");
    emitter.instruction("strb w12, [x11]");                                     // write the closing bracket
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // checkpoint the write pointer across the depth-exit helper call
    emitter.instruction("bl __rt_json_depth_exit");                             // decrement _json_active_depth so a sibling encoder can re-enter cleanly
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the write pointer after the helper call

    // -- compute result --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = output start
    emitter.instruction("sub x2, x11, x1");                                     // x2 = total length

    // -- update concat_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x10, x11, x10");                                   // compute the absolute concat offset after the closing bracket
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- tear down and return --
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    emit_assoc_is_list_shape_aarch64(emitter);
}

/// __rt_json_assoc_is_list_shape: walk the hash's insertion-order chain
/// and decide whether the keys are exactly 0..count-1 in order. Returns
/// x0 = 1 for list-shape (including empty hashes) or 0 otherwise.
///
/// Hash header layout: [count:8][capacity:8][value_type:8][head:8][tail:8]
/// Entry layout: [occupied:8][key_ptr:8][key_len:8][value_lo:8][value_hi:8]
///               [value_tag:8][prev:8][next:8]
/// An integer key is signaled by key_len = -1, with key_ptr holding the
/// integer payload directly.
fn emit_assoc_is_list_shape_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_assoc_is_list_shape ---");
    emitter.label_global("__rt_json_assoc_is_list_shape");
    emitter.instruction("ldr x1, [x0]");                                        // x1 = count
    emitter.instruction("cbz x1, __rt_json_assoc_is_list_shape_yes");           // empty hash → list shape (PHP encodes [])
    emitter.instruction("ldr x2, [x0, #24]");                                   // x2 = head index (insertion-order chain)
    emitter.instruction("cmn x2, #1");                                          // is the head sentinel (-1, empty chain)?
    emitter.instruction("b.eq __rt_json_assoc_is_list_shape_no");               // count > 0 but no head → object form
    emitter.instruction("mov x3, #0");                                          // expected key value for this iteration
    emitter.label("__rt_json_assoc_is_list_shape_loop");
    emitter.instruction("add x4, x0, #40");                                     // entries base = hash + 40 (header size)
    emitter.instruction("lsl x5, x2, #6");                                      // entry index * 64 (entry size)
    emitter.instruction("add x5, x4, x5");                                      // x5 = current entry pointer
    emitter.instruction("ldr x6, [x5, #16]");                                   // x6 = entry.key_len
    emitter.instruction("cmn x6, #1");                                          // is the key an integer (key_len == -1)?
    emitter.instruction("b.ne __rt_json_assoc_is_list_shape_no");               // string keys disqualify list shape
    emitter.instruction("ldr x6, [x5, #8]");                                    // x6 = entry.key_ptr (integer key payload)
    emitter.instruction("cmp x6, x3");                                          // does it match the expected sequential key?
    emitter.instruction("b.ne __rt_json_assoc_is_list_shape_no");               // mismatch → object form
    emitter.instruction("add x3, x3, #1");                                      // expected_key++
    emitter.instruction("ldr x2, [x5, #56]");                                   // x2 = entry.next
    emitter.instruction("cmn x2, #1");                                          // hit the chain sentinel (-1)?
    emitter.instruction("b.ne __rt_json_assoc_is_list_shape_loop");             // continue scanning
    emitter.label("__rt_json_assoc_is_list_shape_yes");
    emitter.instruction("mov x0, #1");                                          // report list shape
    emitter.instruction("ret");                                                 // return from the JSON object encoder helper
    emitter.label("__rt_json_assoc_is_list_shape_no");
    emitter.instruction("mov x0, #0");                                          // report object form
    emitter.instruction("ret");                                                 // return from the JSON object encoder helper
}

fn emit_json_encode_assoc_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_assoc ---");
    emitter.label_global("__rt_json_encode_assoc");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving JSON-assoc scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for hash iteration state and concat-buffer cursors
    emitter.instruction("sub rsp, 112");                                        // reserve local slots (was 96, +16 for the list_mode flag + alignment padding)
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source associative-array pointer across nested JSON helper calls

    // Detect list-shape so [0=>'a',1=>'b'] (and post-unset arrays) emit
    // ["a","b"] instead of {"0":"a","1":"b"}. JSON_FORCE_OBJECT skips the
    // detection so the flag still wins.
    emitter.instruction("mov rdx, QWORD PTR [rip + _json_active_flags]");       // load the active flag bitmask
    emitter.instruction("test rdx, 16");                                        // is JSON_FORCE_OBJECT (bit 16) set?
    emitter.instruction("jne __rt_json_assoc_force_object_mode_x");             // FORCE_OBJECT wins → emit object form
    emitter.instruction("call __rt_json_assoc_is_list_shape");                  // rax = 1 if list-shape, 0 otherwise
    emitter.instruction("mov QWORD PTR [rbp - 96], rax");                       // park the list_mode flag for the loop and close
    emitter.instruction("jmp __rt_json_assoc_after_list_check_x");              // continue in the JSON object encoder control path
    emitter.label("__rt_json_assoc_force_object_mode_x");
    emitter.instruction("mov QWORD PTR [rbp - 96], 0");                         // FORCE_OBJECT path always uses object form

    emitter.label("__rt_json_assoc_after_list_check_x");
    // Enter the recursion-depth check before any output is produced.
    emitter.instruction("call __rt_json_depth_enter");                          // increment _json_active_depth and throw on overflow when requested

    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer offset before appending the JSON object
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the current JSON append
    emitter.instruction("add r11, r10");                                        // compute the current concat-buffer write pointer from the base plus offset
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the encoded-object start pointer for the final result slice
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the current concat-buffer write pointer for the hash iteration loop
    // Write the opening bracket: '[' for list-shape, '{' otherwise.
    emitter.instruction("mov rdx, QWORD PTR [rbp - 96]");                       // reload the list_mode flag
    emitter.instruction("test rdx, rdx");                                       // check the current JSON object encoder condition
    emitter.instruction("jz __rt_json_assoc_open_obj_x");                       // 0 → object form
    emitter.instruction("mov BYTE PTR [r11], 91");                              // ASCII '['
    emitter.instruction("jmp __rt_json_assoc_open_emit_x");                     // continue in the JSON object encoder control path
    emitter.label("__rt_json_assoc_open_obj_x");
    emitter.instruction("mov BYTE PTR [r11], 123");                             // ASCII '{'
    emitter.label("__rt_json_assoc_open_emit_x");
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the opening bracket
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer before entering the hash iteration loop
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the hash iterator cursor to the insertion-order start sentinel
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the number of encoded key/value pairs to zero

    emitter.label("__rt_json_assoc_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the source associative-array pointer for the next insertion-order iteration step
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the current hash iterator cursor for the next insertion-order iteration step
    emitter.instruction("call __rt_hash_iter_next");                            // advance one insertion-order hash entry and return its key plus payload in the x86_64 result registers
    emitter.instruction("cmp rax, -1");                                         // has associative-array iteration reached the done sentinel?
    emitter.instruction("je __rt_json_assoc_close");                            // finish by writing the closing brace once every hash entry has been encoded
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the updated hash iterator cursor for the next insertion-order loop step
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // save the current associative-array key pointer for the JSON key copy loop
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // save the current associative-array key length for the JSON key copy loop
    emitter.instruction("mov QWORD PTR [rbp - 64], rcx");                       // save the current associative-array value low payload word for runtime JSON dispatch
    emitter.instruction("mov QWORD PTR [rbp - 72], r8");                        // save the current associative-array value high payload word for runtime JSON dispatch
    emitter.instruction("mov QWORD PTR [rbp - 80], r9");                        // save the current associative-array runtime value tag for runtime JSON dispatch
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // is this the first encoded associative-array entry in the JSON object?
    emitter.instruction("je __rt_json_assoc_key");                              // skip the comma separator before the first encoded associative-array entry
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the current concat-buffer write pointer before appending the comma separator
    emitter.instruction("mov BYTE PTR [r11], 44");                              // write the JSON comma separator between encoded associative-array entries
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the comma separator
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer after appending the comma separator

    // String keys are encoded through __rt_json_encode_str so every JSON
    // escape (control bytes, quotes, backslashes, multibyte UTF-8, the
    // active flag set) is honored. Integer keys are formatted through
    // __rt_itoa (which writes into its own 21-byte scratch area inside
    // concat_buf and advances _concat_off by 21) and the digit bytes are
    // copied inline into the JSON output position bracketed by quotes.
    // List-mode skips the key + colon prefix and jumps straight to the
    // value emission.
    emitter.label("__rt_json_assoc_key");
    emitter.instruction("mov rdx, QWORD PTR [rbp - 96]");                       // reload the list_mode flag
    emitter.instruction("test rdx, rdx");                                       // check the current JSON object encoder condition
    emitter.instruction("jne __rt_json_assoc_after_key_prefix");                // list mode skips the key + colon prefix
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // load the key pointer (or integer payload when len = -1)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // load the key length (or -1 sentinel for integer keys)
    emitter.instruction("cmp rdx, -1");                                         // is this an integer key?
    emitter.instruction("je __rt_json_assoc_key_int");                          // branch on the current JSON object encoder condition

    // String key: sync _concat_off and tail-call __rt_json_encode_str.
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the current concat-buffer write pointer
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer
    emitter.instruction("mov rcx, r11");                                        // copy the write pointer for the absolute-offset computation
    emitter.instruction("sub rcx, r10");                                        // rcx = absolute concat-buffer offset
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // sync _concat_off so __rt_json_encode_str appends in place
    emitter.instruction("call __rt_json_encode_str");                           // writes "<escaped key>" into concat_buf and returns rax=ptr, rdx=len
    emitter.instruction("mov r11, rax");                                        // recover the start pointer of the encoded key
    emitter.instruction("add r11, rdx");                                        // advance past the closing quote written by encode_str
    emitter.instruction("jmp __rt_json_assoc_key_colon");                       // continue in the JSON object encoder control path

    // Integer key: format via __rt_itoa, then copy digit bytes into the
    // JSON output position. itoa updates _concat_off internally; we copy
    // the digits explicitly so the output ends up bracketed by quotes
    // at the JSON write position.
    emitter.label("__rt_json_assoc_key_int");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the current concat-buffer write pointer
    emitter.instruction("mov BYTE PTR [r11], 34");                              // write the opening JSON key quote
    emitter.instruction("add r11, 1");                                          // advance past the opening quote
    emitter.instruction("mov QWORD PTR [rbp - 88], r11");                       // park the JSON write pointer across the itoa call
    emitter.instruction("call __rt_itoa");                                      // rax = ptr to digits in itoa's scratch area, rdx = digit count
    emitter.instruction("mov r10, rax");                                        // remember the source pointer before the copy loop
    emitter.instruction("mov rcx, rdx");                                        // remember the digit count for the copy loop bound
    emitter.instruction("mov r11, QWORD PTR [rbp - 88]");                       // reload the parked JSON write pointer
    emitter.instruction("xor rdx, rdx");                                        // copy index = 0
    emitter.label("__rt_json_assoc_key_int_copy");
    emitter.instruction("cmp rdx, rcx");                                        // have we copied every digit byte?
    emitter.instruction("jae __rt_json_assoc_key_int_copy_done");               // exit when finished
    emitter.instruction("mov r8b, BYTE PTR [r10 + rdx]");                       // load the next digit byte from itoa's output
    emitter.instruction("mov BYTE PTR [r11 + rdx], r8b");                       // write it into the JSON output position
    emitter.instruction("add rdx, 1");                                          // advance the copy index
    emitter.instruction("jmp __rt_json_assoc_key_int_copy");                    // continue copying digit bytes
    emitter.label("__rt_json_assoc_key_int_copy_done");
    emitter.instruction("add r11, rcx");                                        // advance the JSON write pointer past the digit run
    emitter.instruction("mov BYTE PTR [r11], 34");                              // write the closing JSON key quote
    emitter.instruction("add r11, 1");                                          // advance past the closing quote

    emitter.label("__rt_json_assoc_key_colon");
    emitter.instruction("mov BYTE PTR [r11], 58");                              // write the JSON colon separator between the encoded key and encoded value
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the colon separator
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer after emitting the full JSON key prefix

    emitter.label("__rt_json_assoc_after_key_prefix");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the current write pointer (in case we entered via the list-mode skip)
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the global offset update before nested value encoding
    emitter.instruction("mov rcx, r11");                                        // copy the current write pointer before turning it into an absolute concat offset
    emitter.instruction("sub rcx, r10");                                        // compute the concat-buffer absolute offset for the current JSON value write position
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // publish the concat-buffer offset so nested JSON helpers append after the existing key prefix
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the saved associative-array runtime value tag for runtime JSON dispatch
    emitter.instruction("cmp r10, 0");                                          // is the associative-array value an integer?
    emitter.instruction("je __rt_json_assoc_value_int");                        // encode integer payloads through the decimal integer helper
    emitter.instruction("cmp r10, 1");                                          // is the associative-array value a string?
    emitter.instruction("je __rt_json_assoc_value_str");                        // encode string payloads through the JSON string helper
    emitter.instruction("cmp r10, 2");                                          // is the associative-array value a float?
    emitter.instruction("je __rt_json_assoc_value_float");                      // encode float payloads through the decimal float helper
    emitter.instruction("cmp r10, 3");                                          // is the associative-array value a bool?
    emitter.instruction("je __rt_json_assoc_value_bool");                       // encode bool payloads through the JSON bool helper
    emitter.instruction("cmp r10, 4");                                          // is the associative-array value a nested indexed array?
    emitter.instruction("je __rt_json_assoc_value_array");                      // encode nested indexed arrays recursively
    emitter.instruction("cmp r10, 5");                                          // is the associative-array value a nested associative array?
    emitter.instruction("je __rt_json_assoc_value_assoc");                      // encode nested associative arrays recursively
    emitter.instruction("cmp r10, 6");                                          // is the associative-array value an object instance?
    emitter.instruction("je __rt_json_assoc_value_object");                     // encode objects through the object descriptor walker
    emitter.instruction("cmp r10, 7");                                          // is the associative-array value a boxed mixed payload?
    emitter.instruction("je __rt_json_assoc_value_mixed");                      // encode boxed mixed payloads through the mixed JSON helper
    emitter.instruction("cmp r10, 8");                                          // is the associative-array value explicit null?
    emitter.instruction("je __rt_json_assoc_value_null");                       // encode explicit null payloads through the shared helper
    emitter.instruction("jmp __rt_json_assoc_value_null");                      // unsupported payload families currently degrade to JSON null

    emitter.label("__rt_json_assoc_value_int");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the integer payload from the saved hash-entry low payload word
    emitter.instruction("call __rt_itoa");                                      // encode the integer payload as a decimal JSON slice
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_str");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the string pointer from the saved hash-entry low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // load the string length from the saved hash-entry high payload word
    emitter.instruction("call __rt_json_encode_str");                           // encode the string payload with JSON escaping and quotes
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_float");
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // load the raw float bit-pattern from the saved hash-entry low payload word
    emitter.instruction("movq xmm0, r10");                                      // move the raw float bit-pattern into the x86_64 floating-point argument register
    emitter.instruction("call __rt_json_encode_float");                         // encode the float payload, rejecting Inf/NaN per JSON semantics
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_bool");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the bool payload from the saved hash-entry low payload word
    emitter.instruction("call __rt_json_encode_bool");                          // encode the bool payload as the JSON literals true/false
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_array");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the nested indexed-array pointer from the saved hash-entry low payload word
    emitter.instruction("call __rt_json_encode_array_dynamic");                 // encode the nested indexed array recursively into a JSON slice
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_assoc");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the nested associative-array pointer from the saved hash-entry low payload word
    emitter.instruction("call __rt_json_encode_assoc");                         // encode the nested associative array recursively into a JSON slice
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_object");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the object pointer from the saved hash-entry low payload word
    emitter.instruction("call __rt_json_encode_object");                        // encode the object via the per-class JSON descriptor walker
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_mixed");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the boxed mixed pointer from the saved hash-entry low payload word
    emitter.instruction("call __rt_json_encode_mixed");                         // encode the boxed mixed payload recursively into a JSON slice
    emitter.instruction("jmp __rt_json_assoc_value_copy");                      // copy the encoded JSON value slice into concat_buf

    emitter.label("__rt_json_assoc_value_null");
    emitter.instruction("call __rt_json_encode_null");                          // encode null or unsupported payload families as the JSON null literal

    emitter.label("__rt_json_assoc_value_copy");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the current concat-buffer write pointer before copying the encoded JSON value bytes
    emitter.instruction("xor rcx, rcx");                                        // initialize the encoded-value copy index to the beginning of the returned JSON slice
    emitter.label("__rt_json_assoc_val_copy");
    emitter.instruction("cmp rcx, rdx");                                        // have we copied every byte of the returned encoded JSON value slice?
    emitter.instruction("jae __rt_json_assoc_val_done");                        // finish copying once the entire returned JSON value slice has been appended
    emitter.instruction("mov r10b, BYTE PTR [rax + rcx]");                      // load the next encoded JSON value byte from the returned slice
    emitter.instruction("mov BYTE PTR [r11 + rcx], r10b");                      // copy the encoded JSON value byte into concat_buf at the current write position
    emitter.instruction("add rcx, 1");                                          // advance the encoded-value copy index to the next byte
    emitter.instruction("jmp __rt_json_assoc_val_copy");                        // continue copying until the whole returned JSON value slice has been appended

    emitter.label("__rt_json_assoc_val_done");
    emitter.instruction("add r11, rdx");                                        // advance the concat-buffer write pointer by the copied encoded-value length
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer after appending the encoded JSON value
    emitter.instruction("add QWORD PTR [rbp - 40], 1");                         // increment the number of encoded associative-array entries after appending one full key/value pair
    emitter.instruction("jmp __rt_json_assoc_loop");                            // continue iterating the remaining associative-array entries in insertion order

    emitter.label("__rt_json_assoc_close");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the concat-buffer write pointer after the final encoded JSON entry
    // Write the closing bracket: ']' for list-shape, '}' otherwise.
    emitter.instruction("mov rdx, QWORD PTR [rbp - 96]");                       // reload the list_mode flag
    emitter.instruction("test rdx, rdx");                                       // check the current JSON object encoder condition
    emitter.instruction("jz __rt_json_assoc_close_obj_x");                      // 0 → object form
    emitter.instruction("mov BYTE PTR [r11], 93");                              // ASCII ']'
    emitter.instruction("jmp __rt_json_assoc_close_emit_x");                    // continue in the JSON object encoder control path
    emitter.label("__rt_json_assoc_close_obj_x");
    emitter.instruction("mov BYTE PTR [r11], 125");                             // ASCII '}'
    emitter.label("__rt_json_assoc_close_emit_x");
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the closing bracket
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // checkpoint the write pointer across the depth-exit helper call
    emitter.instruction("call __rt_json_depth_exit");                           // decrement _json_active_depth so a sibling encoder can re-enter cleanly
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the write pointer after the helper call
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the encoded-object start pointer in the leading x86_64 string result register
    emitter.instruction("mov rdx, r11");                                        // copy the final concat-buffer write pointer before turning it into a slice length
    emitter.instruction("sub rdx, rax");                                        // compute the final encoded-object length from write_end - write_start
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the global offset update
    emitter.instruction("mov rcx, r11");                                        // copy the final concat-buffer write pointer before converting it into an absolute offset
    emitter.instruction("sub rcx, r10");                                        // compute the new absolute concat-buffer offset after the encoded JSON object
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // publish the updated concat-buffer offset so later writers append after this JSON object
    emitter.instruction("add rsp, 112");                                        // release the local JSON-assoc scratch frame before returning to generated code
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code
    emitter.instruction("ret");                                                 // return the encoded JSON object slice in the x86_64 string result registers

    emit_assoc_is_list_shape_x86_64(emitter);
}

fn emit_assoc_is_list_shape_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_assoc_is_list_shape ---");
    emitter.label_global("__rt_json_assoc_is_list_shape");
    emitter.instruction("mov rdx, QWORD PTR [rax]");                            // rdx = count
    emitter.instruction("test rdx, rdx");                                       // empty hash?
    emitter.instruction("je __rt_json_assoc_is_list_shape_yes_x");              // empty → list shape (PHP encodes [])
    emitter.instruction("mov rcx, QWORD PTR [rax + 24]");                       // rcx = head index (insertion-order chain)
    emitter.instruction("cmp rcx, -1");                                         // sentinel for empty chain?
    emitter.instruction("je __rt_json_assoc_is_list_shape_no_x");               // count > 0 but no head → object form
    emitter.instruction("xor r8, r8");                                          // expected key value for this iteration
    emitter.label("__rt_json_assoc_is_list_shape_loop_x");
    emitter.instruction("lea r9, [rax + 40]");                                  // entries base = hash + 40 (header size)
    emitter.instruction("mov r10, rcx");                                        // copy entry index for shift
    emitter.instruction("shl r10, 6");                                          // entry index * 64 (entry size)
    emitter.instruction("add r9, r10");                                         // r9 = current entry pointer
    emitter.instruction("mov r11, QWORD PTR [r9 + 16]");                        // r11 = entry.key_len
    emitter.instruction("cmp r11, -1");                                         // is the key an integer (key_len == -1)?
    emitter.instruction("jne __rt_json_assoc_is_list_shape_no_x");              // string keys disqualify list shape
    emitter.instruction("mov r11, QWORD PTR [r9 + 8]");                         // r11 = entry.key_ptr (integer key payload)
    emitter.instruction("cmp r11, r8");                                         // does it match the expected sequential key?
    emitter.instruction("jne __rt_json_assoc_is_list_shape_no_x");              // mismatch → object form
    emitter.instruction("add r8, 1");                                           // expected_key++
    emitter.instruction("mov rcx, QWORD PTR [r9 + 56]");                        // rcx = entry.next
    emitter.instruction("cmp rcx, -1");                                         // hit the chain sentinel?
    emitter.instruction("jne __rt_json_assoc_is_list_shape_loop_x");            // continue scanning
    emitter.label("__rt_json_assoc_is_list_shape_yes_x");
    emitter.instruction("mov rax, 1");                                          // report list shape
    emitter.instruction("ret");                                                 // return from the JSON object encoder helper
    emitter.label("__rt_json_assoc_is_list_shape_no_x");
    emitter.instruction("mov rax, 0");                                          // report object form
    emitter.instruction("ret");                                                 // return from the JSON object encoder helper
}
