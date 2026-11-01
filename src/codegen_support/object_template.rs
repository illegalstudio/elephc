//! Purpose:
//! The shared allocation routine behind `new C(...)`: one call turns a per-class static template
//! into a live object.
//!
//! Called from:
//! - `codegen::lower_inst::objects::allocation_clone::emit_object_allocation_from_template`, at
//!   every fixed-class `new` whose class needs no per-object cells.
//! - `runtime::emitters::managed`, which emits the routine once.
//!
//! Key details:
//! - The template is data: `[payload size in bytes][payload image...]`. The image is exactly the
//!   bytes the inline sequence used to build one store at a time: the class id, every slot zeroed,
//!   the typed-uninitialized markers, and every literal default that needs no allocation (ints,
//!   bools, floats, static strings, nulls, tagged scalars). A `new` site is therefore three
//!   instructions however many properties the class has; before, it was one store per word plus one
//!   to three instructions per default (about 345 instructions for a seven-property class).
//! - Anything that allocates (array and enum defaults, boxed values) stays out of the template and
//!   is assigned by the class's `_class_propinit_<id>` thunk, which the site calls through the
//!   existing table.
//! - Contract: template address in the first argument register, object payload pointer in the
//!   integer result register. Clobbers the caller-saved registers, like the inline sequence did.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_object_new_template` for the active target.
pub(crate) fn emit_object_new_template(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_new_template ---");
    emitter.label_global("__rt_object_new_template");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // preserve frame linkage across the nested calls
            emitter.instruction("str x0, [sp, #-16]!");                         // keep the template while allocating
            emitter.instruction("ldr x0, [x0]");                                // payload size in bytes
            emitter.instruction("bl __rt_heap_alloc");                          // allocate the object payload
            emitter.instruction("mov x9, #4");                                  // heap kind 4 marks object instances for ownership helpers
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the heap header before the payload
            emitter.instruction("bl __rt_object_handle_acquire");               // bind the new object to its PHP object handle
            emitter.instruction("ldr x1, [sp], #16");                           // reload the template
            emitter.instruction("ldr x2, [x1], #8");                            // payload size; x1 now points at the image
            emitter.instruction("ldr x3, [x1], #8");                            // first image word: the class id
            emitter.instruction("str x3, [x0]");                                // store it at payload offset zero
            emitter.instruction("add x4, x0, #8");                              // destination of the slot pairs
            emitter.instruction("sub x2, x2, #8");                              // bytes left after the class id
            let copy = "__rt_object_new_template_copy";
            let tail = "__rt_object_new_template_tail";
            let done = "__rt_object_new_template_done";
            emitter.label(copy);
            emitter.instruction("cmp x2, #16");                                 // a whole slot pair left?
            emitter.instruction(&format!("b.lo {tail}"));                       // no: only the trailing word, if any
            emitter.instruction("ldp x5, x6, [x1], #16");                       // load one slot pair from the image
            emitter.instruction("stp x5, x6, [x4], #16");                       // store it into the object
            emitter.instruction("sub x2, x2, #16");                             // one slot fewer to copy
            emitter.instruction(&format!("b {copy}"));                          // next slot
            emitter.label(tail);
            emitter.instruction(&format!("cbz x2, {done}"));                    // an even payload ends here
            emitter.instruction("ldr x5, [x1]");                                // the trailing property-hash word of the image
            emitter.instruction("str x5, [x4]");                                // ... into the object
            emitter.label(done);
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore frame linkage
            emitter.instruction("ret");                                         // return the object in x0
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish a frame that keeps the nested calls aligned
            emitter.instruction("sub rsp, 16");                                 // slot for the template, keeping 16-byte alignment
            // The template arrives in rdi, the first argument register both call sites load. Reading
            // rax took whatever the site left there (the `_php_site` record): every x86_64 `new`
            // allocated a garbage size and copied a garbage image (no class name, wrong slots).
            emitter.instruction("mov QWORD PTR [rsp], rdi");                    // keep the template while allocating
            emitter.instruction("mov rax, QWORD PTR [rdi]");                    // payload size in bytes
            emitter.instruction("call __rt_heap_alloc");                        // allocate the object payload
            emitter.instruction(&format!(
                "mov r10, 0x{:x}",
                crate::codegen_support::sentinels::x86_64_heap_kind_word(4)
            ));                                                                 // materialize the object heap kind word
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the heap header before the payload
            emitter.instruction("call __rt_object_handle_acquire");             // bind the new object to its PHP object handle
            emitter.instruction("mov rsi, QWORD PTR [rsp]");                    // reload the template
            emitter.instruction("mov rcx, QWORD PTR [rsi]");                    // payload size in bytes
            emitter.instruction("shr rcx, 3");                                  // ... as 8-byte words
            emitter.instruction("add rsi, 8");                                  // skip the size word: the image starts here
            emitter.instruction("mov rdi, rax");                                // destination: the object payload
            emitter.instruction("rep movsq");                                   // copy the image into the object
            emitter.instruction("leave");                                       // release the template slot and restore rbp
            emitter.instruction("ret");                                         // return the object in rax
        }
    }
}

