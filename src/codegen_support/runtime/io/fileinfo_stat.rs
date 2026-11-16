//! Purpose:
//! Exposes user-wrapper class metadata and stream-stat diagnostics through narrow C ABI queries.
//!
//! Called from:
//! - The fileinfo runtime emitter and its prelude after filter creation, before fstat.
//!
//! Key details:
//! - Resolves PHP resource IDs through the existing bounded resource-ID registry.
//! - Reads immutable class-name spans without assuming a trailing NUL byte.
//! - Invokes no PHP callbacks and changes no stream or resource-ID state.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::RESOURCE_ID_TABLE_SLOTS;

/// Emits the missing-method metadata probe and explicit pointer/length class-name accessors.
pub(super) fn emit_fileinfo_stat(emitter: &mut Emitter) {
    emit_close_mode(emitter);
    emit_exception_warning(emitter);
    for (name, missing_stat) in [("elephc_runtime_stream_missing_stat_class", true), ("elephc_runtime_stream_class", false)] {
        let name = emitter.target.extern_symbol(name);
        emitter.label_global(&name);
        match emitter.target.arch {
            Arch::AArch64 => emit_aarch64_probe(emitter, missing_stat),
            Arch::X86_64 => emit_x86_probe(emitter, missing_stat),
        }
    }
    for (name, offset) in [("elephc_runtime_stream_stat_name", 0), ("elephc_runtime_stream_stat_name_length", 8)] {
        // -- expose immutable class-name metadata through leaf C ABI accessors --
        let symbol = emitter.target.extern_symbol(name);
        emitter.label_global(&symbol);
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("ldr x0, [x0, #{offset}]")), // borrow the requested immutable class-name metadata word
            Arch::X86_64 => emitter.instruction(&format!("mov rax, QWORD PTR [rdi + {offset}]")), // borrow the requested immutable class-name metadata word
        }
        emitter.instruction("ret");                                             // return the metadata word through the target C ABI
    }
}

/// Reports an exception-time warning only when PHP has no registered error handler.
fn emit_exception_warning(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let (arg, ret, frame, scratch) = if arm { ("x0", "x0", "sp", "x14") } else { ("rdi", "rax", "rsp", "r11") };
    let name = emitter.target.extern_symbol("elephc_runtime_stream_exception_warning");
    emitter.label_global(&name);
    // -- select default reporting only when no matching PHP handler suppresses it --
    abi::emit_load_symbol_to_reg(emitter, scratch, "_php_error_handler_callable", 0);
    emitter.instruction(if arm { "cmp x14, #0" } else { "test r11, r11" });     // a registered handler cannot run with a pending exception
    emitter.instruction(if arm { "b.eq __rt_finfo_exception_warning_report" } else { "jz __rt_finfo_exception_warning_report" }); // no handler means ordinary default reporting
    abi::emit_load_symbol_to_reg(emitter, scratch, "_php_error_handler_mask", 0);
    emitter.instruction(if arm { "tst x14, #2" } else { "test r11, 2" });       // only a matching E_WARNING handler suppresses the exception-time diagnostic
    emitter.instruction(if arm { "b.ne __rt_finfo_exception_warning_done" } else { "jnz __rt_finfo_exception_warning_done" }); // PHP cannot enter the matching handler while an exception is pending
    emitter.label("__rt_finfo_exception_warning_report");
    // -- emit the default warning while preserving the original pending exception --
    if arm {
        emitter.instruction("sub sp, sp, #48");                                 // reserve the warning inputs and caller linkage
        emitter.instruction("stp x29, x30, [sp, #32]");                         // preserve the caller frame and return address
    } else {
        emitter.instruction("push rbp");                                        // preserve the SysV caller frame
        emitter.instruction("mov rbp, rsp");                                    // establish the warning frame pointer
        emitter.instruction("sub rsp, 32");                                     // keep libc and warning calls aligned
    }
    abi::emit_store_to_address(emitter, arg, frame, 0);
    abi::emit_load_from_address(emitter, arg, frame, 0);
    emitter.bl_c("strlen");
    abi::emit_store_to_address(emitter, ret, frame, 16);
    abi::emit_load_from_address(emitter, if arm { "x1" } else { "rdi" }, frame, 0);
    abi::emit_load_from_address(emitter, if arm { "x2" } else { "rsi" }, frame, 16);
    emitter.instruction(if arm { "bl __rt_diag_warning" } else { "call __rt_diag_warning" }); // preserve default reporting without invoking PHP during a pending exception
    if arm {
        emitter.instruction("ldp x29, x30, [sp, #32]");                         // restore the caller linkage after native reporting
        emitter.instruction("add sp, sp, #48");                                 // release the complete warning frame
    } else {
        emitter.instruction("add rsp, 32");                                     // release the aligned warning inputs
        emitter.instruction("pop rbp");                                         // restore the SysV caller frame
    }
    emitter.label("__rt_finfo_exception_warning_done");
    emitter.instruction("ret");                                                 // leave the original PHP exception available to its caller
}

