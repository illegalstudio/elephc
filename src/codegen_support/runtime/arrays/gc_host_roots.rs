//! Purpose:
//! Reuses cycle-collector edge accounting to publish native owners to a PHP host.
//!
//! Called from:
//! - Both collector emitters for the phpext output profile only.
//!
//! Key details:
//! - A wrapper lease roots native storage during collection but is excluded during publication.
//! - Publication marks the graph and reports object roots without running destructors or sweeping.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use super::gc_candidates::scans;

const MODE: &str = "_elephc_phpext_root_scan";

/// Emits the alternate explicit entry and the ordinary collector's mode selection.
pub(super) fn entry(emitter: &mut Emitter) {
    if !emitter.phpext_output { return; }
    emitter.label_global("__rt_gc_phpext_roots");
    abi::emit_store_imm_to_symbol(emitter, MODE, 0, 1);
    abi::emit_jump(emitter, "__rt_gc_phpext_scan_begin");
}

/// Shares the collector body while retaining ordinary collection semantics by default.
pub(super) fn begin(emitter: &mut Emitter) {
    if !emitter.phpext_output { return; }
    abi::emit_store_zero_to_symbol(emitter, MODE, 0);
    emitter.label_shared("__rt_gc_phpext_scan_begin");
}

/// Subtracts only the lease owned by an existing Zend wrapper during root publication.
pub(super) fn subtract_wrapper(emitter: &mut Emitter) {
    if !emitter.phpext_output { return; }
    let done = "L_phpext_gc_wrapper_owner_done";
    super::gc_candidates::load(emitter, "x17", "r11", MODE);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x17, {done}"));                   // ordinary collection treats host wrappers as external owners
            let saved = ["x9", "x10", "x11", "x12", "x13", "x14", "x15", "x30"];
            for reg in saved { abi::emit_push_reg(emitter, reg); }
            emitter.instruction("add x0, x9, #16");                             // pass the live native payload without exposing a Zend API frame
            emitter.bl_c("elephc_php_native_wrapper_owner");
            for reg in saved.into_iter().rev() { abi::emit_pop_reg(emitter, reg); }
            emitter.instruction("sub w12, w12, w0");                            // retain native owners beyond the one host lease
        }
        Arch::X86_64 => {
            emitter.instruction("test r11, r11");                               // inspect the private mark-only mode
            emitter.instruction(&format!("jz {done}"));                         // leave the regular collector root comparison unchanged
            abi::emit_push_reg(emitter, "r8");
            abi::emit_push_reg(emitter, "r10");
            emitter.instruction("lea rdi, [r8 + 16]");                          // identify the candidate independently of the current PHP wrapper
            emitter.bl_c("elephc_php_native_wrapper_owner");
            abi::emit_pop_reg(emitter, "r10");
            abi::emit_pop_reg(emitter, "r8");
            emitter.instruction("sub r10, rax");                                // exclude exactly the host's ordinary native owner
        }
    }
    emitter.label(done);
}

/// Lets Zend's live property graph own descendants of an object with a host mirror.
pub(super) fn stop_at_exported_object(emitter: &mut Emitter) {
    if !emitter.phpext_output { return; }
    let children = "L_phpext_gc_mark_native_children";
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x17", MODE, 0);
            emitter.instruction(&format!("cbz x17, {children}"));               // ordinary collection traverses the complete native graph
            emitter.instruction("cmp x12, #4");                                 // only exported object fields have a mirrored Zend ownership graph
            emitter.instruction(&format!("b.ne {children}"));                   // non-object roots still lead to their first exported object
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", MODE, 0);
            emitter.instruction("test r10, r10");                               // inspect mark-only publication without disturbing the native pointer
            emitter.instruction(&format!("jz {children}"));                     // regular collection retains full transitive marking
            emitter.instruction("cmp rcx, 4");                                  // the low-byte kind remains live after setting the mark
            emitter.instruction(&format!("jne {children}"));                    // arrays and Mixed cells still lead to exported object roots
        }
    }
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, abi::int_result_reg(emitter), 8);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), abi::int_result_reg(emitter));
    emitter.bl_c("elephc_php_native_object_is_mirrored");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("cmp w0, #0"),                     // select the exported-object cut in the native root graph
        Arch::X86_64 => emitter.instruction("test eax, eax"),                   // test the malloc-only class inventory callback result
    }
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), 8);
    abi::emit_frame_restore(emitter, 32);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("b.ne __rt_gc_mark_reachable_done"),// Zend properties keep descendants alive and observe host writes immediately
        Arch::X86_64 => emitter.instruction("jnz __rt_gc_mark_reachable_done"), // avoid redundant strong host roots for mirrored child edges
    }
    emitter.label(children);
}

