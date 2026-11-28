//! Purpose:
//! Resolves managed reference cells and their persistent boxed children.
//!
//! Called from:
//! - Native reference readers and metadata-authenticated compiler row adapters.
//!
//! Key details:
//! - Unknown owners are rejected before any payload is read.

use crate::{abi, Arch, Emitter};

/// Resolves an owned scalar row to its canonical child without calls or unrelated scratch registers.
/// The metadata flag proves the cell is managed; borrowed constructor slots must never carry it.
pub fn resolve_scalar_property(
    emitter: &mut impl Emitter, address: &str, tag: &str, marker: &str, prefix: &str, scalar_flag: u64,
) {
    debug_assert!(address != tag && address != marker && tag != marker);
    let raw = format!("{prefix}_scalar_raw");
    let done = format!("{prefix}_scalar_done");
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("tst {tag}, #{scalar_flag}"));         // only metadata-authenticated scalar cells permit header inspection
            emitter.instruction(&format!("b.eq {done}"));                       // preserve borrowed and container-specific reference layouts
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {tag}, {scalar_flag}"));         // only metadata-authenticated scalar cells permit header inspection
            emitter.instruction(&format!("jz {done}"));                         // preserve borrowed and container-specific reference layouts
        }
    }
    abi::emit_push_reg(emitter, tag);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {tag}, [{address}, #-8]"));       // inspect the metadata-authenticated cell header
            emitter.instruction(&format!("ubfx {tag}, {tag}, #8, #7"));         // isolate the live cell's physical payload descriptor
            emitter.instruction(&format!("cmp {tag}, #7"));                     // promoted scalar cells own a Mixed PHPRef wrapper
            emitter.instruction(&format!("b.ne {raw}"));                        // raw scalars and strings retain their logical row tag
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {tag}, QWORD PTR [{address} - 8]")); // inspect the metadata-authenticated cell header
            emitter.instruction(&format!("shr {tag}, 8"));                      // position the live cell's physical payload descriptor
            emitter.instruction(&format!("and {tag}, 127"));                    // remove heap and collector flags
            emitter.instruction(&format!("cmp {tag}, 7"));                      // promoted scalar cells own a Mixed PHPRef wrapper
            emitter.instruction(&format!("jne {raw}"));                         // raw scalars and strings retain their logical row tag
        }
    }
    crate::layout::emit_branch_if_null_container(emitter, marker, tag, &raw);
    abi::emit_load_int_immediate(emitter, tag, emitter.uninitialized_property_sentinel());
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {marker}, {tag}"));               // unset owned cells carry a low-word initialization sentinel
            emitter.instruction(&format!("b.eq {raw}"));                        // preserve uninitialized detection before any wrapper dereference
            emitter.instruction(&format!("ldrb {tag}, [{marker}, #-8]", tag = word_register(tag))); // confirm the physical Mixed allocation kind
            emitter.instruction(&format!("cmp {tag}, #5"));                     // raw Iterable containers are never canonical scalar wrappers
            emitter.instruction(&format!("b.ne {raw}"));                        // leave other managed payload shapes unchanged
            emitter.instruction(&format!("ldr {tag}, [{marker}]"));             // inspect the wrapper's logical tag
            emitter.instruction(&format!("cmp {tag}, #7"));                     // canonical aliases own a nested Mixed child
            emitter.instruction(&format!("b.ne {raw}"));                        // ordinary value boxes are not persistent aliases
            emitter.instruction(&format!("ldr {tag}, [{marker}, #16]"));        // inspect the persistent-reference ABI marker
            emitter.instruction(&format!("cmp {tag}, #1"));                     // only marker one owns reference identity
            emitter.instruction(&format!("b.ne {raw}"));                        // preserve ordinary nested Mixed values
            emitter.instruction(&format!("add {address}, {marker}, #8"));       // expose the canonical child's owning pointer slot
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {marker}, {tag}"));               // unset owned cells carry a low-word initialization sentinel
            emitter.instruction(&format!("je {raw}"));                          // preserve uninitialized detection before any wrapper dereference
            emitter.instruction(&format!("cmp BYTE PTR [{marker} - 8], 5"));    // confirm the physical Mixed allocation kind
            emitter.instruction(&format!("jne {raw}"));                         // leave other managed payload shapes unchanged
            emitter.instruction(&format!("cmp QWORD PTR [{marker}], 7"));       // canonical aliases own a nested Mixed child
            emitter.instruction(&format!("jne {raw}"));                         // ordinary value boxes are not persistent aliases
            emitter.instruction(&format!("cmp QWORD PTR [{marker} + 16], 1"));  // only marker one owns reference identity
            emitter.instruction(&format!("jne {raw}"));                         // preserve ordinary nested Mixed values
            emitter.instruction(&format!("lea {address}, [{marker} + 8]"));     // expose the canonical child's owning pointer slot
        }
    }
    abi::emit_pop_reg(emitter, tag);
    abi::emit_load_int_immediate(emitter, tag, 7);
    abi::emit_load_from_address(emitter, marker, address, 0);
    abi::emit_jump(emitter, &done);
    emitter.label(&raw);
    abi::emit_pop_reg(emitter, tag);
    abi::emit_load_from_address(emitter, marker, address, 0);
    emitter.label(&done);
}