/// Bytes of one `new $class` table record (see `dynamic_new_table`).
pub(crate) const DYNAMIC_NEW_RECORD_BYTES: usize = 64;

/// Emits `__rt_dynamic_new_lookup(name ptr, name len, table, count) -> record or 0`.
///
/// The table is an array of fixed 64-byte records whose first two words are the class name's
/// pointer and length. The scan compares lengths first and only then the case-insensitive bytes
/// through `__rt_strcasecmp`, the comparison the per-class ladder used. Everything else a record
/// holds (template, constructor descriptor, class id) is read by the caller.
pub(crate) fn emit_dynamic_new_lookup(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: dynamic_new_lookup ---");
    emitter.label_global("__rt_dynamic_new_lookup");
    let scan = "__rt_dynamic_new_lookup_scan";
    let next = "__rt_dynamic_new_lookup_next";
    let hit = "__rt_dynamic_new_lookup_hit";
    let miss = "__rt_dynamic_new_lookup_miss";
    let done = "__rt_dynamic_new_lookup_done";
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // preserve frame linkage across the nested compare
            emitter.instruction("stp x19, x20, [sp, #-16]!");                   // callee-saved scan state
            emitter.instruction("stp x21, x22, [sp, #-16]!");                   // callee-saved scan state
            emitter.instruction("mov x19, x0");                                 // wanted name pointer
            emitter.instruction("mov x20, x1");                                 // wanted name length
            emitter.instruction("mov x21, x2");                                 // record cursor
            emitter.instruction("mov x22, x3");                                 // records left
            emitter.label(scan);
            emitter.instruction(&format!("cbz x22, {miss}"));                   // no record matched
            emitter.instruction("ldr x9, [x21, #8]");                           // this record's name length
            emitter.instruction("cmp x9, x20");                                 // lengths differ: cannot match
            emitter.instruction(&format!("b.ne {next}"));                       // skip the byte comparison
            emitter.instruction("mov x1, x19");                                 // wanted name pointer
            emitter.instruction("mov x2, x20");                                 // wanted name length
            emitter.instruction("ldr x3, [x21]");                               // candidate name pointer
            emitter.instruction("mov x4, x9");                                  // candidate name length
            emitter.instruction("bl __rt_strcasecmp");                          // PHP class names compare case-insensitively
            emitter.instruction(&format!("cbz x0, {hit}"));                     // equal: this record
            emitter.label(next);
            emitter.instruction(&format!("add x21, x21, #{DYNAMIC_NEW_RECORD_BYTES}")); // next record
            emitter.instruction("sub x22, x22, #1");                            // one record fewer
            emitter.instruction(&format!("b {scan}"));                          // keep scanning
            emitter.label(hit);
            emitter.instruction("mov x0, x21");                                 // the matching record
            emitter.instruction(&format!("b {done}"));                          // return it
            emitter.label(miss);
            emitter.instruction("mov x0, #0");                                  // no record
            emitter.label(done);
            emitter.instruction("ldp x21, x22, [sp], #16");                     // restore scan state
            emitter.instruction("ldp x19, x20, [sp], #16");                     // restore scan state
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore frame linkage
            emitter.instruction("ret");                                         // return the record or 0
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // frame for the nested compare
            emitter.instruction("push r12");                                    // callee-saved scan state
            emitter.instruction("push r13");                                    // callee-saved scan state
            emitter.instruction("push r14");                                    // callee-saved scan state
            emitter.instruction("push r15");                                    // callee-saved scan state
            emitter.instruction("mov r12, rdi");                                // wanted name pointer
            emitter.instruction("mov r13, rsi");                                // wanted name length
            emitter.instruction("mov r14, rdx");                                // record cursor
            emitter.instruction("mov r15, rcx");                                // records left
            emitter.label(scan);
            emitter.instruction("test r15, r15");                               // any record left?
            emitter.instruction(&format!("jz {miss}"));                         // no record matched
            emitter.instruction("mov rax, QWORD PTR [r14 + 8]");                // this record's name length
            emitter.instruction("cmp rax, r13");                                // lengths differ: cannot match
            emitter.instruction(&format!("jne {next}"));                        // skip the byte comparison
            emitter.instruction("mov rdi, r12");                                // wanted name pointer
            emitter.instruction("mov rsi, r13");                                // wanted name length
            emitter.instruction("mov rdx, QWORD PTR [r14]");                    // candidate name pointer
            emitter.instruction("mov rcx, rax");                                // candidate name length
            emitter.instruction("call __rt_strcasecmp");                        // PHP class names compare case-insensitively
            emitter.instruction("test rax, rax");                               // zero = equal
            emitter.instruction(&format!("je {hit}"));                          // this record
            emitter.label(next);
            emitter.instruction(&format!("add r14, {DYNAMIC_NEW_RECORD_BYTES}")); // next record
            emitter.instruction("dec r15");                                     // one record fewer
            emitter.instruction(&format!("jmp {scan}"));                        // keep scanning
            emitter.label(hit);
            emitter.instruction("mov rax, r14");                                // the matching record
            emitter.instruction(&format!("jmp {done}"));                        // return it
            emitter.label(miss);
            emitter.instruction("xor eax, eax");                                // no record
            emitter.label(done);
            emitter.instruction("pop r15");                                     // restore scan state
            emitter.instruction("pop r14");                                     // restore scan state
            emitter.instruction("pop r13");                                     // restore scan state
            emitter.instruction("pop r12");                                     // restore scan state
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the record or 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// The routine allocates, stamps the object heap kind, binds the handle and copies the image on
    /// every target.
    #[test]
    fn the_template_routine_allocates_stamps_binds_and_copies_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_object_new_template(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_object_new_template:"), "{name}");
            assert!(asm.contains("__rt_heap_alloc"), "{name}");
            assert!(asm.contains("__rt_object_handle_acquire"), "{name}");
            let copy = if name == "linux-x86_64" { "rep movsq" } else { "stp x5, x6, [x4], #16" };
            assert!(asm.contains(copy), "{name}: {asm}");
        }
    }

    /// The template is read from the first argument register the call sites load (`x0`, `rdi`):
    /// the x86_64 arm read `rax`, the `_php_site` record the site had just published.
    #[test]
    fn the_template_routine_reads_the_template_from_the_first_argument_register() {
        let mut emitter = Emitter::new(Target::parse("linux-x86_64").unwrap());
        emit_object_new_template(&mut emitter);
        let asm = emitter.output();
        let first = asm
            .lines()
            .map(str::trim)
            .find(|line| line.contains("QWORD PTR [rsp]") && line.starts_with("mov"))
            .unwrap_or_default();
        assert!(first.ends_with("rdi"), "the template must come from rdi: {asm}");
        assert!(asm.contains("mov rax, QWORD PTR [rdi]"), "{asm}");
    }

    /// The lookup routine scans fixed-size records and compares names case-insensitively on every
    /// target.
    #[test]
    fn the_lookup_routine_scans_records_and_compares_names_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_dynamic_new_lookup(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_dynamic_new_lookup:"), "{name}");
            assert!(asm.contains("__rt_strcasecmp"), "{name}");
            assert!(asm.contains(&format!("{DYNAMIC_NEW_RECORD_BYTES}")), "{name}");
        }
    }
}
