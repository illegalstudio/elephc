//! Purpose:
//! Invokes a concrete object's runtime `__debugInfo()` adapter by class id.
//! Exposes one target-aware helper returning an owned boxed `Mixed` result or null.
//!
//! Called from:
//! - Recursive `var_dump()` and `print_r()` object renderers.
//!
//! Key details:
//! - Adapter entries own ABI normalization; this helper only performs bounds-safe dispatch.
//! - The object receiver remains borrowed and the caller owns any non-null Mixed result.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Byte width of one `_class_pr_desc_*` declared-property row.
const PRINT_R_DESC_ROW_BYTES: u64 = 32;

/// Emits `__rt_object_debug_info`, returning an owned boxed Mixed cell or zero.
pub(crate) fn emit_object_debug_info(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_object_debug_info_linux_x86_64(emitter);
        emit_object_dynamic_hash_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: dynamic object __debugInfo dispatch ---");
    emitter.label_global("__rt_object_debug_info");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, "x0", 16);
    emitter.instruction("cbz x0, __rt_object_debug_info_none");                 // a null receiver has no debug projection
    emitter.instruction("ldr x9, [x0]");                                        // load the concrete runtime class id
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");
    emitter.instruction("ldr x10, [x10]");                                      // load the dense class-table extent
    emitter.instruction("cmp x9, x10");                                         // is the concrete class id in range?
    emitter.instruction("b.hs __rt_object_debug_info_none");                    // reject missing or corrupted class metadata
    abi::emit_symbol_address(emitter, "x10", "_class_debug_info_ptrs");
    emitter.instruction("ldr x9, [x10, x9, lsl #3]");                           // resolve this class's uniform debug adapter
    emitter.instruction("cbz x9, __rt_object_debug_info_none");                 // classes without __debugInfo use property metadata
    abi::load_at_offset(emitter, "x0", 16);
    abi::emit_call_reg(emitter, "x9");
    abi::store_at_offset(emitter, "x0", 24);
    emitter.instruction("cbz x0, __rt_object_debug_info_done");                 // a defensive zero result carries no PHP deprecation
    emitter.instruction("ldr x9, [x0]");                                        // inspect the boxed runtime return tag
    emitter.instruction("cmp x9, #4");                                          // did __debugInfo() return an indexed array?
    emitter.instruction("b.eq __rt_object_debug_info_done");                    // indexed arrays are valid debug projections
    emitter.instruction("cmp x9, #5");                                          // did __debugInfo() return an associative array?
    emitter.instruction("b.eq __rt_object_debug_info_done");                    // associative arrays are valid debug projections
    emitter.instruction("cmp x9, #8");                                          // did __debugInfo() return deprecated PHP null?
    emitter.instruction("b.eq __rt_object_debug_info_null");                    // null remains an empty projection after a deprecation
    abi::load_at_offset(emitter, "x0", 24);
    emitter.instruction("bl __rt_decref_mixed");                                // release the invalid owned projection before terminating
    abi::emit_symbol_address(emitter, "x1", "_debug_info_invalid_return");
    emitter.instruction("mov x2, #48");                                         // length of the stable invalid-return fatal
    emitter.instruction("mov x0, #2");                                          // write the fatal diagnostic to stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // expose an abnormal process status
    emitter.syscall(1);
    emitter.label("__rt_object_debug_info_null");
    abi::emit_symbol_address(emitter, "x1", "_debug_info_null_prefix");
    emitter.instruction("mov x2, #32");                                         // length of the deprecation prefix
    emitter.instruction("bl __rt_diag_warning");                                // emit or suppress the deprecation prefix
    abi::load_at_offset(emitter, "x9", 16);
    emitter.instruction("ldr x9, [x9]");                                        // load the concrete runtime class id
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");
    emitter.instruction("ldr x10, [x10]");                                      // load the dense class-name table extent
    emitter.instruction("cmp x9, x10");                                         // is the concrete class id in range?
    emitter.instruction("b.hs __rt_object_debug_info_null_anon");               // unknown ids contribute an empty name
    abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
    emitter.instruction("add x10, x10, x9, lsl #4");                            // select the class-name pointer/length entry
    emitter.instruction("ldr x1, [x10]");                                       // load the concrete class-name pointer
    emitter.instruction("ldr x2, [x10, #8]");                                   // load the concrete class-name length
    emitter.instruction("b __rt_object_debug_info_null_name_ready");            // continue with the resolved name
    emitter.label("__rt_object_debug_info_null_anon");
    abi::emit_symbol_address(emitter, "x1", "_class_name_missing");
    emitter.instruction("mov x2, #0");                                          // the anonymous fallback contributes no bytes
    emitter.label("__rt_object_debug_info_null_name_ready");
    emitter.instruction("bl __rt_diag_warning");                                // emit or suppress the concrete class name
    abi::emit_symbol_address(emitter, "x1", "_debug_info_null_suffix");
    emitter.instruction("mov x2, #61");                                         // length of the deprecation suffix and newline
    emitter.instruction("bl __rt_diag_warning");                                // finish the exact PHP 8.5 deprecation
    emitter.instruction("b __rt_object_debug_info_done");                       // restore the owned null cell for the caller
    emitter.label("__rt_object_debug_info_none");
    emitter.instruction("mov x0, #0");                                          // zero denotes no dynamic debug projection
    abi::store_at_offset(emitter, "x0", 24);
    emitter.label("__rt_object_debug_info_done");
    abi::load_at_offset(emitter, "x0", 24);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
    emit_object_dynamic_hash(emitter);
}

