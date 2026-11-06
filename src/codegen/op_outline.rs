//! Purpose:
//! Subroutine threading for repeated code: an IR instruction whose emitted text already exists
//! elsewhere becomes a `bl` to one shared routine. Still native code, with no bytecode and no
//! dispatch loop: the routine is the same instructions, called.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_block`, after each instruction is lowered.
//! - `emit_pending_op_routines`, between bodies, for the routines those calls need.
//!
//! Key details:
//! - The test is textual and therefore sound, as in `lower_inst::arm_dedup`: frame slots stay
//!   `x29`-relative and `x29` does not move, so identical text means the same behaviour.
//! - An instruction is only shared when it stands alone: every branch stays inside it, and it
//!   names no local label of its body. Labels it defines are renamed per routine.
//! - A routine that calls something saves `x30` on the stack, so a sequence reading `sp` slots
//!   of its frame stays inline: the push would move them.
//! - The first occurrence stays inline, since whether a second one exists is not known yet. The
//!   second builds the routine and every later one calls it.
//! - Each worker emits the routines it needs between `HELPER_MARKER`s and the merge keeps one.
//! - AArch64 only, for now, and off unless `ELEPHC_OUTLINE_OPS` is set.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

use super::context::FunctionContext;
use super::lower_inst::arm_dedup::normalized_arm_text;

/// Below this, `bl` plus the routine's own `ret` leave too little to gain.
const MIN_INSTRUCTIONS: usize = 6;

/// One worker's view of the instructions it has seen.
#[derive(Default)]
pub(crate) struct OpOutlineState {
    seen: HashMap<u64, u32>,
    requested: HashSet<u64>,
    pending: Vec<(u64, String)>,
}

/// Whether this build shares instructions: an experiment until parity is measured with it on.
pub(super) fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("ELEPHC_OUTLINE_OPS").is_some())
}

/// What a sequence needs to run as a routine, or `None` when it must stay inline.
struct Shape {
    /// The sequence calls something, so the routine saves its own return address.
    calls: bool,
    instructions: usize,
}

/// Splits an operand list on everything that cannot be part of a register or symbol.
fn tokens(line: &str) -> impl Iterator<Item = &str> {
    line.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '.')))
        .filter(|token| !token.is_empty())
}

/// Decides whether the text of one instruction can leave its body.
fn shape_of(text: &str) -> Option<Shape> {
    let mut defined = HashSet::new();
    for line in text.lines().map(str::trim) {
        if let Some(name) = line.strip_suffix(':') {
            defined.insert(name);
        }
    }
    let mut instructions = 0;
    let mut calls = false;
    let mut reads_sp = false;
    for line in text.lines().map(str::trim) {
        let code = line.split("//").next().unwrap_or("").trim();
        if code.is_empty() || code.ends_with(':') || code.starts_with([';', '#', '@']) {
            continue;
        }
        if code.starts_with('.') {
            return None;
        }
        instructions += 1;
        let mnemonic = code.split_whitespace().next().unwrap_or("");
        match mnemonic {
            "ret" | "br" => return None,
            "bl" | "blr" => calls = true,
            _ => {}
        }
        for token in tokens(code).skip(1) {
            match token {
                "sp" => reads_sp = true,
                "x30" | "lr" => return None,
                _ => {}
            }
            if token.starts_with('L') && !defined.contains(token) {
                return None;
            }
        }
        let is_branch = matches!(mnemonic, "b" | "cbz" | "cbnz" | "tbz" | "tbnz") || mnemonic.starts_with("b.");
        if is_branch && !tokens(code).last().is_some_and(|target| defined.contains(target)) {
            return None;
        }
    }
    (instructions >= MIN_INSTRUCTIONS && !(calls && reads_sp)).then_some(Shape { calls, instructions })
}

/// The routine for a sequence: its labels made its own, its return address kept when it calls.
fn routine_text(label: &str, hash: u64, normalized: &str, calls: bool) -> String {
    // Only `@<n>` is a renamed label: `@PAGE` and `@PAGEOFF` stay relocations.
    let prefix = format!("L_outl_{hash:016x}_");
    let mut body = String::with_capacity(normalized.len());
    let mut rest = normalized;
    while let Some(at) = rest.find('@') {
        body.push_str(&rest[..at]);
        let digit = rest[at + 1..].starts_with(|c: char| c.is_ascii_digit());
        body.push_str(if digit { prefix.as_str() } else { "@" });
        rest = &rest[at + 1..];
    }
    body.push_str(rest);
    let mut out = format!(".align 2\n.globl {label}\n{label}:\n");
    if calls {
        out.push_str("str x30, [sp, #-16]!\n");
    }
    out.push_str(&body);
    if !body.ends_with('\n') {
        out.push('\n');
    }
    if calls {
        out.push_str("ldr x30, [sp], #16\n");
    }
    out.push_str("ret\n");
    out
}

