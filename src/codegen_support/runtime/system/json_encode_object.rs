//! Purpose:
//! Emits JSON object encoder runtime helper.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system` during runtime emission.
//!
//! Key details:
//! - Public-property walking and JsonSerializable dispatch depend on emitted per-class JSON descriptors.
//! - A serializer returning its enum receiver walks its persistent properties once.

use crate::codegen_support::sentinels::emit_deref_reference_property_slot;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::abi;
use crate::codegen_support::sentinels::{emit_resolve_prop_desc_tag, emit_resolve_tagged_scalar_property_tag};

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
pub(crate) fn emit_json_encode_object(emitter: &mut Emitter, eval_bridge: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_json_encode_object_linux_x86_64(emitter, eval_bridge);
        return;
    }

    emit_dynamic_tail_wrapper_aarch64(emitter);
    emitter.blank();
    emitter.comment("--- runtime: json_encode_object (declared properties) ---");
    emitter.label_global("__rt_json_encode_object_decl");
    super::json_encode_enum::emit_dispatch(emitter);

    // -- set up stack frame --
    // Stack layout (160 bytes):
    //   [sp, #0]   = object pointer
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
    //   [sp, #128..#159] = output-buffer descriptor (start, cursor, end, opening offset)
    emitter.instruction("sub sp, sp, #160");                                    // allocate the object encoder scratch frame plus the output-buffer descriptor
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish a stable frame pointer for the encoder
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer for downstream loads

    // -- enter the recursion-depth check --
    emitter.instruction("bl __rt_json_depth_enter");                            // increment _json_active_depth and throw on overflow when requested

    // -- open the output buffer at the scratch tail --
    emitter.instruction("add x0, sp, #128");                                    // address of the output-buffer descriptor
    emitter.instruction("bl __rt_strbuf_init");                                 // open the buffer at the current scratch offset
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer the helper call clobbered

    if eval_bridge {
        // An object the interpreter declared carries the generic wrapper's class id, which may
        // not even index the descriptor table, and whose descriptor would know no
        // `jsonSerialize()` and no properties. Ask the bridge before the descriptor lookup: the
        // thunk below stands in for the descriptor's method target and hands back what the
        // interpreter would encode, which the shared tail then encodes as the serialized value.
        emitter.instruction("ldr x0, [sp, #0]");                                // pass the object identity to the owner probe
        let probe = emitter.target.extern_symbol("__elephc_eval_object_has_dynamic_owner");
        abi::emit_call_label(emitter, &probe);
        emitter.instruction("cbz x0, __rt_json_obj_native_descriptor");         // a native object keeps its descriptor
        abi::emit_symbol_address(emitter, "x14", "__rt_json_eval_serialize_thunk");
        emitter.instruction("str xzr, [sp, #56]");                              // the bridge thunk already returns a boxed Mixed cell
        emitter.instruction("b __rt_json_obj_have_target");                     // dispatch through the interpreter-backed target
        emitter.label("__rt_json_obj_native_descriptor");
        emitter.instruction("ldr x0, [sp, #0]");                                // reload the object pointer for the class-id read
    }
    // -- bounds-check the class id and resolve the JSON descriptor --
    emitter.instruction("ldr x12, [x0]");                                       // load class_id from the object header
    crate::codegen_support::abi::emit_symbol_address(emitter, "x13", "_class_gc_desc_count");
    emitter.instruction("ldr x13, [x13]");                                      // load the total number of registered class descriptors
    emitter.instruction("cmp x12, x13");                                        // is the class_id within the descriptor table?
    emitter.instruction("b.hs __rt_json_obj_open_only");                        // an out-of-range class_id falls back to an empty object literal
    crate::codegen_support::abi::emit_symbol_address(emitter, "x13", "_class_json_desc_ptrs");
    emitter.instruction("lsl x14, x12, #3");                                    // scale class_id by 8 bytes per descriptor pointer
    emitter.instruction("ldr x13, [x13, x14]");                                 // load the descriptor pointer for the current class
    emitter.instruction("str x13, [sp, #24]");                                  // save the descriptor pointer for downstream loads

    // -- check the JsonSerializable flag --
    emitter.instruction("ldr x12, [x13]");                                      // load the class JSON descriptor flags word
    emitter.instruction("str x12, [sp, #56]");                                  // keep the flags: bits 8..15 name jsonSerialize()'s raw return shape
    emitter.instruction("tst x12, #1");                                         // is JsonSerializable bit set on the descriptor?
    emitter.instruction("b.eq __rt_json_obj_walk_props");                       // ordinary classes walk public properties directly
    emitter.instruction("ldr x14, [x13, #8]");                                  // load the jsonSerialize method symbol from the descriptor
    emitter.instruction("cbz x14, __rt_json_obj_walk_props");                   // missing method targets fall through to the property walker
    emitter.label("__rt_json_obj_have_target");
    emitter.instruction("str x14, [sp, #32]");                                  // park the jsonSerialize target across helper calls that clobber x14
    // The user method's body resets _concat_off and writes scratch at the
    // beginning of concat_buf, which would trash any caller prefix already
    // there (e.g. an enclosing assoc/array encoder's `{"key":` bytes).
    // Persist the caller's prefix to heap, run jsonSerialize, then copy the
    // prefix back before re-establishing concat_off and encoding the result.
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // capture the caller-visible concat-buffer offset before the user method runs
    emitter.instruction("str x10, [sp, #24]");                                  // save the captured offset (= prefix length) across the user method invocation
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_last_error");
    emitter.instruction("ldr x10, [x9]");                                       // capture the outer JSON error state before user code can run nested JSON calls
    emitter.instruction("str x10, [sp, #80]");                                  // save _json_last_error across jsonSerialize()
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x10, [x9]");                                       // capture the outer JSON flag bitmask before user code can change it
    emitter.instruction("str x10, [sp, #88]");                                  // save _json_active_flags across jsonSerialize()
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_active_depth");
    emitter.instruction("ldr x10, [x9]");                                       // capture the outer JSON depth before user code can reset it
    emitter.instruction("str x10, [sp, #96]");                                  // save _json_active_depth across jsonSerialize()
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_indent_depth");
    emitter.instruction("ldr x10, [x9]");                                       // capture the outer pretty-print depth before user code can reset it
    emitter.instruction("str x10, [sp, #104]");                                 // save _json_indent_depth across jsonSerialize()
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_depth_limit");
    emitter.instruction("ldr x10, [x9]");                                       // capture the outer JSON depth limit before user code can change it
    emitter.instruction("str x10, [sp, #72]");                                  // save _json_depth_limit across jsonSerialize()
    emitter.instruction("str xzr, [sp, #40]");                                  // default the saved prefix heap pointer to null for empty prefixes
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the prefix length: x10 still holds _json_depth_limit, and a zero limit skipped the copy while a non-zero one persisted that many bytes
    emitter.instruction("cbz x10, __rt_json_obj_jsonserialize_invoke");         // skip the prefix copy when there is no caller-visible prefix to preserve
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_concat_buf");
    emitter.instruction("mov x2, x10");                                         // copy the prefix length into the str_persist length register
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the caller prefix into a heap-owned buffer
    emitter.instruction("str x1, [sp, #40]");                                   // remember the heap-owned prefix pointer for the post-call restore
    emitter.label("__rt_json_obj_jsonserialize_invoke");
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore the receiver object pointer for the method call
    emitter.instruction("ldr x14, [sp, #32]");                                  // reload the saved jsonSerialize method target
    emitter.instruction("blr x14");                                             // invoke jsonSerialize on the receiver and capture its result
    // -- box a raw return (`jsonSerialize(): string`, `: array`, ...) like a Mixed one --
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload the descriptor flags word
    emitter.instruction("ubfx x9, x9, #8, #8");                                 // extract the raw return tag + 1 (0 = already boxed)
    emitter.instruction("cbz x9, __rt_json_obj_jsonserialize_boxed");           // a Mixed return is already the cell to encode
    emitter.instruction("sub x9, x9, #1");                                      // recover the runtime value tag of the raw return
    emitter.instruction("cmp x9, #1");                                          // a string return already sits in x1/x2
    emitter.instruction("b.eq __rt_json_obj_jsonserialize_box");                // box the string pair as returned
    emitter.instruction("cmp x9, #2");                                          // a float return sits in d0
    emitter.instruction("b.ne __rt_json_obj_jsonserialize_word");               // every other shape is one word in x0
    emitter.instruction("fmov x1, d0");                                         // move the float bits into the payload register
    emitter.instruction("b __rt_json_obj_jsonserialize_box");                   // box the float
    emitter.label("__rt_json_obj_jsonserialize_word");
    emitter.instruction("mov x1, x0");                                          // the one-word payload (int, bool, container, object)
    emitter.label("__rt_json_obj_jsonserialize_box");
    emitter.instruction("stp x9, x1, [sp, #56]");                               // keep the tag and payload to give the method's own reference back
    emitter.instruction("mov x0, x9");                                          // value tag argument for the boxing helper
    emitter.instruction("bl __rt_mixed_from_value");                            // box the raw value (it retains or persists its own copy)
    emitter.instruction("str x0, [sp, #48]");                                   // park the fresh box
    emitter.instruction("ldp x9, x10, [sp, #56]");                              // reload the raw tag and payload
    emitter.instruction("cmp x9, #1");                                          // a returned string is the call's own copy
    emitter.instruction("b.ne __rt_json_obj_jsonserialize_not_string");
    emitter.instruction("mov x0, x10");                                         // the method's string; the box persisted its own copy
    emitter.instruction("bl __rt_heap_free_safe");                              // validated free: scratch or static strings are left alone
    emitter.instruction("b __rt_json_obj_jsonserialize_released");
    emitter.label("__rt_json_obj_jsonserialize_not_string");
    emitter.instruction("cmp x9, #4");                                          // containers and objects carry the method's reference
    emitter.instruction("b.lo __rt_json_obj_jsonserialize_released");           // scalars own nothing to give back
    emitter.instruction("mov x0, x10");                                         // the method's returned container/object
    emitter.instruction("bl __rt_decref_any");                                  // the box holds its own reference now
    emitter.label("__rt_json_obj_jsonserialize_released");
    emitter.instruction("ldr x0, [sp, #48]");                                   // the boxed value to encode
    emitter.label("__rt_json_obj_jsonserialize_boxed");
    emitter.instruction("str x0, [sp, #48]");                                   // park the boxed mixed return value across the prefix-restore loop
    // Copy the heap-owned prefix back into concat_buf[0..prefix_len].
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the saved prefix length
    emitter.instruction("cbz x10, __rt_json_obj_jsonserialize_after_restore");  // skip the prefix restore when no prefix was preserved
    emitter.instruction("ldr x11, [sp, #40]");                                  // reload the heap-owned prefix pointer
    emitter.instruction("cbz x11, __rt_json_obj_jsonserialize_after_restore");  // defensive null guard for the heap-owned prefix pointer
    crate::codegen_support::abi::emit_symbol_address(emitter, "x12", "_concat_buf");
    emitter.instruction("mov x13, #0");                                         // initialize the prefix copy index
    emitter.label("__rt_json_obj_prefix_restore");
    emitter.instruction("cmp x13, x10");                                        // have we copied every prefix byte back?
    emitter.instruction("b.ge __rt_json_obj_jsonserialize_after_restore");      // exit once the entire prefix is restored
    emitter.instruction("ldrb w14, [x11, x13]");                                // load the next prefix byte from the heap-owned copy
    emitter.instruction("strb w14, [x12, x13]");                                // restore the byte into concat_buf at the original offset
    emitter.instruction("add x13, x13, #1");                                    // advance the prefix copy index
    emitter.instruction("b __rt_json_obj_prefix_restore");                      // continue the prefix restore loop
    emitter.label("__rt_json_obj_jsonserialize_after_restore");
    emitter.instruction("ldr x0, [sp, #40]");                                   // load the owned prefix copy after its bytes have been restored
    emitter.instruction("bl __rt_decref_any");                                  // release the temporary string copy, including the null-prefix case
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the saved pre-call concat-buffer offset
    emitter.instruction("str x10, [x9]");                                       // restore the concat-buffer offset so encode_mixed appends at our intended slot
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_last_error");
    emitter.instruction("ldr x10, [sp, #80]");                                  // reload the outer JSON error state after user code returns
    emitter.instruction("str x10, [x9]");                                       // restore _json_last_error before encoding the serialized value
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_active_flags");
    emitter.instruction("ldr x10, [sp, #88]");                                  // reload the outer JSON flag bitmask after user code returns
    emitter.instruction("str x10, [x9]");                                       // restore _json_active_flags before encoding the serialized value
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_active_depth");
    emitter.instruction("ldr x10, [sp, #96]");                                  // reload the outer JSON depth after user code returns
    emitter.instruction("str x10, [x9]");                                       // restore _json_active_depth before recursive JSON encoding resumes
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_indent_depth");
    emitter.instruction("ldr x10, [sp, #104]");                                 // reload the outer pretty-print depth after user code returns
    emitter.instruction("str x10, [x9]");                                       // restore _json_indent_depth before recursive JSON encoding resumes
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_json_depth_limit");
    emitter.instruction("ldr x10, [sp, #72]");                                  // reload the outer JSON depth limit after user code returns
    emitter.instruction("str x10, [x9]");                                       // restore _json_depth_limit before recursive JSON encoding resumes
    emitter.instruction("ldr x0, [sp, #48]");                                   // restore the boxed mixed return value before encoding it
    // -- expose persistent enum properties once on a serializer self return --
    emitter.instruction("cbz x0, __rt_json_obj_jsonserialize_mixed");           // null returns retain the ordinary mixed encoder
    emitter.instruction("ldr x9, [x0]");                                        // inspect the boxed result tag
    emitter.instruction("cmp x9, #6");                                          // only object results can be the receiver
    emitter.instruction("b.ne __rt_json_obj_jsonserialize_mixed");              // other values retain recursive encoding
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the returned object identity
    emitter.instruction("ldr x10, [sp, #0]");                                   // load the original receiver identity
    emitter.instruction("cmp x9, x10");                                         // did jsonSerialize return its own receiver?
    emitter.instruction("b.ne __rt_json_obj_jsonserialize_mixed");              // distinct objects retain recursive encoding
    emitter.instruction("ldr x12, [x10]");                                      // load the receiver class id after identity comparison
    abi::emit_symbol_address(emitter, "x13", "_class_enum_kinds");
    emitter.instruction("ldr x13, [x13, x12, lsl #3]");                         // identify persistent enum receivers
    emitter.instruction("cbz x13, __rt_json_obj_jsonserialize_mixed");          // ordinary classes retain their existing recursive path
    emitter.instruction("bl __rt_decref_mixed");                                // retire the owned result box while its enum receiver remains persistent
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the still-owned receiver after cleanup
    emitter.instruction("ldr x12, [x12]");                                      // recover its class id
    abi::emit_symbol_address(emitter, "x13", "_class_json_desc_ptrs");
    emitter.instruction("ldr x13, [x13, x12, lsl #3]");                         // recover the descriptor overwritten by prefix storage
    emitter.instruction("str x13, [sp, #24]");                                  // restore the descriptor for the property walker
    emitter.instruction("b __rt_json_obj_walk_props");                          // walk the receiver without invoking the serializer again
    emitter.label("__rt_json_obj_jsonserialize_mixed");
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload a non-self result for ordinary recursive encoding
    emitter.instruction("bl __rt_json_encode_mixed");                           // encode the boxed mixed return value as JSON
    // Save the (x1, x2) result across the depth-exit helper call.
    emitter.instruction("stp x1, x2, [sp, #32]");                               // checkpoint the encoded result slice across __rt_json_depth_exit
    emitter.instruction("ldr x0, [sp, #48]");                                   // the serialized value's box: jsonSerialize() returned it owned
    emitter.instruction("bl __rt_decref_mixed");                                // its JSON text is already in concat_buf
    emitter.instruction("bl __rt_json_depth_exit");                             // decrement _json_active_depth so a sibling encoder can re-enter cleanly
    emitter.instruction("ldp x1, x2, [sp, #32]");                               // restore the encoded result slice after the helper call
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address after JsonSerializable encoding
    emitter.instruction("add sp, sp, #160");                                    // deallocate the object encoder scratch frame
    emitter.instruction("ret");                                                 // return the JSON encoded result produced by mixed encoding

    if eval_bridge {
        // jsonSerialize() stand-in for an interpreter-owned object: x0 = object, returns the
        // boxed Mixed value to encode in x0 (null when the interpreter cannot answer).
        emitter.label("__rt_json_eval_serialize_thunk");
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // preserve the encoder's frame across the bridge call
        let serialize = emitter.target.extern_symbol("__elephc_eval_json_serialize");
        abi::emit_call_label(emitter, &serialize);
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore the encoder's frame
        emitter.instruction("ret");                                             // return the boxed value like a compiled jsonSerialize()
    }

    // -- empty object fallback (missing class id) --
    emitter.label("__rt_json_obj_open_only");
    emitter.instruction("add x0, sp, #128");                                    // address of the output-buffer descriptor
    emitter.instruction("mov x1, #2");                                          // room for the empty object literal
    emitter.instruction("bl __rt_json_buf_ensure");                             // x11 = cursor with headroom reserved
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

    emitter.instruction("add x0, sp, #128");                                    // address of the output-buffer descriptor
    emitter.instruction("mov x1, #1");                                          // room for the opening brace
    emitter.instruction("bl __rt_json_buf_ensure");                             // x11 = cursor with headroom reserved
    emitter.instruction("mov w12, #123");                                       // ASCII '{'
    emitter.instruction("strb w12, [x11]");                                     // emit the opening brace before the first property
    emitter.instruction("add x11, x11, #1");                                    // advance past the opening brace
    emitter.instruction("str x11, [sp, #136]");                                 // save the running write pointer after the opening brace
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

    // Reserve room for the separator, the indentation, and the key punctuation.
    emitter.instruction("add x0, sp, #128");                                    // address of the output-buffer descriptor
    emitter.instruction("mov x1, #1");                                          // room for the comma separator
    emitter.instruction("bl __rt_json_buf_ensure");                             // grow first: every punctuation byte of this key prefix fits after it

    // Comma separator before all but the first property
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the property loop index after the reservation
    emitter.instruction("cbz x10, __rt_json_obj_key");                          // skip the comma before the first property entry
    emitter.instruction("ldr x11, [sp, #136]");                                 // reload the running write pointer
    emitter.instruction("mov w12, #44");                                        // ASCII ','
    emitter.instruction("strb w12, [x11]");                                     // emit the comma between encoded property entries
    emitter.instruction("add x11, x11, #1");                                    // advance past the comma separator
    emitter.instruction("str x11, [sp, #136]");                                 // save the running write pointer after the comma

    emitter.label("__rt_json_obj_key");
    emitter.instruction("ldr x11, [sp, #136]");                                 // reload the running write pointer for the key prefix
    emitter.instruction("bl __rt_json_pretty_line");                            // append newline and indentation for this property when pretty-printing
    emitter.instruction("str x11, [sp, #136]");                                 // save the write pointer after any pretty indentation
    emitter.instruction("mov w12, #34");                                        // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                     // emit the opening quote for the property name
    emitter.instruction("add x11, x11, #1");                                    // advance past the opening quote
    emitter.instruction("str x11, [sp, #136]");                                 // record the cursor past the opening quote

    // Append the property name bytes (no escaping — PHP property names cannot
    // legally contain JSON-significant control or escape chars). The append grows
    // the buffer first, so a long name cannot run past it.
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload the descriptor pointer
    emitter.instruction("add x13, x13, #24");                                   // skip the leading flags / jsonSerialize / count words
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the property loop index
    emitter.instruction("add x13, x13, x10, lsl #5");                           // advance to the current 32-byte property descriptor entry
    emitter.instruction("ldr x1, [x13]");                                       // property name pointer
    emitter.instruction("ldr x2, [x13, #8]");                                   // property name length
    emitter.instruction("add x0, sp, #128");                                    // address of the output-buffer descriptor
    emitter.instruction("bl __rt_json_buf_append");                             // copy the name to the cursor; x11 = advanced cursor
    emitter.instruction("mov w12, #34");                                        // ASCII '"'
    emitter.instruction("strb w12, [x11]");                                     // emit the closing quote for the property name
    emitter.instruction("add x11, x11, #1");                                    // advance past the closing quote
    emitter.instruction("mov w12, #58");                                        // ASCII ':'
    emitter.instruction("strb w12, [x11]");                                     // emit the colon separating key and value
    emitter.instruction("add x11, x11, #1");                                    // advance past the colon
    emitter.instruction("bl __rt_json_pretty_colon_space");                     // append the pretty-print key/value space when requested
    emitter.instruction("str x11, [sp, #136]");                                 // save the running write pointer after the key prefix

    // Sync concat_off so nested encoders do not format over the existing prefix.
    emitter.instruction("add x0, sp, #128");                                    // address of the output-buffer descriptor
    emitter.instruction("bl __rt_strbuf_sync");                                 // publish the scratch offset for nested value encoders

    // Load property value from the object instance.
    // Property slot offset = 8 + prop_index * 16
    emitter.instruction("ldr x16, [sp, #48]");                                  // reload the property runtime slot index
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the receiver object pointer
    emitter.instruction("mov x9, #16");                                         // each property slot occupies 16 bytes
    emitter.instruction("mul x9, x16, x9");                                     // compute the property slot byte offset within the instance
    emitter.instruction("add x9, x9, #8");                                      // skip the leading class_id field at offset 0
    emitter.instruction("add x9, x0, x9");                                      // resolve the absolute address of the property slot
    // x14, not x18: Apple AArch64 reserves x18 for the OS, which is free to overwrite it at
    // any point — the load and the store below are two instructions apart, and that is enough.
    // x14 carries nothing across this whole region, and x15 already served the same purpose
    // for the high word to avoid clobbering the caller's x19.
    emitter.instruction("ldr x14, [x9]");                                       // load the property low payload word
    emitter.instruction("ldr x15, [x9, #8]");                                   // load the property high payload word without clobbering caller x19
    emitter.instruction("ldr x17, [sp, #56]");                                  // reload the descriptor tag before it is dispatched on
    emit_deref_reference_property_slot(emitter, "x17", "x14", "x15", "x9");
    emit_resolve_prop_desc_tag(emitter, "x17", "x15", "x9");
    emitter.instruction("str x17, [sp, #56]");                                  // save the tag the slot actually carries back over the descriptor tag
    emitter.instruction("str x14, [sp, #64]");                                  // save the property low payload across the encoder dispatch
    emitter.instruction("str x15, [sp, #72]");                                  // save the property high payload across the encoder dispatch

    // Dispatch on the property type tag. Each branch leaves the encoded
    // result in x1=ptr, x2=len so the shared copy code can append it.
    emitter.instruction("ldr x17, [sp, #56]");                                  // reload the saved property type tag
    emit_resolve_tagged_scalar_property_tag(emitter, "x17", "x15");
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
    // Append the encoded value bytes from (x1, x2) at the running write
    // position. Scalar encoders write in place at the prefix tail, but a
    // grown container or a long string comes back in its own transient block,
    // which the append copies and releases.
    emitter.instruction("add x0, sp, #128");                                    // address of the output-buffer descriptor
    emitter.instruction("bl __rt_json_buf_append");                             // copy the encoded value to the cursor, growing first

    // Advance the loop index and continue.
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the property loop index
    emitter.instruction("add x10, x10, #1");                                    // advance to the next property entry
    emitter.instruction("str x10, [sp, #32]");                                  // save the updated loop index
    emitter.instruction("b __rt_json_obj_loop");                                // continue with the next property entry

    emitter.label("__rt_json_obj_close");
    emitter.instruction("add x0, sp, #128");                                    // address of the output-buffer descriptor
    emitter.instruction("mov x1, #1");                                          // room for the closing brace
    emitter.instruction("bl __rt_json_buf_ensure");                             // x11 = cursor with headroom for the closing line
    emitter.instruction("bl __rt_json_pretty_pop");                             // leave the object indentation level before closing it
    emitter.instruction("ldr x14, [sp, #40]");                                  // reload public property count to decide whether closing needs its own line
    emitter.instruction("cbz x14, __rt_json_obj_close_emit");                   // empty objects stay compact even under JSON_PRETTY_PRINT
    emitter.instruction("bl __rt_json_pretty_line");                            // append the closing-line indentation for non-empty pretty objects
    emitter.label("__rt_json_obj_close_emit");
    emitter.instruction("mov w12, #125");                                       // ASCII '}'
    emitter.instruction("strb w12, [x11]");                                     // emit the closing brace after the last property
    emitter.instruction("add x11, x11, #1");                                    // advance past the closing brace

    emitter.label("__rt_json_obj_finalize");
    emitter.instruction("str x11, [sp, #136]");                                 // checkpoint the write pointer across the depth-exit helper call
    emitter.instruction("bl __rt_json_depth_exit");                             // decrement _json_active_depth before computing the result slice
    // Return the finished slice and publish the scratch offset for the caller.
    emitter.instruction("add x0, sp, #128");                                    // address of the output-buffer descriptor
    emitter.instruction("bl __rt_strbuf_finish");                               // x1/x2 = encoded object, _concat_off published
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #160");                                    // deallocate the object encoder scratch frame
    emitter.instruction("ret");                                                 // return the encoded object slice in the standard string registers
}

