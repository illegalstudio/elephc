//! Purpose:
//! Emits `__rt_json_decode_count`, the count-only fast path selected when
//! `count(json_decode($s, true))` is the sole observation of the decoded graph.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via the system runtime section.
//!
//! Key details:
//! - `__rt_json_validate` owns syntax, depth, UTF-8 and escape validation.
//! - A valid top-level array is counted by skipping its values; a valid top-level object is
//!   counted by deduplicating plain (escape-free, non-numeric-looking) keys.
//! - Codes: >= 0 count, -1 invalid JSON (null), -2 int, -3 float, -4 string, -5 false,
//!   -6 true, -7 null, -8 fallback to the full decoder (numeric roots and unsupported keys).
//! - Every supported target implements the same contract; the fallback path keeps exact
//!   `count()` TypeError and JSON error behaviour.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the count-only decoder helper for the active target.
pub(crate) fn emit_json_decode_count(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }
    emit_aarch64(emitter);
}

/// Emits the AArch64 helper.
fn emit_aarch64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: json_decode_count ---");
    e.label_global("__rt_json_decode_count");
    e.instruction("sub sp, sp, #1152");                                         // reserve locals, the key table, and saved callee registers
    e.instruction("add x9, sp, #1120");                                         // address the linkage pair at the top of the frame
    e.instruction("stp x29, x30, [x9]");                                        // save frame pointer and return address
    e.instruction("add x29, sp, #1120");                                        // establish a stable frame pointer
    e.instruction("str x19, [sp, #40]");                                        // preserve the callee-saved source pointer
    e.instruction("str x20, [sp, #48]");                                        // preserve the callee-saved source length
    e.instruction("str x21, [sp, #56]");                                        // preserve the callee-saved cursor
    e.instruction("str x22, [sp, #64]");                                        // preserve the callee-saved count
    e.instruction("str x23, [sp, #72]");                                        // preserve the callee-saved unique-key count
    e.instruction("str x24, [sp, #80]");                                        // preserve the callee-saved key-table base
    e.instruction("str x1, [sp, #0]");                                          // save the JSON source pointer
    e.instruction("str x2, [sp, #8]");                                          // save the JSON source length
    emit_state_reset_aarch64(e);
    e.instruction("ldr x1, [sp, #0]");                                          // reload the source pointer for validation
    e.instruction("ldr x2, [sp, #8]");                                          // reload the source length for validation
    e.instruction("bl __rt_json_validate");                                     // validate syntax, depth and UTF-8 without building values
    e.instruction("cbz x0, __rt_json_count_fail");                              // invalid JSON falls back to the exact null TypeError
    e.instruction("ldr x19, [sp, #0]");                                         // source pointer for the count scan
    e.instruction("ldr x20, [sp, #8]");                                         // source length for the count scan
    e.instruction("mov x21, xzr");                                              // scan cursor starts at the source beginning
    e.instruction("bl __rt_json_count_skip_ws");                                // skip leading RFC 8259 whitespace
    e.instruction("cmp x21, x20");                                              // is the source empty after trimming?
    e.instruction("b.ge __rt_json_count_fail");                                 // an empty source is invalid JSON
    e.instruction("ldrb w9, [x19, x21]");                                       // inspect the root token
    e.instruction("cmp w9, #91");                                               // '[' starts a top-level array
    e.instruction("b.eq __rt_json_count_array");                                // count the array elements
    e.instruction("cmp w9, #123");                                              // '{' starts a top-level object
    e.instruction("b.eq __rt_json_count_object");                               // count the unique object keys
    e.instruction("cmp w9, #34");                                               // '"' is a valid string root
    e.instruction("b.eq __rt_json_count_string");                               // report the count() string TypeError
    e.instruction("cmp w9, #116");                                              // 't' is a valid true literal
    e.instruction("b.eq __rt_json_count_true");                                 // report the count() true TypeError
    e.instruction("cmp w9, #102");                                              // 'f' is a valid false literal
    e.instruction("b.eq __rt_json_count_false");                                // report the count() false TypeError
    e.instruction("cmp w9, #110");                                              // 'n' is a valid null literal
    e.instruction("b.eq __rt_json_count_null");                                 // report the count() null TypeError
    e.instruction("b __rt_json_count_fallback");                                // numbers use the exact decoder for int/float typing

    e.label("__rt_json_count_array");
    e.instruction("add x21, x21, #1");                                          // consume the opening bracket
    e.instruction("mov x22, xzr");                                              // no elements counted yet
    e.instruction("bl __rt_json_count_skip_ws");                                // skip whitespace before a possible empty-array close
    e.instruction("cmp x21, x20");                                              // did the source end inside the array?
    e.instruction("b.ge __rt_json_count_fail");                                 // validated input cannot end here
    e.instruction("ldrb w9, [x19, x21]");                                       // inspect the first element or closing bracket
    e.instruction("cmp w9, #93");                                               // ']' is an empty array
    e.instruction("b.eq __rt_json_count_array_done");                           // report a zero element count
    e.label("__rt_json_count_array_loop");
    e.instruction("add x22, x22, #1");                                          // count the next element
    e.instruction("bl __rt_json_count_skip_value");                             // skip the complete element value
    e.instruction("bl __rt_json_count_skip_ws");                                // skip whitespace before the separator
    e.instruction("cmp x21, x20");                                              // did the source end after the element?
    e.instruction("b.ge __rt_json_count_fail");                                 // validated input cannot end here
    e.instruction("ldrb w9, [x19, x21]");                                       // inspect the separator
    e.instruction("cmp w9, #44");                                               // ',' continues the array
    e.instruction("b.eq __rt_json_count_array_comma");                          // consume the comma and continue
    e.instruction("cmp w9, #93");                                               // ']' closes the array
    e.instruction("b.eq __rt_json_count_array_done");                           // report the element count
    e.instruction("b __rt_json_count_fail");                                    // validated input cannot reach another byte
    e.label("__rt_json_count_array_comma");
    e.instruction("add x21, x21, #1");                                          // consume the comma
    e.instruction("bl __rt_json_count_skip_ws");                                // skip whitespace before the next element
    e.instruction("b __rt_json_count_array_loop");                              // count the next element
    e.label("__rt_json_count_array_done");
    e.instruction("mov x0, x22");                                               // return the top-level element count
    e.instruction("b __rt_json_count_out");                                     // leave through the shared epilogue

    e.label("__rt_json_count_object");
    e.instruction("add x21, x21, #1");                                          // consume the opening brace
    e.instruction("mov x23, xzr");                                              // no unique keys counted yet
    e.instruction("add x24, sp, #96");                                          // key table base for up to 64 plain keys
    e.instruction("bl __rt_json_count_skip_ws");                                // skip whitespace before a possible empty-object close
    e.instruction("cmp x21, x20");                                              // did the source end inside the object?
    e.instruction("b.ge __rt_json_count_fail");                                 // validated input cannot end here
    e.instruction("ldrb w9, [x19, x21]");                                       // inspect the first key or closing brace
    e.instruction("cmp w9, #125");                                              // '}' is an empty object
    e.instruction("b.eq __rt_json_count_object_done");                          // report a zero unique-key count
    e.label("__rt_json_count_object_loop");
    e.instruction("ldrb w9, [x19, x21]");                                       // the next member must start with a quote
    e.instruction("cmp w9, #34");                                               // is the key a JSON string?
    e.instruction("b.ne __rt_json_count_fallback");                             // a malformed key falls back to the exact decoder
    e.instruction("add x10, x21, #1");                                          // key_start points past the opening quote
    e.instruction("mov x12, x10");                                              // key scan cursor
    e.label("__rt_json_count_key_scan");
    e.instruction("cmp x12, x20");                                              // did the key run past the source?
    e.instruction("b.ge __rt_json_count_fail");                                 // validated input cannot end here
    e.instruction("ldrb w9, [x19, x12]");                                       // load the next key byte
    e.instruction("cmp w9, #92");                                               // an escaped key needs the full decoder
    e.instruction("b.eq __rt_json_count_fallback");                             // fall back for escaped keys
    e.instruction("cmp w9, #34");                                               // is this the closing key quote?
    e.instruction("b.eq __rt_json_count_key_end");                              // finish the plain key
    e.instruction("add x12, x12, #1");                                          // advance the key scan
    e.instruction("b __rt_json_count_key_scan");                                // inspect the next key byte
    e.label("__rt_json_count_key_end");
    e.instruction("str x12, [sp, #88]");                                        // preserve the key end across the dedupe byte comparison
    e.instruction("sub x13, x12, x10");                                         // key byte length
    e.instruction("cbz x13, __rt_json_count_key_plain");                        // an empty key is not numeric
    e.instruction("ldrb w9, [x19, x10]");                                       // inspect the first key byte
    e.instruction("cmp w9, #45");                                               // a leading '-' may normalize to an integer key
    e.instruction("b.eq __rt_json_count_fallback");                             // fall back for numeric-looking keys
    e.instruction("sub w9, w9, #48");                                           // rebase a leading digit onto 0..9
    e.instruction("cmp w9, #9");                                                // does the key start with a digit?
    e.instruction("b.ls __rt_json_count_fallback");                             // fall back for numeric-looking keys
    e.label("__rt_json_count_key_plain");
    e.instruction("mov x14, xzr");                                              // previous-key index
    e.label("__rt_json_count_key_dedupe");
    e.instruction("cmp x14, x23");                                              // has every previous key been compared?
    e.instruction("b.ge __rt_json_count_key_unique");                           // this key is new
    e.instruction("lsl x15, x14, #4");                                          // 16-byte entry stride
    e.instruction("add x15, x24, x15");                                         // address the previous key entry
    e.instruction("ldr x9, [x15]");                                             // previous key pointer
    e.instruction("ldr x11, [x15, #8]");                                        // previous key length
    e.instruction("cmp x11, x13");                                              // do the lengths match?
    e.instruction("b.ne __rt_json_count_key_next");                             // different lengths cannot be equal
    e.instruction("mov x11, xzr");                                              // byte comparison index
    e.label("__rt_json_count_key_bytes");
    e.instruction("cmp x11, x13");                                              // have all key bytes been compared?
    e.instruction("b.ge __rt_json_count_key_duplicate");                        // byte-equal keys are duplicates
    e.instruction("add x16, x10, x11");                                         // current key byte offset
    e.instruction("ldrb w12, [x19, x16]");                                      // current key byte
    e.instruction("add x16, x9, x11");                                          // previous key byte address
    e.instruction("ldrb w17, [x16]");                                           // previous key byte
    e.instruction("cmp w12, w17");                                              // do the bytes match?
    e.instruction("b.ne __rt_json_count_key_next");                             // a mismatch means a different key
    e.instruction("add x11, x11, #1");                                          // compare the next byte
    e.instruction("b __rt_json_count_key_bytes");                               // continue the byte comparison
    e.label("__rt_json_count_key_next");
    e.instruction("add x14, x14, #1");                                          // advance to the next previous key
    e.instruction("b __rt_json_count_key_dedupe");                              // compare against the remaining keys
    e.label("__rt_json_count_key_duplicate");
    e.instruction("b __rt_json_count_key_after");                               // duplicates do not increase the count
    e.label("__rt_json_count_key_unique");
    e.instruction("cmp x23, #64");                                              // is the bounded key table full?
    e.instruction("b.hs __rt_json_count_fallback");                             // larger objects use the exact decoder
    e.instruction("lsl x15, x23, #4");                                          // 16-byte entry stride
    e.instruction("add x15, x24, x15");                                         // address the new key entry
    e.instruction("add x16, x19, x10");                                         // absolute key pointer
    e.instruction("str x16, [x15]");                                            // record the key pointer
    e.instruction("str x13, [x15, #8]");                                        // record the key length
    e.instruction("add x23, x23, #1");                                          // count the new unique key
    e.label("__rt_json_count_key_after");
    e.instruction("ldr x12, [sp, #88]");                                        // restore the key end clobbered by the byte comparison
    e.instruction("add x21, x12, #1");                                          // continue after the closing key quote
    e.instruction("bl __rt_json_count_skip_ws");                                // skip whitespace before the colon
    e.instruction("cmp x21, x20");                                              // did the source end before the colon?
    e.instruction("b.ge __rt_json_count_fail");                                 // validated input cannot end here
    e.instruction("ldrb w9, [x19, x21]");                                       // inspect the colon
    e.instruction("cmp w9, #58");                                               // is this the key/value separator?
    e.instruction("b.ne __rt_json_count_fail");                                 // validated input cannot reach another byte
    e.instruction("add x21, x21, #1");                                          // consume the colon
    e.instruction("bl __rt_json_count_skip_ws");                                // skip whitespace before the value
    e.instruction("bl __rt_json_count_skip_value");                             // skip the complete member value
    e.instruction("bl __rt_json_count_skip_ws");                                // skip whitespace before the separator
    e.instruction("cmp x21, x20");                                              // did the source end after the value?
    e.instruction("b.ge __rt_json_count_fail");                                 // validated input cannot end here
    e.instruction("ldrb w9, [x19, x21]");                                       // inspect the separator
    e.instruction("cmp w9, #44");                                               // ',' continues the object
    e.instruction("b.eq __rt_json_count_object_comma");                         // consume the comma and continue
    e.instruction("cmp w9, #125");                                              // '}' closes the object
    e.instruction("b.eq __rt_json_count_object_done");                          // report the unique-key count
    e.instruction("b __rt_json_count_fail");                                    // validated input cannot reach another byte
    e.label("__rt_json_count_object_comma");
    e.instruction("add x21, x21, #1");                                          // consume the comma
    e.instruction("bl __rt_json_count_skip_ws");                                // skip whitespace before the next key
    e.instruction("b __rt_json_count_object_loop");                             // count the next member
    e.label("__rt_json_count_object_done");
    e.instruction("mov x0, x23");                                               // return the unique top-level key count
    e.instruction("b __rt_json_count_out");                                     // leave through the shared epilogue

    e.label("__rt_json_count_string");
    e.instruction("mov x0, #-4");                                               // count() string TypeError
    e.instruction("b __rt_json_count_out");                                     // leave through the shared epilogue
    e.label("__rt_json_count_true");
    e.instruction("mov x0, #-6");                                               // count() true TypeError
    e.instruction("b __rt_json_count_out");                                     // leave through the shared epilogue
    e.label("__rt_json_count_false");
    e.instruction("mov x0, #-5");                                               // count() false TypeError
    e.instruction("b __rt_json_count_out");                                     // leave through the shared epilogue
    e.label("__rt_json_count_null");
    e.instruction("mov x0, #-7");                                               // count() null TypeError
    e.instruction("b __rt_json_count_out");                                     // leave through the shared epilogue
    e.label("__rt_json_count_fallback");
    e.instruction("mov x0, #-8");                                               // request the exact decoder fallback
    e.instruction("b __rt_json_count_out");                                     // leave through the shared epilogue
    e.label("__rt_json_count_fail");
    e.instruction("mov x0, #-1");                                               // invalid JSON maps to the null TypeError
    e.label("__rt_json_count_out");
    e.instruction("ldr x19, [sp, #40]");                                        // restore the source pointer
    e.instruction("ldr x20, [sp, #48]");                                        // restore the source length
    e.instruction("ldr x21, [sp, #56]");                                        // restore the cursor
    e.instruction("ldr x22, [sp, #64]");                                        // restore the count
    e.instruction("ldr x23, [sp, #72]");                                        // restore the unique-key count
    e.instruction("ldr x24, [sp, #80]");                                        // restore the key-table base
    e.instruction("add x9, sp, #1120");                                        // address the saved linkage pair
    e.instruction("ldp x29, x30, [x9]");                                       // restore frame pointer and return address
    e.instruction("add sp, sp, #1152");                                         // release the helper frame
    e.instruction("ret");                                                       // return the count or status code

    emit_skip_ws_aarch64(e);
    emit_skip_value_aarch64(e);
}

