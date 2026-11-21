//! Purpose:
//! Resolves raw Iterable property metadata to its concrete runtime container tag.
//!
//! Called from:
//! - Shared property payload resolution and the direct JSON property walker.
//!
//! Key details:
//! - Canonical boxed children have tag seven and bypass this raw-layout adapter.
//! - The address and initialization marker are preserved without runtime calls or other scratch registers.

use super::*;
use crate::codegen_support::sentinels::{ITERABLE_PROPERTY_TAG, NULL_SENTINEL};

/// Classifies a raw container after storage flags have been removed, preserving the caller's marker.
pub(crate) fn resolve(emitter: &mut Emitter, address: &str, tag: &str, marker: &str, prefix: &str) {
    debug_assert!(address != tag && address != marker && tag != marker);
    let done = format!("{prefix}_iterable_done");
    let absent = format!("{prefix}_iterable_absent");
    let restore = format!("{prefix}_iterable_restore");
    // -- resolve only raw Iterable rows; scalar and canonical layouts retain their existing tags --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {tag}, #{ITERABLE_PROPERTY_TAG}")); // raw iterable metadata is distinct from a boxed Mixed value
            emitter.instruction(&format!("b.ne {done}"));                       // canonical children already expose their actual value box
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {tag}, {ITERABLE_PROPERTY_TAG}")); // raw iterable metadata is distinct from a boxed Mixed value
            emitter.instruction(&format!("jne {done}"));                        // canonical children already expose their actual value box
        }
    }
    abi::emit_push_reg(emitter, marker);
    branch_if_sentinel(emitter, marker, tag, UNINITIALIZED_TYPED_PROPERTY_SENTINEL, &absent);
    abi::emit_load_from_address(emitter, marker, address, 0);
    for sentinel in [0, NULL_SENTINEL, UNINITIALIZED_TYPED_PROPERTY_SENTINEL] {
        branch_if_sentinel(emitter, marker, tag, sentinel, &absent);
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {tag}, [{marker}, #-8]"));        // read the concrete raw container's heap header
            emitter.instruction(&format!("and {tag}, {tag}, #255"));            // remove collector flags from its heap kind
            emitter.instruction(&format!("cmp {tag}, #2"));                     // arrays start the supported container range
            emitter.instruction(&format!("b.lo {absent}"));                     // other heap families cannot satisfy raw Iterable storage
            emitter.instruction(&format!("cmp {tag}, #4"));                     // Traversable objects end the supported container range
            emitter.instruction(&format!("b.hi {absent}"));                     // boxed values use a separate layout
            emitter.instruction(&format!("add {tag}, {tag}, #2"));              // map array/hash/object heap kinds to runtime value tags
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {tag}, QWORD PTR [{marker} - 8]")); // read the concrete raw container's heap header
            emitter.instruction(&format!("and {tag}, 255"));                    // remove collector flags from its heap kind
            emitter.instruction(&format!("cmp {tag}, 2"));                      // arrays start the supported container range
            emitter.instruction(&format!("jb {absent}"));                       // other heap families cannot satisfy raw Iterable storage
            emitter.instruction(&format!("cmp {tag}, 4"));                      // Traversable objects end the supported container range
            emitter.instruction(&format!("ja {absent}"));                       // boxed values use a separate layout
            emitter.instruction(&format!("add {tag}, 2"));                      // map array/hash/object heap kinds to runtime value tags
        }
    }
    abi::emit_jump(emitter, &restore);
    emitter.label(&absent);
    abi::emit_load_int_immediate(emitter, tag, 8);
    emitter.label(&restore);
    abi::emit_pop_reg(emitter, marker);
    emitter.label(&done);
}

/// Branches before a header read when the borrowed pointer or initialization marker is absent.
fn branch_if_sentinel(emitter: &mut Emitter, value: &str, scratch: &str, sentinel: i64, absent: &str) {
    abi::emit_load_int_immediate(emitter, scratch, sentinel);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {value}, {scratch}"));            // compare the borrowed value without interpreting it as a pointer
            emitter.instruction(&format!("b.eq {absent}"));                     // sentinels have no heap header to inspect
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {value}, {scratch}"));            // compare the borrowed value without interpreting it as a pointer
            emitter.instruction(&format!("je {absent}"));                       // sentinels have no heap header to inspect
        }
    }
}
