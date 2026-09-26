//! Purpose:
//! Emits the helpers that let the object dump walkers (`var_dump`, `print_r`,
//! `var_export`) reach an instance's dynamic-property hash: `__rt_obj_dump_dyn_props`
//! returns the hash pointer, `__rt_obj_dyn_prop_at` returns its Nth entry.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::objects`.
//! - `__rt_var_dump_object`, `__rt_vd_obj_count` (inline form),
//!   `__rt_print_r_object`, and the `__rt_obj_prop_*` var_export accessors.
//!
//! Key details:
//! - The hash is the object's LAST payload word (`payload_size - 8`), the same tail
//!   `get_object_vars()` / `(array)` copy through `__rt_object_to_hash`. It holds
//!   boxed values keyed by the raw property name, in insertion order, and may be
//!   null until the first dynamic write.
//! - Gated by `_class_dump_dyn_prop_flags`, which is the dynamic-tail flag minus the
//!   classes whose `__debugInfo()` folded into a projection (PHP then dumps only the
//!   projected array).
//! - The class id is bounds-checked against `_class_gc_desc_count` with an unsigned
//!   compare, so an incomplete object (class id `-2`) and a stale id report no hash.
//! - Borrowed results: nothing here retains the hash or the entry it yields.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the inline "load this object's dumpable dynamic-property hash" sequence.
///
/// `class_reg` must already hold a class id that passed the `_class_gc_desc_count`
/// bounds check. On exit `out_reg` holds the hash pointer; control jumps to
/// `miss_label` when the class has no dumpable tail or the hash was never allocated.
/// Clobbers `scratch_reg` only (plus `out_reg`), so leaf helpers can inline it.
pub(crate) fn emit_load_dump_dyn_hash(
    emitter: &mut Emitter,
    object_reg: &str,
    class_reg: &str,
    out_reg: &str,
    scratch_reg: &str,
    miss_label: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, scratch_reg, "_class_dump_dyn_prop_flags");
            emitter.instruction(&format!("ldr {scratch_reg}, [{scratch_reg}, {class_reg}, lsl #3]")); // load whether this class dumps a dynamic-property tail
            emitter.instruction(&format!("cbz {scratch_reg}, {miss_label}"));   // no tail (or a __debugInfo projection) dumps nothing more
            abi::emit_symbol_address(emitter, scratch_reg, "_class_object_payload_sizes");
            emitter.instruction(&format!("ldr {scratch_reg}, [{scratch_reg}, {class_reg}, lsl #3]")); // load the class payload size
            emitter.instruction(&format!("sub {scratch_reg}, {scratch_reg}, #8"));   // the dynamic hash is the last payload word
            emitter.instruction(&format!("ldr {out_reg}, [{object_reg}, {scratch_reg}]")); // load the dynamic-property hash pointer
            emitter.instruction(&format!("cbz {out_reg}, {miss_label}"));       // a never-written tail holds no hash yet
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, scratch_reg, "_class_dump_dyn_prop_flags");
            emitter.instruction(&format!("cmp QWORD PTR [{scratch_reg} + {class_reg} * 8], 0")); // does this class dump a dynamic-property tail?
            emitter.instruction(&format!("je {miss_label}"));                   // no tail (or a __debugInfo projection) dumps nothing more
            abi::emit_symbol_address(emitter, scratch_reg, "_class_object_payload_sizes");
            emitter.instruction(&format!("mov {scratch_reg}, QWORD PTR [{scratch_reg} + {class_reg} * 8]")); // load the class payload size
            emitter.instruction(&format!("mov {out_reg}, QWORD PTR [{object_reg} + {scratch_reg} - 8]")); // load the hash from the last payload word
            emitter.instruction(&format!("test {out_reg}, {out_reg}"));         // was the tail ever allocated?
            emitter.instruction(&format!("jz {miss_label}"));                   // a never-written tail holds no hash yet
        }
    }
}

