//! Purpose:
//! Emits PHP's comparison schedule for two through five selected float positions.
//!
//! Called from:
//! - The float-sort range and insertion helpers, including pivot selection.
//!
//! Key details:
//! - Comparison direction and short-circuit edges are observable when a value is NAN.
//! - The position vector is borrowed from the caller's frame and never escapes.

use super::asm::{Asm, Cond};
use crate::codegen_support::abi;

/// Compares two positions from the caller's vector and branches when they are ordered.
fn compare(a: &mut Asm<'_>, lhs: usize, rhs: usize, label: &str) {
    a.load(0, 8);
    a.load(3, 16);
    a.read(1, 3, lhs * 8);
    a.read(2, 3, rhs * 8);
    abi::emit_call_label(a.emitter, "__rt_float_sort_compare");
    a.cmp_result(Cond::Le, label);
}

/// Swaps two positions from the caller's small position vector.
fn swap(a: &mut Asm<'_>, lhs: usize, rhs: usize) {
    a.load(0, 8);
    a.load(3, 16);
    a.read(1, 3, lhs * 8);
    a.read(2, 3, rhs * 8);
    abi::emit_call_label(a.emitter, "__rt_float_sort_swap");
}

/// Sorts `(context, position-vector, count)` with the two-to-five-element PHP schedule.
pub(super) fn emit_small(a: &mut Asm<'_>) {
    a.begin("__rt_float_sort_small");
    a.save(8, 0);
    a.save(16, 1);
    a.save(24, 2);
    a.cmp_imm(2, 2, Cond::Ne, "__rt_float_sort_small_three");
    compare(a, 0, 1, "__rt_float_sort_small_done");
    swap(a, 0, 1);
    a.jump("__rt_float_sort_small_done");
    // -- the three-element schedule intentionally compares c to b in its second arm --
    a.label("__rt_float_sort_small_three");
    compare(a, 0, 1, "__rt_float_sort_small_ab_ordered");
    compare(a, 2, 1, "__rt_float_sort_small_swap_ac");
    swap(a, 0, 1);
    compare(a, 1, 2, "__rt_float_sort_small_extend");
    swap(a, 1, 2);
    a.jump("__rt_float_sort_small_extend");
    a.label("__rt_float_sort_small_swap_ac");
    swap(a, 0, 2);
    a.jump("__rt_float_sort_small_extend");
    a.label("__rt_float_sort_small_ab_ordered");
    compare(a, 1, 2, "__rt_float_sort_small_extend");
    swap(a, 1, 2);
    compare(a, 0, 1, "__rt_float_sort_small_extend");
    swap(a, 0, 1);
    // -- insert the fourth and fifth sampled positions, stopping at the first ordered pair --
    a.label("__rt_float_sort_small_extend");
    for last in 3..=4 {
        a.load(2, 24);
        a.cmp_imm(2, last as i64, Cond::Le, "__rt_float_sort_small_done");
        let next = format!("__rt_float_sort_small_after_{last}");
        for right in (1..=last).rev() {
            compare(a, right - 1, right, &next);
            swap(a, right - 1, right);
        }
        a.label(&next);
    }
    a.label("__rt_float_sort_small_done");
    a.ret();
}