/// Publishes the JSON state expected by the default two-argument `json_decode`.
fn emit_state_reset_aarch64(e: &mut Emitter) {
    abi::emit_load_int_immediate(e, "x9", 512);
    abi::emit_store_reg_to_symbol(e, "x9", "_json_depth_limit", 0);
    abi::emit_store_zero_to_symbol(e, "_json_active_flags", 0);
    abi::emit_store_zero_to_symbol(e, "_json_active_depth", 0);
    abi::emit_store_zero_to_symbol(e, "_json_last_error", 0);
    abi::emit_store_zero_to_symbol(e, "_json_error_source_ptr", 0);
    abi::emit_store_zero_to_symbol(e, "_json_error_source_len", 0);
    abi::emit_store_zero_to_symbol(e, "_json_error_candidate_ptr", 0);
}

/// Emits the whitespace skipper shared by the count scan.
fn emit_skip_ws_aarch64(e: &mut Emitter) {
    e.label("__rt_json_count_skip_ws");
    e.instruction("cmp x21, x20");                                              // has the cursor reached the source end?
    e.instruction("b.ge __rt_json_count_skip_ws_done");                         // stop at the end of input
    e.instruction("ldrb w9, [x19, x21]");                                       // load the next source byte
    e.instruction("cmp w9, #32");                                               // space is RFC 8259 whitespace
    e.instruction("b.eq __rt_json_count_skip_ws_advance");                      // consume it
    e.instruction("cmp w9, #9");                                                // tab is RFC 8259 whitespace
    e.instruction("b.eq __rt_json_count_skip_ws_advance");                      // consume it
    e.instruction("cmp w9, #10");                                               // line feed is RFC 8259 whitespace
    e.instruction("b.eq __rt_json_count_skip_ws_advance");                      // consume it
    e.instruction("cmp w9, #13");                                               // carriage return is RFC 8259 whitespace
    e.instruction("b.ne __rt_json_count_skip_ws_done");                         // stop at the first non-whitespace byte
    e.label("__rt_json_count_skip_ws_advance");
    e.instruction("add x21, x21, #1");                                          // advance past the whitespace byte
    e.instruction("b __rt_json_count_skip_ws");                                 // inspect the next byte
    e.label("__rt_json_count_skip_ws_done");
    e.instruction("ret");                                                       // return to the count scanner
}

