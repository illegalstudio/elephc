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
    //   [sp + 112]     = list_possible flag (1 = can compact object form to JSON array form)
    //   [sp + 120]     = saved x19 (active JSON flags cache)
    emitter.instruction("sub sp, sp, #128");                                    // allocate 128 bytes including list-shape tracking and saved x19
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // set new frame pointer
    emitter.instruction("str x19, [sp, #120]");                                 // preserve caller x19 before caching active JSON flags

    // -- save hash table pointer --
    emitter.instruction("str x0, [sp, #0]");                                    // save hash ptr

    // -- initialize list-shape tracking. Associative arrays are emitted in
    //    object form first; if every key is the sequential integer key
    //    0..count-1, the finished buffer is compacted in-place to `[...]`.
    //    JSON_FORCE_OBJECT disables the compaction path. --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x19, [x9]");                                       // cache the active flag bitmask for the whole associative-array encode
    emitter.instruction("tst x19, #16");                                        // is JSON_FORCE_OBJECT (bit 16) set?
    emitter.instruction("b.ne __rt_json_assoc_force_object_mode");              // FORCE_OBJECT wins → keep object form
    emitter.instruction("mov x0, #1");                                          // start optimistic: keys may still be list-shaped
    emitter.instruction("str x0, [sp, #112]");                                  // park the list_possible flag for the loop and close
    emitter.instruction("b __rt_json_assoc_after_list_check");                  // continue after initializing list tracking
    emitter.label("__rt_json_assoc_force_object_mode");
    emitter.instruction("str xzr, [sp, #112]");                                 // FORCE_OBJECT path never compacts to array form

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

    // -- write opening brace; list-shape arrays rewrite it to '[' after the walk --
    emitter.instruction("mov w12, #123");                                       // ASCII '{'
    emitter.instruction("strb w12, [x11]");                                     // write the provisional opening brace
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos
    emitter.instruction("bl __rt_json_pretty_push");                            // enter one pretty-print indentation level after the container opens

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

    // -- fold list-shape detection into the main hash walk --
    emitter.instruction("ldr x6, [sp, #112]");                                  // reload the optimistic list_possible flag
    emitter.instruction("cbz x6, __rt_json_assoc_list_shape_checked");          // once false, skip further key checks
    emitter.instruction("ldr x6, [sp, #56]");                                   // load key length for the current entry
    emitter.instruction("cmn x6, #1");                                          // is this an integer key?
    emitter.instruction("b.ne __rt_json_assoc_list_shape_clear");               // string keys force object form
    emitter.instruction("ldr x6, [sp, #48]");                                   // load the integer key payload
    emitter.instruction("ldr x7, [sp, #40]");                                   // load the expected sequential key
    emitter.instruction("cmp x6, x7");                                          // does the key match the insertion-order index?
    emitter.instruction("b.eq __rt_json_assoc_list_shape_checked");             // matching key keeps list compaction possible
    emitter.label("__rt_json_assoc_list_shape_clear");
    emitter.instruction("str xzr, [sp, #112]");                                 // mark the encoded object as non-list-shaped
    emitter.label("__rt_json_assoc_list_shape_checked");

    // -- add comma if not first entry --
    emitter.instruction("ldr x5, [sp, #40]");                                   // load items written
    emitter.instruction("cbz x5, __rt_json_assoc_key");                         // skip comma for first
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("mov w12, #44");                                        // ASCII ','
    emitter.instruction("strb w12, [x11]");                                     // write ','
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // save write pos

    // -- write key as a quoted JSON string --
    // String keys are encoded through __rt_json_encode_str so every JSON
    // escape (control bytes, quotes, backslashes, multibyte UTF-8, the active
    // flag set) is honored on the key. Integer keys are formatted through
    // itoa and copied into place inline because their decimal representation
    // never contains JSON-significant bytes. If the completed object is still
    // list-shaped, this key prefix is removed by the compaction pass.
    emitter.label("__rt_json_assoc_key");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos before optional pretty indentation
    emitter.instruction("bl __rt_json_pretty_line");                            // append newline and indentation for this entry when pretty-printing
    emitter.instruction("str x11, [sp, #16]");                                  // save the write pos after any pretty indentation
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
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x12, x11, x10");                                   // compute scratch-safe concat offset from the current key write position
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x12, [x9]");                                       // move itoa scratch after the pretty-printed key prefix
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
    emitter.instruction("bl __rt_json_pretty_colon_space");                     // append the pretty-print key/value space when requested
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

    // -- write closing brace; list-shape arrays rewrite it to ']' after the walk --
    emitter.label("__rt_json_assoc_close");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload write pos
    emitter.instruction("bl __rt_json_pretty_pop");                             // leave the container indentation level before closing it
    emitter.instruction("ldr x5, [sp, #40]");                                   // reload written item count to decide whether closing needs its own line
    emitter.instruction("cbz x5, __rt_json_assoc_close_choose");                // empty containers stay compact even under JSON_PRETTY_PRINT
    emitter.instruction("bl __rt_json_pretty_line");                            // append the closing-line indentation for non-empty pretty containers
    emitter.label("__rt_json_assoc_close_choose");
    emitter.instruction("mov w12, #125");                                       // ASCII '}'
    emitter.instruction("strb w12, [x11]");                                     // write the provisional closing brace
    emitter.instruction("add x11, x11, #1");                                    // advance
    emitter.instruction("str x11, [sp, #16]");                                  // checkpoint the write pointer across the depth-exit helper call
    emitter.instruction("bl __rt_json_depth_exit");                             // decrement _json_active_depth so a sibling encoder can re-enter cleanly
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the write pointer after the helper call

    // -- compact provisional object output to array output when the keys were 0..n-1 --
    emitter.instruction("ldr x12, [sp, #112]");                                 // reload the final list_possible flag
    emitter.instruction("cbz x12, __rt_json_assoc_compute_result");             // non-list objects keep their provisional object bytes
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = output start pointer
    emitter.instruction("mov w12, #91");                                        // ASCII '['
    emitter.instruction("strb w12, [x0]");                                      // rewrite the opening brace to an opening bracket
    emitter.instruction("add x9, x0, #1");                                      // read cursor starts after the opening byte
    emitter.instruction("add x10, x0, #1");                                     // write cursor starts after the opening byte
    emitter.instruction("ldrb w12, [x9]");                                      // inspect the first byte after the opening brace
    emitter.instruction("cmp w12, #125");                                       // was the object empty?
    emitter.instruction("b.eq __rt_json_assoc_compact_close");                  // empty object compacts directly to []
    // The provisional object can already contain pretty-print whitespace.
    // Copy whitespace before each generated `"index":` key, then drop the
    // key prefix itself so pretty list-shape output remains an array layout.
    emitter.label("__rt_json_assoc_compact_key");
    emitter.instruction("ldrb w12, [x9]");                                      // inspect the byte before the skipped key prefix
    emitter.instruction("cmp w12, #125");                                       // did pretty output reach the closing object brace?
    emitter.instruction("b.eq __rt_json_assoc_compact_close");                  // replace the object close with an array close
    emitter.instruction("cmp w12, #34");                                        // reached the opening quote of the integer key?
    emitter.instruction("b.eq __rt_json_assoc_compact_key_start");              // skip the generated key prefix itself
    emitter.instruction("strb w12, [x10]");                                     // preserve pretty-print whitespace before the array value
    emitter.instruction("add x9, x9, #1");                                      // advance the read cursor over the copied whitespace
    emitter.instruction("add x10, x10, #1");                                    // advance the compacted write cursor
    emitter.instruction("b __rt_json_assoc_compact_key");                       // keep copying whitespace until the generated key starts
    emitter.label("__rt_json_assoc_compact_key_start");
    emitter.instruction("add x9, x9, #1");                                      // skip the opening quote of the integer key
    emitter.label("__rt_json_assoc_compact_key_scan");
    emitter.instruction("ldrb w12, [x9]");                                      // load the next key byte
    emitter.instruction("add x9, x9, #1");                                      // advance through the key byte
    emitter.instruction("cmp w12, #34");                                        // reached the closing key quote?
    emitter.instruction("b.ne __rt_json_assoc_compact_key_scan");               // keep skipping key digits
    emitter.instruction("add x9, x9, #1");                                      // skip the colon after the key
    // Pretty object output inserts a key/value space after `:`, but the
    // compacted array form has no key/value separator. Drop only that local
    // separator; value-owned spaces still copy through the value scanner.
    emitter.label("__rt_json_assoc_compact_value_ws");
    emitter.instruction("ldrb w12, [x9]");                                      // inspect whitespace between the object colon and value
    emitter.instruction("cmp w12, #32");                                        // pretty-print object form inserts a single space after `:`
    emitter.instruction("b.ne __rt_json_assoc_compact_value_init");             // first non-space byte belongs to the value
    emitter.instruction("add x9, x9, #1");                                      // drop the key/value separator space for array output
    emitter.instruction("b __rt_json_assoc_compact_value_ws");                  // continue through any repeated separator spaces
    emitter.label("__rt_json_assoc_compact_value_init");
    emitter.instruction("mov x13, #0");                                         // nested container depth = 0
    emitter.instruction("mov x14, #0");                                         // string-mode flag = false
    // Commas and braces only delimit the synthetic top-level object while
    // depth == 0 and we are not inside a JSON string. Nested arrays/objects
    // and string contents must be copied verbatim into the compacted value.
    emitter.label("__rt_json_assoc_compact_value");
    emitter.instruction("ldrb w12, [x9]");                                      // load the next value/delimiter byte
    emitter.instruction("cbnz x14, __rt_json_assoc_compact_in_string");         // strings copy bytes until an unescaped quote
    emitter.instruction("cbnz x13, __rt_json_assoc_compact_not_delim");         // nested containers own their commas/braces
    emitter.instruction("cmp w12, #44");                                        // top-level comma after a value?
    emitter.instruction("b.eq __rt_json_assoc_compact_comma");                  // keep the comma and start the next key
    emitter.instruction("cmp w12, #125");                                       // top-level object close after the final value?
    emitter.instruction("b.eq __rt_json_assoc_compact_close");                  // replace it with the array close
    emitter.label("__rt_json_assoc_compact_not_delim");
    emitter.instruction("cmp w12, #34");                                        // value string opening quote?
    emitter.instruction("b.eq __rt_json_assoc_compact_string_open");            // enter string-copy mode
    emitter.instruction("cmp w12, #91");                                        // nested array opening bracket?
    emitter.instruction("b.eq __rt_json_assoc_compact_depth_inc");              // enter one nested container level
    emitter.instruction("cmp w12, #123");                                       // nested object opening brace?
    emitter.instruction("b.eq __rt_json_assoc_compact_depth_inc");              // enter one nested container level
    emitter.instruction("cmp w12, #93");                                        // nested array closing bracket?
    emitter.instruction("b.eq __rt_json_assoc_compact_depth_dec");              // leave one nested container level
    emitter.instruction("cmp w12, #125");                                       // nested object closing brace?
    emitter.instruction("b.eq __rt_json_assoc_compact_depth_dec");              // leave one nested container level
    emitter.label("__rt_json_assoc_compact_copy");
    emitter.instruction("strb w12, [x10]");                                     // copy the value byte to the compacted output
    emitter.instruction("add x9, x9, #1");                                      // advance the read cursor
    emitter.instruction("add x10, x10, #1");                                    // advance the write cursor
    emitter.instruction("b __rt_json_assoc_compact_value");                     // continue copying the current value
    emitter.label("__rt_json_assoc_compact_string_open");
    emitter.instruction("mov x14, #1");                                         // remember that subsequent bytes are inside a JSON string
    emitter.instruction("b __rt_json_assoc_compact_copy");                      // copy the opening quote
    emitter.label("__rt_json_assoc_compact_depth_inc");
    emitter.instruction("add x13, x13, #1");                                    // increment nested container depth
    emitter.instruction("b __rt_json_assoc_compact_copy");                      // copy the nested opening delimiter
    emitter.label("__rt_json_assoc_compact_depth_dec");
    emitter.instruction("sub x13, x13, #1");                                    // decrement nested container depth
    emitter.instruction("b __rt_json_assoc_compact_copy");                      // copy the nested closing delimiter
    emitter.label("__rt_json_assoc_compact_in_string");
    emitter.instruction("strb w12, [x10]");                                     // copy the string byte
    emitter.instruction("add x9, x9, #1");                                      // advance the read cursor
    emitter.instruction("add x10, x10, #1");                                    // advance the write cursor
    emitter.instruction("cmp w12, #92");                                        // escaped JSON string byte?
    emitter.instruction("b.eq __rt_json_assoc_compact_string_escape");          // copy the escaped byte without interpreting it
    emitter.instruction("cmp w12, #34");                                        // unescaped closing quote?
    emitter.instruction("b.ne __rt_json_assoc_compact_value");                  // stay inside the string
    emitter.instruction("mov x14, #0");                                         // leave string mode after the closing quote
    emitter.instruction("b __rt_json_assoc_compact_value");                     // continue scanning after the string
    emitter.label("__rt_json_assoc_compact_string_escape");
    emitter.instruction("ldrb w12, [x9]");                                      // load the escaped byte after the backslash
    emitter.instruction("strb w12, [x10]");                                     // copy the escaped byte verbatim
    emitter.instruction("add x9, x9, #1");                                      // advance past the escaped byte
    emitter.instruction("add x10, x10, #1");                                    // advance the compacted write cursor
    emitter.instruction("b __rt_json_assoc_compact_value");                     // resume string scanning
    emitter.label("__rt_json_assoc_compact_comma");
    emitter.instruction("strb w12, [x10]");                                     // keep the comma between compacted array elements
    emitter.instruction("add x9, x9, #1");                                      // advance past the comma in the object form
    emitter.instruction("add x10, x10, #1");                                    // advance past the comma in the array form
    emitter.instruction("b __rt_json_assoc_compact_key");                       // compact the next key/value pair
    emitter.label("__rt_json_assoc_compact_close");
    emitter.instruction("mov w12, #93");                                        // ASCII ']'
    emitter.instruction("strb w12, [x10]");                                     // write the closing array bracket
    emitter.instruction("add x10, x10, #1");                                    // advance past the closing array bracket
    emitter.instruction("mov x11, x10");                                        // x11 = compacted write end
    emitter.instruction("str x11, [sp, #16]");                                  // persist the compacted write end for concat_off

    emitter.label("__rt_json_assoc_compute_result");

    // -- compute result --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = output start
    emitter.instruction("sub x2, x11, x1");                                     // x2 = total length

    // -- update concat_off --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x10, x11, x10");                                   // compute the absolute concat offset after the closing bracket
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- tear down and return --
    emitter.instruction("ldr x19, [sp, #120]");                                 // restore caller x19 after using it as the flag cache
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_json_encode_assoc_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_assoc ---");
    emitter.label_global("__rt_json_encode_assoc");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving JSON-assoc scratch space
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for hash iteration state and concat-buffer cursors
    emitter.instruction("sub rsp, 112");                                        // reserve local slots including list-shape tracking and saved r15
    emitter.instruction("mov QWORD PTR [rbp - 104], r15");                      // preserve caller r15 before caching active JSON flags
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source associative-array pointer across nested JSON helper calls

    // Track list-shape while the object form is emitted. If every key is the
    // sequential integer key 0..count-1, the finished buffer is compacted
    // from {"0":...} to [...]. JSON_FORCE_OBJECT disables that compaction.
    emitter.instruction("mov r15, QWORD PTR [rip + _json_active_flags]");       // cache the active flag bitmask for the whole associative-array encode
    emitter.instruction("test r15, 16");                                        // is JSON_FORCE_OBJECT (bit 16) set?
    emitter.instruction("jne __rt_json_assoc_force_object_mode_x");             // FORCE_OBJECT wins → keep object form
    emitter.instruction("mov QWORD PTR [rbp - 96], 1");                         // start optimistic: keys may still be list-shaped
    emitter.instruction("jmp __rt_json_assoc_after_list_check_x");              // continue after initializing list tracking
    emitter.label("__rt_json_assoc_force_object_mode_x");
    emitter.instruction("mov QWORD PTR [rbp - 96], 0");                         // FORCE_OBJECT path never compacts to array form

    emitter.label("__rt_json_assoc_after_list_check_x");
    // Enter the recursion-depth check before any output is produced.
    emitter.instruction("call __rt_json_depth_enter");                          // increment _json_active_depth and throw on overflow when requested

    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer offset before appending the JSON object
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the current JSON append
    emitter.instruction("add r11, r10");                                        // compute the current concat-buffer write pointer from the base plus offset
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the encoded-object start pointer for the final result slice
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the current concat-buffer write pointer for the hash iteration loop
    // Write the opening brace; list-shape arrays rewrite it to '[' after the walk.
    emitter.instruction("mov BYTE PTR [r11], 123");                             // ASCII '{'
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the opening bracket
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the updated write pointer before entering the hash iteration loop
    emitter.instruction("call __rt_json_pretty_push");                          // enter one pretty-print indentation level after the container opens
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
    emitter.instruction("cmp QWORD PTR [rbp - 96], 0");                         // is list-shape compaction still possible?
    emitter.instruction("je __rt_json_assoc_list_shape_checked_x");             // skip key checks after the first mismatch
    emitter.instruction("cmp QWORD PTR [rbp - 56], -1");                        // is this an integer key?
    emitter.instruction("jne __rt_json_assoc_list_shape_clear_x");              // string keys force object form
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // load the integer key payload
    emitter.instruction("cmp r10, QWORD PTR [rbp - 40]");                       // does it match the insertion-order index?
    emitter.instruction("je __rt_json_assoc_list_shape_checked_x");             // matching key keeps list compaction possible
    emitter.label("__rt_json_assoc_list_shape_clear_x");
    emitter.instruction("mov QWORD PTR [rbp - 96], 0");                         // mark the encoded object as non-list-shaped
    emitter.label("__rt_json_assoc_list_shape_checked_x");
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
    // copied inline into the JSON output position bracketed by quotes. If
    // the completed object is still list-shaped, this key prefix is removed
    // by the compaction pass.
    emitter.label("__rt_json_assoc_key");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload write pos before optional pretty indentation
    emitter.instruction("call __rt_json_pretty_line");                          // append newline and indentation for this entry when pretty-printing
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the write pos after any pretty indentation
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
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base before positioning itoa scratch
    emitter.instruction("mov rcx, r11");                                        // copy the current key write pointer for the concat-offset calculation
    emitter.instruction("sub rcx, r10");                                        // compute scratch-safe concat offset from the current key write position
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // move itoa scratch after the pretty-printed key prefix
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
    emitter.instruction("call __rt_json_pretty_colon_space");                   // append the pretty-print key/value space when requested
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
    // Write the closing brace; list-shape arrays rewrite it to ']' after the walk.
    emitter.instruction("call __rt_json_pretty_pop");                           // leave the container indentation level before closing it
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // did the container contain any entries?
    emitter.instruction("je __rt_json_assoc_close_choose_x");                   // empty containers stay compact even under JSON_PRETTY_PRINT
    emitter.instruction("call __rt_json_pretty_line");                          // append the closing-line indentation for non-empty pretty containers
    emitter.label("__rt_json_assoc_close_choose_x");
    emitter.instruction("mov BYTE PTR [r11], 125");                             // ASCII '}'
    emitter.instruction("add r11, 1");                                          // advance the concat-buffer write pointer past the closing bracket
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // checkpoint the write pointer across the depth-exit helper call
    emitter.instruction("call __rt_json_depth_exit");                           // decrement _json_active_depth so a sibling encoder can re-enter cleanly
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the write pointer after the helper call
    emitter.instruction("cmp QWORD PTR [rbp - 96], 0");                         // was every emitted key sequential?
    emitter.instruction("je __rt_json_assoc_compute_result_x");                 // non-list objects keep their provisional object bytes
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // rax = output start pointer
    emitter.instruction("mov BYTE PTR [rax], 91");                              // rewrite the opening brace to an opening bracket
    emitter.instruction("lea r10, [rax + 1]");                                  // read cursor starts after the opening byte
    emitter.instruction("lea r11, [rax + 1]");                                  // write cursor starts after the opening byte
    emitter.instruction("cmp BYTE PTR [r10], 125");                             // was the object empty?
    emitter.instruction("je __rt_json_assoc_compact_close_x");                  // empty object compacts directly to []
    // The provisional object can already contain pretty-print whitespace.
    // Copy whitespace before each generated `"index":` key, then drop the
    // key prefix itself so pretty list-shape output remains an array layout.
    emitter.label("__rt_json_assoc_compact_key_x");
    emitter.instruction("mov r8b, BYTE PTR [r10]");                             // inspect the byte before the skipped key prefix
    emitter.instruction("cmp r8b, 125");                                        // did pretty output reach the closing object brace?
    emitter.instruction("je __rt_json_assoc_compact_close_x");                  // replace the object close with an array close
    emitter.instruction("cmp r8b, 34");                                         // reached the opening quote of the integer key?
    emitter.instruction("je __rt_json_assoc_compact_key_start_x");              // skip the generated key prefix itself
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // preserve pretty-print whitespace before the array value
    emitter.instruction("add r10, 1");                                          // advance the read cursor over the copied whitespace
    emitter.instruction("add r11, 1");                                          // advance the compacted write cursor
    emitter.instruction("jmp __rt_json_assoc_compact_key_x");                   // keep copying whitespace until the generated key starts
    emitter.label("__rt_json_assoc_compact_key_start_x");
    emitter.instruction("add r10, 1");                                          // skip the opening quote of the integer key
    emitter.label("__rt_json_assoc_compact_key_scan_x");
    emitter.instruction("mov r8b, BYTE PTR [r10]");                             // load the next key byte
    emitter.instruction("add r10, 1");                                          // advance through the key byte
    emitter.instruction("cmp r8b, 34");                                         // reached the closing key quote?
    emitter.instruction("jne __rt_json_assoc_compact_key_scan_x");              // keep skipping key digits
    emitter.instruction("add r10, 1");                                          // skip the colon after the key
    // Pretty object output inserts a key/value space after `:`, but the
    // compacted array form has no key/value separator. Drop only that local
    // separator; value-owned spaces still copy through the value scanner.
    emitter.label("__rt_json_assoc_compact_value_ws_x");
    emitter.instruction("mov r8b, BYTE PTR [r10]");                             // inspect whitespace between the object colon and value
    emitter.instruction("cmp r8b, 32");                                         // pretty-print object form inserts a single space after `:`
    emitter.instruction("jne __rt_json_assoc_compact_value_init_x");            // first non-space byte belongs to the value
    emitter.instruction("add r10, 1");                                          // drop the key/value separator space for array output
    emitter.instruction("jmp __rt_json_assoc_compact_value_ws_x");              // continue through any repeated separator spaces
    emitter.label("__rt_json_assoc_compact_value_init_x");
    emitter.instruction("xor ecx, ecx");                                        // nested container depth = 0
    emitter.instruction("xor r9d, r9d");                                        // string-mode flag = false
    // Commas and braces only delimit the synthetic top-level object while
    // depth == 0 and we are not inside a JSON string. Nested arrays/objects
    // and string contents must be copied verbatim into the compacted value.
    emitter.label("__rt_json_assoc_compact_value_x");
    emitter.instruction("mov r8b, BYTE PTR [r10]");                             // load the next value/delimiter byte
    emitter.instruction("test r9, r9");                                         // currently inside a JSON string?
    emitter.instruction("jnz __rt_json_assoc_compact_in_string_x");             // strings copy bytes until an unescaped quote
    emitter.instruction("test rcx, rcx");                                       // currently inside a nested container?
    emitter.instruction("jnz __rt_json_assoc_compact_not_delim_x");             // nested containers own their commas/braces
    emitter.instruction("cmp r8b, 44");                                         // top-level comma after a value?
    emitter.instruction("je __rt_json_assoc_compact_comma_x");                  // keep the comma and start the next key
    emitter.instruction("cmp r8b, 125");                                        // top-level object close after the final value?
    emitter.instruction("je __rt_json_assoc_compact_close_x");                  // replace it with the array close
    emitter.label("__rt_json_assoc_compact_not_delim_x");
    emitter.instruction("cmp r8b, 34");                                         // value string opening quote?
    emitter.instruction("je __rt_json_assoc_compact_string_open_x");            // enter string-copy mode
    emitter.instruction("cmp r8b, 91");                                         // nested array opening bracket?
    emitter.instruction("je __rt_json_assoc_compact_depth_inc_x");              // enter one nested container level
    emitter.instruction("cmp r8b, 123");                                        // nested object opening brace?
    emitter.instruction("je __rt_json_assoc_compact_depth_inc_x");              // enter one nested container level
    emitter.instruction("cmp r8b, 93");                                         // nested array closing bracket?
    emitter.instruction("je __rt_json_assoc_compact_depth_dec_x");              // leave one nested container level
    emitter.instruction("cmp r8b, 125");                                        // nested object closing brace?
    emitter.instruction("je __rt_json_assoc_compact_depth_dec_x");              // leave one nested container level
    emitter.label("__rt_json_assoc_compact_copy_x");
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // copy the value byte to the compacted output
    emitter.instruction("add r10, 1");                                          // advance the read cursor
    emitter.instruction("add r11, 1");                                          // advance the write cursor
    emitter.instruction("jmp __rt_json_assoc_compact_value_x");                 // continue copying the current value
    emitter.label("__rt_json_assoc_compact_string_open_x");
    emitter.instruction("mov r9, 1");                                           // remember that subsequent bytes are inside a JSON string
    emitter.instruction("jmp __rt_json_assoc_compact_copy_x");                  // copy the opening quote
    emitter.label("__rt_json_assoc_compact_depth_inc_x");
    emitter.instruction("add rcx, 1");                                          // increment nested container depth
    emitter.instruction("jmp __rt_json_assoc_compact_copy_x");                  // copy the nested opening delimiter
    emitter.label("__rt_json_assoc_compact_depth_dec_x");
    emitter.instruction("sub rcx, 1");                                          // decrement nested container depth
    emitter.instruction("jmp __rt_json_assoc_compact_copy_x");                  // copy the nested closing delimiter
    emitter.label("__rt_json_assoc_compact_in_string_x");
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // copy the string byte
    emitter.instruction("add r10, 1");                                          // advance the read cursor
    emitter.instruction("add r11, 1");                                          // advance the write cursor
    emitter.instruction("cmp r8b, 92");                                         // escaped JSON string byte?
    emitter.instruction("je __rt_json_assoc_compact_string_escape_x");          // copy the escaped byte without interpreting it
    emitter.instruction("cmp r8b, 34");                                         // unescaped closing quote?
    emitter.instruction("jne __rt_json_assoc_compact_value_x");                 // stay inside the string
    emitter.instruction("xor r9d, r9d");                                        // leave string mode after the closing quote
    emitter.instruction("jmp __rt_json_assoc_compact_value_x");                 // continue scanning after the string
    emitter.label("__rt_json_assoc_compact_string_escape_x");
    emitter.instruction("mov r8b, BYTE PTR [r10]");                             // load the escaped byte after the backslash
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // copy the escaped byte verbatim
    emitter.instruction("add r10, 1");                                          // advance past the escaped byte
    emitter.instruction("add r11, 1");                                          // advance the compacted write cursor
    emitter.instruction("jmp __rt_json_assoc_compact_value_x");                 // resume string scanning
    emitter.label("__rt_json_assoc_compact_comma_x");
    emitter.instruction("mov BYTE PTR [r11], r8b");                             // keep the comma between compacted array elements
    emitter.instruction("add r10, 1");                                          // advance past the comma in the object form
    emitter.instruction("add r11, 1");                                          // advance past the comma in the array form
    emitter.instruction("jmp __rt_json_assoc_compact_key_x");                   // compact the next key/value pair
    emitter.label("__rt_json_assoc_compact_close_x");
    emitter.instruction("mov BYTE PTR [r11], 93");                              // write the closing array bracket
    emitter.instruction("add r11, 1");                                          // advance past the closing array bracket
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // persist the compacted write end for concat_off

    emitter.label("__rt_json_assoc_compute_result_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the encoded-object start pointer in the leading x86_64 string result register
    emitter.instruction("mov rdx, r11");                                        // copy the final concat-buffer write pointer before turning it into a slice length
    emitter.instruction("sub rdx, rax");                                        // compute the final encoded-object length from write_end - write_start
    emitter.instruction("lea r10, [rip + _concat_buf]");                        // materialize the concat-buffer base pointer for the global offset update
    emitter.instruction("mov rcx, r11");                                        // copy the final concat-buffer write pointer before converting it into an absolute offset
    emitter.instruction("sub rcx, r10");                                        // compute the new absolute concat-buffer offset after the encoded JSON object
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // publish the updated concat-buffer offset so later writers append after this JSON object
    emitter.instruction("mov r15, QWORD PTR [rbp - 104]");                      // restore caller r15 after using it as the flag cache
    emitter.instruction("add rsp, 112");                                        // release the local JSON-assoc scratch frame before returning to generated code
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code
    emitter.instruction("ret");                                                 // return the encoded JSON object slice in the x86_64 string result registers
}