/// Selects the 32-bit ARM register spelling used by a byte-sized heap-kind load.
fn word_register(register: &str) -> String {
    format!("w{}", register.strip_prefix('x').expect("ARM metadata tag uses an integer register"))
}

/// Returns a live marker-one Mixed wrapper, or zero for every raw or uninitialized layout.
pub fn emit(emitter: &mut impl Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let none = "__rt_reference_cell_canonical_none";
    let done = "__rt_reference_cell_canonical_done";
    emitter.blank();
    emitter.label_global("__rt_reference_cell_canonical");
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_call_label(emitter, "__rt_reference_cell_owner");
    abi::emit_branch_if_int_result_zero(emitter, done);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [x0, #-8]");                           // inspect only an authenticated managed cell header
            emitter.instruction("ubfx x9, x9, #8, #7");                         // isolate the physical payload descriptor
            emitter.instruction("cmp x9, #7");                                  // a canonical cell owns a boxed Mixed payload
            emitter.instruction(&format!("b.ne {none}"));                       // raw scalar and string words cannot be wrapper pointers
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rax - 8]");                // inspect only an authenticated managed cell header
            emitter.instruction("shr r10, 8");                                  // position the physical payload descriptor
            emitter.instruction("and r10d, 0x7f");                              // discard collector flags and the heap marker
            emitter.instruction("cmp r10, 7");                                  // a canonical cell owns a boxed Mixed payload
            emitter.instruction(&format!("jne {none}"));                        // raw scalar and string words cannot be wrapper pointers
        }
    }
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_load_int_immediate(emitter, scratch, emitter.uninitialized_property_sentinel());
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {result}, {scratch}"));           // unset cells retain the low-word initialization sentinel
            emitter.instruction(&format!("b.eq {none}"));                       // never inspect a sentinel as a heap payload
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {result}, {scratch}"));           // unset cells retain the low-word initialization sentinel
            emitter.instruction(&format!("je {none}"));                         // never inspect a sentinel as a heap payload
        }
    }
    abi::emit_call_label(emitter, "__rt_gc_candidate_live");
    abi::emit_branch_if_int_result_zero(emitter, done);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("ldrb w9, [x0, #-8]");                          // descriptor seven also permits raw iterable containers
            emitter.instruction("cmp w9, #5");                                  // only managed Mixed allocations can hold a PHPRef wrapper
            emitter.instruction(&format!("b.ne {none}"));                       // reject arrays, objects and other owned payload kinds
            emitter.instruction("ldr x9, [x0]");                                // inspect the logical Mixed tag after heap authentication
            emitter.instruction("cmp x9, #7");                                  // persistent aliases own a nested boxed child
            emitter.instruction(&format!("b.ne {none}"));                       // ordinary concrete boxes keep their raw cell representation
            emitter.instruction("ldr x9, [x0, #16]");                           // distinguish aliases from ordinary nested Mixed values
            emitter.instruction("cmp x9, #1");                                  // marker one is the persistent PHP reference ABI
            emitter.instruction(&format!("b.ne {none}"));                       // never promote an ordinary nested value into an alias
        }
        Arch::X86_64 => {
            emitter.instruction("cmp BYTE PTR [rax - 8], 5");                   // only managed Mixed allocations can hold a PHPRef wrapper
            emitter.instruction(&format!("jne {none}"));                        // raw iterable containers share descriptor seven
            emitter.instruction("cmp QWORD PTR [rax], 7");                      // persistent aliases own a nested boxed child
            emitter.instruction(&format!("jne {none}"));                        // ordinary concrete boxes keep their raw cell representation
            emitter.instruction("cmp QWORD PTR [rax + 16], 1");                 // marker one distinguishes aliases from ordinary nested values
            emitter.instruction(&format!("jne {none}"));                        // never promote an ordinary nested value into an alias
        }
    }
    abi::emit_jump(emitter, done);
    emitter.label(none);
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label(done);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}
