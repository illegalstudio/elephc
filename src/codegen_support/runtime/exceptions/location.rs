//! Purpose:
//! Resolves effective Throwable locations for native/eval getters and fatal reports.
//!
//! Called from:
//! - Native Throwable accessors, the eval method bridge, and uncaught reporting.
//!
//! Key details:
//! - Borrowed filenames remain owned by the receiver; only the public string getter persists them.
//! - ErrorException overrides use the filename cell at offset 72 and nullable line words 88/96.
//! - Explicit zero lines and empty filenames are known locations; synthesized origins remain unknown.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits a borrowed location lookup and the PHP-visible ownership adapters.
pub(crate) fn emit_location(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_throwable_location_borrowed");
    match emitter.target.arch {
        Arch::AArch64 => emit_borrowed_aarch64(emitter),
        Arch::X86_64 => emit_borrowed_x86_64(emitter),
    }
    for file in [true, false] {
        emitter.blank();
        emitter.label_global(if file { "__rt_throwable_get_file" } else { "__rt_throwable_get_line" });
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("stp x29, x30, [sp, #-16]!");               // preserve linkage across the borrowed lookup and optional string copy
                abi::emit_call_label(emitter, "__rt_throwable_location_borrowed");
                if file { abi::emit_call_label(emitter, "__rt_str_persist"); }
                emitter.instruction("ldp x29, x30, [sp], #16");                 // restore the getter's caller after result materialization
            }
            Arch::X86_64 => {
                emitter.instruction("push rbp");                                // preserve caller linkage and align nested calls
                emitter.instruction("mov rbp, rsp");                            // establish the ordinary getter call frame
                abi::emit_call_label(emitter, "__rt_throwable_location_borrowed");
                if file {
                    abi::emit_call_label(emitter, "__rt_str_persist");
                } else {
                    emitter.instruction("mov rax, rcx");                        // expose the resolved integer line in its PHP result register
                }
                emitter.instruction("pop rbp");                                 // restore caller linkage after result materialization
            }
        }
        emitter.instruction("ret");                                             // return the public getter's declared PHP representation
    }
}

/// Returns line in x0, borrowed filename in x1/x2, and location-known flag in x3.
/// This leaf preserves the receiver until both nullable overrides have been inspected.
fn emit_borrowed_aarch64(emitter: &mut Emitter) {
    emitter.instruction("mov x9, x0");                                          // retain the receiver while the creation line replaces the integer result
    emit_creation_line_load(emitter, "x9", "x0", "x10", "__rt_location_default");
    abi::emit_symbol_address(emitter, "x1", "_script_source_file");
    abi::emit_symbol_address(emitter, "x10", "_script_source_file_len");
    emitter.instruction("ldr x2, [x10]");                                       // borrow the default filename length without clobbering the receiver
    emitter.instruction("cmp x0, #0");                                          // synthesized creation lines have no trustworthy source origin
    emitter.instruction("cset x3, ne");                                         // record whether a native allocation supplied a line
    emitter.instruction("cmp x2, #0");                                          // an absent default filename cannot identify a source location
    emitter.instruction("csel x3, xzr, x3, eq");                                // require a default filename unless an override establishes a location
    abi::emit_symbol_address(emitter, "x11", "_error_exception_class_id");
    emitter.instruction("ldr x11, [x11]");                                      // obtain the ErrorException root id or its absent sentinel
    abi::emit_symbol_address(emitter, "x12", "_class_gc_desc_count");
    emitter.instruction("ldr x12, [x12]");                                      // bound the ancestry walk before indexing parent metadata
    abi::emit_symbol_address(emitter, "x13", "_class_parent_ids");
    emitter.instruction("ldr x10, [x9]");                                       // begin ancestry lookup at the receiver's runtime class
    emitter.label("__rt_location_ancestry");
    emitter.instruction("cmp x10, x12");                                        // negative parents and absent ids are outside the emitted table
    emitter.instruction("b.hs __rt_location_return");                           // ordinary roots retain the default location
    emitter.instruction("cmp x10, x11");                                        // only ErrorException layouts carry the extended nullable fields
    emitter.instruction("b.eq __rt_location_overrides");                        // apply overrides for the root and every descendant
    emitter.instruction("ldr x10, [x13, x10, lsl #3]");                         // advance to the direct parent without touching location results
    emitter.instruction("cmn x10, #2");                                         // preserve the runtime's absent-parent metadata invariant
    emitter.instruction("b.eq __rt_location_absent_metadata");                  // a broken chain must not silently discard location overrides
    emitter.instruction("b __rt_location_ancestry");                            // keep walking until a matching or terminal parent
    emitter.label("__rt_location_overrides");
    emitter.instruction("ldr x10, [x9, #72]");                                  // borrow the optional boxed filename
    emitter.instruction("cbz x10, __rt_location_line_override");                // an absent cell preserves the default filename
    emitter.instruction("ldr x11, [x10]");                                      // inspect the nullable filename tag
    emitter.instruction("cmp x11, #8");                                         // null requests the original source filename
    emitter.instruction("b.eq __rt_location_line_override");                    // preserve the default filename for null
    emitter.instruction("ldr x1, [x10, #8]");                                   // borrow explicit filename bytes from the live receiver
    emitter.instruction("ldr x2, [x10, #16]");                                  // preserve an explicitly empty filename length
    emitter.instruction("mov x3, #1");                                          // an explicit filename establishes a location even when empty
    emitter.label("__rt_location_line_override");
    emitter.instruction("ldr x10, [x9, #96]");                                  // inspect the nullable scalar line tag
    emitter.instruction("cmp x10, #8");                                         // null requests the original construction line
    emitter.instruction("b.eq __rt_location_return");                           // preserve the default line for null
    emitter.instruction("ldr x0, [x9, #88]");                                   // return the explicit line, including zero or a negative integer
    emitter.instruction("mov x3, #1");                                          // distinguish explicit zero from an unknown synthesized origin
    emitter.label("__rt_location_return");
    emitter.instruction("ret");                                                 // return borrowed location words without allocating or retaining
    emitter.label("__rt_location_absent_metadata");
    super::matches::emit_absent_metadata_fatal(emitter);
}