/// Emits the value skipper shared by both container scans.
fn emit_skip_value_aarch64(e: &mut Emitter) {
    e.label("__rt_json_count_skip_value");
    e.instruction("cmp x21, x20");                                              // did the source end before the value?
    e.instruction("b.ge __rt_json_count_skip_value_done");                      // validated input cannot end here
    e.instruction("ldrb w9, [x19, x21]");                                       // inspect the first value byte
    e.instruction("cmp w9, #34");                                               // a quoted string
    e.instruction("b.eq __rt_json_count_skip_string");                          // skip to its closing quote
    e.instruction("cmp w9, #123");                                              // a nested object
    e.instruction("b.eq __rt_json_count_skip_container");                       // skip to its matching brace
    e.instruction("cmp w9, #91");                                               // a nested array
    e.instruction("b.eq __rt_json_count_skip_container");                       // skip to its matching bracket
    e.label("__rt_json_count_skip_scalar");
    e.instruction("cmp x21, x20");                                              // has the scalar reached the source end?
    e.instruction("b.ge __rt_json_count_skip_value_done");                      // stop at the end of input
    e.instruction("ldrb w9, [x19, x21]");                                       // load the next scalar byte
    e.instruction("cmp w9, #44");                                               // ',' ends the scalar
    e.instruction("b.eq __rt_json_count_skip_value_done");                      // leave the separator for the caller
    e.instruction("cmp w9, #125");                                              // '}' ends the scalar
    e.instruction("b.eq __rt_json_count_skip_value_done");                      // leave the separator for the caller
    e.instruction("cmp w9, #93");                                               // ']' ends the scalar
    e.instruction("b.eq __rt_json_count_skip_value_done");                      // leave the separator for the caller
    e.instruction("cmp w9, #32");                                               // whitespace ends the scalar
    e.instruction("b.eq __rt_json_count_skip_value_done");                      // leave the whitespace for the caller
    e.instruction("cmp w9, #9");                                                // whitespace ends the scalar
    e.instruction("b.eq __rt_json_count_skip_value_done");                      // leave the whitespace for the caller
    e.instruction("cmp w9, #10");                                               // whitespace ends the scalar
    e.instruction("b.eq __rt_json_count_skip_value_done");                      // leave the whitespace for the caller
    e.instruction("cmp w9, #13");                                               // whitespace ends the scalar
    e.instruction("b.eq __rt_json_count_skip_value_done");                      // leave the whitespace for the caller
    e.instruction("add x21, x21, #1");                                          // advance through the scalar token
    e.instruction("b __rt_json_count_skip_scalar");                             // inspect the next scalar byte
    e.label("__rt_json_count_skip_string");
    e.instruction("add x21, x21, #1");                                          // consume the opening quote
    e.label("__rt_json_count_skip_string_loop");
    e.instruction("cmp x21, x20");                                              // did the string run past the source?
    e.instruction("b.ge __rt_json_count_skip_value_done");                      // validated input cannot end here
    e.instruction("ldrb w9, [x19, x21]");                                       // load the next string byte
    e.instruction("cmp w9, #92");                                               // an escape consumes its escaped byte
    e.instruction("b.eq __rt_json_count_skip_string_escape");                   // skip the backslash pair
    e.instruction("cmp w9, #34");                                               // is this the closing quote?
    e.instruction("b.eq __rt_json_count_skip_string_end");                      // finish the string
    e.instruction("add x21, x21, #1");                                          // advance through the string
    e.instruction("b __rt_json_count_skip_string_loop");                        // inspect the next string byte
    e.label("__rt_json_count_skip_string_escape");
    e.instruction("add x21, x21, #2");                                          // skip the backslash and its escaped byte
    e.instruction("b __rt_json_count_skip_string_loop");                        // continue the string scan
    e.label("__rt_json_count_skip_string_end");
    e.instruction("add x21, x21, #1");                                          // consume the closing quote
    e.instruction("ret");                                                       // return to the count scanner
    e.label("__rt_json_count_skip_container");
    e.instruction("mov x9, #1");                                                // nesting depth starts at the opened container
    e.instruction("add x21, x21, #1");                                          // consume the opening bracket or brace
    e.label("__rt_json_count_skip_container_loop");
    e.instruction("cmp x21, x20");                                              // did the container run past the source?
    e.instruction("b.ge __rt_json_count_skip_value_done");                      // validated input cannot end here
    e.instruction("ldrb w10, [x19, x21]");                                      // load the next container byte
    e.instruction("cmp w10, #34");                                              // a string inside the container
    e.instruction("b.eq __rt_json_count_skip_container_string");                // skip its escaped/plain bytes
    e.instruction("cmp w10, #123");                                             // a nested object opens
    e.instruction("b.eq __rt_json_count_skip_container_open");                  // increment the depth
    e.instruction("cmp w10, #91");                                              // a nested array opens
    e.instruction("b.eq __rt_json_count_skip_container_open");                  // increment the depth
    e.instruction("cmp w10, #125");                                             // an object closes
    e.instruction("b.eq __rt_json_count_skip_container_close");                 // decrement the depth
    e.instruction("cmp w10, #93");                                              // an array closes
    e.instruction("b.eq __rt_json_count_skip_container_close");                 // decrement the depth
    e.instruction("add x21, x21, #1");                                          // advance through ordinary container bytes
    e.instruction("b __rt_json_count_skip_container_loop");                     // inspect the next container byte
    e.label("__rt_json_count_skip_container_open");
    e.instruction("add x9, x9, #1");                                            // enter a nested container
    e.instruction("add x21, x21, #1");                                          // consume the opening bracket
    e.instruction("b __rt_json_count_skip_container_loop");                     // inspect the next container byte
    e.label("__rt_json_count_skip_container_close");
    e.instruction("sub x9, x9, #1");                                            // leave a nested container
    e.instruction("add x21, x21, #1");                                          // consume the closing bracket
    e.instruction("cbz x9, __rt_json_count_skip_value_done");                   // return after the outermost close
    e.instruction("b __rt_json_count_skip_container_loop");                     // inspect the next container byte
    e.label("__rt_json_count_skip_container_string");
    e.instruction("add x21, x21, #1");                                          // consume the opening quote
    e.label("__rt_json_count_skip_container_string_loop");
    e.instruction("cmp x21, x20");                                              // did the nested string run past the source?
    e.instruction("b.ge __rt_json_count_skip_value_done");                      // validated input cannot end here
    e.instruction("ldrb w10, [x19, x21]");                                      // load the next nested string byte
    e.instruction("cmp w10, #92");                                              // an escape consumes its escaped byte
    e.instruction("b.eq __rt_json_count_skip_container_string_escape");         // skip the backslash pair
    e.instruction("cmp w10, #34");                                              // is this the nested closing quote?
    e.instruction("b.eq __rt_json_count_skip_container_string_end");            // resume the container scan
    e.instruction("add x21, x21, #1");                                          // advance through the nested string
    e.instruction("b __rt_json_count_skip_container_string_loop");              // inspect the next nested string byte
    e.label("__rt_json_count_skip_container_string_escape");
    e.instruction("add x21, x21, #2");                                          // skip the backslash and its escaped byte
    e.instruction("b __rt_json_count_skip_container_string_loop");              // continue the nested string scan
    e.label("__rt_json_count_skip_container_string_end");
    e.instruction("add x21, x21, #1");                                          // consume the nested closing quote
    e.instruction("b __rt_json_count_skip_container_loop");                     // resume the container scan
    e.label("__rt_json_count_skip_value_done");
    e.instruction("ret");                                                       // return the cursor to the count scanner
}

