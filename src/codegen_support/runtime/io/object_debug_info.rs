//! Purpose:
//! Invokes a concrete object's runtime `__debugInfo()` adapter by class id.
//! Exposes one target-aware helper returning an owned boxed `Mixed` result or null.
//!
//! Called from:
//! - Recursive `var_dump()` object rendering.
//!
//! Key details:
//! - Adapter entries own ABI normalization; this helper only performs bounds-safe dispatch.
//! - The object receiver remains borrowed and the caller owns any non-null Mixed result.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

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

#[cfg(test)]
mod tests {
    use super::emit_object_debug_info;
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// Verifies AArch64 emits the dynamic debug-info and property-hash helpers.
    #[test]
    fn aarch64_object_debug_info_runtime_is_complete() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_object_debug_info(&mut emitter);
        let asm = emitter.output();
        for expected in [
            "__rt_object_debug_info:",
            "_class_debug_info_ptrs",
            "blr x9",
            "__rt_object_dynamic_hash:",
            "_class_object_dynamic_prop_flags",
        ] {
            assert!(asm.contains(expected), "missing {expected} in:\n{asm}");
        }
    }

    /// Verifies Linux x86_64 emits the same dynamic object helper contract.
    #[test]
    fn x86_64_object_debug_info_runtime_is_complete() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_object_debug_info(&mut emitter);
        let asm = emitter.output();
        for expected in [
            "__rt_object_debug_info:",
            "_class_debug_info_ptrs",
            "call r11",
            "__rt_object_dynamic_hash:",
            "_class_object_dynamic_prop_flags",
        ] {
            assert!(asm.contains(expected), "missing {expected} in:\n{asm}");
        }
    }
}
