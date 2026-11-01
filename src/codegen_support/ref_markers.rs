//! Purpose:
//! Emits the inline snippets that let a value walker read THROUGH an array element that is a
//! PHP reference, instead of treating the internal marker as an unknown value.
//!
//! Called from:
//! - Runtime walkers that dispatch on a boxed Mixed tag (`json_encode`, `print_r`, ...).
//!
//! Key details:
//! - An element bound with `$a[$k] = &$x` holds a marker box `{tag, cell, source_tag}`: tag 12
//!   owns a share of a request-global ref cell, tag 13 a share of a promoted local ref cell. The
//!   cell's first word holds the referenced value; a `source_tag` of 7 means it is a boxed Mixed
//!   handle, which is the only shape a local promoted for an element reference ever has.
//! - Walkers reach these snippets only from their unknown-tag fallback, so the ordinary tags pay
//!   nothing for them. Tag 11 is deliberately excluded: it also names type-erased Iterable
//!   records and borrowed invoker cells whose layouts differ.

use crate::codegen_support::callable_invoker_args::{
    ARRAY_GLOBAL_REF_CELL_TAG, ARRAY_LOCAL_REF_CELL_TAG,
};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Follows a boxed reference marker in `box_reg` to the Mixed box it currently references.
///
/// When `box_reg` holds a tag-12/13 marker whose cell stores a boxed Mixed handle, `box_reg` is
/// replaced by that (borrowed) handle and control jumps to `retry_label`, which must re-dispatch
/// on it from the walker's null check. Any other value -- including a typed cell -- jumps to
/// `fallback_label`. `scratch` is clobbered; `local` names this site's private labels.
pub(crate) fn emit_follow_boxed_ref_marker(
    emitter: &mut Emitter,
    box_reg: &str,
    scratch: &str,
    retry_label: &str,
    fallback_label: &str,
    local: &str,
) {
    let marker_label = format!("{local}_ref_marker");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {scratch}, [{box_reg}]"));        // reload the boxed tag the dispatch did not recognize
            emitter.instruction(&format!("cmp {scratch}, #{}", ARRAY_GLOBAL_REF_CELL_TAG)); // an element that owns a global reference?
            emitter.instruction(&format!("b.eq {marker_label}"));               // read through it
            emitter.instruction(&format!("cmp {scratch}, #{}", ARRAY_LOCAL_REF_CELL_TAG)); // an element that owns a local reference?
            emitter.instruction(&format!("b.ne {fallback_label}"));             // any other tag keeps the walker's own fallback
            emitter.label(&marker_label);
            emitter.instruction(&format!("ldr {scratch}, [{box_reg}, #16]"));   // load the runtime tag of the value the cell stores
            emitter.instruction(&format!("cmp {scratch}, #7"));                 // only boxed Mixed handles can be re-dispatched as a box
            emitter.instruction(&format!("b.ne {fallback_label}"));             // a typed cell has no box to hand back
            emitter.instruction(&format!("ldr {scratch}, [{box_reg}, #8]"));    // load the shared reference cell
            emitter.instruction(&format!("ldr {box_reg}, [{scratch}]"));        // borrow the Mixed box the reference currently holds
            emitter.instruction(&format!("b {retry_label}"));                   // dispatch again on the referenced value
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {scratch}, QWORD PTR [{box_reg}]")); // reload the boxed tag the dispatch did not recognize
            emitter.instruction(&format!("cmp {scratch}, {}", ARRAY_GLOBAL_REF_CELL_TAG)); // an element that owns a global reference?
            emitter.instruction(&format!("je {marker_label}"));                 // read through it
            emitter.instruction(&format!("cmp {scratch}, {}", ARRAY_LOCAL_REF_CELL_TAG)); // an element that owns a local reference?
            emitter.instruction(&format!("jne {fallback_label}"));              // any other tag keeps the walker's own fallback
            emitter.label(&marker_label);
            emitter.instruction(&format!("mov {scratch}, QWORD PTR [{box_reg} + 16]")); // load the runtime tag of the value the cell stores
            emitter.instruction(&format!("cmp {scratch}, 7"));                  // only boxed Mixed handles can be re-dispatched as a box
            emitter.instruction(&format!("jne {fallback_label}"));              // a typed cell has no box to hand back
            emitter.instruction(&format!("mov {scratch}, QWORD PTR [{box_reg} + 8]")); // load the shared reference cell
            emitter.instruction(&format!("mov {box_reg}, QWORD PTR [{scratch}]")); // borrow the Mixed box the reference currently holds
            emitter.instruction(&format!("jmp {retry_label}"));                 // dispatch again on the referenced value
        }
    }
}

/// Follows a reference marker already flattened into a `(tag, lo, hi)` value triple.
///
/// Walkers that copy a box's three words into registers see a marker as `tag = 12|13`,
/// `lo = cell`, `hi = source tag`. This rewrites the triple in place into the referenced value's
/// own triple -- `(7, box, 0)` for a Mixed cell, `(source_tag, cell[0], cell[8])` for a typed
/// one -- and jumps to `retry_label`. Any other tag jumps to `fallback_label`.
pub(crate) fn emit_follow_ref_marker_triple(
    emitter: &mut Emitter,
    regs: (&str, &str, &str),
    retry_label: &str,
    fallback_label: &str,
    local: &str,
) {
    let (tag, lo, hi) = regs;
    let marker_label = format!("{local}_ref_triple");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {tag}, #{}", ARRAY_GLOBAL_REF_CELL_TAG)); // an element that owns a global reference?
            emitter.instruction(&format!("b.eq {marker_label}"));               // read through it
            emitter.instruction(&format!("cmp {tag}, #{}", ARRAY_LOCAL_REF_CELL_TAG)); // an element that owns a local reference?
            emitter.instruction(&format!("b.ne {fallback_label}"));             // any other tag keeps the walker's own fallback
            emitter.label(&marker_label);
            emitter.instruction(&format!("mov {tag}, {hi}"));                   // the referenced value's tag is the marker's source tag
            emitter.instruction(&format!("ldr {hi}, [{lo}, #8]"));              // the cell's second word (a string length, or unused)
            emitter.instruction(&format!("ldr {lo}, [{lo}]"));                  // the cell's first word (the payload, or a Mixed box)
            emitter.instruction(&format!("b {retry_label}"));                   // dispatch again on the referenced value
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {tag}, {}", ARRAY_GLOBAL_REF_CELL_TAG)); // an element that owns a global reference?
            emitter.instruction(&format!("je {marker_label}"));                 // read through it
            emitter.instruction(&format!("cmp {tag}, {}", ARRAY_LOCAL_REF_CELL_TAG)); // an element that owns a local reference?
            emitter.instruction(&format!("jne {fallback_label}"));              // any other tag keeps the walker's own fallback
            emitter.label(&marker_label);
            emitter.instruction(&format!("mov {tag}, {hi}"));                   // the referenced value's tag is the marker's source tag
            emitter.instruction(&format!("mov {hi}, QWORD PTR [{lo} + 8]"));    // the cell's second word (a string length, or unused)
            emitter.instruction(&format!("mov {lo}, QWORD PTR [{lo}]"));        // the cell's first word (the payload, or a Mixed box)
            emitter.instruction(&format!("jmp {retry_label}"));                 // dispatch again on the referenced value
        }
    }
}