/// Returns borrowed filename in rax/rdx, line in rcx, and location-known flag in r8.
/// All scratch registers are caller-saved under the supported x86_64 ABI.
fn emit_borrowed_x86_64(emitter: &mut Emitter) {
    emitter.instruction("mov r10, rax");                                        // retain the receiver while materializing location results
    emit_creation_line_load(emitter, "r10", "rcx", "r11", "__rt_location_default");
    abi::emit_symbol_address(emitter, "rax", "_script_source_file");
    abi::emit_symbol_address(emitter, "r11", "_script_source_file_len");
    emitter.instruction("mov rdx, QWORD PTR [r11]");                            // borrow the default filename length
    emitter.instruction("xor r8d, r8d");                                        // initialize the location-known flag before its byte assignment
    emitter.instruction("test rcx, rcx");                                       // synthesized creation lines have no trustworthy source origin
    emitter.instruction("setne r8b");                                           // record whether native allocation supplied a line
    emitter.instruction("xor r11d, r11d");                                      // prepare the unknown-location value
    emitter.instruction("test rdx, rdx");                                       // an absent default filename cannot identify a source location
    emitter.instruction("cmovz r8, r11");                                       // require a default filename unless an override establishes a location
    abi::emit_symbol_address(emitter, "r11", "_error_exception_class_id");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // obtain the ErrorException root id or its absent sentinel
    abi::emit_symbol_address(emitter, "rdi", "_class_gc_desc_count");
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // bound the ancestry walk before indexing parent metadata
    abi::emit_symbol_address(emitter, "rsi", "_class_parent_ids");
    emitter.instruction("mov r9, QWORD PTR [r10]");                             // begin ancestry lookup at the receiver's runtime class
    emitter.label("__rt_location_ancestry");
    emitter.instruction("cmp r9, rdi");                                         // negative parents and absent ids are outside the emitted table
    emitter.instruction("jae __rt_location_return");                            // ordinary roots retain the default location
    emitter.instruction("cmp r9, r11");                                         // only ErrorException layouts carry the extended nullable fields
    emitter.instruction("je __rt_location_overrides");                          // apply overrides for the root and every descendant
    emitter.instruction("mov r9, QWORD PTR [rsi + r9 * 8]");                    // advance to the direct parent without touching location results
    emitter.instruction("cmp r9, -2");                                          // preserve the runtime's absent-parent metadata invariant
    emitter.instruction("je __rt_location_absent_metadata");                    // a broken chain must not silently discard location overrides
    emitter.instruction("jmp __rt_location_ancestry");                          // keep walking until a matching or terminal parent
    emitter.label("__rt_location_overrides");
    emitter.instruction("mov r9, QWORD PTR [r10 + 72]");                        // borrow the optional boxed filename
    emitter.instruction("test r9, r9");                                         // absent cells preserve the default filename
    emitter.instruction("jz __rt_location_line_override");                      // continue with the independent line override
    emitter.instruction("cmp QWORD PTR [r9], 8");                               // null requests the original source filename
    emitter.instruction("je __rt_location_line_override");                      // preserve the default filename for null
    emitter.instruction("mov rax, QWORD PTR [r9 + 8]");                         // borrow explicit filename bytes from the live receiver
    emitter.instruction("mov rdx, QWORD PTR [r9 + 16]");                        // preserve an explicitly empty filename length
    emitter.instruction("mov r8, 1");                                           // an explicit filename establishes a location even when empty
    emitter.label("__rt_location_line_override");
    emitter.instruction("cmp QWORD PTR [r10 + 96], 8");                         // null requests the original construction line
    emitter.instruction("je __rt_location_return");                             // preserve the default line for null
    emitter.instruction("mov rcx, QWORD PTR [r10 + 88]");                       // return the explicit line, including zero or a negative integer
    emitter.instruction("mov r8, 1");                                           // distinguish explicit zero from an unknown synthesized origin
    emitter.label("__rt_location_return");
    emitter.instruction("ret");                                                 // return borrowed location words without allocating or retaining
    emitter.label("__rt_location_absent_metadata");
    super::matches::emit_absent_metadata_fatal(emitter);
}