/// Emits the x86_64 helper with the same status contract as the AArch64 body.
fn emit_x86_64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: json_decode_count ---");
    e.label_global("__rt_json_decode_count");
    e.instruction("push rbp");                                                  // preserve the caller frame pointer
    e.instruction("mov rbp, rsp");                                              // establish a stable frame base
    e.instruction("sub rsp, 1152");                                             // reserve locals and the 64-entry key table
    e.instruction("mov QWORD PTR [rbp - 8], rax");                              // save the JSON source pointer
    e.instruction("mov QWORD PTR [rbp - 16], rdx");                             // save the JSON source length
    emit_state_reset_x86_64(e);
    e.instruction("mov rax, QWORD PTR [rbp - 8]");                              // reload the source pointer for validation
    e.instruction("mov rdx, QWORD PTR [rbp - 16]");                             // reload the source length for validation
    e.instruction("call __rt_json_validate");                                   // validate syntax, depth and UTF-8 without building values
    e.instruction("test rax, rax");                                             // did validation succeed?
    e.instruction("jz __rt_json_count_fail_x");                                 // invalid JSON maps to the null TypeError
    e.instruction("mov rdi, QWORD PTR [rbp - 8]");                              // source pointer for the count scan
    e.instruction("mov rsi, QWORD PTR [rbp - 16]");                             // source length for the count scan
    e.instruction("xor ecx, ecx");                                              // scan cursor starts at the source beginning
    e.instruction("call __rt_json_count_skip_ws_x");                            // skip leading RFC 8259 whitespace
    e.instruction("cmp rcx, rsi");                                              // is the source empty after trimming?
    e.instruction("jge __rt_json_count_fail_x");                                // an empty source is invalid JSON
    e.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                           // inspect the root token
    e.instruction("cmp r8d, 91");                                               // '[' starts a top-level array
    e.instruction("je __rt_json_count_array_x");                                // count the array elements
    e.instruction("cmp r8d, 123");                                              // '{' starts a top-level object
    e.instruction("je __rt_json_count_object_x");                               // count the unique object keys
    e.instruction("cmp r8d, 34");                                               // '"' is a valid string root
    e.instruction("je __rt_json_count_string_x");                               // report the count() string TypeError
    e.instruction("cmp r8d, 116");                                              // 't' is a valid true literal
    e.instruction("je __rt_json_count_true_x");                                 // report the count() true TypeError
    e.instruction("cmp r8d, 102");                                              // 'f' is a valid false literal
    e.instruction("je __rt_json_count_false_x");                                // report the count() false TypeError
    e.instruction("cmp r8d, 110");                                              // 'n' is a valid null literal
    e.instruction("je __rt_json_count_null_x");                                 // report the count() null TypeError
    e.instruction("jmp __rt_json_count_fallback_x");                            // numbers use the exact decoder for int/float typing

    e.label("__rt_json_count_array_x");
    e.instruction("add rcx, 1");                                                // consume the opening bracket
    e.instruction("mov QWORD PTR [rbp - 24], 0");                               // no elements counted yet
    e.instruction("call __rt_json_count_skip_ws_x");                            // skip whitespace before a possible empty-array close
    e.instruction("cmp rcx, rsi");                                              // did the source end inside the array?
    e.instruction("jge __rt_json_count_fail_x");                                // validated input cannot end here
    e.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                           // inspect the first element or closing bracket
    e.instruction("cmp r8d, 93");                                               // ']' is an empty array
    e.instruction("je __rt_json_count_array_done_x");                           // report a zero element count
    e.label("__rt_json_count_array_loop_x");
    e.instruction("add QWORD PTR [rbp - 24], 1");                               // count the next element
    e.instruction("call __rt_json_count_skip_value_x");                         // skip the complete element value
    e.instruction("call __rt_json_count_skip_ws_x");                            // skip whitespace before the separator
    e.instruction("cmp rcx, rsi");                                              // did the source end after the element?
    e.instruction("jge __rt_json_count_fail_x");                                // validated input cannot end here
    e.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                           // inspect the separator
    e.instruction("cmp r8d, 44");                                               // ',' continues the array
    e.instruction("je __rt_json_count_array_comma_x");                          // consume the comma and continue
    e.instruction("cmp r8d, 93");                                               // ']' closes the array
    e.instruction("je __rt_json_count_array_done_x");                           // report the element count
    e.instruction("jmp __rt_json_count_fail_x");                                // validated input cannot reach another byte
    e.label("__rt_json_count_array_comma_x");
    e.instruction("add rcx, 1");                                                // consume the comma
    e.instruction("call __rt_json_count_skip_ws_x");                            // skip whitespace before the next element
    e.instruction("jmp __rt_json_count_array_loop_x");                          // count the next element
    e.label("__rt_json_count_array_done_x");
    e.instruction("mov rax, QWORD PTR [rbp - 24]");                             // return the top-level element count
    e.instruction("jmp __rt_json_count_out_x");                                 // leave through the shared epilogue

    e.label("__rt_json_count_object_x");
    e.instruction("add rcx, 1");                                                // consume the opening brace
    e.instruction("mov QWORD PTR [rbp - 32], 0");                               // no unique keys counted yet
    e.instruction("call __rt_json_count_skip_ws_x");                            // skip whitespace before a possible empty-object close
    e.instruction("cmp rcx, rsi");                                              // did the source end inside the object?
    e.instruction("jge __rt_json_count_fail_x");                                // validated input cannot end here
    e.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                           // inspect the first key or closing brace
    e.instruction("cmp r8d, 125");                                              // '}' is an empty object
    e.instruction("je __rt_json_count_object_done_x");                          // report a zero unique-key count
    e.label("__rt_json_count_object_loop_x");
    e.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                           // the next member must start with a quote
    e.instruction("cmp r8d, 34");                                               // is the key a JSON string?
    e.instruction("jne __rt_json_count_fallback_x");                            // a malformed key falls back to the exact decoder
    e.instruction("lea r10, [rcx + 1]");                                        // key_start points past the opening quote
    e.instruction("mov r11, r10");                                              // key scan cursor
    e.label("__rt_json_count_key_scan_x");
    e.instruction("cmp r11, rsi");                                              // did the key run past the source?
    e.instruction("jge __rt_json_count_fail_x");                                // validated input cannot end here
    e.instruction("movzx r8d, BYTE PTR [rdi + r11]");                           // load the next key byte
    e.instruction("cmp r8d, 92");                                               // an escaped key needs the full decoder
    e.instruction("je __rt_json_count_fallback_x");                             // fall back for escaped keys
    e.instruction("cmp r8d, 34");                                               // is this the closing key quote?
    e.instruction("je __rt_json_count_key_end_x");                              // finish the plain key
    e.instruction("add r11, 1");                                                // advance the key scan
    e.instruction("jmp __rt_json_count_key_scan_x");                            // inspect the next key byte
    e.label("__rt_json_count_key_end_x");
    e.instruction("mov QWORD PTR [rbp - 72], r11");                             // preserve the key end across the dedupe length load
    e.instruction("mov QWORD PTR [rbp - 40], r10");                             // save the key start offset
    e.instruction("mov r9, r11");                                               // copy the key end before deriving the length
    e.instruction("sub r9, r10");                                               // key byte length
    e.instruction("mov QWORD PTR [rbp - 48], r9");                              // save the key byte length
    e.instruction("test r9, r9");                                               // is the key empty?
    e.instruction("jz __rt_json_count_key_plain_x");                            // an empty key is not numeric
    e.instruction("movzx r8d, BYTE PTR [rdi + r10]");                           // inspect the first key byte
    e.instruction("cmp r8d, 45");                                               // a leading '-' may normalize to an integer key
    e.instruction("je __rt_json_count_fallback_x");                             // fall back for numeric-looking keys
    e.instruction("sub r8d, 48");                                               // rebase a leading digit onto 0..9
    e.instruction("cmp r8d, 9");                                                // does the key start with a digit?
    e.instruction("jbe __rt_json_count_fallback_x");                            // fall back for numeric-looking keys
    e.label("__rt_json_count_key_plain_x");
    e.instruction("mov QWORD PTR [rbp - 56], 0");                               // previous-key index
    e.label("__rt_json_count_key_dedupe_x");
    e.instruction("mov r8, QWORD PTR [rbp - 56]");                              // load the previous-key index
    e.instruction("cmp r8, QWORD PTR [rbp - 32]");                              // has every previous key been compared?
    e.instruction("jge __rt_json_count_key_unique_x");                          // this key is new
    e.instruction("shl r8, 4");                                                 // 16-byte entry stride
    e.instruction("lea r9, [rbp - 1152]");                                      // key table base
    e.instruction("add r9, r8");                                                // address the previous key entry
    e.instruction("mov r10, QWORD PTR [r9]");                                   // previous key pointer
    e.instruction("mov r11, QWORD PTR [r9 + 8]");                               // previous key length
    e.instruction("cmp r11, QWORD PTR [rbp - 48]");                             // do the lengths match?
    e.instruction("jne __rt_json_count_key_next_x");                            // different lengths cannot be equal
    e.instruction("mov QWORD PTR [rbp - 64], 0");                               // byte comparison index
    e.label("__rt_json_count_key_bytes_x");
    e.instruction("mov r8, QWORD PTR [rbp - 64]");                              // load the byte comparison index
    e.instruction("cmp r8, QWORD PTR [rbp - 48]");                              // have all key bytes been compared?
    e.instruction("jge __rt_json_count_key_duplicate_x");                       // byte-equal keys are duplicates
    e.instruction("mov r9, QWORD PTR [rbp - 40]");                              // load the current key start
    e.instruction("add r9, r8");                                                // current key byte offset
    e.instruction("movzx edx, BYTE PTR [rdi + r9]");                            // current key byte
    e.instruction("mov rax, r10");                                              // copy the previous key base before indexing it
    e.instruction("add rax, r8");                                               // previous key byte address
    e.instruction("movzx r9d, BYTE PTR [rax]");                                 // previous key byte
    e.instruction("cmp edx, r9d");                                              // do the bytes match?
    e.instruction("jne __rt_json_count_key_next_x");                            // a mismatch means a different key
    e.instruction("add QWORD PTR [rbp - 64], 1");                               // compare the next byte
    e.instruction("jmp __rt_json_count_key_bytes_x");                           // continue the byte comparison
    e.label("__rt_json_count_key_next_x");
    e.instruction("add QWORD PTR [rbp - 56], 1");                               // advance to the next previous key
    e.instruction("jmp __rt_json_count_key_dedupe_x");                          // compare against the remaining keys
    e.label("__rt_json_count_key_duplicate_x");
    e.instruction("jmp __rt_json_count_key_after_x");                           // duplicates do not increase the count
    e.label("__rt_json_count_key_unique_x");
    e.instruction("mov r8, QWORD PTR [rbp - 32]");                              // load the unique-key count
    e.instruction("cmp r8, 64");                                                // is the bounded key table full?
    e.instruction("jae __rt_json_count_fallback_x");                            // larger objects use the exact decoder
    e.instruction("shl r8, 4");                                                 // 16-byte entry stride
    e.instruction("lea r9, [rbp - 1152]");                                      // key table base
    e.instruction("add r9, r8");                                                // address the new key entry
    e.instruction("mov r10, QWORD PTR [rbp - 40]");                             // key start offset
    e.instruction("add r10, rdi");                                              // absolute key pointer
    e.instruction("mov QWORD PTR [r9], r10");                                   // record the key pointer
    e.instruction("mov r10, QWORD PTR [rbp - 48]");                             // load the key byte length
    e.instruction("mov QWORD PTR [r9 + 8], r10");                               // record the key length
    e.instruction("add QWORD PTR [rbp - 32], 1");                               // count the new unique key
    e.label("__rt_json_count_key_after_x");
    e.instruction("mov r11, QWORD PTR [rbp - 72]");                             // restore the key end clobbered by the dedupe length load
    e.instruction("lea rcx, [r11 + 1]");                                        // continue after the closing key quote
    e.instruction("call __rt_json_count_skip_ws_x");                            // skip whitespace before the colon
    e.instruction("cmp rcx, rsi");                                              // did the source end before the colon?
    e.instruction("jge __rt_json_count_fail_x");                                // validated input cannot end here
    e.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                           // inspect the colon
    e.instruction("cmp r8d, 58");                                               // is this the key/value separator?
    e.instruction("jne __rt_json_count_fail_x");                                // validated input cannot reach another byte
    e.instruction("add rcx, 1");                                                // consume the colon
    e.instruction("call __rt_json_count_skip_ws_x");                            // skip whitespace before the value
    e.instruction("call __rt_json_count_skip_value_x");                         // skip the complete member value
    e.instruction("call __rt_json_count_skip_ws_x");                            // skip whitespace before the separator
    e.instruction("cmp rcx, rsi");                                              // did the source end after the value?
    e.instruction("jge __rt_json_count_fail_x");                                // validated input cannot end here
    e.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                           // inspect the separator
    e.instruction("cmp r8d, 44");                                               // ',' continues the object
    e.instruction("je __rt_json_count_object_comma_x");                         // consume the comma and continue
    e.instruction("cmp r8d, 125");                                              // '}' closes the object
    e.instruction("je __rt_json_count_object_done_x");                          // report the unique-key count
    e.instruction("jmp __rt_json_count_fail_x");                                // validated input cannot reach another byte
    e.label("__rt_json_count_object_comma_x");
    e.instruction("add rcx, 1");                                                // consume the comma
    e.instruction("call __rt_json_count_skip_ws_x");                            // skip whitespace before the next key
    e.instruction("jmp __rt_json_count_object_loop_x");                         // count the next member
    e.label("__rt_json_count_object_done_x");
    e.instruction("mov rax, QWORD PTR [rbp - 32]");                             // return the unique top-level key count
    e.instruction("jmp __rt_json_count_out_x");                                 // leave through the shared epilogue

    e.label("__rt_json_count_string_x");
    e.instruction("mov rax, -4");                                               // count() string TypeError
    e.instruction("jmp __rt_json_count_out_x");                                 // leave through the shared epilogue
    e.label("__rt_json_count_true_x");
    e.instruction("mov rax, -6");                                               // count() true TypeError
    e.instruction("jmp __rt_json_count_out_x");                                 // leave through the shared epilogue
    e.label("__rt_json_count_false_x");
    e.instruction("mov rax, -5");                                               // count() false TypeError
    e.instruction("jmp __rt_json_count_out_x");                                 // leave through the shared epilogue
    e.label("__rt_json_count_null_x");
    e.instruction("mov rax, -7");                                               // count() null TypeError
    e.instruction("jmp __rt_json_count_out_x");                                 // leave through the shared epilogue
    e.label("__rt_json_count_fallback_x");
    e.instruction("mov rax, -8");                                               // request the exact decoder fallback
    e.instruction("jmp __rt_json_count_out_x");                                 // leave through the shared epilogue
    e.label("__rt_json_count_fail_x");
    e.instruction("mov rax, -1");                                               // invalid JSON maps to the null TypeError
    e.label("__rt_json_count_out_x");
    e.instruction("mov rsp, rbp");                                              // release the helper frame
    e.instruction("pop rbp");                                                   // restore the caller frame pointer
    e.instruction("ret");                                                       // return the count or status code

    emit_skip_ws_x86_64(e);
    emit_skip_value_x86_64(e);
}

