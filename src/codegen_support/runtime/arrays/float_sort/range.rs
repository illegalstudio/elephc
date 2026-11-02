//! Purpose:
//! Emits the float sort's partition recursion and bounded insertion scans.
//!
//! Called from:
//! - `super::emit_float_sort()`; generated calls enter from `__rt_sort_float`.
//!
//! Key details:
//! - Comparison scheduling follows PHP's Zend sort, including NAN-sensitive operand order.
//!   Reference: <https://github.com/php/php-src/blob/PHP-8.5/Zend/zend_sort.c>.
//! - Only the smaller partition recurses, keeping stack depth logarithmic.
//! - All cursors have explicit bounds even when comparisons are inconsistent.

use super::asm::{Asm, Cond};
use crate::codegen_support::abi;

/// Passes the caller's five-word vector to the small-position sorter.
fn small(a: &mut Asm<'_>, count: i64) {
    a.load(0, 8);
    let reg = a.reg(1);
    abi::emit_frame_slot_address(a.emitter, reg, 160);
    a.imm(2, count);
    abi::emit_call_label(a.emitter, "__rt_float_sort_small");
}

/// Sorts `(context, start, length)` with median sampling and a bounded partition scan.
pub(super) fn emit_range(a: &mut Asm<'_>) {
    a.begin("__rt_float_sort_range");
    a.save(8, 0);
    a.save(16, 1);
    a.save(24, 2);
    a.label("__rt_float_sort_range_again");
    a.load(2, 24);
    a.cmp_imm(2, 2, Cond::Lt, "__rt_float_sort_range_done");
    a.cmp_imm(2, 16, Cond::Le, "__rt_float_sort_range_insert");
    // -- sample the first, middle and last positions, or five positions for large ranges --
    a.load(1, 16);
    a.combine(1, 2, false);
    a.save(32, 1);
    a.load(0, 24);
    a.shift(0, 1, false);
    a.load(1, 16);
    a.combine(0, 1, false);
    a.save(40, 0);
    a.save(160, 1);
    a.cmp_imm(2, 1024, Cond::Ge, "__rt_float_sort_range_five");
    a.save(152, 0);
    a.load(0, 32);
    a.offset(0, -1);
    a.save(144, 0);
    small(a, 3);
    a.jump("__rt_float_sort_range_sampled");
    a.label("__rt_float_sort_range_five");
    a.load(0, 24);
    a.shift(0, 2, false);
    a.save(88, 0);
    a.combine(1, 0, false);
    a.save(152, 1);
    a.load(1, 40);
    a.save(144, 1);
    a.combine(1, 0, false);
    a.save(136, 1);
    a.load(1, 32);
    a.offset(1, -1);
    a.save(128, 1);
    small(a, 5);
    a.label("__rt_float_sort_range_sampled");
    a.load(1, 16);
    a.offset(1, 1);
    a.save(48, 1);
    a.call("__rt_float_sort_swap", &[8, 48, 40]);
    a.load(1, 48);
    a.offset(1, 1);
    a.save(56, 1);
    a.load(1, 32);
    a.offset(1, -1);
    a.save(64, 1);
    // -- scan inwards, checking cursor convergence after every advance --
    a.label("__rt_float_sort_range_left_scan");
    a.call("__rt_float_sort_compare", &[8, 48, 56]);
    a.cmp_result(Cond::Le, "__rt_float_sort_range_right_start");
    a.load(1, 56);
    a.offset(1, 1);
    a.save(56, 1);
    a.load(2, 64);
    a.cmp(1, 2, Cond::Eq, "__rt_float_sort_range_partitioned");
    a.jump("__rt_float_sort_range_left_scan");
    a.label("__rt_float_sort_range_right_start");
    a.load(1, 64);
    a.offset(1, -1);
    a.save(64, 1);
    a.load(2, 56);
    a.cmp(1, 2, Cond::Eq, "__rt_float_sort_range_partitioned");
    a.label("__rt_float_sort_range_right_scan");
    a.call("__rt_float_sort_compare", &[8, 64, 48]);
    a.cmp_result(Cond::Le, "__rt_float_sort_range_exchange");
    a.jump("__rt_float_sort_range_right_start");
    a.label("__rt_float_sort_range_exchange");
    a.call("__rt_float_sort_swap", &[8, 56, 64]);
    a.load(1, 56);
    a.offset(1, 1);
    a.save(56, 1);
    a.load(2, 64);
    a.cmp(1, 2, Cond::Ne, "__rt_float_sort_range_left_scan");
    // -- place the pivot and recurse only into the smaller partition --
    a.label("__rt_float_sort_range_partitioned");
    a.load(2, 56);
    a.offset(2, -1);
    a.save(40, 2);
    a.call("__rt_float_sort_swap", &[8, 48, 40]);
    a.load(1, 40);
    a.load(2, 16);
    a.combine(1, 2, true);
    a.save(72, 1);
    a.load(2, 32);
    a.load(3, 56);
    a.combine(2, 3, true);
    a.save(80, 2);
    a.cmp(1, 2, Cond::Lt, "__rt_float_sort_range_recurse_left");
    a.call("__rt_float_sort_range", &[8, 56, 80]);
    a.load(2, 72);
    a.save(24, 2);
    a.jump("__rt_float_sort_range_again");
    a.label("__rt_float_sort_range_recurse_left");
    a.call("__rt_float_sort_range", &[8, 16, 72]);
    a.load(1, 56);
    a.save(16, 1);
    a.load(2, 80);
    a.save(24, 2);
    a.jump("__rt_float_sort_range_again");
    a.label("__rt_float_sort_range_insert");
    a.call("__rt_float_sort_insert", &[8, 16, 24]);
    a.label("__rt_float_sort_range_done");
    a.ret();
}