/// x86_64 SysV ABI implementation of `__rt_json_encode_object`.
///
/// Mirrors the ARM64 path but uses rbp-relative stack slots and System V
/// calling conventions. Dispatches to `jsonSerialize()` when the class
/// implements `JsonSerializable`, otherwise walks the public-property table.
///
/// Input:
///   rax = object pointer (SysV first integer argument)
///
/// Output:
///   rax = ptr in concat_buf, rdx = length
///
/// Stack layout (rbp-relative, 128 bytes):
///   [rbp - 8]  = object pointer
///   [rbp - 128..-97] = output-buffer descriptor (start, cursor, end, opening offset)
///   [rbp - 32] = descriptor pointer / prefix length
///   [rbp - 40] = property loop index / _json_last_error
///   [rbp - 48] = property count / _json_active_flags
///   [rbp - 56] = scratch (prop_index)
///   [rbp - 64] = scratch (type_tag) / saved prefix ptr
///   [rbp - 72] = scratch (prop value lo)
///   [rbp - 80] = scratch (prop value hi) / _json_active_depth
///   [rbp - 88] = scratch (prop name_ptr) / _json_indent_depth
///   [rbp - 96] = scratch (prop name_len) / _json_depth_limit
///
/// Called from:
/// - `emit_json_encode_object` when `emitter.target.arch == Arch::X86_64`.
fn emit_json_encode_object_linux_x86_64(emitter: &mut Emitter, eval_bridge: bool) {
    emit_dynamic_tail_wrapper_x86_64(emitter);
    emitter.blank();
    emitter.comment("--- runtime: json_encode_object (declared properties) ---");
    emitter.label_global("__rt_json_encode_object_decl");
    super::json_encode_enum::emit_dispatch(emitter);

    // Stack layout (rbp-relative, 128 bytes reserved):
    //   [rbp - 8]  = object pointer
    //   [rbp - 128..-97] = output-buffer descriptor (start, cursor, end, opening offset)
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
    emitter.instruction("sub rsp, 128");                                        // reserve the encoder scratch frame plus the output-buffer descriptor at [rbp - 128]
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the object pointer

    // Enter the recursion-depth check.
    emitter.instruction("call __rt_json_depth_enter");                          // increment _json_active_depth and throw on overflow when requested

    // Open the output buffer at the scratch tail.
    emitter.instruction("lea rdi, [rbp - 128]");                                // address of the output-buffer descriptor
    emitter.instruction("call __rt_strbuf_init");                               // open the buffer at the current scratch offset

    if eval_bridge {
        // See the AArch64 emitter: an interpreter-owned object is answered by the bridge, before
        // its wrapper class id is used to index the descriptor table.
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // pass the object identity to the owner probe
        let probe = emitter.target.extern_symbol("__elephc_eval_object_has_dynamic_owner");
        abi::emit_call_label(emitter, &probe);
        emitter.instruction("test rax, rax");                                   // does the interpreter own this object?
        emitter.instruction("jz __rt_json_obj_native_descriptor_x");            // a native object keeps its descriptor
        abi::emit_symbol_address(emitter, "rdx", "__rt_json_eval_serialize_thunk");
        emitter.instruction("mov QWORD PTR [rbp - 16], 0");                     // the bridge thunk already returns a boxed Mixed cell
        emitter.instruction("jmp __rt_json_obj_have_target_x");                 // dispatch through the interpreter-backed target
        emitter.label("__rt_json_obj_native_descriptor_x");
    }
    // Bounds-check the class id and resolve the JSON descriptor.
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the receiver for descriptor lookup
    emitter.instruction("mov rcx, QWORD PTR [rax]");                            // load class_id from the object header
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_class_gc_desc_count", 0);    // load the total number of registered class descriptors
    emitter.instruction("cmp rcx, rdi");                                        // is the class_id within the descriptor table?
    emitter.instruction("jae __rt_json_obj_open_only_x");                       // an out-of-range class_id falls back to an empty object literal
    abi::emit_symbol_address(emitter, "rdi", "_class_json_desc_ptrs");          // materialize the descriptor pointer table base
    emitter.instruction("mov rdi, QWORD PTR [rdi + rcx*8]");                    // load the descriptor pointer for the current class
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // save the descriptor pointer for downstream loads

    // Check the JsonSerializable flag.
    emitter.instruction("mov rcx, QWORD PTR [rdi]");                            // load the class JSON descriptor flags word
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // keep the flags (the jsonSerialize path never reads the output start)
    emitter.instruction("test rcx, 1");                                         // is JsonSerializable bit set?
    emitter.instruction("je __rt_json_obj_walk_props_x");                       // ordinary classes walk public properties directly
    emitter.instruction("mov rdx, QWORD PTR [rdi + 8]");                        // load the jsonSerialize method symbol from the descriptor
    emitter.instruction("test rdx, rdx");                                       // is the method target populated?
    emitter.instruction("je __rt_json_obj_walk_props_x");                       // missing method targets fall through to the property walker
    emitter.label("__rt_json_obj_have_target_x");
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // park the jsonSerialize target across helper calls that clobber rdx
    // The user method's body resets _concat_off and writes scratch at the
    // beginning of concat_buf, which would trash any caller prefix already
    // there. Persist the caller's prefix to heap, run jsonSerialize, then
    // copy the prefix back before re-establishing concat_off and encoding.
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // capture the caller-visible concat-buffer offset before the user method runs
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save the captured offset (= prefix length) across the user method invocation
    abi::emit_load_symbol_to_reg(emitter, "r10", "_json_last_error", 0);        // capture the outer JSON error state before user code can run nested JSON calls
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save _json_last_error across jsonSerialize()
    abi::emit_load_symbol_to_reg(emitter, "r10", "_json_active_flags", 0);      // capture the outer JSON flag bitmask before user code can change it
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save _json_active_flags across jsonSerialize()
    abi::emit_load_symbol_to_reg(emitter, "r10", "_json_active_depth", 0);      // capture the outer JSON depth before user code can reset it
    emitter.instruction("mov QWORD PTR [rbp - 80], r10");                       // save _json_active_depth across jsonSerialize()
    abi::emit_load_symbol_to_reg(emitter, "r10", "_json_indent_depth", 0);      // capture the outer pretty-print depth before user code can reset it
    emitter.instruction("mov QWORD PTR [rbp - 88], r10");                       // save _json_indent_depth across jsonSerialize()
    abi::emit_load_symbol_to_reg(emitter, "r10", "_json_depth_limit", 0);       // capture the outer JSON depth limit before user code can change it
    emitter.instruction("mov QWORD PTR [rbp - 96], r10");                       // save _json_depth_limit across jsonSerialize()
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // default the saved prefix heap pointer to null for empty prefixes
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the prefix length: r10 still holds _json_depth_limit, and a zero limit skipped the copy while a non-zero one persisted that many bytes
    emitter.instruction("test r10, r10");                                       // is there any caller-visible prefix to preserve?
    emitter.instruction("jz __rt_json_obj_jsonserialize_invoke_x");             // skip the prefix copy when the prefix is empty
    abi::emit_symbol_address(emitter, "rax", "_concat_buf");                    // materialize the concat-buffer base for the str_persist input
    emitter.instruction("mov rdx, r10");                                        // copy the prefix length into the str_persist length register
    emitter.instruction("call __rt_str_persist");                               // duplicate the caller prefix into a heap-owned buffer
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // remember the heap-owned prefix pointer for the post-call restore
    emitter.label("__rt_json_obj_jsonserialize_invoke_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the receiver object pointer as the SysV $this argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // reload the saved jsonSerialize method target
    emitter.instruction("call rdx");                                            // invoke jsonSerialize on the receiver and capture its result
    // -- box a raw return (`jsonSerialize(): string`, `: array`, ...) like a Mixed one --
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the descriptor flags word
    emitter.instruction("shr r10, 8");                                          // move the raw return tag + 1 to the low byte
    emitter.instruction("and r10, 255");                                        // isolate it (0 = already boxed)
    emitter.instruction("jz __rt_json_obj_jsonserialize_boxed_x");              // a Mixed return is already the cell to encode
    emitter.instruction("sub r10, 1");                                          // recover the runtime value tag of the raw return
    emitter.instruction("cmp r10, 1");                                          // a string return sits in rax/rdx
    emitter.instruction("jne __rt_json_obj_jsonserialize_not_str_x");
    emitter.instruction("mov rdi, rax");                                        // string pointer as the payload low word
    emitter.instruction("mov rsi, rdx");                                        // string length as the payload high word
    emitter.instruction("jmp __rt_json_obj_jsonserialize_box_x");               // box the string pair
    emitter.label("__rt_json_obj_jsonserialize_not_str_x");
    emitter.instruction("cmp r10, 2");                                          // a float return sits in xmm0
    emitter.instruction("jne __rt_json_obj_jsonserialize_word_x");
    emitter.instruction("movq rdi, xmm0");                                      // move the float bits into the payload register
    emitter.instruction("jmp __rt_json_obj_jsonserialize_box_x");               // box the float
    emitter.label("__rt_json_obj_jsonserialize_word_x");
    emitter.instruction("mov rdi, rax");                                        // the one-word payload (int, bool, container, object)
    emitter.label("__rt_json_obj_jsonserialize_box_x");
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // keep the raw tag to give the method's own reference back
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // keep the raw payload for the same release
    emitter.instruction("mov rax, r10");                                        // value tag argument for the boxing helper
    emitter.instruction("call __rt_mixed_from_value");                          // box the raw value (it retains or persists its own copy)
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // park the fresh box
    emitter.instruction("cmp QWORD PTR [rbp - 16], 1");                         // a returned string is the call's own copy
    emitter.instruction("jne __rt_json_obj_jsonserialize_not_string_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the method's string; the box persisted its own copy
    emitter.instruction("call __rt_heap_free_safe");                            // validated free: scratch or static strings are left alone
    emitter.instruction("jmp __rt_json_obj_jsonserialize_released_x");
    emitter.label("__rt_json_obj_jsonserialize_not_string_x");
    emitter.instruction("cmp QWORD PTR [rbp - 16], 4");                         // containers and objects carry the method's reference
    emitter.instruction("jb __rt_json_obj_jsonserialize_released_x");           // scalars own nothing to give back
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the method's returned container/object
    emitter.instruction("call __rt_decref_any");                                // the box holds its own reference now
    emitter.label("__rt_json_obj_jsonserialize_released_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // the boxed value to encode
    emitter.label("__rt_json_obj_jsonserialize_boxed_x");
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // park the boxed mixed return value across the prefix-restore loop
    // Copy the heap-owned prefix back into concat_buf[0..prefix_len].
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the saved prefix length
    emitter.instruction("test r10, r10");                                       // anything to restore?
    emitter.instruction("jz __rt_json_obj_jsonserialize_after_restore_x");      // skip restore for empty prefixes
    emitter.instruction("mov r11, QWORD PTR [rbp - 64]");                       // reload the heap-owned prefix pointer
    emitter.instruction("test r11, r11");                                       // defensive null guard for the heap-owned prefix pointer
    emitter.instruction("jz __rt_json_obj_jsonserialize_after_restore_x");      // skip restore when no heap copy was made
    abi::emit_symbol_address(emitter, "r9", "_concat_buf");                     // materialize the concat-buffer base for the prefix restore loop
    emitter.instruction("xor rcx, rcx");                                        // initialize the prefix copy index
    emitter.label("__rt_json_obj_prefix_restore_x");
    emitter.instruction("cmp rcx, r10");                                        // have we copied every prefix byte back?
    emitter.instruction("jae __rt_json_obj_jsonserialize_after_restore_x");     // exit once the entire prefix is restored
    emitter.instruction("mov dl, BYTE PTR [r11 + rcx]");                        // load the next prefix byte from the heap-owned copy
    emitter.instruction("mov BYTE PTR [r9 + rcx], dl");                         // restore the byte into concat_buf at the original offset
    emitter.instruction("add rcx, 1");                                          // advance the prefix copy index
    emitter.instruction("jmp __rt_json_obj_prefix_restore_x");                  // continue the prefix restore loop
    emitter.label("__rt_json_obj_jsonserialize_after_restore_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // load the owned prefix copy after its bytes have been restored
    emitter.instruction("call __rt_decref_any");                                // release the temporary string copy, including the null-prefix case
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the saved pre-call concat-buffer offset
    abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);            // restore the concat-buffer offset so encode_mixed appends at our intended slot
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the outer JSON error state after user code returns
    abi::emit_store_reg_to_symbol(emitter, "r10", "_json_last_error", 0);       // restore _json_last_error before encoding the serialized value
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the outer JSON flag bitmask after user code returns
    abi::emit_store_reg_to_symbol(emitter, "r10", "_json_active_flags", 0);     // restore _json_active_flags before encoding the serialized value
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload the outer JSON depth after user code returns
    abi::emit_store_reg_to_symbol(emitter, "r10", "_json_active_depth", 0);     // restore _json_active_depth before recursive JSON encoding resumes
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // reload the outer pretty-print depth after user code returns
    abi::emit_store_reg_to_symbol(emitter, "r10", "_json_indent_depth", 0);     // restore _json_indent_depth before recursive JSON encoding resumes
    emitter.instruction("mov r10, QWORD PTR [rbp - 96]");                       // reload the outer JSON depth limit after user code returns
    abi::emit_store_reg_to_symbol(emitter, "r10", "_json_depth_limit", 0);      // restore _json_depth_limit before recursive JSON encoding resumes
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // restore the boxed mixed return value before encoding it
    // -- expose persistent enum properties once on a serializer self return --
    emitter.instruction("test rax, rax");                                       // null returns retain the ordinary mixed encoder
    emitter.instruction("je __rt_json_obj_jsonserialize_mixed_x");              // skip object inspection for a null result
    emitter.instruction("cmp QWORD PTR [rax], 6");                              // only object results can be the receiver
    emitter.instruction("jne __rt_json_obj_jsonserialize_mixed_x");             // other values retain recursive encoding
    emitter.instruction("mov rcx, QWORD PTR [rax + 8]");                        // load the returned object identity
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 8]");                        // compare against the original receiver identity
    emitter.instruction("jne __rt_json_obj_jsonserialize_mixed_x");             // distinct objects retain recursive encoding
    emitter.instruction("mov rcx, QWORD PTR [rcx]");                            // load the receiver class id after identity comparison
    abi::emit_symbol_address(emitter, "rdi", "_class_enum_kinds");
    emitter.instruction("cmp QWORD PTR [rdi + rcx*8], 0");                      // identify persistent enum receivers
    emitter.instruction("je __rt_json_obj_jsonserialize_mixed_x");              // ordinary classes retain their existing recursive path
    emitter.instruction("call __rt_decref_mixed");                              // retire the owned result box while its enum receiver remains persistent
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the still-owned receiver after cleanup
    emitter.instruction("mov rcx, QWORD PTR [rax]");                            // recover its class id
    abi::emit_symbol_address(emitter, "rdi", "_class_json_desc_ptrs");
    emitter.instruction("mov rdi, QWORD PTR [rdi + rcx*8]");                    // recover the descriptor overwritten by prefix storage
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // restore the descriptor for the property walker
    emitter.instruction("jmp __rt_json_obj_walk_props_x");                      // walk the receiver without invoking the serializer again
    emitter.label("__rt_json_obj_jsonserialize_mixed_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload a non-self result for ordinary recursive encoding
    emitter.instruction("call __rt_json_encode_mixed");                         // encode the boxed mixed return value as JSON
    // Save the (rax, rdx) result across __rt_json_depth_exit.
    emitter.instruction("mov QWORD PTR [rbp - 120], rax");                      // checkpoint the encoded result pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // checkpoint the encoded result length
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // the serialized value's box: jsonSerialize() returned it owned
    emitter.instruction("call __rt_decref_mixed");                              // its JSON text is already in concat_buf
    emitter.instruction("call __rt_json_depth_exit");                           // decrement _json_active_depth so a sibling encoder can re-enter cleanly
    emitter.instruction("mov rax, QWORD PTR [rbp - 120]");                      // restore the encoded result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // restore the encoded result length
    emitter.instruction("mov rsp, rbp");                                        // unwind the encoder scratch frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the encoded result produced by mixed encoding

    if eval_bridge {
        // jsonSerialize() stand-in for an interpreter-owned object: rdi = object, returns the
        // boxed Mixed value to encode in rax (null when the interpreter cannot answer).
        emitter.label("__rt_json_eval_serialize_thunk");
        emitter.instruction("push rbp");                                        // preserve the encoder's frame pointer
        emitter.instruction("mov rbp, rsp");                                    // align the stack for the bridge call
        let serialize = emitter.target.extern_symbol("__elephc_eval_json_serialize");
        abi::emit_call_label(emitter, &serialize);
        emitter.instruction("pop rbp");                                         // restore the encoder's frame pointer
        emitter.instruction("ret");                                             // return the boxed value like a compiled jsonSerialize()
    }
    emitter.label("__rt_json_obj_open_only_x");
    emitter.instruction("lea rdi, [rbp - 128]");                                // address of the output-buffer descriptor
    emitter.instruction("mov esi, 2");                                          // room for the empty object literal
    emitter.instruction("call __rt_json_buf_ensure");                           // r11 = cursor with headroom reserved
    emitter.instruction("mov BYTE PTR [r11], 123");                             // emit the opening brace for the empty object literal
    emitter.instruction("add r11, 1");                                          // advance past the opening brace
    emitter.instruction("mov BYTE PTR [r11], 125");                             // emit the closing brace for the empty object literal
    emitter.instruction("add r11, 1");                                          // advance past the closing brace
    emitter.instruction("jmp __rt_json_obj_finalize_x");                        // jump to the shared finalization tail

    emitter.label("__rt_json_obj_walk_props_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the descriptor pointer
    emitter.instruction("mov rcx, QWORD PTR [rdi + 16]");                       // load the public property count
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // save the public property count for the loop guard

    emitter.instruction("lea rdi, [rbp - 128]");                                // address of the output-buffer descriptor
    emitter.instruction("mov esi, 1");                                          // room for the opening brace
    emitter.instruction("call __rt_json_buf_ensure");                           // r11 = cursor with headroom reserved
    emitter.instruction("mov BYTE PTR [r11], 123");                             // emit the opening brace before the first property
    emitter.instruction("add r11, 1");                                          // advance past the opening brace
    emitter.instruction("mov QWORD PTR [rbp - 120], r11");                      // save the running write pointer after the opening brace
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

    // Reserve room for the separator, the indentation, and the key punctuation.
    emitter.instruction("lea rdi, [rbp - 128]");                                // address of the output-buffer descriptor
    emitter.instruction("mov esi, 1");                                          // room for the comma separator
    emitter.instruction("call __rt_json_buf_ensure");                           // grow first: every punctuation byte of this key prefix fits after it

    // Comma separator before all but the first property.
    emitter.instruction("cmp QWORD PTR [rbp - 40], 0");                         // is this the first property entry?
    emitter.instruction("je __rt_json_obj_key_x");                              // skip the comma before the first property
    emitter.instruction("mov r11, QWORD PTR [rbp - 120]");                      // reload the running write pointer
    emitter.instruction("mov BYTE PTR [r11], 44");                              // emit the comma separator between properties
    emitter.instruction("add r11, 1");                                          // advance past the comma
    emitter.instruction("mov QWORD PTR [rbp - 120], r11");                      // save the running write pointer after the comma

    emitter.label("__rt_json_obj_key_x");
    emitter.instruction("mov r11, QWORD PTR [rbp - 120]");                      // reload the running write pointer for the key prefix
    emitter.instruction("call __rt_json_pretty_line");                          // append newline and indentation for this property when pretty-printing
    emitter.instruction("mov QWORD PTR [rbp - 120], r11");                      // save the write pointer after any pretty indentation
    emitter.instruction("mov BYTE PTR [r11], 34");                              // emit the opening quote for the property name
    emitter.instruction("add r11, 1");                                          // advance past the opening quote
    emitter.instruction("mov QWORD PTR [rbp - 120], r11");                      // record the cursor past the opening quote
    emitter.instruction("mov rsi, QWORD PTR [rbp - 88]");                       // reload the property name pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 96]");                       // reload the property name length
    emitter.instruction("lea rdi, [rbp - 128]");                                // address of the output-buffer descriptor
    emitter.instruction("call __rt_json_buf_append");                           // copy the name to the cursor, growing first; r11 = advanced cursor
    emitter.instruction("mov BYTE PTR [r11], 34");                              // emit the closing quote for the property name
    emitter.instruction("add r11, 1");                                          // advance past the closing quote
    emitter.instruction("mov BYTE PTR [r11], 58");                              // emit the colon separating key and value
    emitter.instruction("add r11, 1");                                          // advance past the colon
    emitter.instruction("call __rt_json_pretty_colon_space");                   // append the pretty-print key/value space when requested
    emitter.instruction("mov QWORD PTR [rbp - 120], r11");                      // save the running write pointer after the key prefix

    // Sync concat_off so nested encoders do not format over the existing prefix.
    emitter.instruction("lea rdi, [rbp - 128]");                                // address of the output-buffer descriptor
    emitter.instruction("call __rt_strbuf_sync");                               // publish the scratch offset for nested value encoders

    // Load property value from the object instance (slot offset = 8 + prop_index * 16).
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // reload the property runtime slot index
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the receiver object pointer
    emitter.instruction("mov rcx, r8");                                         // copy the slot index for the offset computation
    emitter.instruction("shl rcx, 4");                                          // multiply by 16 (slot stride)
    emitter.instruction("add rcx, 8");                                          // skip the leading class_id field at offset 0
    emitter.instruction("add rax, rcx");                                        // resolve the absolute address of the property slot
    emitter.instruction("mov rdx, QWORD PTR [rax]");                            // load the property low payload word
    emitter.instruction("mov rsi, QWORD PTR [rax + 8]");                        // load the property high payload word
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload the descriptor tag before it is dispatched on
    emit_deref_reference_property_slot(emitter, "r9", "rdx", "rsi", "rcx");
    emit_resolve_prop_desc_tag(emitter, "r9", "rsi", "rcx");
    emitter.instruction("mov QWORD PTR [rbp - 64], r9");                        // save the tag the slot actually carries back over the descriptor tag
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // save the property low payload across the encoder dispatch
    emitter.instruction("mov QWORD PTR [rbp - 80], rsi");                       // save the property high payload across the encoder dispatch

    // Dispatch on the property type tag.
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload the saved property type tag
    emit_resolve_tagged_scalar_property_tag(emitter, "r9", "rsi");
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
    emitter.instruction("mov rsi, rax");                                        // pass the encoded value pointer to the append helper (rdx = length)
    emitter.instruction("lea rdi, [rbp - 128]");                                // address of the output-buffer descriptor
    emitter.instruction("call __rt_json_buf_append");                           // copy the encoded value to the cursor, growing first, and release a transient child buffer

    // Advance the loop index and continue.
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the property loop index
    emitter.instruction("add rcx, 1");                                          // advance to the next property entry
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the updated loop index
    emitter.instruction("jmp __rt_json_obj_loop_x");                            // continue with the next property entry

    emitter.label("__rt_json_obj_close_x");
    emitter.instruction("lea rdi, [rbp - 128]");                                // address of the output-buffer descriptor
    emitter.instruction("mov esi, 1");                                          // room for the closing brace
    emitter.instruction("call __rt_json_buf_ensure");                           // r11 = cursor with headroom for the closing line
    emitter.instruction("call __rt_json_pretty_pop");                           // leave the object indentation level before closing it
    emitter.instruction("cmp QWORD PTR [rbp - 48], 0");                         // did the object contain any public properties?
    emitter.instruction("je __rt_json_obj_close_emit_x");                       // empty objects stay compact even under JSON_PRETTY_PRINT
    emitter.instruction("call __rt_json_pretty_line");                          // append the closing-line indentation for non-empty pretty objects
    emitter.label("__rt_json_obj_close_emit_x");
    emitter.instruction("mov BYTE PTR [r11], 125");                             // emit the closing brace after the last property
    emitter.instruction("add r11, 1");                                          // advance past the closing brace

    emitter.label("__rt_json_obj_finalize_x");
    emitter.instruction("mov QWORD PTR [rbp - 120], r11");                      // checkpoint the write pointer across the depth-exit helper call
    emitter.instruction("call __rt_json_depth_exit");                           // decrement _json_active_depth before computing the result slice
    emitter.instruction("lea rdi, [rbp - 128]");                                // address of the output-buffer descriptor
    emitter.instruction("call __rt_strbuf_finish");                             // rax/rdx = encoded object, _concat_off published
    emitter.instruction("mov rsp, rbp");                                        // unwind the encoder scratch frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the encoded object slice in the standard string registers
}

/// Emits the `__rt_json_encode_object` entry that routes an instance with live dynamic
/// properties through the object-to-hash projection, and every other instance to the
/// descriptor walker.
///
/// PHP encodes an object's public properties followed by its dynamic ones. The descriptor walker
/// knows only the declared rows, so a property created on an `#[AllowDynamicProperties]` instance
/// (`$o->fresh = 1`) vanished from the output. `__rt_object_to_hash` in non-cast mode and global
/// scope is exactly the public-plus-dynamic projection `get_object_vars()` returns, and the
/// stdClass encoder renders a hash as `{...}`. A `JsonSerializable` class keeps its method, and an
/// instance whose tail is absent or empty keeps the walker, so only the case that was wrong changes.
fn emit_dynamic_tail_wrapper_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_object (dynamic-property entry) ---");
    emitter.label_global("__rt_json_encode_object");
    emitter.instruction("ldr x12, [x0]");                                       // load class_id from the object header
    abi::emit_symbol_address(emitter, "x13", "_class_gc_desc_count");
    emitter.instruction("ldr x13, [x13]");                                      // load the number of registered class descriptors
    emitter.instruction("cmp x12, x13");                                        // is the class id a registered one?
    emitter.instruction("b.hs __rt_json_encode_object_miss");                   // unknown ids keep the descriptor walker
    abi::emit_symbol_address(emitter, "x13", "_class_json_desc_ptrs");
    emitter.instruction("ldr x13, [x13, x12, lsl #3]");                         // load the JSON descriptor of this class
    emitter.instruction("cbz x13, __rt_json_encode_object_miss");               // a class without a descriptor keeps the walker
    emitter.instruction("ldr x14, [x13]");                                      // load the descriptor flags word
    emitter.instruction("tst x14, #1");                                         // JsonSerializable classes dispatch to their method
    emitter.instruction("b.ne __rt_json_encode_object_miss");                   // keep the walker, which owns that dispatch
    abi::emit_symbol_address(emitter, "x11", "_class_object_dynamic_prop_flags");
    emitter.instruction("ldr x11, [x11, x12, lsl #3]");                         // does this class reserve a dynamic-property tail?
    emitter.instruction("cbz x11, __rt_json_encode_object_miss");               // no tail means no dynamic properties
    abi::emit_symbol_address(emitter, "x11", "_class_object_payload_sizes");
    emitter.instruction("ldr x11, [x11, x12, lsl #3]");                         // load the exact object payload size
    emitter.instruction("sub x11, x11, #8");                                    // the dynamic-property hash occupies the final word
    emitter.instruction("ldr x1, [x0, x11]");                                   // load the borrowed dynamic-property hash
    emitter.instruction("cbz x1, __rt_json_encode_object_miss");                // never written: keep the walker
    emitter.instruction("ldr x9, [x1]");                                        // load the dynamic hash entry count
    emitter.instruction("cbz x9, __rt_json_encode_object_miss");                // emptied: keep the walker
    emitter.instruction("sub sp, sp, #48");                                     // reserve the wrapper frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the wrapper frame pointer
    emitter.instruction("mov x1, #0");                                          // non-cast mode: visible names, not mangled keys
    emitter.instruction("mov x2, #-1");                                         // global scope: public properties only
    emitter.instruction("bl __rt_object_to_hash");                              // project public and dynamic properties into a fresh hash
    emitter.instruction("str x0, [sp, #0]");                                    // keep the owned projection for its release
    emitter.instruction("bl __rt_json_encode_stdclass");                        // encode the projection as a JSON object
    emitter.instruction("str x1, [sp, #8]");                                    // keep the encoded string pointer across the release
    emitter.instruction("str x2, [sp, #16]");                                   // keep the encoded string length across the release
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the projection
    emitter.instruction("bl __rt_decref_any");                                  // release the temporary projection
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore the encoded string pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // restore the encoded string length
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the wrapper frame
    emitter.instruction("ret");                                                 // return the encoded object
    emitter.label("__rt_json_encode_object_miss");
    emitter.instruction("b __rt_json_encode_object_decl");                    // no live dynamic properties: the descriptor walker encodes the object
}

/// x86_64 twin of [`emit_dynamic_tail_wrapper_aarch64`].
fn emit_dynamic_tail_wrapper_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_encode_object (dynamic-property entry) ---");
    emitter.label_global("__rt_json_encode_object");
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load class_id from the object header
    abi::emit_symbol_address(emitter, "r11", "_class_gc_desc_count");
    emitter.instruction("cmp r10, QWORD PTR [r11]");                            // is the class id a registered one?
    emitter.instruction("jae __rt_json_encode_object_miss_x");                    // unknown ids keep the descriptor walker
    abi::emit_symbol_address(emitter, "r11", "_class_json_desc_ptrs");
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // load the JSON descriptor of this class
    emitter.instruction("test r11, r11");                                       // does the class have a descriptor?
    emitter.instruction("jz __rt_json_encode_object_miss_x");                     // a class without a descriptor keeps the walker
    emitter.instruction("test QWORD PTR [r11], 1");                             // JsonSerializable classes dispatch to their method
    emitter.instruction("jnz __rt_json_encode_object_miss_x");                    // keep the walker, which owns that dispatch
    abi::emit_symbol_address(emitter, "r11", "_class_object_dynamic_prop_flags");
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // does this class reserve a dynamic-property tail?
    emitter.instruction("test r11, r11");                                       // test the tail flag
    emitter.instruction("jz __rt_json_encode_object_miss_x");                     // no tail means no dynamic properties
    abi::emit_symbol_address(emitter, "r11", "_class_object_payload_sizes");
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // load the exact object payload size
    emitter.instruction("sub r11, 8");                                          // the dynamic-property hash occupies the final word
    emitter.instruction("mov r11, QWORD PTR [rax + r11]");                      // load the borrowed dynamic-property hash
    emitter.instruction("test r11, r11");                                       // has the lazy hash been allocated?
    emitter.instruction("jz __rt_json_encode_object_miss_x");                     // never written: keep the walker
    emitter.instruction("cmp QWORD PTR [r11], 0");                              // does the hash hold any entry?
    emitter.instruction("je __rt_json_encode_object_miss_x");                     // emptied: keep the walker
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the wrapper frame
    emitter.instruction("sub rsp, 32");                                         // reserve the projection and result slots
    emitter.instruction("mov rdx, 0");                                          // non-cast mode: visible names, not mangled keys
    emitter.instruction("mov rcx, -1");                                         // global scope: public properties only
    abi::emit_call_label(emitter, "__rt_object_to_hash");                       // project public and dynamic properties into a fresh hash
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the owned projection for its release
    abi::emit_call_label(emitter, "__rt_json_encode_stdclass");                 // encode the projection as a JSON object
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // keep the encoded string pointer across the release
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // keep the encoded string length across the release
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the projection
    abi::emit_call_label(emitter, "__rt_decref_any");                           // release the temporary projection
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // restore the encoded string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // restore the encoded string length
    emitter.instruction("mov rsp, rbp");                                        // release the wrapper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the encoded object
    emitter.label("__rt_json_encode_object_miss_x");
    emitter.instruction("jmp __rt_json_encode_object_decl");                   // no live dynamic properties: the descriptor walker encodes the object
}