/// Loads compact or ordinary creation metadata without calls or ownership changes.
/// The receiver, result, and scratch registers must be distinct; `stem` scopes branch labels.
pub(crate) fn emit_creation_line_load(
    emitter: &mut Emitter,
    object: &str,
    result: &str,
    scratch: &str,
    stem: &str,
) {
    debug_assert!(object != result && object != scratch && result != scratch);
    let compact = format!("{stem}_compact_line");
    let done = format!("{stem}_line_loaded");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldurb {scratch_w}, [{object}, #-8]", scratch_w = scratch.replacen('x', "w", 1))); // inspect the physical layout without GC metadata bits
            emitter.instruction(&format!("cmp {scratch}, #6"));                 // compact runtime Throwables retain their fixed metadata word
            emitter.instruction(&format!("b.eq {compact}"));                    // select the compact payload layout
            emitter.instruction(&format!("ldr {scratch}, [{object}]"));         // resolve the ordinary class id
            abi::emit_symbol_address(emitter, result, "_class_throwable_line_offsets");
            emitter.instruction(&format!("ldr {result}, [{result}, {scratch}, lsl #3]")); // locate metadata beyond PHP property and hash storage
            emitter.instruction(&format!("cbz {result}, {done}"));              // absent metadata reports an unknown origin
            emitter.instruction(&format!("ldr {result}, [{object}, {result}]")); // read the ordinary object's dedicated creation line
            emitter.instruction(&format!("b {done}"));                          // skip the compact payload read
            emitter.label(&compact);
            emitter.instruction(&format!("ldr {result}, [{object}, #32]"));     // read the compact Throwable creation line
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("movzx {scratch}, BYTE PTR [{object} - 8]")); // inspect the heap kind without GC or magic bits
            emitter.instruction(&format!("cmp {scratch}, 6"));                  // compact runtime Throwables retain their fixed metadata word
            emitter.instruction(&format!("je {compact}"));                      // select the compact payload layout
            emitter.instruction(&format!("mov {scratch}, QWORD PTR [{object}]")); // resolve the ordinary class id
            abi::emit_symbol_address(emitter, result, "_class_throwable_line_offsets");
            emitter.instruction(&format!("mov {result}, QWORD PTR [{result} + {scratch} * 8]")); // locate metadata beyond PHP property and hash storage
            emitter.instruction(&format!("test {result}, {result}"));           // test whether this layout reserves construction metadata
            emitter.instruction(&format!("jz {done}"));                         // absent metadata reports an unknown origin
            emitter.instruction(&format!("mov {result}, QWORD PTR [{object} + {result}]")); // read the ordinary object's dedicated creation line
            emitter.instruction(&format!("jmp {done}"));                        // skip the compact payload read
            emitter.label(&compact);
            emitter.instruction(&format!("mov {result}, QWORD PTR [{object} + 32]")); // read the compact Throwable creation line
        }
    }
    emitter.label(&done);
}