/// `__rt_obj_dump_dyn_props`: an object's dumpable dynamic-property hash, or 0.
///
/// Leaf helper. Input: AArch64 x0 / x86_64 rdi = object pointer (0 allowed).
/// Output: AArch64 x0 / x86_64 rax = borrowed hash pointer, or 0 when the object
/// has no dynamic properties to dump. Clobbers x9-x11 / r9-r11 only.
pub(crate) fn emit_obj_dump_dyn_props(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: obj_dump_dyn_props ---");
    emitter.label_global("__rt_obj_dump_dyn_props");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cbz x0, __rt_obj_dump_dyn_props_none");        // a null instance has no dynamic properties
            emitter.instruction("ldr x9, [x0]");                                // load the runtime class id from the object header
            abi::emit_symbol_address(emitter, "x10", "_class_gc_desc_count");   // resolve the class-id table extent
            emitter.instruction("ldr x10, [x10]");                              // load the number of registered class ids
            emitter.instruction("cmp x9, x10");                                 // is the class id within the metadata tables?
            emitter.instruction("b.hs __rt_obj_dump_dyn_props_none");           // incomplete or unknown ids carry no dumpable tail
            emit_load_dump_dyn_hash(emitter, "x0", "x9", "x0", "x11", "__rt_obj_dump_dyn_props_none");
            emitter.instruction("ret");                                         // return the borrowed hash pointer
            emitter.label("__rt_obj_dump_dyn_props_none");
            emitter.instruction("mov x0, #0");                                  // report "no dynamic properties"
            emitter.instruction("ret");                                         // return to caller
        }
        Arch::X86_64 => {
            emitter.instruction("test rdi, rdi");                               // a null instance has no dynamic properties
            emitter.instruction("jz __rt_obj_dump_dyn_props_none_x86");         // report "no dynamic properties"
            emitter.instruction("mov r9, QWORD PTR [rdi]");                     // load the runtime class id from the object header
            abi::emit_symbol_address(emitter, "r10", "_class_gc_desc_count");   // resolve the class-id table extent
            emitter.instruction("cmp r9, QWORD PTR [r10]");                     // is the class id within the metadata tables?
            emitter.instruction("jae __rt_obj_dump_dyn_props_none_x86");        // incomplete or unknown ids carry no dumpable tail
            emit_load_dump_dyn_hash(emitter, "rdi", "r9", "rax", "r11", "__rt_obj_dump_dyn_props_none_x86");
            emitter.instruction("ret");                                         // return the borrowed hash pointer
            emitter.label("__rt_obj_dump_dyn_props_none_x86");
            emitter.instruction("xor eax, eax");                                // report "no dynamic properties"
            emitter.instruction("ret");                                         // return to caller
        }
    }
}