/// Replaces the instruction emitted since `mark` with a call, when its text has been seen before.
pub(super) fn outline_instruction(ctx: &mut FunctionContext<'_>, mark: usize) {
    if ctx.emitter.target.arch != Arch::AArch64 {
        return;
    }
    let text = ctx.emitter.text_since(mark);
    let Some(shape) = shape_of(text) else {
        return;
    };
    debug_assert!(shape.instructions >= MIN_INSTRUCTIONS);
    let normalized = normalized_arm_text(text);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized.hash(&mut hasher);
    let hash = hasher.finish();
    let state = ctx.shared.op_outline_state();
    let occurrences = state.seen.entry(hash).or_insert(0);
    *occurrences += 1;
    if *occurrences < 2 {
        return;
    }
    let label = format!("_eir_outl_{hash:016x}");
    if state.requested.insert(hash) {
        let routine = routine_text(&label, hash, &normalized, shape.calls);
        state.pending.push((hash, routine));
    }
    ctx.emitter.rollback_to(mark);
    ctx.emitter.instruction(&format!("bl {label}"));
}

/// Emits the routines requested since the last drain, each between the markers the merge reads.
pub(super) fn emit_pending_op_routines(emitter: &mut Emitter, state: &mut OpOutlineState) {
    use super::context::{HELPER_MARKER_CLOSE, HELPER_MARKER_OPEN};
    for (hash, routine) in std::mem::take(&mut state.pending) {
        emitter.comment(&format!("{}{:012x}", HELPER_MARKER_OPEN, hash & 0xffff_ffff_ffff));
        emitter.raw(&routine);
        emitter.comment(HELPER_MARKER_CLOSE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEAF: &str = "ldr x9, [x29, #-8]\nadd x9, x9, #1\nstr x9, [x29, #-8]\nldr x10, [x29, #-16]\nadd x10, x10, x9\nstr x10, [x29, #-16]\n";

    /// Verifies a self-contained sequence qualifies, and a short one does not.
    #[test]
    fn a_self_contained_sequence_qualifies() {
        assert!(shape_of(LEAF).is_some_and(|shape| !shape.calls));
        assert!(shape_of("ldr x9, [x29, #-8]\nadd x9, x9, #1\n").is_none());
    }

    /// Verifies anything that leaves the sequence keeps it inline.
    #[test]
    fn a_sequence_that_leaves_stays_inline() {
        assert!(shape_of(&format!("{LEAF}b Lz123\n")).is_none());
        assert!(shape_of(&format!("{LEAF}cbz x9, Lz123\n")).is_none());
        assert!(shape_of(&format!("{LEAF}b __rt_throw_current\n")).is_none());
        assert!(shape_of(&format!("{LEAF}cbnz x9, __rt_throw_current\n")).is_none());
        assert!(shape_of(&format!("{LEAF}ret\n")).is_none());
        assert!(shape_of(&format!("{LEAF}adrp x9, Lz123@PAGE\n")).is_none());
    }

    /// Verifies a branch inside the sequence is allowed.
    #[test]
    fn an_inner_branch_qualifies() {
        assert!(shape_of(&format!("cbz x9, Lz1\n{LEAF}Lz1:\n")).is_some());
    }

    /// Verifies a call next to `sp` slots stays inline, since the saved `x30` would move them.
    #[test]
    fn a_call_beside_stack_slots_stays_inline() {
        assert!(shape_of(&format!("{LEAF}bl __rt_x\n")).is_some_and(|shape| shape.calls));
        assert!(shape_of(&format!("{LEAF}ldr x9, [sp, #8]\n")).is_some());
        assert!(shape_of(&format!("{LEAF}bl __rt_x\nldr x9, [sp, #8]\n")).is_none());
    }

    /// Verifies a calling routine keeps its return address around the body.
    #[test]
    fn a_calling_routine_saves_its_return_address() {
        let routine = routine_text("_r", 1, "bl __rt_x\n", true);
        assert!(routine.contains("str x30, [sp, #-16]!\nbl __rt_x\nldr x30, [sp], #16\nret\n"));
        assert!(!routine_text("_r", 1, "add x9, x9, #1\n", false).contains("x30"));
    }

    /// Verifies labels are renamed and relocation operators are left alone.
    #[test]
    fn a_routine_renames_labels_and_keeps_relocations() {
        let routine = routine_text("_r", 1, "adrp x9, _s@PAGE\nb @0\n@0:\n", false);
        assert!(routine.contains("adrp x9, _s@PAGE\n"));
        assert!(routine.contains("b L_outl_0000000000000001_0\n"));
        assert!(routine.contains("L_outl_0000000000000001_0:\n"));
    }
}
