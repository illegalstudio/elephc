//! Purpose:
//! Emits x86_64 output-buffer, string-byte, and lifetime wrappers.
//!
//! Called from:
//! - The eval bridge runtime facade and sibling bridge emitters.
//!
//! Key details:
//! - Output state and final-object ownership remain shared with native code.

use super::*;

/// Emits x86_64 output-buffer, string-byte, and lifetime wrappers.
pub(super) fn emit_x86_64_output(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_echo");
    emitter.instruction("mov rax, rdi");                                        // move the C boxed value argument into mixed echo input
    emitter.instruction("jmp __rt_mixed_write_stdout");                         // echo one boxed mixed value and return to Rust

    // -- output-buffering (ob_*) bridge: expose the runtime buffer stack to the
    //    eval interpreter so static and eval'd code share one ob state. --
    label_c_global(emitter, "__elephc_eval_ob_start");
    emitter.instruction("jmp __rt_ob_start");                                   // push a new output buffer; returns 1/0 in rax

    label_c_global(emitter, "__elephc_eval_ob_level");
    emitter.instruction("jmp __rt_ob_level");                                   // return the buffer-stack depth in rax

    label_c_global(emitter, "__elephc_eval_ob_length");
    emitter.instruction("jmp __rt_ob_length");                                  // return the top buffer length (or -1) in rax

    label_c_global(emitter, "__elephc_eval_ob_clean");
    emitter.instruction("jmp __rt_ob_clean");                                   // truncate the top buffer; returns 1/0 in rax

    label_c_global(emitter, "__elephc_eval_ob_flush");
    emitter.instruction("jmp __rt_ob_flush");                                   // flush the top buffer to the parent sink; returns 1/0 in rax

    label_c_global(emitter, "__elephc_eval_ob_end");
    emitter.instruction("test rdi, rdi");                                       // a zero flush flag discards instead of flushing
    emitter.instruction("jz __elephc_eval_ob_end_clean_path_x86");              // route the discard variant to end_clean
    emitter.instruction("jmp __rt_ob_end_flush");                               // flush, pop, and free the top buffer
    emitter.label("__elephc_eval_ob_end_clean_path_x86");
    emitter.instruction("jmp __rt_ob_end_clean");                               // discard, pop, and free the top buffer

    label_c_global(emitter, "__elephc_eval_ob_contents");
    crate::codegen::abi::emit_symbol_address(emitter, "r9", "_ob_level");       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("test r10, r10");                                       // is any buffer active?
    emitter.instruction("jz __elephc_eval_ob_contents_none_x86");               // no active buffer — report failure to Rust
    emitter.instruction("sub r10, 1");                                          // top slot index = depth - 1
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_ob_ptrs");       // materialize the buffer-pointer slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // load the top buffer base pointer
    emitter.instruction("mov QWORD PTR [rdi], rax");                            // store the buffer pointer through the caller's out_ptr
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_ob_lens");       // materialize the used-bytes slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + r10*8]");                    // load the top buffer's used byte count
    emitter.instruction("mov QWORD PTR [rsi], rax");                            // store the byte count through the caller's out_len
    emitter.instruction("mov eax, 1");                                          // report success to Rust
    emitter.instruction("ret");                                                 // return to Rust (bytes are copied immediately)
    emitter.label("__elephc_eval_ob_contents_none_x86");
    emitter.instruction("xor eax, eax");                                        // report "no active buffer" to Rust
    emitter.instruction("ret");                                                 // return to Rust

    label_c_global(emitter, "__elephc_eval_ob_stats");
    crate::codegen::abi::emit_symbol_address(emitter, "r9", "_ob_level");       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("cmp rdi, r10");                                        // is the requested slot index within the stack?
    emitter.instruction("jae __elephc_eval_ob_stats_none_x86");                 // out-of-range (or negative) index — report failure
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_ob_lens");       // materialize the used-bytes slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + rdi*8]");                    // load the slot's used byte count
    emitter.instruction("mov QWORD PTR [rsi], rax");                            // store it through the caller's out_used
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_ob_caps");       // materialize the capacity slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + rdi*8]");                    // load the slot's capacity
    emitter.instruction("mov QWORD PTR [rdx], rax");                            // store it through the caller's out_size
    emitter.instruction("mov eax, 1");                                          // report success to Rust
    emitter.instruction("ret");                                                 // return to Rust
    emitter.label("__elephc_eval_ob_stats_none_x86");
    emitter.instruction("xor eax, eax");                                        // report "no such buffer" to Rust
    emitter.instruction("ret");                                                 // return to Rust

    label_c_global(emitter, "__elephc_eval_ob_implicit_flush");
    crate::codegen::abi::emit_symbol_address(emitter, "r9", "_ob_implicit_flush"); // materialize the stored implicit-flush flag address
    emitter.instruction("mov QWORD PTR [r9], rdi");                             // store the (inert) implicit-flush flag
    emitter.instruction("ret");                                                 // return to Rust

    label_c_global(emitter, "__elephc_eval_ob_start_ex");
    emitter.instruction("test rdi, rdi");                                       // a zero has-handler flag selects the default handler
    emitter.instruction("jz __elephc_eval_ob_start_ex_default_x86");            // route the default-handler variant
    crate::codegen::abi::emit_symbol_address(emitter, "rdi", "__rt_ob_eval_trampoline"); // eval handlers invoke through the installed magician hook
    emitter.instruction("jmp __rt_ob_start_ex");                                // start the buffer (env/chunk/flags/name already aligned)
    emitter.label("__elephc_eval_ob_start_ex_default_x86");
    emitter.instruction("xor esi, esi");                                        // default handlers carry no env word
    emitter.instruction("jmp __rt_ob_start_ex");                                // start the buffer with the default handler

    label_c_global(emitter, "__elephc_eval_install_ob_handler_hook");
    crate::codegen::abi::emit_symbol_address(emitter, "r9", "_elephc_eval_ob_handler_fn"); // materialize the eval handler hook slot
    emitter.instruction("mov QWORD PTR [r9], rdi");                             // install the magician ob-handler callback
    emitter.instruction("ret");                                                 // return to Rust

    label_c_global(emitter, "__elephc_eval_ob_get_clean_pop");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve slots for the out-pointer storage addresses
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the caller's out_ptr storage address
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the caller's out_len storage address
    emitter.instruction("call __rt_ob_get_clean_pop");                          // gate, run the handler, pop → raw pair (or null)
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the out_ptr storage address
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the out_len storage address
    emitter.instruction("test rax, rax");                                       // did the composite helper return contents?
    emitter.instruction("jz __elephc_eval_ob_get_clean_none_x86");              // refused — report failure to Rust
    emitter.instruction("mov QWORD PTR [r9], rax");                             // store the owned raw-contents pointer for Rust
    emitter.instruction("mov QWORD PTR [r10], rdx");                            // store the raw-contents length for Rust
    emitter.instruction("mov eax, 1");                                          // report success to Rust
    emitter.instruction("jmp __elephc_eval_ob_get_clean_done_x86");             // finish
    emitter.label("__elephc_eval_ob_get_clean_none_x86");
    emitter.instruction("xor eax, eax");                                        // report refusal to Rust
    emitter.label("__elephc_eval_ob_get_clean_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the out-pointer slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the success flag

    label_c_global(emitter, "__elephc_eval_ob_get_flush_pop");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve slots for the out-pointer storage addresses
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the caller's out_ptr storage address
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the caller's out_len storage address
    emitter.instruction("call __rt_ob_get_flush_pop");                          // gate, run the handler, flush, pop → raw pair (or null)
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the out_ptr storage address
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the out_len storage address
    emitter.instruction("test rax, rax");                                       // did the composite helper return contents?
    emitter.instruction("jz __elephc_eval_ob_get_flush_none_x86");              // refused — report failure to Rust
    emitter.instruction("mov QWORD PTR [r9], rax");                             // store the owned raw-contents pointer for Rust
    emitter.instruction("mov QWORD PTR [r10], rdx");                            // store the raw-contents length for Rust
    emitter.instruction("mov eax, 1");                                          // report success to Rust
    emitter.instruction("jmp __elephc_eval_ob_get_flush_done_x86");             // finish
    emitter.label("__elephc_eval_ob_get_flush_none_x86");
    emitter.instruction("xor eax, eax");                                        // report refusal to Rust
    emitter.label("__elephc_eval_ob_get_flush_done_x86");
    emitter.instruction("add rsp, 16");                                         // release the out-pointer slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the success flag

    label_c_global(emitter, "__elephc_eval_ob_release_string");
    emitter.instruction("mov rax, rdi");                                        // move the owned string into the runtime release register
    emitter.instruction("jmp __rt_decref_any");                                 // release one bridge-returned owned string

    label_c_global(emitter, "__elephc_eval_ob_slot_meta");
    crate::codegen::abi::emit_symbol_address(emitter, "r9", "_ob_level");       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("cmp rdi, r10");                                        // is the requested slot index within the stack?
    emitter.instruction("jae __elephc_eval_ob_slot_meta_none_x86");             // out-of-range index — report failure
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_ob_chunk_sizes"); // materialize the chunk-size slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + rdi*8]");                    // load the slot's chunk size
    emitter.instruction("mov QWORD PTR [rsi], rax");                            // store it through the caller's out_chunk
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_ob_flags");      // materialize the flags slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + rdi*8]");                    // load the slot's stored flags word
    emitter.instruction("mov QWORD PTR [rdx], rax");                            // store it through the caller's out_flags
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_ob_handler_stubs"); // materialize the handler-stub slot array
    emitter.instruction("mov rax, QWORD PTR [r11 + rdi*8]");                    // load the slot's handler stub
    emitter.instruction("test rax, rax");                                       // is a user handler installed?
    emitter.instruction("setnz al");                                            // bit 0 = user-handler flag
    emitter.instruction("movzx rax, al");                                       // zero-extend the packed bits
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_ob_started");    // materialize the started-flag slot array
    emitter.instruction("mov r10, QWORD PTR [r11 + rdi*8]");                    // load the slot's started flag
    emitter.instruction("test r10, r10");                                       // has the handler run at least once?
    emitter.instruction("jz __elephc_eval_ob_slot_meta_store_x86");             // an unstarted handler keeps bit 1 clear
    emitter.instruction("or rax, 2");                                           // bit 1 = started flag
    emitter.label("__elephc_eval_ob_slot_meta_store_x86");
    emitter.instruction("mov QWORD PTR [rcx], rax");                            // store the packed user/started bits
    emitter.instruction("mov eax, 1");                                          // report success to Rust
    emitter.instruction("ret");                                                 // return to Rust
    emitter.label("__elephc_eval_ob_slot_meta_none_x86");
    emitter.instruction("xor eax, eax");                                        // report "no such buffer" to Rust
    emitter.instruction("ret");                                                 // return to Rust

    label_c_global(emitter, "__elephc_eval_ob_slot_name");
    crate::codegen::abi::emit_symbol_address(emitter, "r9", "_ob_level");       // materialize the address of the buffer-stack depth
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current buffer-stack depth
    emitter.instruction("cmp rdi, r10");                                        // is the requested slot index within the stack?
    emitter.instruction("jae __elephc_eval_ob_slot_name_none_x86");             // out-of-range index — report failure
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_ob_name_ptrs");  // materialize the handler-name pointer array
    emitter.instruction("mov rax, QWORD PTR [r11 + rdi*8]");                    // load the slot's display-name pointer
    emitter.instruction("mov QWORD PTR [rsi], rax");                            // store it through the caller's out_ptr
    crate::codegen::abi::emit_symbol_address(emitter, "r11", "_ob_name_lens");  // materialize the handler-name length array
    emitter.instruction("mov rax, QWORD PTR [r11 + rdi*8]");                    // load the slot's display-name length
    emitter.instruction("mov QWORD PTR [rdx], rax");                            // store it through the caller's out_len
    emitter.instruction("mov eax, 1");                                          // report success to Rust (bytes copied immediately)
    emitter.instruction("ret");                                                 // return to Rust
    emitter.label("__elephc_eval_ob_slot_name_none_x86");
    emitter.instruction("xor eax, eax");                                        // report "no such buffer" to Rust
    emitter.instruction("ret");                                                 // return to Rust

    label_c_global(emitter, "__elephc_eval_value_string_bytes");
    emitter.instruction("push rbp");                                            // preserve the Rust caller frame pointer across string casting
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve slots for the caller's output pointers
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the caller's out_ptr storage address
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the caller's out_len storage address
    emitter.instruction("mov rax, rdi");                                        // move the boxed eval value into mixed_cast_string input
    emitter.instruction("call __rt_mixed_cast_string");                         // cast the boxed eval value to a PHP string pair
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the optional out_ptr storage address
    emitter.instruction("test r10, r10");                                       // did the caller request the string pointer?
    emitter.instruction("jz __elephc_eval_value_string_bytes_len");             // skip pointer storage when the caller passed null
    emitter.instruction("mov QWORD PTR [r10], rax");                            // store the string pointer for Rust to copy immediately
    emitter.label("__elephc_eval_value_string_bytes_len");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the optional out_len storage address
    emitter.instruction("test r10, r10");                                       // did the caller request the string length?
    emitter.instruction("jz __elephc_eval_value_string_bytes_done");            // skip length storage when the caller passed null
    emitter.instruction("mov QWORD PTR [r10], rdx");                            // store the string byte length for Rust
    emitter.label("__elephc_eval_value_string_bytes_done");
    emitter.instruction("mov rax, 1");                                          // report successful string conversion to Rust
    emitter.instruction("add rsp, 16");                                         // release the string-bytes wrapper slots
    emitter.instruction("pop rbp");                                             // restore the Rust caller frame pointer
    emitter.instruction("ret");                                                 // return the success flag to Rust

    label_c_global(emitter, "__elephc_eval_value_truthy");
    emitter.instruction("mov rax, rdi");                                        // move the C boxed value argument into mixed truthiness input
    emitter.instruction("jmp __rt_mixed_cast_bool");                            // cast one boxed mixed value to PHP truthiness for eval

    label_c_global(emitter, "__elephc_eval_value_retain");
    emitter.instruction("mov rax, rdi");                                        // move the C boxed Mixed argument into the internal retain register
    emitter.instruction("jmp __rt_incref");                                     // retain one eval-owned boxed Mixed cell

    label_c_global(emitter, "__elephc_eval_value_final_object_identity");
    emitter.instruction("mov rax, rdi");                                        // inspect the C boxed Mixed argument without changing refcounts
    emitter.instruction("test rax, rax");                                       // null handles cannot release an object
    emitter.instruction("jz __elephc_eval_value_final_object_none_x86");        // report no final object for null handles
    emitter.label("__elephc_eval_value_final_object_loop_x86");
    emitter.instruction("mov r10d, DWORD PTR [rax - 12]");                      // load the current Mixed wrapper refcount
    emitter.instruction("cmp r10d, 1");                                         // only the last Mixed owner can free the wrapped payload
    emitter.instruction("jne __elephc_eval_value_final_object_none_x86");       // non-final wrapper releases cannot run destructors yet
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the current Mixed runtime tag
    emitter.instruction("cmp r10, 7");                                          // tag 7 means the payload is another boxed Mixed
    emitter.instruction("jne __elephc_eval_value_final_object_check_object_x86"); // concrete payloads can be tested for object ownership
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // follow the nested Mixed payload pointer
    emitter.instruction("test rax, rax");                                       // null nested payloads cannot release an object
    emitter.instruction("jnz __elephc_eval_value_final_object_loop_x86");       // continue while nested Mixed payloads are present
    emitter.instruction("jmp __elephc_eval_value_final_object_none_x86");       // return zero for null nested payloads
    emitter.label("__elephc_eval_value_final_object_check_object_x86");
    emitter.instruction("cmp r10, 6");                                          // tag 6 means the concrete payload is a PHP object
    emitter.instruction("jne __elephc_eval_value_final_object_none_x86");       // non-object releases have no dynamic destructor
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the object payload pointer from the Mixed cell
    emit_branch_if_null_container(
        emitter,
        "r10",
        "r11",
        "__elephc_eval_value_final_object_none_x86",
    );
    emitter.instruction("mov r11d, DWORD PTR [r10 - 12]");                      // load the object refcount that the Mixed release will decrement
    emitter.instruction("cmp r11d, 1");                                         // only the final object owner can run the destructor
    emitter.instruction("jne __elephc_eval_value_final_object_none_x86");       // defer destructor execution until object refcount is final
    emitter.instruction("mov rax, r10");                                        // return the object identity pointer to Rust
    emitter.instruction("ret");                                                 // finish the final-object query
    emitter.label("__elephc_eval_value_final_object_none_x86");
    emitter.instruction("xor eax, eax");                                        // report that no dynamic destructor should run
    emitter.instruction("ret");                                                 // return zero to Rust

    label_c_global(emitter, "__elephc_eval_warning");
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_error_reporting", 0); // load the PHP diagnostic mask shared with static code
    emitter.instruction("test r10, 2");                                         // is E_WARNING enabled for eval diagnostics?
    emitter.instruction("jz __elephc_eval_warning_done_x86");                   // suppress eval warning output when its mask bit is disabled
    emitter.instruction("jmp __rt_diag_warning");                               // emit or suppress one eval runtime warning
    emitter.label("__elephc_eval_warning_done_x86");
    emitter.instruction("ret");                                                 // return without output when E_WARNING is disabled

    label_c_global(emitter, "__elephc_eval_deprecated");
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_error_reporting", 0); // load the PHP diagnostic mask shared with static code
    emitter.instruction("test r10, 8192");                                      // is E_DEPRECATED enabled for eval diagnostics?
    emitter.instruction("jz __elephc_eval_deprecated_done_x86");                // suppress eval deprecation output when its mask bit is disabled
    emitter.instruction("jmp __rt_diag_write");                                 // emit or suppress one eval runtime deprecation
    emitter.label("__elephc_eval_deprecated_done_x86");
    emitter.instruction("ret");                                                 // return without output when E_DEPRECATED is disabled

    label_c_global(emitter, "__elephc_eval_error_reporting");
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "rax", "_rt_error_reporting", 0); // preserve the previous PHP diagnostic mask
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0); // load the active nested @ depth
    emitter.instruction("mov r11, 4437");                                      // PHP's fatal-only mask exposed during @
    emitter.instruction("and r11, rax");                                       // retain only fatal levels from the current user mask
    emitter.instruction("test r10, r10");                                      // is eval running inside a suppressed expression?
    emitter.instruction("cmovnz rax, r11");                                    // return the fatal-only mask while suppressed
    emitter.instruction("test rsi, rsi");                                       // did PHP supply a concrete integer level?
    emitter.instruction("jz __elephc_eval_error_reporting_done_x86");           // a missing/null level is a query only
    crate::codegen::abi::emit_store_reg_to_symbol(emitter, "rdi", "_rt_error_reporting", 0); // publish the replacement PHP diagnostic mask
    emitter.label("__elephc_eval_error_reporting_done_x86");
    emitter.instruction("ret");                                                 // return the previous mask to Rust

    label_c_global(emitter, "__elephc_eval_value_release");
    emitter.instruction("mov rax, rdi");                                        // move the C boxed Mixed argument into the internal release register
    emitter.instruction("jmp __rt_decref_mixed");                               // release one eval-owned boxed Mixed cell
}
