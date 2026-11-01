//! Purpose:
//! Emits a class-id switch as a binary search over the sorted ids, with short linear leaves.
//!
//! Called from:
//! - `super::eval_constructor_helpers` and `super::eval_method_helpers`, for the eval bridge's
//!   constructor, method and builtin-Throwable dispatch.
//!
//! Key details:
//! - Those switches used to test every class in turn. A whole-framework build carries one
//!   case per class the bridge can construct or call, so an interpreted `new` of a compiled
//!   class walked hundreds of compare-and-branch steps first -- ~2.6% of a Symfony request.
//! - Each test reloads the id through `load_id`, as the linear chains did: a group body is
//!   free to clobber every scratch register before control reaches the next test.
//! - A leaf keeps the chain's `{stem}_next_{id}{suffix}` labels; no match anywhere falls
//!   through to the code after the switch, exactly where the chain used to end.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Cases a leaf tests linearly; below this a compare tree costs more than it saves.
const LEAF_CASES: usize = 8;

/// How one switch reads its class id and names its labels.
pub(super) struct ClassIdSwitch<'a> {
    /// Emits the instructions that leave the class id in the given register.
    pub load_id: &'a dyn Fn(&mut Emitter, &str),
    /// Label stem shared by every label of this switch.
    pub stem: &'a str,
    /// Per-target label suffix (`""` on AArch64, `"_x"` on x86_64, as the chains used).
    pub suffix: &'a str,
}

/// Emits the switch over `ids` (ascending, distinct); `emit_case(emitter, i)` emits the body for
/// `ids[i]`. A body that does not branch away falls through to the next test, as in a chain.
pub(super) fn emit_class_id_switch(
    emitter: &mut Emitter,
    switch: &ClassIdSwitch<'_>,
    ids: &[u64],
    emit_case: &mut dyn FnMut(&mut Emitter, usize),
) {
    debug_assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    let miss = format!("{}_switch_miss{}", switch.stem, switch.suffix);
    emit_node(emitter, switch, ids, 0, ids.len(), emit_case, &miss);
    emitter.label(&miss);
}

/// Emits the subtree for `ids[lo..hi]`; every path out of it that matches nothing reaches `miss`.
fn emit_node(
    emitter: &mut Emitter,
    switch: &ClassIdSwitch<'_>,
    ids: &[u64],
    lo: usize,
    hi: usize,
    emit_case: &mut dyn FnMut(&mut Emitter, usize),
    miss: &str,
) {
    let (id_reg, imm_reg) = match emitter.target.arch {
        Arch::AArch64 => ("x9", "x10"),
        Arch::X86_64 => ("r11", "r10"),
    };
    if hi - lo <= LEAF_CASES {
        for index in lo..hi {
            let next = format!("{}_next_{}{}", switch.stem, ids[index], switch.suffix);
            (switch.load_id)(emitter, id_reg);
            abi::emit_load_int_immediate(emitter, imm_reg, ids[index] as i64);
            emit_compare(emitter, id_reg, imm_reg);
            match emitter.target.arch {
                Arch::AArch64 => emitter.instruction(&format!("b.ne {}", next)), // not this class: try the next case
                Arch::X86_64 => emitter.instruction(&format!("jne {}", next)),   // not this class: try the next case
            }
            emit_case(emitter, index);
            emitter.label(&next);
        }
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("b {}", miss)),       // no class of this leaf matched
            Arch::X86_64 => emitter.instruction(&format!("jmp {}", miss)),      // no class of this leaf matched
        }
        return;
    }
    let mid = lo + (hi - lo) / 2;
    let lower = format!("{}_below_{}{}", switch.stem, ids[mid], switch.suffix);
    (switch.load_id)(emitter, id_reg);
    abi::emit_load_int_immediate(emitter, imm_reg, ids[mid] as i64);
    emit_compare(emitter, id_reg, imm_reg);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b.lo {}", lower)),        // the id sorts below this pivot: search the lower half
        Arch::X86_64 => emitter.instruction(&format!("jb {}", lower)),           // the id sorts below this pivot: search the lower half
    }
    emit_node(emitter, switch, ids, mid, hi, emit_case, miss);
    emitter.label(&lower);
    emit_node(emitter, switch, ids, lo, mid, emit_case, miss);
}

/// Compares the loaded class id with the case immediate.
fn emit_compare(emitter: &mut Emitter, id_reg: &str, imm_reg: &str) {
    emitter.instruction(&format!("cmp {}, {}", id_reg, imm_reg));               // compare the receiver class id with this case
}