/// Publishes the JSON state expected by the default two-argument `json_decode` on x86_64.
fn emit_state_reset_x86_64(e: &mut Emitter) {
    abi::emit_load_int_immediate(e, "r10", 512);
    abi::emit_store_reg_to_symbol(e, "r10", "_json_depth_limit", 0);
    abi::emit_store_zero_to_symbol(e, "_json_active_flags", 0);
    abi::emit_store_zero_to_symbol(e, "_json_active_depth", 0);
    abi::emit_store_zero_to_symbol(e, "_json_last_error", 0);
    abi::emit_store_zero_to_symbol(e, "_json_error_source_ptr", 0);
    abi::emit_store_zero_to_symbol(e, "_json_error_source_len", 0);
    abi::emit_store_zero_to_symbol(e, "_json_error_candidate_ptr", 0);
}

/// Emits the x86_64 whitespace skipper shared by the count scan.
fn emit_skip_ws_x86_64(e: &mut Emitter) {
    e.label("__rt_json_count_skip_ws_x");
    e.instruction("cmp rcx, rsi");                                              // has the cursor reached the source end?
    e.instruction("jge __rt_json_count_skip_ws_done_x");                        // stop at the end of input
    e.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                           // load the next source byte
    e.instruction("cmp r8d, 32");                                               // space is RFC 8259 whitespace
    e.instruction("je __rt_json_count_skip_ws_advance_x");                      // consume it
    e.instruction("cmp r8d, 9");                                                // tab is RFC 8259 whitespace
    e.instruction("je __rt_json_count_skip_ws_advance_x");                      // consume it
    e.instruction("cmp r8d, 10");                                               // line feed is RFC 8259 whitespace
    e.instruction("je __rt_json_count_skip_ws_advance_x");                      // consume it
    e.instruction("cmp r8d, 13");                                               // carriage return is RFC 8259 whitespace
    e.instruction("jne __rt_json_count_skip_ws_done_x");                        // stop at the first non-whitespace byte
    e.label("__rt_json_count_skip_ws_advance_x");
    e.instruction("add rcx, 1");                                                // advance past the whitespace byte
    e.instruction("jmp __rt_json_count_skip_ws_x");                             // inspect the next byte
    e.label("__rt_json_count_skip_ws_done_x");
    e.instruction("ret");                                                       // return to the count scanner
}