/// Advances an insertion cursor stored in a frame slot.
fn advance(a: &mut Asm<'_>, slot: usize, delta: i64) {
    a.load(1, slot);
    a.offset(1, delta);
    a.save(slot, 1);
}

/// Rotates the pending element left to its selected insertion position.
fn rotate(a: &mut Asm<'_>, prefix: &str, next: &str) {
    a.load(1, 40);
    a.save(56, 1);
    a.label(&format!("{prefix}_rotate"));
    a.load(1, 56);
    a.load(2, 48);
    a.cmp(1, 2, Cond::Le, next);
    a.offset(1, -1);
    a.save(64, 1);
    a.call("__rt_float_sort_swap", &[8, 56, 64]);
    advance(a, 56, -1);
    a.jump(&format!("{prefix}_rotate"));
}

/// Inserts a short range using PHP's six-element sentinel and two-at-a-time backward scan.
pub(super) fn emit_insert(a: &mut Asm<'_>) {
    a.begin("__rt_float_sort_insert");
    a.save(8, 0);
    a.save(16, 1);
    a.save(24, 2);
    a.cmp_imm(2, 5, Cond::Gt, "__rt_float_sort_insert_six");
    for position in 0..5 {
        a.save(160 - position * 8, 1);
        a.offset(1, 1);
    }
    a.load(0, 8);
    let reg = a.reg(1);
    abi::emit_frame_slot_address(a.emitter, reg, 160);
    a.load(2, 24);
    abi::emit_call_label(a.emitter, "__rt_float_sort_small");
    a.jump("__rt_float_sort_insert_done");
    // -- establish the first six positions with a bounded single-step scan --
    a.label("__rt_float_sort_insert_six");
    a.load(1, 16);
    a.combine(1, 2, false);
    a.save(32, 1);
    a.load(1, 16);
    a.offset(1, 6);
    a.save(72, 1);
    a.load(1, 16);
    a.offset(1, 1);
    a.save(40, 1);
    a.label("__rt_float_sort_insert_first");
    a.load(1, 40);
    a.offset(1, -1);
    a.save(48, 1);
    a.call("__rt_float_sort_compare", &[8, 48, 40]);
    a.cmp_result(Cond::Le, "__rt_float_sort_insert_first_next");
    a.label("__rt_float_sort_insert_first_back");
    a.load(1, 48);
    a.load(2, 16);
    a.cmp(1, 2, Cond::Eq, "__rt_float_sort_insert_first_move");
    advance(a, 48, -1);
    a.call("__rt_float_sort_compare", &[8, 48, 40]);
    a.cmp_result(Cond::Gt, "__rt_float_sort_insert_first_back");
    advance(a, 48, 1);
    a.label("__rt_float_sort_insert_first_move");
    rotate(a, "__rt_float_sort_insert_first", "__rt_float_sort_insert_first_next");
    a.label("__rt_float_sort_insert_first_next");
    advance(a, 40, 1);
    a.load(1, 40);
    a.load(2, 72);
    a.cmp(1, 2, Cond::Lt, "__rt_float_sort_insert_first");
    // -- continue with Zend's two-step probe and its reversed boundary comparison --
    a.label("__rt_float_sort_insert_rest");
    a.load(1, 40);
    a.load(2, 32);
    a.cmp(1, 2, Cond::Ge, "__rt_float_sort_insert_done");
    a.offset(1, -1);
    a.save(48, 1);
    a.call("__rt_float_sort_compare", &[8, 48, 40]);
    a.cmp_result(Cond::Le, "__rt_float_sort_insert_rest_next");
    a.label("__rt_float_sort_insert_rest_back");
    advance(a, 48, -2);
    a.call("__rt_float_sort_compare", &[8, 48, 40]);
    a.cmp_result(Cond::Le, "__rt_float_sort_insert_rest_forward");
    a.load(1, 48);
    a.load(2, 16);
    a.cmp(1, 2, Cond::Eq, "__rt_float_sort_insert_rest_move");
    a.offset(2, 1);
    a.cmp(1, 2, Cond::Ne, "__rt_float_sort_insert_rest_back");
    advance(a, 48, -1);
    a.call("__rt_float_sort_compare", &[8, 40, 48]);
    a.cmp_result(Cond::Le, "__rt_float_sort_insert_rest_move");
    advance(a, 48, 1);
    a.jump("__rt_float_sort_insert_rest_move");
    a.label("__rt_float_sort_insert_rest_forward");
    advance(a, 48, 1);
    a.call("__rt_float_sort_compare", &[8, 48, 40]);
    a.cmp_result(Cond::Gt, "__rt_float_sort_insert_rest_move");
    advance(a, 48, 1);
    a.label("__rt_float_sort_insert_rest_move");
    rotate(a, "__rt_float_sort_insert_rest", "__rt_float_sort_insert_rest_next");
    a.label("__rt_float_sort_insert_rest_next");
    advance(a, 40, 1);
    a.jump("__rt_float_sort_insert_rest");
    a.label("__rt_float_sort_insert_done");
    a.ret();
}
