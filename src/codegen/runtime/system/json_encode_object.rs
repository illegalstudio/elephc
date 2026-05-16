//! Purpose:
//! Emits JSON object encoder runtime helper.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen::runtime::system` during runtime emission.
//!
//! Key details:
//! - Public-property walking and JsonSerializable dispatch depend on emitted per-class JSON descriptors.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_json_encode_object: encode a PHP object instance as JSON.
///
/// Looks up the per-class JSON descriptor (emitted by runtime/data.rs) and
/// either dispatches to the class's `jsonSerialize()` method when the class
/// implements `JsonSerializable`, or walks the public-property table and
/// emits `{"name":<value>,...}`.
///
/// Input:
///   ARM64: x0 = object pointer
///   x86_64: rax = object pointer
///
/// Output:
///   ARM64: x1 = ptr in concat_buf, x2 = length
///   x86_64: rax = ptr, rdx = length
pub(crate) fn emit_json_encode_object(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_object_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: json_encode_object ---");
    emitter.label_global("__rt_json_encode_object");

    // -- set up stack frame --
    // Stack layout (128 bytes):
    //   [sp, #0]   = object pointer
    //   [sp, #8]   = output start ptr
    //   [sp, #16]  = current write pos
    //   [sp, #24]  = descriptor pointer
    //   [sp, #32]  = property index (loop counter)
    //   [sp, #40]  = property count
    //   [sp, #48]  = scratch (prop_index)
    //   [sp, #56]  = scratch (type_tag)
    //   [sp, #64]  = scratch (prop value lo)
    //   [sp, #72]  = scratch (prop value hi)
    //   [sp, #80]  = JsonSerializable saved _json_last_error
    //   [sp, #88]  = JsonSerializable saved _json_active_flags
    //   [sp, #96]  = JsonSerializable saved _json_active_depth
    //   [sp, #104] = JsonSerializable saved _json_indent_depth
    //   [sp, #112] = saved x29
    //   [sp, #120] = saved x30
    emitter.instruction("sub sp, sp, #128");                                    // allocate the object encoder scratch frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish a stable frame pointer for the encoder
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer for downstream loads

    // -- enter the recursion-depth check --
    emitter.instruction("bl __rt_json_depth_enter");                            // increment _json_active_depth and throw on overflow when requested

    // -- get current write position in concat_buf --
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // load the current concat-buffer offset
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_concat_buf");
    emitter.instruction("add x11, x11, x10");                                   // compute the write pointer for the encoded object
    emitter.instruction("str x11, [sp, #8]");                                   // save the start pointer for the final result slice
    emitter.instruction("str x11, [sp, #16]");                                  // save the running write pointer for the encoder loop

    // -- bounds-check the class id and resolve the JSON descriptor --
    emitter.instruction("ldr x12, [x0]");                                       // load class_id from the object header
    crate::codegen::abi::emit_symbol_address(emitter, "x13", "_class_gc_desc_count");
    emitter.instruction("ldr x13, [x13]");                                      // load the total number of registered class descriptors
    emitter.instruction("cmp x12, x13");                                        // is the class_id within the descriptor table?
    emitter.instruction("b.hs __rt_json_obj_open_only");                        // an out-of-range class_id falls back to an empty object literal
    crate::codegen::abi::emit_symbol_address(emitter, "x13", "_class_json_desc_ptrs");
    emitter.instruction("lsl x14, x12, #3");                                    // scale class_id by 8 bytes per descriptor pointer
    emitter.instruction("ldr x13, [x13, x14]");                                 // load the descriptor pointer for the current class
    emitter.instruction("str x13, [sp, #24]");                                  // save the descriptor pointer for downstream loads

    // -- check the JsonSerializable flag --
    emitter.instruction("ldr x12, [x13]");                                      // load the class JSON descriptor flags word
    emitter.instruction("tst x12, #1");                                         // is JsonSerializable bit set on the descriptor?
    emitter.instruction("b.eq __rt_json_obj_walk_props");                       // ordinary classes walk public properties directly
    emitter.instruction("ldr x14, [x13, #8]");                                  // load the jsonSerialize method symbol from the descriptor
    emitter.instruction("cbz x14, __rt_json_obj_walk_props");                   // missing method targets fall through to the property walker
    emitter.instruction("str x14, [sp, #32]");                                  // park the jsonSerialize target across helper calls that clobber x14
    // The user method's body resets _concat_off and writes scratch at the
    // beginning of concat_buf, which would trash any caller prefix already
    // there (e.g. an enclosing assoc/array encoder's `{"key":` bytes).
    // Persist the caller's prefix to heap, run jsonSerialize, then copy the
    // prefix back before re-establishing concat_off and encoding the result.
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // capture the caller-visible concat-buffer offset before the user method runs
    emitter.instruction("str x10, [sp, #24]");                                  // save the captured offset (= prefix length) across the user method invocation
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_last_error");
    emitter.instruction("ldr x10, [x9]");                                       // capture the outer JSON error state before user code can run nested JSON calls
    emitter.instruction("str x10, [sp, #80]");                                  // save _json_last_error across jsonSerialize()
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x10, [x9]");                                       // capture the outer JSON flag bitmask before user code can change it
    emitter.instruction("str x10, [sp, #88]");                                  // save _json_active_flags across jsonSerialize()
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_depth");
    emitter.instruction("ldr x10, [x9]");                                       // capture the outer JSON depth before user code can reset it
    emitter.instruction("str x10, [sp, #96]");                                  // save _json_active_depth across jsonSerialize()
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_indent_depth");
    emitter.instruction("ldr x10, [x9]");                                       // capture the outer pretty-print depth before user code can reset it
    emitter.instruction("str x10, [sp, #104]");                                 // save _json_indent_depth across jsonSerialize()
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_depth_limit");
    emitter.instruction("ldr x10, [x9]");                                       // capture the outer JSON depth limit before user code can change it
    emitter.instruction("str x10, [sp, #72]");                                  // save _json_depth_limit across jsonSerialize()
    emitter.instruction("str xzr, [sp, #40]");                                  // default the saved prefix heap pointer to null for empty prefixes
    emitter.instruction("cbz x10, __rt_json_obj_jsonserialize_invoke");         // skip the prefix copy when there is no caller-visible prefix to preserve
    crate::codegen::abi::emit_symbol_address(emitter, "x1", "_concat_buf");
    emitter.instruction("mov x2, x10");                                         // copy the prefix length into the str_persist length register
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the caller prefix into a heap-owned buffer
    emitter.instruction("str x1, [sp, #40]");                                   // remember the heap-owned prefix pointer for the post-call restore
    emitter.label("__rt_json_obj_jsonserialize_invoke");
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the receiver object pointer for the method call
    emitter.instruction("ldr x14, [sp, #32]");                                  // reload the saved jsonSerialize method target
    emitter.instruction("blr x14");                                             // invoke jsonSerialize on the receiver and capture the boxed mixed result
    emitter.instruction("str x0, [sp, #48]");                                   // park the boxed mixed return value across the prefix-restore loop
    // Copy the heap-owned prefix back into concat_buf[0..prefix_len].
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the saved prefix length
    emitter.instruction("cbz x10, __rt_json_obj_jsonserialize_after_restore");  // skip the prefix restore when no prefix was preserved
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload the heap-owned prefix pointer
    emitter.instruction("cbz x11, __rt_json_obj_jsonserialize_after_restore");  // defensive null guard for the heap-owned prefix pointer
    crate::codegen::abi::emit_symbol_address(emitter, "x12", "_concat_buf");
    emitter.instruction("mov x13, #0");                                         // initialize the prefix copy index
    emitter.label("__rt_json_obj_prefix_restore");
    emitter.instruction("cmp x13, x10");                                        // have we copied every prefix byte back?
    emitter.instruction("b.ge __rt_json_obj_jsonserialize_after_restore");      // exit once the entire prefix is restored
    emitter.instruction("ldrb w14, [x11, x13]");                                // load the next prefix byte from the heap-owned copy
    emitter.instruction("strb w14, [x12, x13]");                                // restore the byte into concat_buf at the original offset
    emitter.instruction("add x13, x13, #1");                                    // advance the prefix copy index
    emitter.instruction("b __rt_json_obj_prefix_restore");                      // continue the prefix restore loop
    emitter.label("__rt_json_obj_jsonserialize_after_restore");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the saved pre-call concat-buffer offset
    emitter.instruction("str x10, [x9]");                                       // restore the concat-buffer offset so encode_mixed appends at our intended slot
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_last_error");
    emitter.instruction("ldr x10, [sp, #80]");                                  // reload the outer JSON error state after user code returns
    emitter.instruction("str x10, [x9]");                                       // restore _json_last_error before encoding the serialized value
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x10, [sp, #88]");                                  // reload the outer JSON flag bitmask after user code returns
    emitter.instruction("str x10, [x9]");                                       // restore _json_active_flags before encoding the serialized value
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_active_depth");
    emitter.instruction("ldr x10, [sp, #96]");                                  // reload the outer JSON depth after user code returns
    emitter.instruction("str x10, [x9]");                                       // restore _json_active_depth before recursive JSON encoding resumes
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_indent_depth");
    emitter.instruction("ldr x10, [sp, #104]");                                 // reload the outer pretty-print depth after user code returns
    emitter.instruction("str x10, [x9]");                                       // restore _json_indent_depth before recursive JSON encoding resumes
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_json_depth_limit");
    emitter.instruction("ldr x10, [sp, #72]");                                  // reload the outer JSON depth limit after user code returns
    emitter.instruction("str x10, [x9]");                                       // restore _json_depth_limit before recursive JSON encoding resumes
    emitter.instruction("ldr x0, [sp, #48]");                                   // restore the boxed mixed return value before encoding it
    emitter.instruction("bl __rt_json_encode_mixed");                           // encode the boxed mixed return value as JSON
    // Save the (x1, x2) result across the depth-exit helper call.
    emitter.instruction("stp x1, x2, [sp, #32]");                               // checkpoint the encoded result slice across __rt_json_depth_exit
    emitter.instruction("bl __rt_json_depth_exit");                             // decrement _json_active_depth so a sibling encoder can re-enter cleanly
    emitter.instruction("ldp x1, x2, [sp, #32]");                               // restore the encoded result slice after the helper call
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address after JsonSerializable encoding
    emitter.instruction("add sp, sp, #128");                                    // deallocate the object encoder scratch frame
    emitter.instruction("ret");                                                 // return the JSON encoded result produced by mixed encoding

    // -- empty object fallback (missing class id) --
    emitter.label("__rt_json_obj_open_only");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the running write pointer for the open-brace fallback
    emitter.instruction("mov w12, #123");                                       // ASCII '{'
    emitter.instruction("strb w12, [x11]");                                     // emit the opening brace for the empty object literal
    emitter.instruction("add x11, x11, #1");                                    // advance past the opening brace
    emitter.instruction("mov w12, #125");                                       // ASCII '}'
    emitter.instruction("strb w12, [x11]");                                     // emit the closing brace for the empty object literal
    emitter.instruction("add x11, x11, #1");                                    // advance past the closing brace
    emitter.instruction("b __rt_json_obj_finalize");                            // jump to the shared finalization tail

    // -- walk public properties from the descriptor --
    emitter.label("__rt_json_obj_walk_props");
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the descriptor pointer
    emitter.instruction("ldr x14, [x13, #16]");                                 // load the public property count
    emitter.instruction("str x14, [sp, #40]");                                  // save the public property count for the loop guard

    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the running write pointer for the opening brace
    emitter.instruction("mov w12, #123");                                       // ASCII '{'
    emitter.instruction("strb w12, [x11]");                                     // emit the opening brace before the first property
    emitter.instruction("add x11, x11, #1");                                    // advance past the opening brace
    emitter.instruction("str x11, [sp, #16]");                                  // save the running write pointer after the opening brace
    emitter.instruction("bl __rt_json_pretty_push");                            // enter one pretty-print indentation level after the object opens
    emitter.instruction("str xzr, [sp, #32]");                                  // initialize the property loop index to zero

    emitter.label("__rt_json_obj_loop");
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the current property loop index
    emitter.instruction("ldr x14, [sp, #40]");                                  // reload the total public property count
    emitter.instruction("cmp x10, x14");                                        // are there more properties to encode?
    emitter.instruction("b.ge __rt_json_obj_close");                            // emit the closing brace once every property is encoded

    // Compute descriptor entry address: descriptor + 24 + index * 32
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the descriptor pointer for the current iteration
    emitter.instruction("add x13, x13, #24");                                   // skip the leading flags / jsonSerialize / count words
    emitter.instruction("mov x9, #32");                                         // each descriptor entry occupies 32 bytes
    emitter.instruction("mul x9, x10, x9");                                     // compute the byte offset of the current entry
    emitter.instruction("add x13, x13, x9");                                    // advance to the requested property descriptor entry

    // Save name_ptr / name_len / prop_index / type_tag in scratch slots
    emitter.instruction("ldr x14, [x13]");                                      // load the property name pointer
    emitter.instruction("ldr x15, [x13, #8]");                                  // load the property name length
    emitter.instruction("ldr x16, [x13, #16]");                                 // load the property runtime slot index
    emitter.instruction("ldr x17, [x13, #24]");                                 // load the property compile-time type tag
    emitter.instruction("str x16, [sp, #48]");                                  // save the property slot index for value loading
    emitter.instruction("str x17, [sp, #56]");                                  // save the property type tag for runtime dispatch

    // Comma separator before all but the first property
    emitter.instruction("cbz x10, __rt_json_obj_key");                          // skip the comma before the first property entry
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the running write pointer
    emitter.instruction("mov w12, #44");                                        // ASCII ','
    emitter.instruction("strb w12, [x11]");                                     // emit the comma between encoded property entries
    emitter.instruction("add x11, x11, #1");                                    // advance past the comma separator
    emitter.instruction("str x11, [sp, #16]");                                  // save the running write pointer after the comma

    emitter.label("__rt_json_obj_key");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the running write pointer for the key prefix
    emitter.instruction("bl __rt_json_pretty_line");                            // append newline and indentation for this property when pretty-printing
    emitter.instruction("str x11, [sp, #16]");                                  // save the write pointer after any pretty indentation
    emitter.instruction("mov w12, #34");                                        // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                     // emit the opening quote for the property name
    emitter.instruction("add x11, x11, #1");                                    // advance past the opening quote

    // Copy property name bytes to the concat buffer (no escaping — PHP property
    // names cannot legally contain JSON-significant control or escape chars).
    emitter.instruction("mov x9, #0");                                          // initialize the property-name copy index
    emitter.label("__rt_json_obj_key_copy");
    emitter.instruction("cmp x9, x15");                                         // have we copied every byte of the property name?
    emitter.instruction("b.ge __rt_json_obj_key_done");                         // exit the copy loop once the name is fully written
    emitter.instruction("ldrb w12, [x14, x9]");                                 // load the next property-name byte from the descriptor string
    emitter.instruction("strb w12, [x11, x9]");                                 // copy the byte into the concat buffer
    emitter.instruction("add x9, x9, #1");                                      // advance the copy index
    emitter.instruction("b __rt_json_obj_key_copy");                            // continue copying the property name
    emitter.label("__rt_json_obj_key_done");
    emitter.instruction("add x11, x11, x15");                                   // advance the write pointer past the copied property name
    emitter.instruction("mov w12, #34");                                        // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                     // emit the closing quote for the property name
    emitter.instruction("add x11, x11, #1");                                    // advance past the closing quote
    emitter.instruction("mov w12, #58");                                        // ASCII ':'
    emitter.instruction("strb w12, [x11]");                                     // emit the colon separating key and value
    emitter.instruction("add x11, x11, #1");                                    // advance past the colon
    emitter.instruction("bl __rt_json_pretty_colon_space");                     // append the pretty-print key/value space when requested
    emitter.instruction("str x11, [sp, #16]");                                  // save the running write pointer after the key prefix

    // Sync concat_off so nested encoders append after the existing prefix.
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_buf");
    emitter.instruction("sub x12, x11, x9");                                    // compute the absolute concat-buffer offset for the prefix tail
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_off");
    emitter.instruction("str x12, [x10]");                                      // publish the concat offset for nested value encoders

    // Load property value from the object instance.
    // Property slot offset = 8 + prop_index * 16
    emitter.instruction("ldr x16, [sp, #48]");                                  // reload the property runtime slot index
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the receiver object pointer
    emitter.instruction("mov x9, #16");                                         // each property slot occupies 16 bytes
    emitter.instruction("mul x9, x16, x9");                                     // compute the property slot byte offset within the instance
    emitter.instruction("add x9, x9, #8");                                      // skip the leading class_id field at offset 0
    emitter.instruction("add x9, x0, x9");                                      // resolve the absolute address of the property slot
    emitter.instruction("ldr x18, [x9]");                                       // load the property low payload word
    emitter.instruction("ldr x15, [x9, #8]");                                   // load the property high payload word without clobbering caller x19
    emitter.instruction("str x18, [sp, #64]");                                  // save the property low payload across the encoder dispatch
    emitter.instruction("str x15, [sp, #72]");                                  // save the property high payload across the encoder dispatch

    // Dispatch on the property type tag. Each branch leaves the encoded
    // result in x1=ptr, x2=len so the shared copy code can append it.
    emitter.instruction("ldr x17, [sp, #56]");                                  // reload the saved property type tag
    emitter.instruction("cmp x17, #0");                                         // tag 0 = integer
    emitter.instruction("b.eq __rt_json_obj_val_int");                          // branch on the current JSON object encoder condition
    emitter.instruction("cmp x17, #1");                                         // tag 1 = string
    emitter.instruction("b.eq __rt_json_obj_val_str");                          // branch on the current JSON object encoder condition
    emitter.instruction("cmp x17, #2");                                         // tag 2 = float
    emitter.instruction("b.eq __rt_json_obj_val_float");                        // branch on the current JSON object encoder condition
    emitter.instruction("cmp x17, #3");                                         // tag 3 = bool
    emitter.instruction("b.eq __rt_json_obj_val_bool");                         // branch on the current JSON object encoder condition
    emitter.instruction("cmp x17, #4");                                         // tag 4 = indexed array
    emitter.instruction("b.eq __rt_json_obj_val_array");                        // branch on the current JSON object encoder condition
    emitter.instruction("cmp x17, #5");                                         // tag 5 = associative array
    emitter.instruction("b.eq __rt_json_obj_val_assoc");                        // branch on the current JSON object encoder condition
    emitter.instruction("cmp x17, #6");                                         // tag 6 = nested object
    emitter.instruction("b.eq __rt_json_obj_val_object");                       // branch on the current JSON object encoder condition
    emitter.instruction("cmp x17, #7");                                         // tag 7 = boxed mixed
    emitter.instruction("b.eq __rt_json_obj_val_mixed");                        // branch on the current JSON object encoder condition
    emitter.instruction("b __rt_json_obj_val_null");                            // every other tag (null, resource, ...) falls back to JSON null

    emitter.label("__rt_json_obj_val_int");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load the integer payload
    emitter.instruction("bl __rt_itoa");                                        // format the integer as a decimal string
    emitter.instruction("b __rt_json_obj_val_copy");                            // jump to the shared result copy

    emitter.label("__rt_json_obj_val_str");
    emitter.instruction("ldr x1, [sp, #64]");                                   // load the string pointer payload
    emitter.instruction("ldr x2, [sp, #72]");                                   // load the string length payload
    emitter.instruction("bl __rt_json_encode_str");                             // encode the string as a quoted JSON string
    emitter.instruction("b __rt_json_obj_val_copy");                            // jump to the shared result copy

    emitter.label("__rt_json_obj_val_float");
    emitter.instruction("ldr x9, [sp, #64]");                                   // load the float bit pattern payload
    emitter.instruction("fmov d0, x9");                                         // move the float bits into the FP argument register
    emitter.instruction("bl __rt_json_encode_float");                           // encode the float, rejecting Inf/NaN per JSON semantics
    emitter.instruction("b __rt_json_obj_val_copy");                            // jump to the shared result copy

    emitter.label("__rt_json_obj_val_bool");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load the bool payload
    emitter.instruction("bl __rt_json_encode_bool");                            // encode the bool as the true/false literal
    emitter.instruction("b __rt_json_obj_val_copy");                            // jump to the shared result copy

    emitter.label("__rt_json_obj_val_array");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load the indexed-array pointer payload
    emitter.instruction("bl __rt_json_encode_array_dynamic");                   // encode the array via the dynamic array helper
    emitter.instruction("b __rt_json_obj_val_copy");                            // jump to the shared result copy

    emitter.label("__rt_json_obj_val_assoc");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load the associative-array pointer payload
    emitter.instruction("bl __rt_json_encode_assoc");                           // encode the hash table as a JSON object
    emitter.instruction("b __rt_json_obj_val_copy");                            // jump to the shared result copy

    emitter.label("__rt_json_obj_val_object");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load the nested object pointer payload
    emitter.instruction("bl __rt_json_encode_object");                          // recursively encode the nested object
    emitter.instruction("b __rt_json_obj_val_copy");                            // jump to the shared result copy

    emitter.label("__rt_json_obj_val_mixed");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load the boxed mixed pointer payload
    emitter.instruction("bl __rt_json_encode_mixed");                           // encode the boxed mixed payload recursively
    emitter.instruction("b __rt_json_obj_val_copy");                            // jump to the shared result copy

    emitter.label("__rt_json_obj_val_null");
    emitter.instruction("bl __rt_json_encode_null");                            // encode unsupported tags as the JSON null literal

    emitter.label("__rt_json_obj_val_copy");
    // Copy the encoded value bytes from (x1, x2) into the running write
    // position. Most encoders already write into concat_buf at the prefix
    // tail, but we copy unconditionally so this stays correct for helpers
    // whose result lives elsewhere.
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the running write pointer
    emitter.instruction("mov x9, #0");                                          // initialize the value copy index
    emitter.label("__rt_json_obj_val_copy_loop");
    emitter.instruction("cmp x9, x2");                                          // have we copied every byte of the encoded value?
    emitter.instruction("b.ge __rt_json_obj_val_copy_done");                    // exit the value copy loop once finished
    emitter.instruction("ldrb w12, [x1, x9]");                                  // load the next encoded value byte
    emitter.instruction("strb w12, [x11, x9]");                                 // store it into the concat buffer at the running write pointer
    emitter.instruction("add x9, x9, #1");                                      // advance the value copy index
    emitter.instruction("b __rt_json_obj_val_copy_loop");                       // continue copying the encoded value
    emitter.label("__rt_json_obj_val_copy_done");
    emitter.instruction("add x11, x11, x2");                                    // advance the write pointer past the copied value
    emitter.instruction("str x11, [sp, #16]");                                  // save the running write pointer for the next iteration

    // Advance the loop index and continue.
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the property loop index
    emitter.instruction("add x10, x10, #1");                                    // advance to the next property entry
    emitter.instruction("str x10, [sp, #32]");                                  // save the updated loop index
    emitter.instruction("b __rt_json_obj_loop");                                // continue with the next property entry

    emitter.label("__rt_json_obj_close");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the running write pointer for the closing brace
    emitter.instruction("bl __rt_json_pretty_pop");                             // leave the object indentation level before closing it
    emitter.instruction("ldr x14, [sp, #40]");                                  // reload public property count to decide whether closing needs its own line
    emitter.instruction("cbz x14, __rt_json_obj_close_emit");                   // empty objects stay compact even under JSON_PRETTY_PRINT
    emitter.instruction("bl __rt_json_pretty_line");                            // append the closing-line indentation for non-empty pretty objects
    emitter.label("__rt_json_obj_close_emit");
    emitter.instruction("mov w12, #125");                                       // ASCII '}'
    emitter.instruction("strb w12, [x11]");                                     // emit the closing brace after the last property
    emitter.instruction("add x11, x11, #1");                                    // advance past the closing brace

    emitter.label("__rt_json_obj_finalize");
    emitter.instruction("str x11, [sp, #16]");                                  // checkpoint the write pointer across the depth-exit helper call
    emitter.instruction("bl __rt_json_depth_exit");                             // decrement _json_active_depth before computing the result slice
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the write pointer after the helper call
    // Compute the result slice from the saved start pointer and updated
    // write pointer, then publish the new concat offset for the caller.
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the encoded-object start pointer
    emitter.instruction("sub x2, x11, x1");                                     // compute the encoded-object byte length
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_concat_buf");
    emitter.instruction("sub x10, x11, x10");                                   // compute the absolute concat-buffer offset after the closing brace
    emitter.instruction("str x10, [x9]");                                       // publish the concat-buffer offset for the next encoder
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // deallocate the object encoder scratch frame
    emitter.instruction("ret");                                                 // return the encoded object slice in the standard string registers
}

fn emit_json_encode_object_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_object ---");
    emitter.label_global("__rt_json_encode_object");

    // Stack layout (rbp-relative, 96 bytes reserved):
    //   [rbp - 8]  = object pointer
    //   [rbp - 16] = output start ptr
    //   [rbp - 24] = current write pos
    //   [rbp - 32] = descriptor pointer
    //   [rbp - 40] = property loop index
    //   [rbp - 48] = property count
    //   [rbp - 56] = scratch (prop_index)
    //   [rbp - 64] = scratch (type_tag)
    //   [rbp - 72] = scratch (prop value lo)
    //   [rbp - 80] = scratch (prop value hi)
    //   [rbp - 88] = scratch (prop name_ptr)
    //   [rbp - 96] = scratch (prop name_len)
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the object encoder
    emitter.instruction("sub rsp, 96");                                         // reserve the encoder scratch frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the object pointer

    // Enter the recursion-depth check.
    emitter.instruction("call __rt_json_depth_enter");                          // increment _json_active_depth and throw on overflow when requested

    // Get the current write position in the concat buffer.
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // load the current concat-buffer offset
    emitter.instruction("lea r11, [rip + _concat_buf]");                        // materialize the concat buffer base
    emitter.instruction("add r11, r10");                                        // compute the write pointer for the encoded object
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the start pointer for the final result slice
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the running write pointer for the encoder loop

    // Bounds-check the class id and resolve the JSON descriptor.
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the object pointer (depth_enter clobbered rax)
    emitter.instruction("mov rcx, QWORD PTR [rax]");                            // load class_id from the object header
    emitter.instruction("mov rdi, QWORD PTR [rip + _class_gc_desc_count]");     // load the total number of registered class descriptors
    emitter.instruction("cmp rcx, rdi");                                        // is the class_id within the descriptor table?
    emitter.instruction("jae __rt_json_obj_open_only_x");                       // an out-of-range class_id falls back to an empty object literal
    emitter.instruction("lea rdi, [rip + _class_json_desc_ptrs]");              // materialize the descriptor pointer table base
    emitter.instruction("mov rdi, QWORD PTR [rdi + rcx*8]");                    // load the descriptor pointer for the current class
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the descriptor pointer for downstream loads

    // Check the JsonSerializable flag.
    emitter.instruction("mov rcx, QWORD PTR [rdi]");                            // load the class JSON descriptor flags word
    emitter.instruction("test rcx, 1");                                         // is JsonSerializable bit set?
    emitter.instruction("je __rt_json_obj_walk_props_x");                       // ordinary classes walk public properties directly
    emitter.instruction("mov rdx, QWORD PTR [rdi + 8]");                        // load the jsonSerialize method symbol from the descriptor
    emitter.instruction("test rdx, rdx");                                       // is the method target populated?
    emitter.instruction("je __rt_json_obj_walk_props_x");                       // missing method targets fall through to the property walker
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // park the jsonSerialize target across helper calls that clobber rdx
    // The user method's body resets _concat_off and writes scratch at the
    // beginning of concat_buf, which would trash any caller prefix already
    // there. Persist the caller's prefix to heap, run jsonSerialize, then
    // copy the prefix back before re-establishing concat_off and encoding.
    emitter.instruction("mov r10, QWORD PTR [rip + _concat_off]");              // capture the caller-visible concat-buffer offset before the user method runs
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the captured offset (= prefix length) across the user method invocation
    emitter.instruction("mov r10, QWORD PTR [rip + _json_last_error]");         // capture the outer JSON error state before user code can run nested JSON calls
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save _json_last_error across jsonSerialize()
    emitter.instruction("mov r10, QWORD PTR [rip + _json_active_flags]");       // capture the outer JSON flag bitmask before user code can change it
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save _json_active_flags across jsonSerialize()
    emitter.instruction("mov r10, QWORD PTR [rip + _json_active_depth]");       // capture the outer JSON depth before user code can reset it
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // save _json_active_depth across jsonSerialize()
    emitter.instruction("mov r10, QWORD PTR [rip + _json_indent_depth]");       // capture the outer pretty-print depth before user code can reset it
    emitter.instruction("mov QWORD PTR [rbp - 88], r10");                       // save _json_indent_depth across jsonSerialize()
    emitter.instruction("mov r10, QWORD PTR [rip + _json_depth_limit]");        // capture the outer JSON depth limit before user code can change it
    emitter.instruction("mov QWORD PTR [rbp - 96], r10");                       // save _json_depth_limit across jsonSerialize()
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // default the saved prefix heap pointer to null for empty prefixes
    emitter.instruction("test r10, r10");                                       // is there any caller-visible prefix to preserve?
    emitter.instruction("jz __rt_json_obj_jsonserialize_invoke_x");             // skip the prefix copy when the prefix is empty
    emitter.instruction("lea rax, [rip + _concat_buf]");                        // materialize the concat-buffer base for the str_persist input
    emitter.instruction("mov rdx, r10");                                        // copy the prefix length into the str_persist length register
    emitter.instruction("call __rt_str_persist");                               // duplicate the caller prefix into a heap-owned buffer
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // remember the heap-owned prefix pointer for the post-call restore
    emitter.label("__rt_json_obj_jsonserialize_invoke_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the receiver object pointer for the method call
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // reload the saved jsonSerialize method target
    emitter.instruction("call rdx");                                            // invoke jsonSerialize on the receiver and capture the boxed mixed result
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // park the boxed mixed return value across the prefix-restore loop
    // Copy the heap-owned prefix back into concat_buf[0..prefix_len].
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the saved prefix length
    emitter.instruction("test r10, r10");                                       // anything to restore?
    emitter.instruction("jz __rt_json_obj_jsonserialize_after_restore_x");      // skip restore for empty prefixes
    emitter.instruction("mov r11, QWORD PTR [rbp - 64]");                       // reload the heap-owned prefix pointer
    emitter.instruction("test r11, r11");                                       // defensive null guard for the heap-owned prefix pointer
    emitter.instruction("jz __rt_json_obj_jsonserialize_after_restore_x");      // skip restore when no heap copy was made
    emitter.instruction("lea r9, [rip + _concat_buf]");                         // materialize the concat-buffer base for the prefix restore loop
    emitter.instruction("xor rcx, rcx");                                        // initialize the prefix copy index
    emitter.label("__rt_json_obj_prefix_restore_x");
    emitter.instruction("cmp rcx, r10");                                        // have we copied every prefix byte back?
    emitter.instruction("jae __rt_json_obj_jsonserialize_after_restore_x");     // exit once the entire prefix is restored
    emitter.instruction("mov dl, BYTE PTR [r11 + rcx]");                        // load the next prefix byte from the heap-owned copy
    emitter.instruction("mov BYTE PTR [r9 + rcx], dl");                         // restore the byte into concat_buf at the original offset
    emitter.instruction("add rcx, 1");                                          // advance the prefix copy index
    emitter.instruction("jmp __rt_json_obj_prefix_restore_x");                  // continue the prefix restore loop
    emitter.label("__rt_json_obj_jsonserialize_after_restore_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the saved pre-call concat-buffer offset
    emitter.instruction("mov QWORD PTR [rip + _concat_off], r10");              // restore the concat-buffer offset so encode_mixed appends at our intended slot
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the outer JSON error state after user code returns
    emitter.instruction("mov QWORD PTR [rip + _json_last_error], r10");         // restore _json_last_error before encoding the serialized value
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the outer JSON flag bitmask after user code returns
    emitter.instruction("mov QWORD PTR [rip + _json_active_flags], r10");       // restore _json_active_flags before encoding the serialized value
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the outer JSON depth after user code returns
    emitter.instruction("mov QWORD PTR [rip + _json_active_depth], r10");       // restore _json_active_depth before recursive JSON encoding resumes
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // reload the outer pretty-print depth after user code returns
    emitter.instruction("mov QWORD PTR [rip + _json_indent_depth], r10");       // restore _json_indent_depth before recursive JSON encoding resumes
    emitter.instruction("mov r10, QWORD PTR [rbp - 96]");                       // reload the outer JSON depth limit after user code returns
    emitter.instruction("mov QWORD PTR [rip + _json_depth_limit], r10");        // restore _json_depth_limit before recursive JSON encoding resumes
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // restore the boxed mixed return value before encoding it
    emitter.instruction("call __rt_json_encode_mixed");                         // encode the boxed mixed return value as JSON
    // Save the (rax, rdx) result across __rt_json_depth_exit.
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // checkpoint the encoded result pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // checkpoint the encoded result length
    emitter.instruction("call __rt_json_depth_exit");                           // decrement _json_active_depth so a sibling encoder can re-enter cleanly
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // restore the encoded result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // restore the encoded result length
    emitter.instruction("mov rsp, rbp");                                        // unwind the encoder scratch frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the encoded result produced by mixed encoding

    emitter.label("__rt_json_obj_open_only_x");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the running write pointer for the open-brace fallback
    emitter.instruction("mov BYTE PTR [r11], 123");                             // emit the opening brace for the empty object literal
    emitter.instruction("add r11, 1");                                          // advance past the opening brace
    emitter.instruction("mov BYTE PTR [r11], 125");                             // emit the closing brace for the empty object literal
    emitter.instruction("add r11, 1");                                          // advance past the closing brace
    emitter.instruction("jmp __rt_json_obj_finalize_x");                        // jump to the shared finalization tail

    emitter.label("__rt_json_obj_walk_props_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the descriptor pointer
    emitter.instruction("mov rcx, QWORD PTR [rdi + 16]");                       // load the public property count
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save the public property count for the loop guard

    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the running write pointer for the opening brace
    emitter.instruction("mov BYTE PTR [r11], 123");                             // emit the opening brace before the first property
    emitter.instruction("add r11, 1");                                          // advance past the opening brace
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the running write pointer after the opening brace
    emitter.instruction("call __rt_json_pretty_push");                          // enter one pretty-print indentation level after the object opens
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the property loop index to zero

    emitter.label("__rt_json_obj_loop_x");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the current property loop index
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reload the total public property count
    emitter.instruction("cmp rcx, rdx");                                        // are there more properties to encode?
    emitter.instruction("jae __rt_json_obj_close_x");                           // emit the closing brace once every property is encoded

    // Compute descriptor entry address: descriptor + 24 + index * 32
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the descriptor pointer for the current iteration
    emitter.instruction("add rdi, 24");                                         // skip the leading flags / jsonSerialize / count words
    emitter.instruction("mov rax, rcx");                                        // copy the loop index for the byte-offset computation
    emitter.instruction("shl rax, 5");                                          // multiply the index by 32 (entry stride)
    emitter.instruction("add rdi, rax");                                        // advance to the requested property descriptor entry

    // Save name_ptr / name_len / prop_index / type_tag in scratch slots.
    emitter.instruction("mov rsi, QWORD PTR [rdi]");                            // load the property name pointer
    emitter.instruction("mov rdx, QWORD PTR [rdi + 8]");                        // load the property name length
    emitter.instruction("mov r8, QWORD PTR [rdi + 16]");                        // load the property runtime slot index
    emitter.instruction("mov r9, QWORD PTR [rdi + 24]");                        // load the property compile-time type tag
    emitter.instruction("mov QWORD PTR [rbp - 88], rsi");                       // save the property name pointer for the key copy loop
    emitter.instruction("mov QWORD PTR [rbp - 96], rdx");                       // save the property name length for the key copy loop
    emitter.instruction("mov QWORD PTR [rbp - 56], r8");                        // save the property slot index for value loading
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // save the property type tag for runtime dispatch

    // Comma separator before all but the first property.
    emitter.instruction("test rcx, rcx");                                       // is this the first property entry?
    emitter.instruction("je __rt_json_obj_key_x");                              // skip the comma before the first property
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the running write pointer
    emitter.instruction("mov BYTE PTR [r11], 44");                              // emit the comma separator between properties
    emitter.instruction("add r11, 1");                                          // advance past the comma
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the running write pointer after the comma

    emitter.label("__rt_json_obj_key_x");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the running write pointer for the key prefix
    emitter.instruction("call __rt_json_pretty_line");                          // append newline and indentation for this property when pretty-printing
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the write pointer after any pretty indentation
    emitter.instruction("mov BYTE PTR [r11], 34");                              // emit the opening quote for the property name
    emitter.instruction("add r11, 1");                                          // advance past the opening quote
    emitter.instruction("mov rsi, QWORD PTR [rbp - 88]");                       // reload the property name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 96]");                       // reload the property name length
    emitter.instruction("xor rcx, rcx");                                        // initialize the key copy index

    emitter.label("__rt_json_obj_key_copy_x");
    emitter.instruction("cmp rcx, rdx");                                        // have we copied every byte of the property name?
    emitter.instruction("jae __rt_json_obj_key_done_x");                        // exit the copy loop once the name is fully written
    emitter.instruction("mov r8b, BYTE PTR [rsi + rcx]");                       // load the next property-name byte from the descriptor string
    emitter.instruction("mov BYTE PTR [r11 + rcx], r8b");                       // copy the byte into the concat buffer
    emitter.instruction("add rcx, 1");                                          // advance the copy index
    emitter.instruction("jmp __rt_json_obj_key_copy_x");                        // continue copying the property name

    emitter.label("__rt_json_obj_key_done_x");
    emitter.instruction("add r11, rdx");                                        // advance the write pointer past the copied property name
    emitter.instruction("mov BYTE PTR [r11], 34");                              // emit the closing quote for the property name
    emitter.instruction("add r11, 1");                                          // advance past the closing quote
    emitter.instruction("mov BYTE PTR [r11], 58");                              // emit the colon separating key and value
    emitter.instruction("add r11, 1");                                          // advance past the colon
    emitter.instruction("call __rt_json_pretty_colon_space");                   // append the pretty-print key/value space when requested
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the running write pointer after the key prefix

    // Sync concat_off so nested encoders append after the existing prefix.
    emitter.instruction("lea rdi, [rip + _concat_buf]");                        // materialize the concat buffer base
    emitter.instruction("mov rcx, r11");                                        // copy the running write pointer for the offset computation
    emitter.instruction("sub rcx, rdi");                                        // compute the absolute concat offset for the prefix tail
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // publish the concat offset for nested value encoders

    // Load property value from the object instance (slot offset = 8 + prop_index * 16).
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // reload the property runtime slot index
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the receiver object pointer
    emitter.instruction("mov rcx, r8");                                         // copy the slot index for the offset computation
    emitter.instruction("shl rcx, 4");                                          // multiply by 16 (slot stride)
    emitter.instruction("add rcx, 8");                                          // skip the leading class_id field at offset 0
    emitter.instruction("add rax, rcx");                                        // resolve the absolute address of the property slot
    emitter.instruction("mov rdx, QWORD PTR [rax]");                            // load the property low payload word
    emitter.instruction("mov rsi, QWORD PTR [rax + 8]");                        // load the property high payload word
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // save the property low payload across the encoder dispatch
    emitter.instruction("mov QWORD PTR [rbp - 80], rsi");                       // save the property high payload across the encoder dispatch

    // Dispatch on the property type tag.
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload the saved property type tag
    emitter.instruction("cmp r9, 0");                                           // tag 0 = integer
    emitter.instruction("je __rt_json_obj_val_int_x");                          // branch on the current JSON object encoder condition
    emitter.instruction("cmp r9, 1");                                           // tag 1 = string
    emitter.instruction("je __rt_json_obj_val_str_x");                          // branch on the current JSON object encoder condition
    emitter.instruction("cmp r9, 2");                                           // tag 2 = float
    emitter.instruction("je __rt_json_obj_val_float_x");                        // branch on the current JSON object encoder condition
    emitter.instruction("cmp r9, 3");                                           // tag 3 = bool
    emitter.instruction("je __rt_json_obj_val_bool_x");                         // branch on the current JSON object encoder condition
    emitter.instruction("cmp r9, 4");                                           // tag 4 = indexed array
    emitter.instruction("je __rt_json_obj_val_array_x");                        // branch on the current JSON object encoder condition
    emitter.instruction("cmp r9, 5");                                           // tag 5 = associative array
    emitter.instruction("je __rt_json_obj_val_assoc_x");                        // branch on the current JSON object encoder condition
    emitter.instruction("cmp r9, 6");                                           // tag 6 = nested object
    emitter.instruction("je __rt_json_obj_val_object_x");                       // branch on the current JSON object encoder condition
    emitter.instruction("cmp r9, 7");                                           // tag 7 = boxed mixed
    emitter.instruction("je __rt_json_obj_val_mixed_x");                        // branch on the current JSON object encoder condition
    emitter.instruction("jmp __rt_json_obj_val_null_x");                        // every other tag falls back to JSON null

    emitter.label("__rt_json_obj_val_int_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // load the integer payload
    emitter.instruction("call __rt_itoa");                                      // format the integer as a decimal string
    emitter.instruction("jmp __rt_json_obj_val_copy_x");                        // continue in the JSON object encoder control path

    emitter.label("__rt_json_obj_val_str_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // load the string pointer payload
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // load the string length payload
    emitter.instruction("call __rt_json_encode_str");                           // encode the string as a quoted JSON string
    emitter.instruction("jmp __rt_json_obj_val_copy_x");                        // continue in the JSON object encoder control path

    emitter.label("__rt_json_obj_val_float_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // load the float bit pattern payload
    emitter.instruction("movq xmm0, r10");                                      // move the float bits into the FP argument register
    emitter.instruction("call __rt_json_encode_float");                         // encode the float, rejecting Inf/NaN per JSON semantics
    emitter.instruction("jmp __rt_json_obj_val_copy_x");                        // continue in the JSON object encoder control path

    emitter.label("__rt_json_obj_val_bool_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // load the bool payload
    emitter.instruction("call __rt_json_encode_bool");                          // encode the bool as the true/false literal
    emitter.instruction("jmp __rt_json_obj_val_copy_x");                        // continue in the JSON object encoder control path

    emitter.label("__rt_json_obj_val_array_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // load the indexed-array pointer payload
    emitter.instruction("call __rt_json_encode_array_dynamic");                 // encode the array via the dynamic array helper
    emitter.instruction("jmp __rt_json_obj_val_copy_x");                        // continue in the JSON object encoder control path

    emitter.label("__rt_json_obj_val_assoc_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // load the associative-array pointer payload
    emitter.instruction("call __rt_json_encode_assoc");                         // encode the hash table as a JSON object
    emitter.instruction("jmp __rt_json_obj_val_copy_x");                        // continue in the JSON object encoder control path

    emitter.label("__rt_json_obj_val_object_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // load the nested object pointer payload
    emitter.instruction("call __rt_json_encode_object");                        // recursively encode the nested object
    emitter.instruction("jmp __rt_json_obj_val_copy_x");                        // continue in the JSON object encoder control path

    emitter.label("__rt_json_obj_val_mixed_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // load the boxed mixed pointer payload
    emitter.instruction("call __rt_json_encode_mixed");                         // encode the boxed mixed payload recursively
    emitter.instruction("jmp __rt_json_obj_val_copy_x");                        // continue in the JSON object encoder control path

    emitter.label("__rt_json_obj_val_null_x");
    emitter.instruction("call __rt_json_encode_null");                          // encode unsupported tags as the JSON null literal

    emitter.label("__rt_json_obj_val_copy_x");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the running write pointer
    emitter.instruction("xor rcx, rcx");                                        // initialize the value copy index

    emitter.label("__rt_json_obj_val_copy_loop_x");
    emitter.instruction("cmp rcx, rdx");                                        // have we copied every byte of the encoded value?
    emitter.instruction("jae __rt_json_obj_val_copy_done_x");                   // exit the value copy loop once finished
    emitter.instruction("mov r8b, BYTE PTR [rax + rcx]");                       // load the next encoded value byte
    emitter.instruction("mov BYTE PTR [r11 + rcx], r8b");                       // store it into the concat buffer at the running write pointer
    emitter.instruction("add rcx, 1");                                          // advance the value copy index
    emitter.instruction("jmp __rt_json_obj_val_copy_loop_x");                   // continue copying the encoded value

    emitter.label("__rt_json_obj_val_copy_done_x");
    emitter.instruction("add r11, rdx");                                        // advance the write pointer past the copied value
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save the running write pointer for the next iteration

    // Advance the loop index and continue.
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the property loop index
    emitter.instruction("add rcx, 1");                                          // advance to the next property entry
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the updated loop index
    emitter.instruction("jmp __rt_json_obj_loop_x");                            // continue with the next property entry

    emitter.label("__rt_json_obj_close_x");
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the running write pointer for the closing brace
    emitter.instruction("call __rt_json_pretty_pop");                           // leave the object indentation level before closing it
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // did the object contain any public properties?
    emitter.instruction("je __rt_json_obj_close_emit_x");                       // empty objects stay compact even under JSON_PRETTY_PRINT
    emitter.instruction("call __rt_json_pretty_line");                          // append the closing-line indentation for non-empty pretty objects
    emitter.label("__rt_json_obj_close_emit_x");
    emitter.instruction("mov BYTE PTR [r11], 125");                             // emit the closing brace after the last property
    emitter.instruction("add r11, 1");                                          // advance past the closing brace

    emitter.label("__rt_json_obj_finalize_x");
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // checkpoint the write pointer across the depth-exit helper call
    emitter.instruction("call __rt_json_depth_exit");                           // decrement _json_active_depth before computing the result slice
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the write pointer after the helper call
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the encoded-object start pointer
    emitter.instruction("mov rdx, r11");                                        // copy the final write pointer for the length computation
    emitter.instruction("sub rdx, rax");                                        // compute the encoded-object byte length
    emitter.instruction("lea rdi, [rip + _concat_buf]");                        // materialize the concat buffer base
    emitter.instruction("mov rcx, r11");                                        // copy the final write pointer for the offset update
    emitter.instruction("sub rcx, rdi");                                        // compute the absolute concat-buffer offset after the closing brace
    emitter.instruction("mov QWORD PTR [rip + _concat_off], rcx");              // publish the concat-buffer offset for the next encoder
    emitter.instruction("mov rsp, rbp");                                        // unwind the encoder scratch frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the encoded object slice in the standard string registers
}