/// Reports marked native objects, skipping every destructive collector phase.
pub(super) fn publish(emitter: &mut Emitter) {
    if !emitter.phpext_output { return; }
    let normal = "L_phpext_gc_regular_sweep";
    abi::emit_load_symbol_to_reg(emitter, if emitter.target.arch == Arch::AArch64 { "x17" } else { "r11" }, MODE, 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x17, {normal}"));                 // the ordinary collector continues through destructor and sweep phases
            emitter.instruction("ldr x9, [sp, #16]");                           // start reporting from the same bounded heap snapshot
            emitter.instruction("str x9, [sp]");                                // retain the cursor across malloc-only host callbacks
        }
        Arch::X86_64 => {
            emitter.instruction("test r11, r11");                               // distinguish publication from destructive collection
            emitter.instruction(&format!("jz {normal}"));                       // preserve the ordinary sweep path
            emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                 // restart at the collector's captured beginning
            emitter.instruction("mov QWORD PTR [rbp - 24], r8");                // preserve the outer scan cursor
        }
    }
    emitter.label("L_phpext_gc_publish_loop");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp]");                                // reload the current header or candidate slot
            emitter.instruction("ldr x10, [sp, #8]");                           // keep publication inside the captured initial heap window
            emitter.instruction("cmp x9, x10");                                 // test whether every candidate was reported
            emitter.instruction("b.hs __rt_gc_collect_cycles_finish");          // retire collector metadata without reclaiming graph nodes
        }
        Arch::X86_64 => {
            emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                // recover the cursor after the preceding callback
            emitter.instruction("cmp r8, QWORD PTR [rbp - 16]");                // test the initial bounded snapshot end
            emitter.instruction("jae __rt_gc_collect_cycles_finish");           // use ordinary collector-state cleanup without sweeping
        }
    }
    scans::header(emitter, "x9", "r8", "host_publish", None);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr w10, [x9, #4]");                           // ignore retired allocation headers
            emitter.instruction("cbz w10, L_phpext_gc_publish_next");           // a freed payload has no class or host identity
            emitter.instruction("ldr x10, [x9, #8]");                           // inspect the native allocation kind and reachability bit
            emitter.instruction("and x11, x10, #0xff");                         // distinguish objects from arrays and boxed values
            emitter.instruction("cmp x11, #4");                                 // only fixed native instances have exportable class identities
            emitter.instruction("b.ne L_phpext_gc_publish_next");               // other heap nodes still contributed edges during marking
            emitter.instruction("ubfx x1, x10, #16, #1");                       // report reachability from owners other than wrapper pins
            emitter.instruction("add x0, x9, #16");                             // pass the stable native instance pointer privately
        }
        Arch::X86_64 => {
            emitter.instruction("cmp DWORD PTR [r8 + 4], 0");                   // skip dead heap blocks before reading their payload
            emitter.instruction("je L_phpext_gc_publish_next");                 // retired blocks have no host owner to publish
            emitter.instruction("mov r10, QWORD PTR [r8 + 8]");                 // inspect the allocation kind and final mark
            emitter.instruction("mov r11, r10");                                // isolate kind without losing the mark word
            emitter.instruction("and r11, 255");                                // retain only the native kind discriminator
            emitter.instruction("cmp r11, 4");                                  // exported user objects use the fixed object kind
            emitter.instruction("jne L_phpext_gc_publish_next");                // skip scalar and container allocations
            emitter.instruction("mov rsi, r10");                                // move the reachability flag into the callback argument
            emitter.instruction("shr rsi, 16");                                 // position the external-root mark in the low bit
            emitter.instruction("and esi, 1");                                  // pass a canonical boolean
            emitter.instruction("lea rdi, [r8 + 16]");                          // pass the candidate payload through the host C ABI
        }
    }
    emitter.bl_c("elephc_php_native_object_root");
    emitter.label("L_phpext_gc_publish_next");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("ldr x9, [sp]"),                   // restore the original cursor after callback-clobbered registers
        Arch::X86_64 => emitter.instruction("mov r8, QWORD PTR [rbp - 24]"),    // retain the bounded scan position across host bookkeeping
    }
    scans::header(emitter, "x9", "r8", "host_publish_next", None);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr w10, [x9]");                               // read the payload size from the restored header
            emitter.instruction("add x9, x9, x10");                             // advance across the payload
            emitter.instruction("add x9, x9, #16");                             // include its uniform header
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10d, DWORD PTR [r8]");                    // recover the payload size after callback bookkeeping
            emitter.instruction("lea r8, [r8 + r10 + 16]");                     // advance to the next physical allocation header
        }
    }
    scans::next(emitter, "x9", "r8", "[sp]", "[rbp - 24]", "host_publish");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("str x9, [sp]"),                   // preserve the next report cursor
        Arch::X86_64 => emitter.instruction("mov QWORD PTR [rbp - 24], r8"),    // preserve the next report cursor
    }
    abi::emit_jump(emitter, "L_phpext_gc_publish_loop");
    emitter.label(normal);
}

/// Clears private publication state after a complete or skipped collector entry.
pub(super) fn finish(emitter: &mut Emitter) {
    if emitter.phpext_output { abi::emit_store_zero_to_symbol(emitter, MODE, 0); }
}