/// Queries or replaces the native-extension close mode, returning its prior state.
fn emit_close_mode(emitter: &mut Emitter) {
    let name = emitter.target.extern_symbol("elephc_runtime_stream_close_mode");
    emitter.label_global(&name);
    // -- query or replace the close mode without losing its previous value --
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x14", "_stream_close_silent", 0);
            emitter.instruction("tbnz x0, #63, __rt_finfo_close_mode_return");  // a negative mode only queries the current state
            abi::emit_store_reg_to_symbol(emitter, "x0", "_stream_close_silent", 0);
            emitter.label("__rt_finfo_close_mode_return");
            emitter.instruction("mov x0, x14");                                 // return the previous mode without the store helper's x9 scratch
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "rax", "_stream_close_silent", 0);
            emitter.instruction("test rdi, rdi");                               // a negative mode only queries the current state
            emitter.instruction("js __rt_finfo_close_mode_return");             // leave the mode unchanged for a query
            abi::emit_store_reg_to_symbol(emitter, "rdi", "_stream_close_silent", 0);
            emitter.label("__rt_finfo_close_mode_return");
        }
    }
    emitter.instruction("ret");                                                 // return the previous mode through the target C ABI
}

/// Emits an AArch64 leaf probe using only C ABI caller-saved registers.
fn emit_aarch64_probe(emitter: &mut Emitter, missing_stat: bool) {
    let suffix = if missing_stat { "stat" } else { "class" };
    // -- resolve the displayed PHP resource ID to its native payload --
    abi::emit_symbol_address(emitter, "x9", "_resource_id_vals");
    abi::emit_symbol_address(emitter, "x10", "_resource_id_keys");
    emitter.instruction("mov x11, #0");                                         // start at the first resource-ID slot
    emitter.label(&format!("__rt_finfo_{suffix}_scan"));
    emitter.instruction(&format!("cmp x11, #{RESOURCE_ID_TABLE_SLOTS}"));       // bound the resource-ID scan
    emitter.instruction(&format!("b.hs __rt_finfo_{suffix}_empty"));            // absent resource IDs have no wrapper metadata
    emitter.instruction("ldr x12, [x9, x11, lsl #3]");                          // read the displayed resource ID in this slot
    emitter.instruction("cmp x12, x0");                                         // match the caller's resource ID
    emitter.instruction(&format!("b.eq __rt_finfo_{suffix}_found"));            // resolve its native payload after a match
    emitter.instruction("add x11, x11, #1");                                    // inspect the next resource-ID slot
    emitter.instruction(&format!("b __rt_finfo_{suffix}_scan"));                // continue the bounded scan
    emitter.label(&format!("__rt_finfo_{suffix}_found"));
    // -- map the native wrapper descriptor to a live object and validated class --
    emitter.instruction("ldr x12, [x10, x11, lsl #3]");                         // load the native resource payload
    emitter.instruction("mov x13, #0x4000");                                    // materialize the high word of the synthetic wrapper base
    emitter.instruction("lsl x13, x13, #16");                                   // form the wrapper descriptor base 0x40000000
    emitter.instruction("sub x12, x12, x13");                                   // convert a wrapper descriptor into its handle-table index
    emitter.instruction("cmp x12, #256");                                       // reject non-wrapper payloads and out-of-range slots
    emitter.instruction(&format!("b.hs __rt_finfo_{suffix}_empty"));            // unsigned comparison also rejects descriptors below the base
    abi::emit_symbol_address(emitter, "x13", "_user_wrapper_handles");
    emitter.instruction("ldr x12, [x13, x12, lsl #3]");                         // borrow the live wrapper object
    emitter.instruction(&format!("cbz x12, __rt_finfo_{suffix}_empty"));        // closed wrapper handles expose no metadata
    emitter.instruction("ldr x12, [x12]");                                      // read the object's dense class ID
    abi::emit_load_symbol_to_reg(emitter, "x13", "_class_name_count", 0);
    emitter.instruction("cmp x12, x13");                                        // check the class metadata bounds before any table access
    emitter.instruction(&format!("b.hs __rt_finfo_{suffix}_empty"));            // reject unknown runtime class IDs
    abi::emit_symbol_address(emitter, "x13", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x13, [x13, x12, lsl #3]");                         // load the wrapper class's method table
    emitter.instruction(&format!("cbz x13, __rt_finfo_{suffix}_empty"));        // tolerate classes without a wrapper vtable
    if missing_stat {
        emitter.instruction("ldr x14, [x13, #64]");                             // vtable slot eight is stream_stat
        emitter.instruction(&format!("cbnz x14, __rt_finfo_{suffix}_empty"));   // implemented methods need no missing-method warning
    }
    abi::emit_symbol_address(emitter, "x13", "_class_name_entries");
    emitter.instruction("lsl x12, x12, #4");                                    // scale the class ID by the 16-byte pointer/length row
    emitter.instruction("add x0, x13, x12");                                    // return the immutable class-name metadata row
    emitter.instruction("ret");                                                 // leave the class-name span available until program exit
    emitter.label(&format!("__rt_finfo_{suffix}_empty"));
    // -- return no class metadata when the resource has no matching wrapper --
    emitter.instruction("mov x0, #0");                                          // no missing stream_stat method was found
    emitter.instruction("ret");                                                 // return a null metadata pointer
}

/// Emits the x86_64 leaf probe using only SysV caller-saved registers.
fn emit_x86_probe(emitter: &mut Emitter, missing_stat: bool) {
    let suffix = if missing_stat { "stat" } else { "class" };
    // -- resolve the displayed PHP resource ID without minting or changing an ID --
    abi::emit_symbol_address(emitter, "r8", "_resource_id_vals");
    abi::emit_symbol_address(emitter, "r9", "_resource_id_keys");
    emitter.instruction("xor ecx, ecx");                                        // start at the first resource-ID slot
    emitter.label(&format!("__rt_finfo_{suffix}_scan"));
    emitter.instruction(&format!("cmp rcx, {RESOURCE_ID_TABLE_SLOTS}"));        // bound the resource-ID scan
    emitter.instruction(&format!("jae __rt_finfo_{suffix}_empty"));             // absent resource IDs have no wrapper metadata
    emitter.instruction("cmp QWORD PTR [r8 + rcx * 8], rdi");                   // match the caller's displayed resource ID
    emitter.instruction(&format!("je __rt_finfo_{suffix}_found"));              // resolve the native payload after a match
    emitter.instruction("inc rcx");                                             // inspect the next resource-ID slot
    emitter.instruction(&format!("jmp __rt_finfo_{suffix}_scan"));              // continue the bounded scan
    emitter.label(&format!("__rt_finfo_{suffix}_found"));
    emitter.instruction("mov rax, QWORD PTR [r9 + rcx * 8]");                   // load the native resource payload
    emitter.instruction("sub rax, 0x40000000");                                 // convert a wrapper descriptor into its handle-table index
    emitter.instruction("cmp rax, 256");                                        // reject non-wrapper payloads and out-of-range slots
    emitter.instruction(&format!("jae __rt_finfo_{suffix}_empty"));             // unsigned comparison also rejects descriptors below the base
    abi::emit_symbol_address(emitter, "r8", "_user_wrapper_handles");
    emitter.instruction("mov rax, QWORD PTR [r8 + rax * 8]");                   // borrow the live wrapper object
    emitter.instruction("test rax, rax");                                       // detect a cleared wrapper handle
    emitter.instruction(&format!("jz __rt_finfo_{suffix}_empty"));              // closed handles expose no metadata
    emitter.instruction("mov rax, QWORD PTR [rax]");                            // read the object's dense class ID
    abi::emit_load_symbol_to_reg(emitter, "r9", "_class_name_count", 0);
    emitter.instruction("cmp rax, r9");                                         // check class metadata bounds before any table access
    emitter.instruction(&format!("jae __rt_finfo_{suffix}_empty"));             // reject unknown runtime class IDs
    abi::emit_symbol_address(emitter, "r8", "_user_wrapper_vtable_ptrs");
    emitter.instruction("mov r8, QWORD PTR [r8 + rax * 8]");                    // load the wrapper class's method table
    emitter.instruction("test r8, r8");                                         // detect a class without a wrapper vtable
    emitter.instruction(&format!("jz __rt_finfo_{suffix}_empty"));              // tolerate classes without a wrapper method table
    if missing_stat {
        emitter.instruction("cmp QWORD PTR [r8 + 64], 0");                      // vtable slot eight is stream_stat
        emitter.instruction(&format!("jne __rt_finfo_{suffix}_empty"));         // implemented methods need no missing-method warning
    }
    abi::emit_symbol_address(emitter, "r8", "_class_name_entries");
    emitter.instruction("shl rax, 4");                                          // scale the class ID by its 16-byte pointer/length row
    emitter.instruction("add rax, r8");                                         // return the immutable class-name metadata row
    emitter.instruction("ret");                                                 // leave the class-name span available until program exit
    emitter.label(&format!("__rt_finfo_{suffix}_empty"));
    emitter.instruction("xor eax, eax");                                        // no missing stream_stat method was found
    emitter.instruction("ret");                                                 // return a null metadata pointer
}