/// `__rt_obj_dyn_prop_at`: the Nth (0-based, insertion order) dumpable dynamic property.
///
/// Input: AArch64 x0=object x1=ordinal / x86_64 rdi=object rsi=ordinal.
/// Output mirrors `__rt_hash_iter_next_value` with a found flag in place of the cursor:
/// AArch64 x0=found x1=key ptr x2=key len (-1 = integer key) x3=value lo x4=value hi
/// x5=value tag; x86_64 rax=found rdi=key ptr rdx=key len rcx=lo r8=hi r9=tag.
/// Every returned word is borrowed from the hash. `found` is 0 for an out-of-range
/// ordinal or an object without a dumpable tail.
pub(crate) fn emit_obj_dyn_prop_at(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: obj_dyn_prop_at ---");
    emitter.label_global("__rt_obj_dyn_prop_at");

    match emitter.target.arch {
        Arch::AArch64 => {
            // Frame (48 bytes): [0] hash, [8] cursor, [16] remaining steps, [32] x29, [40] x30.
            emitter.instruction("sub sp, sp, #48");                             // allocate the ordinal-walk frame
            emitter.instruction("stp x29, x30, [sp, #32]");                     // save frame pointer and return address
            emitter.instruction("add x29, sp, #32");                            // establish the ordinal-walk frame pointer
            emitter.instruction("str x1, [sp, #16]");                           // remember the requested ordinal
            emitter.instruction("bl __rt_obj_dump_dyn_props");                  // x0 = dynamic-property hash, or 0
            emitter.instruction("cbz x0, __rt_obj_dyn_prop_at_none");           // no tail means no Nth dynamic property
            emitter.instruction("str x0, [sp, #0]");                            // keep the hash pointer across iteration calls
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the requested ordinal
            emitter.instruction("cmp x9, #0");                                  // reject a negative ordinal
            emitter.instruction("b.lt __rt_obj_dyn_prop_at_none");              // negative ordinals name no entry
            emitter.instruction("ldr x10, [x0]");                               // load the live entry count from the hash header
            emitter.instruction("cmp x9, x10");                                 // is the ordinal within the entry count?
            emitter.instruction("b.hs __rt_obj_dyn_prop_at_none");              // past the last entry → not found
            emitter.instruction("str xzr, [sp, #8]");                           // iterator cursor = start of insertion order
            emitter.label("__rt_obj_dyn_prop_at_loop");
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the hash pointer
            emitter.instruction("ldr x1, [sp, #8]");                            // reload the iterator cursor
            emitter.instruction("bl __rt_hash_iter_next_value");                // x0=cursor x1=key x2=len x3=lo x4=hi x5=tag
            emitter.instruction("str x0, [sp, #8]");                            // save the advanced cursor
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the remaining step count
            emitter.instruction("cbz x9, __rt_obj_dyn_prop_at_found");          // this entry is the requested ordinal
            emitter.instruction("sub x9, x9, #1");                              // one fewer entry to skip
            emitter.instruction("str x9, [sp, #16]");                           // save the remaining step count
            emitter.instruction("b __rt_obj_dyn_prop_at_loop");                 // advance to the next entry
            emitter.label("__rt_obj_dyn_prop_at_found");
            emitter.instruction("mov x0, #1");                                  // report the entry as found
            emitter.instruction("b __rt_obj_dyn_prop_at_done");                 // return the entry tuple
            emitter.label("__rt_obj_dyn_prop_at_none");
            emitter.instruction("mov x0, #0");                                  // report "no such dynamic property"
            emitter.label("__rt_obj_dyn_prop_at_done");
            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore frame pointer and return address
            emitter.instruction("add sp, sp, #48");                             // release the ordinal-walk frame
            emitter.instruction("ret");                                         // return to caller
        }
        Arch::X86_64 => {
            // rbp-relative frame: [-8] hash, [-16] cursor, [-24] remaining steps.
            emitter.instruction("push rbp");                                    // save caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the ordinal-walk frame pointer
            emitter.instruction("sub rsp, 32");                                 // allocate the ordinal-walk frame
            emitter.instruction("mov QWORD PTR [rbp - 24], rsi");               // remember the requested ordinal
            emitter.instruction("call __rt_obj_dump_dyn_props");                // rax = dynamic-property hash, or 0
            emitter.instruction("test rax, rax");                               // does the object carry a dumpable tail?
            emitter.instruction("jz __rt_obj_dyn_prop_at_none_x86");            // no tail means no Nth dynamic property
            emitter.instruction("mov QWORD PTR [rbp - 8], rax");                // keep the hash pointer across iteration calls
            emitter.instruction("mov r10, QWORD PTR [rbp - 24]");               // reload the requested ordinal
            emitter.instruction("cmp r10, 0");                                  // reject a negative ordinal
            emitter.instruction("jl __rt_obj_dyn_prop_at_none_x86");            // negative ordinals name no entry
            emitter.instruction("cmp r10, QWORD PTR [rax]");                    // is the ordinal within the entry count?
            emitter.instruction("jae __rt_obj_dyn_prop_at_none_x86");           // past the last entry → not found
            emitter.instruction("mov QWORD PTR [rbp - 16], 0");                 // iterator cursor = start of insertion order
            emitter.label("__rt_obj_dyn_prop_at_loop_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // reload the hash pointer
            emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");               // reload the iterator cursor
            emitter.instruction("call __rt_hash_iter_next_value");              // rax=cursor rdi=key rdx=len rcx=lo r8=hi r9=tag
            emitter.instruction("mov QWORD PTR [rbp - 16], rax");               // save the advanced cursor
            emitter.instruction("mov r10, QWORD PTR [rbp - 24]");               // reload the remaining step count
            emitter.instruction("test r10, r10");                               // is this entry the requested ordinal?
            emitter.instruction("jz __rt_obj_dyn_prop_at_found_x86");           // return this entry
            emitter.instruction("sub r10, 1");                                  // one fewer entry to skip
            emitter.instruction("mov QWORD PTR [rbp - 24], r10");               // save the remaining step count
            emitter.instruction("jmp __rt_obj_dyn_prop_at_loop_x86");           // advance to the next entry
            emitter.label("__rt_obj_dyn_prop_at_found_x86");
            emitter.instruction("mov eax, 1");                                  // report the entry as found
            emitter.instruction("jmp __rt_obj_dyn_prop_at_done_x86");           // return the entry tuple
            emitter.label("__rt_obj_dyn_prop_at_none_x86");
            emitter.instruction("xor eax, eax");                                // report "no such dynamic property"
            emitter.label("__rt_obj_dyn_prop_at_done_x86");
            emitter.instruction("add rsp, 32");                                 // release the ordinal-walk frame
            emitter.instruction("pop rbp");                                     // restore caller frame pointer
            emitter.instruction("ret");                                         // return to caller
        }
    }
}