/// Emits the x86_64 value skipper shared by both container scans.
fn emit_skip_value_x86_64(e: &mut Emitter) {
    e.label("__rt_json_count_skip_value_x");
    e.instruction("cmp rcx, rsi");                                              // did the source end before the value?
    e.instruction("jge __rt_json_count_skip_value_done_x");                     // validated input cannot end here
    e.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                           // inspect the first value byte
    e.instruction("cmp r8d, 34");                                               // a quoted string
    e.instruction("je __rt_json_count_skip_string_x");                          // skip to its closing quote
    e.instruction("cmp r8d, 123");                                              // a nested object
    e.instruction("je __rt_json_count_skip_container_x");                       // skip to its matching brace
    e.instruction("cmp r8d, 91");                                               // a nested array
    e.instruction("je __rt_json_count_skip_container_x");                       // skip to its matching bracket
    e.label("__rt_json_count_skip_scalar_x");
    e.instruction("cmp rcx, rsi");                                              // has the scalar reached the source end?
    e.instruction("jge __rt_json_count_skip_value_done_x");                     // stop at the end of input
    e.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                           // load the next scalar byte
    e.instruction("cmp r8d, 44");                                               // ',' ends the scalar
    e.instruction("je __rt_json_count_skip_value_done_x");                      // leave the separator for the caller
    e.instruction("cmp r8d, 125");                                              // '}' ends the scalar
    e.instruction("je __rt_json_count_skip_value_done_x");                      // leave the separator for the caller
    e.instruction("cmp r8d, 93");                                               // ']' ends the scalar
    e.instruction("je __rt_json_count_skip_value_done_x");                      // leave the separator for the caller
    e.instruction("cmp r8d, 32");                                               // whitespace ends the scalar
    e.instruction("je __rt_json_count_skip_value_done_x");                      // leave the whitespace for the caller
    e.instruction("cmp r8d, 9");                                                // whitespace ends the scalar
    e.instruction("je __rt_json_count_skip_value_done_x");                      // leave the whitespace for the caller
    e.instruction("cmp r8d, 10");                                               // whitespace ends the scalar
    e.instruction("je __rt_json_count_skip_value_done_x");                      // leave the whitespace for the caller
    e.instruction("cmp r8d, 13");                                               // whitespace ends the scalar
    e.instruction("je __rt_json_count_skip_value_done_x");                      // leave the whitespace for the caller
    e.instruction("add rcx, 1");                                                // advance through the scalar token
    e.instruction("jmp __rt_json_count_skip_scalar_x");                         // inspect the next scalar byte
    e.label("__rt_json_count_skip_string_x");
    e.instruction("add rcx, 1");                                                // consume the opening quote
    e.label("__rt_json_count_skip_string_loop_x");
    e.instruction("cmp rcx, rsi");                                              // did the string run past the source?
    e.instruction("jge __rt_json_count_skip_value_done_x");                     // validated input cannot end here
    e.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                           // load the next string byte
    e.instruction("cmp r8d, 92");                                               // an escape consumes its escaped byte
    e.instruction("je __rt_json_count_skip_string_escape_x");                   // skip the backslash pair
    e.instruction("cmp r8d, 34");                                               // is this the closing quote?
    e.instruction("je __rt_json_count_skip_string_end_x");                      // finish the string
    e.instruction("add rcx, 1");                                                // advance through the string
    e.instruction("jmp __rt_json_count_skip_string_loop_x");                    // inspect the next string byte
    e.label("__rt_json_count_skip_string_escape_x");
    e.instruction("add rcx, 2");                                                // skip the backslash and its escaped byte
    e.instruction("jmp __rt_json_count_skip_string_loop_x");                    // continue the string scan
    e.label("__rt_json_count_skip_string_end_x");
    e.instruction("add rcx, 1");                                                // consume the closing quote
    e.instruction("ret");                                                       // return to the count scanner
    e.label("__rt_json_count_skip_container_x");
    e.instruction("mov r9, 1");                                                 // nesting depth starts at the opened container
    e.instruction("add rcx, 1");                                                // consume the opening bracket or brace
    e.label("__rt_json_count_skip_container_loop_x");
    e.instruction("cmp rcx, rsi");                                              // did the container run past the source?
    e.instruction("jge __rt_json_count_skip_value_done_x");                     // validated input cannot end here
    e.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                           // load the next container byte
    e.instruction("cmp r8d, 34");                                               // a string inside the container
    e.instruction("je __rt_json_count_skip_container_string_x");                // skip its escaped/plain bytes
    e.instruction("cmp r8d, 123");                                              // a nested object opens
    e.instruction("je __rt_json_count_skip_container_open_x");                  // increment the depth
    e.instruction("cmp r8d, 91");                                               // a nested array opens
    e.instruction("je __rt_json_count_skip_container_open_x");                  // increment the depth
    e.instruction("cmp r8d, 125");                                              // an object closes
    e.instruction("je __rt_json_count_skip_container_close_x");                 // decrement the depth
    e.instruction("cmp r8d, 93");                                               // an array closes
    e.instruction("je __rt_json_count_skip_container_close_x");                 // decrement the depth
    e.instruction("add rcx, 1");                                                // advance through ordinary container bytes
    e.instruction("jmp __rt_json_count_skip_container_loop_x");                 // inspect the next container byte
    e.label("__rt_json_count_skip_container_open_x");
    e.instruction("add r9, 1");                                                 // enter a nested container
    e.instruction("add rcx, 1");                                                // consume the opening bracket
    e.instruction("jmp __rt_json_count_skip_container_loop_x");                 // inspect the next container byte
    e.label("__rt_json_count_skip_container_close_x");
    e.instruction("sub r9, 1");                                                 // leave a nested container
    e.instruction("add rcx, 1");                                                // consume the closing bracket
    e.instruction("test r9, r9");                                               // was this the outermost close?
    e.instruction("jz __rt_json_count_skip_value_done_x");                      // return after the outermost close
    e.instruction("jmp __rt_json_count_skip_container_loop_x");                 // inspect the next container byte
    e.label("__rt_json_count_skip_container_string_x");
    e.instruction("add rcx, 1");                                                // consume the opening quote
    e.label("__rt_json_count_skip_container_string_loop_x");
    e.instruction("cmp rcx, rsi");                                              // did the nested string run past the source?
    e.instruction("jge __rt_json_count_skip_value_done_x");                     // validated input cannot end here
    e.instruction("movzx r8d, BYTE PTR [rdi + rcx]");                           // load the next nested string byte
    e.instruction("cmp r8d, 92");                                               // an escape consumes its escaped byte
    e.instruction("je __rt_json_count_skip_container_string_escape_x");         // skip the backslash pair
    e.instruction("cmp r8d, 34");                                               // is this the nested closing quote?
    e.instruction("je __rt_json_count_skip_container_string_end_x");            // resume the container scan
    e.instruction("add rcx, 1");                                                // advance through the nested string
    e.instruction("jmp __rt_json_count_skip_container_string_loop_x");          // inspect the next nested string byte
    e.label("__rt_json_count_skip_container_string_escape_x");
    e.instruction("add rcx, 2");                                                // skip the backslash and its escaped byte
    e.instruction("jmp __rt_json_count_skip_container_string_loop_x");          // continue the nested string scan
    e.label("__rt_json_count_skip_container_string_end_x");
    e.instruction("add rcx, 1");                                                // consume the nested closing quote
    e.instruction("jmp __rt_json_count_skip_container_loop_x");                 // resume the container scan
    e.label("__rt_json_count_skip_value_done_x");
    e.instruction("ret");                                                       // return the cursor to the count scanner
}