/// Emits the Linux x86_64 dynamic `__debugInfo()` dispatcher.
fn emit_object_debug_info_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: dynamic object __debugInfo dispatch ---");
    emitter.label_global("__rt_object_debug_info");
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, "rdi", 16);
    emitter.instruction("test rdi, rdi");                                       // does the caller provide a real object receiver?
    emitter.instruction("jz __rt_object_debug_info_none_x86");                  // a null receiver has no debug projection
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // load the concrete runtime class id
    abi::emit_symbol_address(emitter, "r10", "_class_name_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the dense class-table extent
    emitter.instruction("cmp r9, r10");                                         // is the concrete class id in range?
    emitter.instruction("jae __rt_object_debug_info_none_x86");                 // reject missing or corrupted class metadata
    abi::emit_symbol_address(emitter, "r10", "_class_debug_info_ptrs");
    emitter.instruction("mov r11, QWORD PTR [r10 + r9*8]");                     // resolve this class's uniform debug adapter
    emitter.instruction("test r11, r11");                                       // does this class expose __debugInfo?
    emitter.instruction("jz __rt_object_debug_info_none_x86");                  // classes without it use property metadata
    abi::load_at_offset(emitter, "rdi", 16);
    abi::emit_call_reg(emitter, "r11");
    abi::store_at_offset(emitter, "rax", 24);
    emitter.instruction("test rax, rax");                                       // did the adapter return a defensive zero?
    emitter.instruction("jz __rt_object_debug_info_done_x86");                  // zero carries no PHP deprecation
    emitter.instruction("cmp QWORD PTR [rax], 4");                              // did __debugInfo() return an indexed array?
    emitter.instruction("je __rt_object_debug_info_done_x86");                  // indexed arrays are valid debug projections
    emitter.instruction("cmp QWORD PTR [rax], 5");                              // did __debugInfo() return an associative array?
    emitter.instruction("je __rt_object_debug_info_done_x86");                  // associative arrays are valid debug projections
    emitter.instruction("cmp QWORD PTR [rax], 8");                              // did __debugInfo() return deprecated PHP null?
    emitter.instruction("je __rt_object_debug_info_null_x86");                  // null remains an empty projection after a deprecation
    abi::load_at_offset(emitter, "rax", 24);
    emitter.instruction("call __rt_decref_mixed");                              // release the invalid owned projection before terminating
    emitter.instruction("mov edi, 2");                                          // write the fatal diagnostic to Linux stderr
    abi::emit_symbol_address(emitter, "rsi", "_debug_info_invalid_return");
    emitter.instruction("mov edx, 48");                                         // length of the stable invalid-return fatal
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the fatal diagnostic
    emitter.instruction("mov edi, 1");                                          // expose an abnormal process status
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate before a catch can resume execution
    emitter.label("__rt_object_debug_info_null_x86");
    abi::emit_symbol_address(emitter, "rdi", "_debug_info_null_prefix");
    emitter.instruction("mov esi, 32");                                         // length of the deprecation prefix
    emitter.instruction("call __rt_diag_warning");                              // emit or suppress the deprecation prefix
    abi::load_at_offset(emitter, "r9", 16);
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the concrete runtime class id
    abi::emit_symbol_address(emitter, "r10", "_class_name_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the dense class-name table extent
    emitter.instruction("cmp r9, r10");                                         // is the concrete class id in range?
    emitter.instruction("jae __rt_object_debug_info_null_anon_x86");            // unknown ids contribute an empty name
    abi::emit_symbol_address(emitter, "r10", "_class_name_entries");
    emitter.instruction("imul r9, r9, 16");                                     // convert the class id to a 16-byte table offset
    emitter.instruction("add r10, r9");                                         // select this class's name pointer/length entry
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // load the concrete class-name pointer
    emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                        // load the concrete class-name length
    emitter.instruction("jmp __rt_object_debug_info_null_name_ready_x86");      // continue with the resolved name
    emitter.label("__rt_object_debug_info_null_anon_x86");
    abi::emit_symbol_address(emitter, "rdi", "_class_name_missing");
    emitter.instruction("xor esi, esi");                                        // the anonymous fallback contributes no bytes
    emitter.label("__rt_object_debug_info_null_name_ready_x86");
    emitter.instruction("call __rt_diag_warning");                              // emit or suppress the concrete class name
    abi::emit_symbol_address(emitter, "rdi", "_debug_info_null_suffix");
    emitter.instruction("mov esi, 61");                                         // length of the deprecation suffix and newline
    emitter.instruction("call __rt_diag_warning");                              // finish the exact PHP 8.5 deprecation
    emitter.instruction("jmp __rt_object_debug_info_done_x86");                 // restore the owned null cell for the caller
    emitter.label("__rt_object_debug_info_none_x86");
    emitter.instruction("xor eax, eax");                                        // zero denotes no dynamic debug projection
    abi::store_at_offset(emitter, "rax", 24);
    emitter.label("__rt_object_debug_info_done_x86");
    abi::load_at_offset(emitter, "rax", 24);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

/// Emits the AArch64 lookup for a concrete object's optional dynamic-property hash tail.
fn emit_object_dynamic_hash(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object dynamic-property hash lookup ---");
    emitter.label_global("__rt_object_dynamic_hash");
    emitter.instruction("cbz x0, __rt_object_dynamic_hash_none");               // a null object has no dynamic-property storage
    emitter.instruction("ldr x9, [x0]");                                        // load the concrete runtime class id
    abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");
    emitter.instruction("ldr x10, [x10]");                                      // load the dense class metadata extent
    emitter.instruction("cmp x9, x10");                                         // is the class id represented by metadata?
    emitter.instruction("b.hs __rt_object_dynamic_hash_none");                  // unknown class ids have no dynamic-property tail
    abi::emit_symbol_address(emitter, "x10", "_class_object_dynamic_prop_flags");
    emitter.instruction("ldr x11, [x10, x9, lsl #3]");                          // load this class's dynamic-tail flag
    emitter.instruction("cbz x11, __rt_object_dynamic_hash_none");              // ordinary fixed-layout classes have no tail
    abi::emit_symbol_address(emitter, "x10", "_class_object_payload_sizes");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // load the full object payload size
    emitter.instruction("cmp x10, #8");                                         // can the payload contain the trailing hash pointer?
    emitter.instruction("b.lo __rt_object_dynamic_hash_none");                  // reject malformed metadata without reading before the object
    emitter.instruction("sub x10, x10, #8");                                    // compute the trailing hash-slot offset
    emitter.instruction("ldr x0, [x0, x10]");                                   // return the optional dynamic-property hash pointer
    emitter.instruction("ret");                                                 // return to the renderer
    emitter.label("__rt_object_dynamic_hash_none");
    emitter.instruction("mov x0, #0");                                          // zero denotes no dynamic properties
    emitter.instruction("ret");                                                 // return the empty lookup result
}

/// Emits the Linux x86_64 lookup for a concrete object's dynamic-property hash tail.
fn emit_object_dynamic_hash_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object dynamic-property hash lookup ---");
    emitter.label_global("__rt_object_dynamic_hash");
    emitter.instruction("test rdi, rdi");                                       // a null object has no dynamic-property storage
    emitter.instruction("jz __rt_object_dynamic_hash_none_x86");                // return the empty lookup result
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // load the concrete runtime class id
    abi::emit_symbol_address(emitter, "r10", "_class_gc_desc_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the dense class metadata extent
    emitter.instruction("cmp r9, r10");                                         // is the class id represented by metadata?
    emitter.instruction("jae __rt_object_dynamic_hash_none_x86");               // unknown class ids have no dynamic-property tail
    abi::emit_symbol_address(emitter, "r10", "_class_object_dynamic_prop_flags");
    emitter.instruction("cmp QWORD PTR [r10 + r9*8], 0");                       // does this class reserve the dynamic hash tail?
    emitter.instruction("je __rt_object_dynamic_hash_none_x86");                // ordinary fixed-layout classes have no tail
    abi::emit_symbol_address(emitter, "r10", "_class_object_payload_sizes");
    emitter.instruction("mov r10, QWORD PTR [r10 + r9*8]");                     // load the full object payload size
    emitter.instruction("cmp r10, 8");                                          // can the payload contain the trailing hash pointer?
    emitter.instruction("jb __rt_object_dynamic_hash_none_x86");                // reject malformed metadata without reading before the object
    emitter.instruction("sub r10, 8");                                          // compute the trailing hash-slot offset
    emitter.instruction("mov rax, QWORD PTR [rdi + r10]");                      // return the optional dynamic-property hash pointer
    emitter.instruction("ret");                                                 // return to the renderer
    emitter.label("__rt_object_dynamic_hash_none_x86");
    emitter.instruction("xor eax, eax");                                        // zero denotes no dynamic properties
    emitter.instruction("ret");                                                 // return the empty lookup result
}

/// Emits the declared-property fallback used when a class has no `__debugInfo()`.
fn emit_print_r_declared_properties(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_print_r_declared_properties_linux_x86_64(emitter);
        emit_print_r_dynamic_properties_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: print_r declared object properties ---");
    emitter.label_global("__rt_print_r_object_declared");
    abi::emit_frame_prologue(emitter, 96);
    abi::store_at_offset(emitter, "x0", 16);
    abi::store_at_offset(emitter, "x1", 24);
    emitter.instruction("ldr x9, [x0]");                                        // load the concrete runtime class id
    abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");
    emitter.instruction("ldr x10, [x10]");                                      // load the dense descriptor-table extent
    emitter.instruction("cmp x9, x10");                                         // is the class id represented in runtime metadata?
    emitter.instruction("b.hs __rt_print_r_object_declared_missing");           // unknown ids use the empty descriptor
    abi::emit_symbol_address(emitter, "x10", "_class_pr_desc_ptrs");
    emitter.instruction("ldr x10, [x10, x9, lsl #3]");                          // load this class's print_r descriptor
    emitter.instruction("b __rt_print_r_object_declared_desc_ready");           // continue with the resolved descriptor
    emitter.label("__rt_print_r_object_declared_missing");
    abi::emit_symbol_address(emitter, "x10", "_class_pr_desc_missing");
    emitter.label("__rt_print_r_object_declared_desc_ready");
    abi::store_at_offset(emitter, "x10", 32);
    emitter.instruction("ldr x9, [x10]");                                       // load the declared-property row count
    abi::store_at_offset(emitter, "x9", 40);
    emitter.instruction("mov x9, #0");                                          // property index starts at zero
    abi::store_at_offset(emitter, "x9", 48);
    abi::load_at_offset(emitter, "x0", 24);
    emitter.instruction("bl __rt_print_r_open");                                // write the object's opening parenthesis

    emitter.label("__rt_print_r_object_declared_loop");
    abi::load_at_offset(emitter, "x9", 48);
    abi::load_at_offset(emitter, "x10", 40);
    emitter.instruction("cmp x9, x10");                                         // inspected every declared property?
    emitter.instruction("b.hs __rt_print_r_object_declared_done");              // finish after the final row
    abi::load_at_offset(emitter, "x11", 32);
    emitter.instruction(&format!("mov x12, #{}", PRINT_R_DESC_ROW_BYTES));      // materialize the row stride
    emitter.instruction("mul x12, x9, x12");                                    // compute this row's byte offset
    emitter.instruction("add x11, x11, x12");                                   // advance into the descriptor table
    emitter.instruction("add x11, x11, #8");                                    // skip the leading row count
    abi::store_at_offset(emitter, "x11", 56);
    emitter.instruction("ldr x12, [x11, #16]");                                 // load the property's object-slot offset
    abi::load_at_offset(emitter, "x13", 16);
    emitter.instruction("add x12, x13, x12");                                   // resolve the absolute property slot
    abi::store_at_offset(emitter, "x12", 64);
    emitter.instruction("ldr x13, [x12, #8]");                                  // load the slot high word for initialization state
    emitter.instruction("movz x14, #0xfffd");                                   // low halfword of the uninitialized sentinel
    emitter.instruction("movk x14, #0xffff, lsl #16");                          // second sentinel halfword
    emitter.instruction("movk x14, #0xffff, lsl #32");                          // third sentinel halfword
    emitter.instruction("movk x14, #0x7fff, lsl #48");                          // top sentinel halfword
    emitter.instruction("cmp x13, x14");                                        // is this typed property still uninitialized?
    emitter.instruction("b.eq __rt_print_r_object_declared_next");              // print_r omits uninitialized properties

    abi::load_at_offset(emitter, "x11", 56);
    emitter.instruction("ldr x0, [x11]");                                       // load the unquoted visibility-aware key pointer
    emitter.instruction("ldr x1, [x11, #8]");                                   // load the exact key length
    abi::load_at_offset(emitter, "x2", 24);
    emitter.instruction("add x2, x2, #4");                                      // property entries are indented four spaces inside the body
    emitter.instruction("bl __rt_print_r_str_key");                             // write `<indent>[key] => `
    abi::load_at_offset(emitter, "x11", 56);
    abi::load_at_offset(emitter, "x12", 64);
    emitter.instruction("ldr x0, [x11, #24]");                                  // load the property's runtime value tag
    emitter.instruction("ldr x1, [x12]");                                       // load the property value low word
    emitter.instruction("ldr x2, [x12, #8]");                                   // load the property value high word
    abi::load_at_offset(emitter, "x3", 24);
    emitter.instruction("add x3, x3, #8");                                      // nested containers indent beneath the property entry
    emitter.instruction("bl __rt_print_r_value");                               // render the property value recursively
    abi::emit_symbol_address(emitter, "x1", "_pr_nl");
    emitter.instruction("mov x2, #1");                                          // terminate this property entry
    emitter.instruction("bl __rt_pr_write");                                    // write the trailing newline

    emitter.label("__rt_print_r_object_declared_next");
    abi::load_at_offset(emitter, "x9", 48);
    emitter.instruction("add x9, x9, #1");                                      // advance to the next descriptor row
    abi::store_at_offset(emitter, "x9", 48);
    emitter.instruction("b __rt_print_r_object_declared_loop");                 // continue the declared-property walk

    emitter.label("__rt_print_r_object_declared_done");
    abi::load_at_offset(emitter, "x0", 16);
    abi::load_at_offset(emitter, "x1", 24);
    emitter.instruction("bl __rt_print_r_object_dynamic");                      // append public dynamic properties in insertion order
    abi::load_at_offset(emitter, "x0", 24);
    emitter.instruction("bl __rt_print_r_close");                               // write the object's closing parenthesis
    abi::emit_frame_restore(emitter, 96);
    abi::emit_return(emitter);
    emit_print_r_dynamic_properties(emitter);
}

/// Emits the Linux x86_64 declared-property fallback for `print_r()` objects.
fn emit_print_r_declared_properties_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r declared object properties ---");
    emitter.label_global("__rt_print_r_object_declared");
    abi::emit_frame_prologue(emitter, 96);
    abi::store_at_offset(emitter, "rdi", 16);
    abi::store_at_offset(emitter, "rsi", 24);
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // load the concrete runtime class id
    abi::emit_symbol_address(emitter, "r10", "_class_gc_desc_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the dense descriptor-table extent
    emitter.instruction("cmp r9, r10");                                         // is the class id represented in runtime metadata?
    emitter.instruction("jae __rt_print_r_object_declared_missing_x86");        // unknown ids use the empty descriptor
    abi::emit_symbol_address(emitter, "r10", "_class_pr_desc_ptrs");
    emitter.instruction("mov r10, QWORD PTR [r10 + r9*8]");                     // load this class's print_r descriptor
    emitter.instruction("jmp __rt_print_r_object_declared_desc_ready_x86");     // continue with the resolved descriptor
    emitter.label("__rt_print_r_object_declared_missing_x86");
    abi::emit_symbol_address(emitter, "r10", "_class_pr_desc_missing");
    emitter.label("__rt_print_r_object_declared_desc_ready_x86");
    abi::store_at_offset(emitter, "r10", 32);
    emitter.instruction("mov r9, QWORD PTR [r10]");                             // load the declared-property row count
    abi::store_at_offset(emitter, "r9", 40);
    emitter.instruction("xor eax, eax");                                        // property index starts at zero
    abi::store_at_offset(emitter, "rax", 48);
    abi::load_at_offset(emitter, "rdi", 24);
    emitter.instruction("call __rt_print_r_open");                              // write the object's opening parenthesis

    emitter.label("__rt_print_r_object_declared_loop_x86");
    abi::load_at_offset(emitter, "r9", 48);
    abi::load_at_offset(emitter, "r10", 40);
    emitter.instruction("cmp r9, r10");                                         // inspected every declared property?
    emitter.instruction("jae __rt_print_r_object_declared_done_x86");           // finish after the final row
    abi::load_at_offset(emitter, "r11", 32);
    emitter.instruction(&format!("imul r9, r9, {}", PRINT_R_DESC_ROW_BYTES));   // compute this row's byte offset
    emitter.instruction("add r11, r9");                                         // advance into the descriptor table
    emitter.instruction("add r11, 8");                                          // skip the leading row count
    abi::store_at_offset(emitter, "r11", 56);
    emitter.instruction("mov r10, QWORD PTR [r11 + 16]");                       // load the property's object-slot offset
    abi::load_at_offset(emitter, "r12", 16);
    emitter.instruction("add r10, r12");                                        // resolve the absolute property slot
    abi::store_at_offset(emitter, "r10", 64);
    emitter.instruction("mov r12, QWORD PTR [r10 + 8]");                        // load the slot high word for initialization state
    emitter.instruction("movabs r13, 0x7ffffffffffffffd");                      // materialize the uninitialized-property sentinel
    emitter.instruction("cmp r12, r13");                                        // is this typed property still uninitialized?
    emitter.instruction("je __rt_print_r_object_declared_next_x86");            // print_r omits uninitialized properties

    abi::load_at_offset(emitter, "r11", 56);
    emitter.instruction("mov rdi, QWORD PTR [r11]");                            // load the unquoted visibility-aware key pointer
    emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                        // load the exact key length
    abi::load_at_offset(emitter, "rdx", 24);
    emitter.instruction("add rdx, 4");                                          // property entries are indented four spaces inside the body
    emitter.instruction("call __rt_print_r_str_key");                           // write `<indent>[key] => `
    abi::load_at_offset(emitter, "r11", 56);
    abi::load_at_offset(emitter, "r10", 64);
    emitter.instruction("mov rdi, QWORD PTR [r11 + 24]");                       // load the property's runtime value tag
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the property value low word
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the property value high word
    abi::load_at_offset(emitter, "rcx", 24);
    emitter.instruction("add rcx, 8");                                          // nested containers indent beneath the property entry
    emitter.instruction("call __rt_print_r_value");                             // render the property value recursively
    abi::emit_symbol_address(emitter, "rsi", "_pr_nl");
    emitter.instruction("mov edx, 1");                                          // terminate this property entry
    emitter.instruction("call __rt_pr_write");                                  // write the trailing newline

    emitter.label("__rt_print_r_object_declared_next_x86");
    abi::load_at_offset(emitter, "r9", 48);
    emitter.instruction("add r9, 1");                                           // advance to the next descriptor row
    abi::store_at_offset(emitter, "r9", 48);
    emitter.instruction("jmp __rt_print_r_object_declared_loop_x86");           // continue the declared-property walk

    emitter.label("__rt_print_r_object_declared_done_x86");
    abi::load_at_offset(emitter, "rdi", 16);
    abi::load_at_offset(emitter, "rsi", 24);
    emitter.instruction("call __rt_print_r_object_dynamic");                    // append public dynamic properties in insertion order
    abi::load_at_offset(emitter, "rdi", 24);
    emitter.instruction("call __rt_print_r_close");                             // write the object's closing parenthesis
    abi::emit_frame_restore(emitter, 96);
    abi::emit_return(emitter);
}

/// Emits the AArch64 body-only walker for public dynamic object properties.
fn emit_print_r_dynamic_properties(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r dynamic object properties ---");
    emitter.label_global("__rt_print_r_object_dynamic");
    abi::emit_frame_prologue(emitter, 128);
    abi::store_at_offset(emitter, "x0", 16);
    abi::store_at_offset(emitter, "x1", 24);
    emitter.instruction("bl __rt_object_dynamic_hash");                         // resolve the object's optional dynamic-property hash
    abi::store_at_offset(emitter, "x0", 32);
    emitter.instruction("cbz x0, __rt_print_r_object_dynamic_done");            // classes without a populated tail add no rows
    emitter.instruction("ldr x9, [x0]");                                        // load the insertion-ordered dynamic entry count
    abi::store_at_offset(emitter, "x9", 40);
    emitter.instruction("mov x9, #0");                                          // iterator cursor starts before the first entry
    abi::store_at_offset(emitter, "x9", 48);
    abi::store_at_offset(emitter, "x9", 56);
    emitter.label("__rt_print_r_object_dynamic_loop");
    abi::load_at_offset(emitter, "x9", 56);
    abi::load_at_offset(emitter, "x10", 40);
    emitter.instruction("cmp x9, x10");                                         // rendered every dynamic property?
    emitter.instruction("b.hs __rt_print_r_object_dynamic_done");               // finish after the insertion-order tail
    abi::load_at_offset(emitter, "x0", 32);
    abi::load_at_offset(emitter, "x1", 48);
    emitter.instruction("bl __rt_hash_iter_next");                              // fetch key, payload, tag, and next cursor
    abi::store_at_offset(emitter, "x0", 48);
    abi::store_at_offset(emitter, "x1", 64);
    abi::store_at_offset(emitter, "x2", 72);
    abi::store_at_offset(emitter, "x3", 80);
    abi::store_at_offset(emitter, "x4", 88);
    abi::store_at_offset(emitter, "x5", 96);
    abi::load_at_offset(emitter, "x2", 72);
    emitter.instruction("cmn x2, #1");                                          // did key normalization produce an integer key?
    emitter.instruction("b.eq __rt_print_r_object_dynamic_int_key");            // render numeric dynamic names without quotes
    abi::load_at_offset(emitter, "x0", 64);
    abi::load_at_offset(emitter, "x1", 72);
    abi::load_at_offset(emitter, "x2", 24);
    emitter.instruction("add x2, x2, #4");                                      // object entries indent four spaces inside the body
    emitter.instruction("bl __rt_print_r_str_key");                             // write the public dynamic property key
    emitter.instruction("b __rt_print_r_object_dynamic_value");                 // continue with its value
    emitter.label("__rt_print_r_object_dynamic_int_key");
    abi::load_at_offset(emitter, "x0", 64);
    abi::load_at_offset(emitter, "x1", 24);
    emitter.instruction("add x1, x1, #4");                                      // object entries indent four spaces inside the body
    emitter.instruction("bl __rt_print_r_int_key");                             // write the normalized numeric property name
    emitter.label("__rt_print_r_object_dynamic_value");
    abi::load_at_offset(emitter, "x0", 96);
    abi::load_at_offset(emitter, "x1", 80);
    abi::load_at_offset(emitter, "x2", 88);
    abi::load_at_offset(emitter, "x3", 24);
    emitter.instruction("add x3, x3, #8");                                      // nested containers indent beneath the property entry
    emitter.instruction("bl __rt_print_r_value");                               // render the dynamic Mixed payload recursively
    abi::emit_symbol_address(emitter, "x1", "_pr_nl");
    emitter.instruction("mov x2, #1");                                          // terminate this property entry
    emitter.instruction("bl __rt_pr_write");                                    // write the trailing newline
    abi::load_at_offset(emitter, "x9", 56);
    emitter.instruction("add x9, x9, #1");                                      // count this rendered dynamic property
    abi::store_at_offset(emitter, "x9", 56);
    emitter.instruction("b __rt_print_r_object_dynamic_loop");                  // continue through insertion order
    emitter.label("__rt_print_r_object_dynamic_done");
    abi::emit_frame_restore(emitter, 128);
    abi::emit_return(emitter);
}

/// Emits the Linux x86_64 body-only walker for public dynamic object properties.
fn emit_print_r_dynamic_properties_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r dynamic object properties ---");
    emitter.label_global("__rt_print_r_object_dynamic");
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the dynamic-property walk frame
    emitter.instruction("sub rsp, 96");                                         // reserve object, hash, cursor, and entry payload slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the object pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the object body indent
    emitter.instruction("call __rt_object_dynamic_hash");                       // resolve the object's optional dynamic-property hash
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the hash pointer
    emitter.instruction("test rax, rax");                                       // is the tail populated?
    emitter.instruction("jz __rt_print_r_object_dynamic_done_x86");             // classes without dynamic properties add no rows
    emitter.instruction("mov r9, QWORD PTR [rax]");                             // load the insertion-ordered entry count
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // save the loop bound
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // iterator cursor starts before the first entry
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // rendered entry count starts at zero
    emitter.label("__rt_print_r_object_dynamic_loop_x86");
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // reload the rendered entry count
    emitter.instruction("cmp r9, QWORD PTR [rbp - 32]");                        // rendered every dynamic property?
    emitter.instruction("jae __rt_print_r_object_dynamic_done_x86");            // finish after the insertion-order tail
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // hash pointer → iterator
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // cursor → iterator
    emitter.instruction("call __rt_hash_iter_next");                            // fetch key, payload, tag, and next cursor
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the next cursor
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // save the key pointer or integer payload
    emitter.instruction("mov QWORD PTR [rbp - 64], rdx");                       // save the key length or -1 integer marker
    emitter.instruction("mov QWORD PTR [rbp - 72], rcx");                       // save the value low word
    emitter.instruction("mov QWORD PTR [rbp - 80], r8");                        // save the value high word
    emitter.instruction("mov QWORD PTR [rbp - 88], r9");                        // save the runtime value tag
    emitter.instruction("cmp rdx, -1");                                         // did key normalization produce an integer key?
    emitter.instruction("je __rt_print_r_object_dynamic_int_key_x86");          // render numeric dynamic names without quotes
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // string property name pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // string property name length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the object body indent
    emitter.instruction("add rdx, 4");                                          // object entries indent four spaces inside the body
    emitter.instruction("call __rt_print_r_str_key");                           // write the public dynamic property key
    emitter.instruction("jmp __rt_print_r_object_dynamic_value_x86");           // continue with its value
    emitter.label("__rt_print_r_object_dynamic_int_key_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // numeric property name payload
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the object body indent
    emitter.instruction("add rsi, 4");                                          // object entries indent four spaces inside the body
    emitter.instruction("call __rt_print_r_int_key");                           // write the normalized numeric property name
    emitter.label("__rt_print_r_object_dynamic_value_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // value tag → recursive renderer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // value low word → recursive renderer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // value high word → recursive renderer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the object body indent
    emitter.instruction("add rcx, 8");                                          // nested containers indent beneath the property entry
    emitter.instruction("call __rt_print_r_value");                             // render the dynamic Mixed payload recursively
    abi::emit_symbol_address(emitter, "rsi", "_pr_nl");
    emitter.instruction("mov edx, 1");                                          // terminate this property entry
    emitter.instruction("call __rt_pr_write");                                  // write the trailing newline
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // count this rendered dynamic property
    emitter.instruction("jmp __rt_print_r_object_dynamic_loop_x86");            // continue through insertion order
    emitter.label("__rt_print_r_object_dynamic_done_x86");
    emitter.instruction("add rsp, 96");                                         // release the dynamic-property walk frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the declared-property renderer
}

/// Emits `__rt_print_r_object`, rendering a concrete object's class header and
/// its runtime `__debugInfo()` projection at the caller-provided paren indent.
pub(crate) fn emit_print_r_object(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_print_r_declared_properties(emitter);
        emit_print_r_object_linux_x86_64(emitter);
        return;
    }

    emit_print_r_declared_properties(emitter);

    emitter.blank();
    emitter.comment("--- runtime: print_r dynamic object projection ---");
    emitter.label_global("__rt_print_r_object");
    abi::emit_frame_prologue(emitter, 64);
    abi::store_at_offset(emitter, "x0", 16);
    abi::store_at_offset(emitter, "x1", 24);
    emitter.instruction("mov x9, #0");                                          // recursive occurrences own no debug projection
    abi::store_at_offset(emitter, "x9", 32);
    emitter.instruction("bl __rt_vd_seen_find");                                // detect a recursive object before invoking user code again
    abi::store_at_offset(emitter, "x0", 40);
    emitter.instruction("cbnz x0, __rt_print_r_object_projection_ready");       // recursive occurrences print only the PHP recursion marker
    abi::load_at_offset(emitter, "x0", 16);
    emitter.instruction("bl __rt_object_debug_info");                           // invoke __debugInfo before protecting the returned property walk
    abi::store_at_offset(emitter, "x0", 32);
    abi::load_at_offset(emitter, "x0", 16);
    emitter.instruction("bl __rt_vd_seen_push");                                // protect only the returned properties, matching php-src
    emitter.instruction("b __rt_print_r_object_projection_ready");              // continue with the owned projection
    emitter.label("__rt_print_r_object_projection_ready");

    // -- write `<ConcreteClass> Object\n` at the current value position --
    abi::load_at_offset(emitter, "x9", 16);
    emitter.instruction("ldr x9, [x9]");                                        // load the concrete runtime class id
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");
    emitter.instruction("ldr x10, [x10]");                                      // load the dense class-name table extent
    emitter.instruction("cmp x9, x10");                                         // is this class id represented in the name table?
    emitter.instruction("b.hs __rt_print_r_object_anon");                       // unknown class ids use the empty fallback name
    abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
    emitter.instruction("add x10, x10, x9, lsl #4");                            // select the 16-byte name pointer/length entry
    emitter.instruction("ldr x1, [x10]");                                       // load the concrete class-name pointer
    emitter.instruction("ldr x2, [x10, #8]");                                   // load the concrete class-name length
    emitter.instruction("b __rt_print_r_object_name_ready");                    // continue with the resolved class name
    emitter.label("__rt_print_r_object_anon");
    abi::emit_symbol_address(emitter, "x1", "_class_name_missing");
    emitter.instruction("mov x2, #0");                                          // the anonymous fallback contributes no bytes
    emitter.label("__rt_print_r_object_name_ready");
    emitter.instruction("bl __rt_pr_write");                                    // write the concrete class name
    abi::emit_symbol_address(emitter, "x1", "_pr_object_suffix");
    emitter.instruction("mov x2, #8");                                          // len(" Object\n") = 8
    emitter.instruction("bl __rt_pr_write");                                    // finish the PHP object header line
    abi::load_at_offset(emitter, "x9", 40);
    emitter.instruction("cbnz x9, __rt_print_r_object_recursion");              // a revisited object has no second projected body

    // -- render the projected array body without an `Array\n` prefix --
    abi::load_at_offset(emitter, "x9", 32);
    emitter.instruction("cbz x9, __rt_print_r_object_declared_body");           // classes without a projection expose declared properties
    emitter.instruction("ldr x10, [x9]");                                       // read the boxed projection's runtime value tag
    emitter.instruction("cmp x10, #4");                                         // indexed-array projection?
    emitter.instruction("b.eq __rt_print_r_object_indexed");                    // use the indexed walker directly
    emitter.instruction("cmp x10, #5");                                         // associative-array projection?
    emitter.instruction("b.eq __rt_print_r_object_hash");                       // use the hash walker directly
    emitter.instruction("b __rt_print_r_object_empty");                         // null or unsupported projections render empty parentheses
    emitter.label("__rt_print_r_object_declared_body");
    abi::load_at_offset(emitter, "x0", 16);
    abi::load_at_offset(emitter, "x1", 24);
    emitter.instruction("bl __rt_print_r_object_declared");                     // render initialized declared properties with visibility keys
    emitter.instruction("b __rt_print_r_object_release");                       // declared-property body complete
    emitter.label("__rt_print_r_object_indexed");
    emitter.instruction("ldr x0, [x9, #8]");                                    // load the raw indexed-array payload
    abi::load_at_offset(emitter, "x1", 24);
    emitter.instruction("bl __rt_print_r_indexed");                             // render the projected body at the requested indent
    emitter.instruction("b __rt_print_r_object_release");                       // projection body complete
    emitter.label("__rt_print_r_object_hash");
    emitter.instruction("ldr x0, [x9, #8]");                                    // load the raw associative-array payload
    abi::load_at_offset(emitter, "x1", 24);
    emitter.instruction("bl __rt_print_r_debug_hash");                          // render and demangle the projected property keys
    emitter.instruction("b __rt_print_r_object_release");                       // projection body complete
    emitter.label("__rt_print_r_object_empty");
    abi::load_at_offset(emitter, "x0", 24);
    emitter.instruction("bl __rt_print_r_open");                                // write the empty object's opening parenthesis
    abi::load_at_offset(emitter, "x0", 24);
    emitter.instruction("bl __rt_print_r_close");                               // write the empty object's closing parenthesis
    emitter.label("__rt_print_r_object_release");
    abi::load_at_offset(emitter, "x0", 32);
    emitter.instruction("bl __rt_decref_mixed");                                // release the owned dynamic projection after rendering
    emitter.instruction("bl __rt_vd_seen_pop");                                 // remove this object from the active print walk
    emitter.instruction("b __rt_print_r_object_done");                          // skip the recursion-only marker path
    emitter.label("__rt_print_r_object_recursion");
    abi::emit_symbol_address(emitter, "x1", "_pr_recursion");
    emitter.instruction("mov x2, #12");                                         // len(" *RECURSION*") = 12; the owning walker adds the newline
    emitter.instruction("bl __rt_pr_write");                                    // match PHP's object recursion marker exactly
    emitter.label("__rt_print_r_object_done");
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}

/// Emits the Linux x86_64 dynamic object renderer used by `print_r()`.
fn emit_print_r_object_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: print_r dynamic object projection ---");
    emitter.label_global("__rt_print_r_object");
    abi::emit_frame_prologue(emitter, 64);
    abi::store_at_offset(emitter, "rdi", 16);
    abi::store_at_offset(emitter, "rsi", 24);
    emitter.instruction("xor eax, eax");                                        // recursive occurrences own no debug projection
    abi::store_at_offset(emitter, "rax", 32);
    abi::emit_call_label(emitter, "__rt_vd_seen_find");
    abi::store_at_offset(emitter, "rax", 40);
    emitter.instruction("test rax, rax");                                       // is this object already active in the print walk?
    emitter.instruction("jnz __rt_print_r_object_projection_ready_x86");        // recursive occurrences print only the PHP recursion marker
    abi::load_at_offset(emitter, "rdi", 16);
    abi::emit_call_label(emitter, "__rt_object_debug_info");
    abi::store_at_offset(emitter, "rax", 32);
    abi::load_at_offset(emitter, "rdi", 16);
    emitter.instruction("call __rt_vd_seen_push");                              // protect only the returned properties, matching php-src
    emitter.instruction("jmp __rt_print_r_object_projection_ready_x86");        // continue with the owned projection
    emitter.label("__rt_print_r_object_projection_ready_x86");

    abi::load_at_offset(emitter, "r9", 16);
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // load the concrete runtime class id
    abi::emit_symbol_address(emitter, "r10", "_class_name_count");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load the dense class-name table extent
    emitter.instruction("cmp r9, r10");                                         // is this class id represented in the name table?
    emitter.instruction("jae __rt_print_r_object_anon_x86");                    // unknown class ids use the empty fallback name
    abi::emit_symbol_address(emitter, "r10", "_class_name_entries");
    emitter.instruction("imul r9, r9, 16");                                     // convert the class id to a 16-byte table offset
    emitter.instruction("add r10, r9");                                         // select this class's name pointer/length entry
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the concrete class-name pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the concrete class-name length
    emitter.instruction("jmp __rt_print_r_object_name_ready_x86");              // continue with the resolved class name
    emitter.label("__rt_print_r_object_anon_x86");
    abi::emit_symbol_address(emitter, "rsi", "_class_name_missing");
    emitter.instruction("xor edx, edx");                                        // the anonymous fallback contributes no bytes
    emitter.label("__rt_print_r_object_name_ready_x86");
    emitter.instruction("call __rt_pr_write");                                  // write the concrete class name
    abi::emit_symbol_address(emitter, "rsi", "_pr_object_suffix");
    emitter.instruction("mov edx, 8");                                          // len(" Object\n") = 8
    emitter.instruction("call __rt_pr_write");                                  // finish the PHP object header line
    abi::load_at_offset(emitter, "r9", 40);
    emitter.instruction("test r9, r9");                                         // is this the recursive occurrence?
    emitter.instruction("jnz __rt_print_r_object_recursion_x86");               // recursive occurrences have no second projected body

    abi::load_at_offset(emitter, "r9", 32);
    emitter.instruction("test r9, r9");                                         // did dynamic dispatch produce a projection?
    emitter.instruction("jz __rt_print_r_object_declared_body_x86");            // classes without one expose declared properties
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // read the boxed projection's runtime value tag
    emitter.instruction("cmp r10, 4");                                          // indexed-array projection?
    emitter.instruction("je __rt_print_r_object_indexed_x86");                  // use the indexed walker directly
    emitter.instruction("cmp r10, 5");                                          // associative-array projection?
    emitter.instruction("je __rt_print_r_object_hash_x86");                     // use the hash walker directly
    emitter.instruction("jmp __rt_print_r_object_empty_x86");                   // null or unsupported projections render empty parentheses
    emitter.label("__rt_print_r_object_declared_body_x86");
    abi::load_at_offset(emitter, "rdi", 16);
    abi::load_at_offset(emitter, "rsi", 24);
    emitter.instruction("call __rt_print_r_object_declared");                   // render initialized declared properties with visibility keys
    emitter.instruction("jmp __rt_print_r_object_release_x86");                 // declared-property body complete
    emitter.label("__rt_print_r_object_indexed_x86");
    emitter.instruction("mov rdi, QWORD PTR [r9 + 8]");                         // load the raw indexed-array payload
    abi::load_at_offset(emitter, "rsi", 24);
    emitter.instruction("call __rt_print_r_indexed");                           // render the projected body at the requested indent
    emitter.instruction("jmp __rt_print_r_object_release_x86");                 // projection body complete
    emitter.label("__rt_print_r_object_hash_x86");
    emitter.instruction("mov rdi, QWORD PTR [r9 + 8]");                         // load the raw associative-array payload
    abi::load_at_offset(emitter, "rsi", 24);
    emitter.instruction("call __rt_print_r_debug_hash");                        // render and demangle the projected property keys
    emitter.instruction("jmp __rt_print_r_object_release_x86");                 // projection body complete
    emitter.label("__rt_print_r_object_empty_x86");
    abi::load_at_offset(emitter, "rdi", 24);
    emitter.instruction("call __rt_print_r_open");                              // write the empty object's opening parenthesis
    abi::load_at_offset(emitter, "rdi", 24);
    emitter.instruction("call __rt_print_r_close");                             // write the empty object's closing parenthesis
    emitter.label("__rt_print_r_object_release_x86");
    abi::load_at_offset(emitter, "rax", 32);
    emitter.instruction("call __rt_decref_mixed");                              // release the owned dynamic projection after rendering
    emitter.instruction("call __rt_vd_seen_pop");                               // remove this object from the active print walk
    emitter.instruction("jmp __rt_print_r_object_done_x86");                    // skip the recursion-only marker path
    emitter.label("__rt_print_r_object_recursion_x86");
    abi::emit_symbol_address(emitter, "rsi", "_pr_recursion");
    emitter.instruction("mov edx, 12");                                         // len(" *RECURSION*") = 12; the owning walker adds the newline
    emitter.instruction("call __rt_pr_write");                                  // match PHP's object recursion marker exactly
    emitter.label("__rt_print_r_object_done_x86");
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::{emit_object_debug_info, emit_print_r_object};
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// Verifies AArch64 emits concrete-class dispatch, recursive print rendering, and cleanup.
    #[test]
    fn aarch64_object_debug_projection_runtime_is_complete() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_object_debug_info(&mut emitter);
        emit_print_r_object(&mut emitter);
        let asm = emitter.output();
        for expected in [
            "__rt_object_debug_info:",
            "_class_debug_info_ptrs",
            "blr x9",
            "__rt_print_r_object:",
            "__rt_print_r_object_declared:",
            "_class_pr_desc_ptrs",
            "bl __rt_print_r_hash",
            "bl __rt_print_r_indexed",
            "bl __rt_decref_mixed",
        ] {
            assert!(asm.contains(expected), "missing {expected} in:\n{asm}");
        }
    }

    /// Verifies Linux x86_64 emits the same concrete-class projection contract.
    #[test]
    fn x86_64_object_debug_projection_runtime_is_complete() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_object_debug_info(&mut emitter);
        emit_print_r_object(&mut emitter);
        let asm = emitter.output();
        for expected in [
            "__rt_object_debug_info:",
            "_class_debug_info_ptrs",
            "call r11",
            "__rt_print_r_object:",
            "__rt_print_r_object_declared:",
            "_class_pr_desc_ptrs",
            "call __rt_print_r_hash",
            "call __rt_print_r_indexed",
            "call __rt_decref_mixed",
        ] {
            assert!(asm.contains(expected), "missing {expected} in:\n{asm}");
        }
    }
}
