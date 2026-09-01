//! Purpose:
//! Emits AArch64 output-buffer, string-byte, and lifetime wrappers.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Output state and final-object ownership remain shared with native code.

use super::*;

/// Emits AArch64 output-buffer, string-byte, and lifetime wrappers.
pub(super) fn emit_aarch64_output(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_echo");
    emitter.instruction("b __rt_mixed_write_stdout");                           // echo one boxed mixed value and return to Rust

    // -- output-buffering (ob_*) bridge: expose the runtime buffer stack to the
    //    eval interpreter so static and eval'd code share one ob state. --
    label_c_global(emitter, "__elephc_eval_ob_start");
    emitter.instruction("b __rt_ob_start");                                     // push a new output buffer; returns 1/0 in x0

    label_c_global(emitter, "__elephc_eval_ob_level");
    emitter.instruction("b __rt_ob_level");                                     // return the buffer-stack depth in x0

    label_c_global(emitter, "__elephc_eval_ob_length");
    emitter.instruction("b __rt_ob_length");                                    // return the top buffer length (or -1) in x0

    label_c_global(emitter, "__elephc_eval_ob_clean");
    emitter.instruction("b __rt_ob_clean");                                     // truncate the top buffer; returns 1/0 in x0

    label_c_global(emitter, "__elephc_eval_ob_flush");
    emitter.instruction("b __rt_ob_flush");                                     // flush the top buffer to the parent sink; returns 1/0 in x0

    label_c_global(emitter, "__elephc_eval_ob_end");
    emitter.instruction("cbz x0, __elephc_eval_ob_end_clean_path");             // a zero flush flag discards instead of flushing
    emitter.instruction("b __rt_ob_end_flush");                                 // flush, pop, and free the top buffer
    emitter.label("__elephc_eval_ob_end_clean_path");
    emitter.instruction("b __rt_ob_end_clean");                                 // discard, pop, and free the top buffer

    label_c_global(emitter, "__elephc_eval_ob_contents");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_ob_level");       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction("cbz x10, __elephc_eval_ob_contents_none");             // no active buffer — report failure to Rust
    emitter.instruction("sub x10, x10, #1");                                    // top slot index = depth - 1
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_ob_ptrs");       // materialize the buffer-pointer slot array
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // load the top buffer base pointer
    emitter.instruction("str x12, [x0]");                                       // store the buffer pointer through the caller's out_ptr
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_ob_lens");       // materialize the used-bytes slot array
    emitter.instruction("ldr x12, [x11, x10, lsl #3]");                         // load the top buffer's used byte count
    emitter.instruction("str x12, [x1]");                                       // store the byte count through the caller's out_len
    emitter.instruction("mov x0, #1");                                          // report success to Rust
    emitter.instruction("ret");                                                 // return to Rust (bytes are copied immediately)
    emitter.label("__elephc_eval_ob_contents_none");
    emitter.instruction("mov x0, #0");                                          // report "no active buffer" to Rust
    emitter.instruction("ret");                                                 // return to Rust

    label_c_global(emitter, "__elephc_eval_ob_stats");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_ob_level");       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction("cmp x0, x10");                                         // is the requested slot index within the stack?
    emitter.instruction("b.hs __elephc_eval_ob_stats_none");                    // out-of-range (or negative) index — report failure
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_ob_lens");       // materialize the used-bytes slot array
    emitter.instruction("ldr x12, [x11, x0, lsl #3]");                          // load the slot's used byte count
    emitter.instruction("str x12, [x1]");                                       // store it through the caller's out_used
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_ob_caps");       // materialize the capacity slot array
    emitter.instruction("ldr x12, [x11, x0, lsl #3]");                          // load the slot's capacity
    emitter.instruction("str x12, [x2]");                                       // store it through the caller's out_size
    emitter.instruction("mov x0, #1");                                          // report success to Rust
    emitter.instruction("ret");                                                 // return to Rust
    emitter.label("__elephc_eval_ob_stats_none");
    emitter.instruction("mov x0, #0");                                          // report "no such buffer" to Rust
    emitter.instruction("ret");                                                 // return to Rust

    label_c_global(emitter, "__elephc_eval_ob_implicit_flush");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_ob_implicit_flush"); // materialize the stored implicit-flush flag address
    emitter.instruction("str x0, [x9]");                                        // store the (inert) implicit-flush flag
    emitter.instruction("ret");                                                 // return to Rust

    label_c_global(emitter, "__elephc_eval_ob_start_ex");
    emitter.instruction("cbz x0, __elephc_eval_ob_start_ex_default");           // a zero has-handler flag selects the default handler
    crate::codegen::abi::emit_symbol_address(emitter, "x0", "__rt_ob_eval_trampoline"); // eval handlers invoke through the installed magician hook
    emitter.instruction("b __rt_ob_start_ex");                                  // start the buffer (env/chunk/flags/name already aligned)
    emitter.label("__elephc_eval_ob_start_ex_default");
    emitter.instruction("mov x1, #0");                                          // default handlers carry no env word
    emitter.instruction("b __rt_ob_start_ex");                                  // start the buffer with the default handler

    label_c_global(emitter, "__elephc_eval_install_ob_handler_hook");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_elephc_eval_ob_handler_fn"); // materialize the eval handler hook slot
    emitter.instruction("str x0, [x9]");                                        // install the magician ob-handler callback
    emitter.instruction("ret");                                                 // return to Rust

    label_c_global(emitter, "__elephc_eval_ob_get_clean_pop");
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // save frame pointer and return address plus out-pointer slots
    emitter.instruction("mov x29, sp");                                         // establish the wrapper frame pointer
    emitter.instruction("stp x0, x1, [sp, #16]");                               // save the caller's out_ptr/out_len storage addresses
    emitter.instruction("bl __rt_ob_get_clean_pop");                            // gate, run the handler, pop → raw pair (or null)
    emitter.instruction("ldp x9, x10, [sp, #16]");                              // reload the out-pointer storage addresses
    emitter.instruction("cbz x1, __elephc_eval_ob_get_clean_none");             // refused — report failure to Rust
    emitter.instruction("str x1, [x9]");                                        // store the owned raw-contents pointer for Rust
    emitter.instruction("str x2, [x10]");                                       // store the raw-contents length for Rust
    emitter.instruction("mov x0, #1");                                          // report success to Rust
    emitter.instruction("b __elephc_eval_ob_get_clean_done");                   // finish
    emitter.label("__elephc_eval_ob_get_clean_none");
    emitter.instruction("mov x0, #0");                                          // report refusal to Rust
    emitter.label("__elephc_eval_ob_get_clean_done");
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the success flag

    label_c_global(emitter, "__elephc_eval_ob_get_flush_pop");
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // save frame pointer and return address plus out-pointer slots
    emitter.instruction("mov x29, sp");                                         // establish the wrapper frame pointer
    emitter.instruction("stp x0, x1, [sp, #16]");                               // save the caller's out_ptr/out_len storage addresses
    emitter.instruction("bl __rt_ob_get_flush_pop");                            // gate, run the handler, flush, pop → raw pair (or null)
    emitter.instruction("ldp x9, x10, [sp, #16]");                              // reload the out-pointer storage addresses
    emitter.instruction("cbz x1, __elephc_eval_ob_get_flush_none");             // refused — report failure to Rust
    emitter.instruction("str x1, [x9]");                                        // store the owned raw-contents pointer for Rust
    emitter.instruction("str x2, [x10]");                                       // store the raw-contents length for Rust
    emitter.instruction("mov x0, #1");                                          // report success to Rust
    emitter.instruction("b __elephc_eval_ob_get_flush_done");                   // finish
    emitter.label("__elephc_eval_ob_get_flush_none");
    emitter.instruction("mov x0, #0");                                          // report refusal to Rust
    emitter.label("__elephc_eval_ob_get_flush_done");
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the success flag

    label_c_global(emitter, "__elephc_eval_ob_release_string");
    emitter.instruction("b __rt_decref_any");                                   // release one bridge-returned owned string

    label_c_global(emitter, "__elephc_eval_ob_slot_meta");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_ob_level");       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction("cmp x0, x10");                                         // is the requested slot index within the stack?
    emitter.instruction("b.hs __elephc_eval_ob_slot_meta_none");                // out-of-range index — report failure
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_ob_chunk_sizes"); // materialize the chunk-size slot array
    emitter.instruction("ldr x12, [x11, x0, lsl #3]");                          // load the slot's chunk size
    emitter.instruction("str x12, [x1]");                                       // store it through the caller's out_chunk
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_ob_flags");      // materialize the flags slot array
    emitter.instruction("ldr x12, [x11, x0, lsl #3]");                          // load the slot's stored flags word
    emitter.instruction("str x12, [x2]");                                       // store it through the caller's out_flags
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_ob_handler_stubs"); // materialize the handler-stub slot array
    emitter.instruction("ldr x12, [x11, x0, lsl #3]");                          // load the slot's handler stub
    emitter.instruction("cmp x12, #0");                                         // is a user handler installed?
    emitter.instruction("cset x12, ne");                                        // bit 0 = user-handler flag
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_ob_started");    // materialize the started-flag slot array
    emitter.instruction("ldr x13, [x11, x0, lsl #3]");                          // load the slot's started flag
    emitter.instruction("cmp x13, #0");                                         // has the handler run at least once?
    emitter.instruction("cset x13, ne");                                        // normalize the started flag to 0/1
    emitter.instruction("orr x12, x12, x13, lsl #1");                           // bit 1 = started flag
    emitter.instruction("str x12, [x3]");                                       // store the packed user/started bits
    emitter.instruction("mov x0, #1");                                          // report success to Rust
    emitter.instruction("ret");                                                 // return to Rust
    emitter.label("__elephc_eval_ob_slot_meta_none");
    emitter.instruction("mov x0, #0");                                          // report "no such buffer" to Rust
    emitter.instruction("ret");                                                 // return to Rust

    label_c_global(emitter, "__elephc_eval_ob_slot_name");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_ob_level");       // materialize the address of the buffer-stack depth
    emitter.instruction("ldr x10, [x9]");                                       // load the current buffer-stack depth
    emitter.instruction("cmp x0, x10");                                         // is the requested slot index within the stack?
    emitter.instruction("b.hs __elephc_eval_ob_slot_name_none");                // out-of-range index — report failure
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_ob_name_ptrs");  // materialize the handler-name pointer array
    emitter.instruction("ldr x12, [x11, x0, lsl #3]");                          // load the slot's display-name pointer
    emitter.instruction("str x12, [x1]");                                       // store it through the caller's out_ptr
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_ob_name_lens");  // materialize the handler-name length array
    emitter.instruction("ldr x12, [x11, x0, lsl #3]");                          // load the slot's display-name length
    emitter.instruction("str x12, [x2]");                                       // store it through the caller's out_len
    emitter.instruction("mov x0, #1");                                          // report success to Rust (bytes copied immediately)
    emitter.instruction("ret");                                                 // return to Rust
    emitter.label("__elephc_eval_ob_slot_name_none");
    emitter.instruction("mov x0, #0");                                          // report "no such buffer" to Rust
    emitter.instruction("ret");                                                 // return to Rust

    label_c_global(emitter, "__elephc_eval_value_string_bytes");
    emitter.instruction("sub sp, sp, #48");                                     // allocate a wrapper frame for output pointers
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address across string casting
    emitter.instruction("add x29, sp, #32");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save the caller's out_ptr storage address
    emitter.instruction("str x2, [sp, #8]");                                    // save the caller's out_len storage address
    emitter.instruction("bl __rt_mixed_cast_string");                           // cast the boxed eval value to a PHP string pair
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the optional out_ptr storage address
    emitter.instruction("cbz x9, __elephc_eval_value_string_bytes_len");        // skip pointer storage when the caller passed null
    emitter.instruction("str x1, [x9]");                                        // store the string pointer for Rust to copy immediately
    emitter.label("__elephc_eval_value_string_bytes_len");
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the optional out_len storage address
    emitter.instruction("cbz x10, __elephc_eval_value_string_bytes_done");      // skip length storage when the caller passed null
    emitter.instruction("str x2, [x10]");                                       // store the string byte length for Rust
    emitter.label("__elephc_eval_value_string_bytes_done");
    emitter.instruction("mov x0, #1");                                          // report successful string conversion to Rust
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the string-bytes wrapper frame
    emitter.instruction("ret");                                                 // return the success flag to Rust

    label_c_global(emitter, "__elephc_eval_value_truthy");
    emitter.instruction("b __rt_mixed_cast_bool");                              // cast one boxed mixed value to PHP truthiness for eval

    label_c_global(emitter, "__elephc_eval_value_retain");
    emitter.instruction("b __rt_incref");                                       // retain one eval-owned boxed Mixed cell

    label_c_global(emitter, "__elephc_eval_value_final_object_identity");
    emitter.instruction("cbz x0, __elephc_eval_value_final_object_none");       // null handles cannot release an object
    emitter.label("__elephc_eval_value_final_object_loop");
    emitter.instruction("ldr w9, [x0, #-12]");                                  // load the current Mixed wrapper refcount
    emitter.instruction("cmp w9, #1");                                          // only the last Mixed owner can free the wrapped payload
    emitter.instruction("b.ne __elephc_eval_value_final_object_none");          // non-final wrapper releases cannot run destructors yet
    emitter.instruction("ldr x9, [x0]");                                        // load the current Mixed runtime tag
    emitter.instruction("cmp x9, #7");                                          // tag 7 means the payload is another boxed Mixed
    emitter.instruction("b.ne __elephc_eval_value_final_object_check_object");  // concrete payloads can be tested for object ownership
    emitter.instruction("ldr x0, [x0, #8]");                                    // follow the nested Mixed payload pointer
    emitter.instruction("cbnz x0, __elephc_eval_value_final_object_loop");      // continue while nested Mixed payloads are present
    emitter.instruction("b __elephc_eval_value_final_object_none");             // null nested payloads cannot release an object
    emitter.label("__elephc_eval_value_final_object_check_object");
    emitter.instruction("cmp x9, #6");                                          // tag 6 means the concrete payload is a PHP object
    emitter.instruction("b.ne __elephc_eval_value_final_object_none");          // non-object releases have no dynamic destructor
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the object payload pointer from the Mixed cell
    emit_branch_if_null_container(
        emitter,
        "x9",
        "x10",
        "__elephc_eval_value_final_object_none",
    );
    emitter.instruction("ldr w10, [x9, #-12]");                                 // load the object refcount that the Mixed release will decrement
    emitter.instruction("cmp w10, #1");                                         // only the final object owner can run the destructor
    emitter.instruction("csel x0, x9, xzr, eq");                                // return object identity only for the final release
    emitter.instruction("ret");                                                 // return the candidate object identity to Rust
    emitter.label("__elephc_eval_value_final_object_none");
    emitter.instruction("mov x0, xzr");                                         // report that no dynamic destructor should run
    emitter.instruction("ret");                                                 // return zero to Rust

    label_c_global(emitter, "__elephc_eval_warning");
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "x9", "_rt_error_reporting", 0); // load the PHP diagnostic mask shared with static code
    emitter.instruction("tbz x9, #1, __elephc_eval_warning_done");              // suppress eval E_WARNING output when its mask bit is disabled
    emitter.instruction("mov x2, x1");                                          // move warning length into the runtime diagnostic length register
    emitter.instruction("mov x1, x0");                                          // move warning pointer into the runtime diagnostic buffer register
    emitter.instruction("b __rt_diag_warning");                                 // emit or suppress one eval runtime warning
    emitter.label("__elephc_eval_warning_done");
    emitter.instruction("ret");                                                 // return without output when E_WARNING is disabled

    label_c_global(emitter, "__elephc_eval_deprecated");
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "x9", "_rt_error_reporting", 0); // load the PHP diagnostic mask shared with static code
    emitter.instruction("tbz x9, #13, __elephc_eval_deprecated_done");          // suppress eval E_DEPRECATED output when its mask bit is disabled
    emitter.instruction("mov x2, x1");                                          // move deprecation length into the runtime diagnostic length register
    emitter.instruction("mov x1, x0");                                          // move deprecation pointer into the runtime diagnostic buffer register
    emitter.instruction("b __rt_diag_write");                                   // emit or suppress one eval runtime deprecation
    emitter.label("__elephc_eval_deprecated_done");
    emitter.instruction("ret");                                                 // return without output when E_DEPRECATED is disabled

    label_c_global(emitter, "__elephc_eval_error_reporting");
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_error_reporting", 0); // preserve the previous PHP diagnostic mask outside symbol-address scratch x9
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "x11", "_rt_diag_suppression", 0); // load the active nested @ depth
    emitter.instruction("mov x12, #4437");                                      // PHP's fatal-only mask exposed during @
    emitter.instruction("and x12, x10, x12");                                  // retain only fatal levels from the current user mask
    emitter.instruction("cmp x11, #0");                                        // is eval running inside a suppressed expression?
    emitter.instruction("csel x10, x10, x12, eq");                             // return the ordinary mask outside @ and fatal-only mask inside
    emitter.instruction("cbz x1, __elephc_eval_error_reporting_done");          // a missing/null level is a query only
    crate::codegen::abi::emit_store_reg_to_symbol(emitter, "x0", "_rt_error_reporting", 0); // publish the replacement PHP diagnostic mask
    emitter.label("__elephc_eval_error_reporting_done");
    emitter.instruction("mov x0, x10");                                         // return the previous mask to the eval interpreter
    emitter.instruction("ret");                                                 // return to Rust

    label_c_global(emitter, "__elephc_eval_value_release");
    emitter.instruction("b __rt_decref_mixed");                                 // release one eval-owned boxed Mixed cell
}
