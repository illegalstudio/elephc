//! Purpose:
//! Emits PHP-compatible sorting for raw float and boxed scalar array slots.
//!
//! Called from:
//! - `runtime::emitters::managed` and the typed array sort backend.
//!
//! Key details:
//! - Matches Zend's comparison schedule, including its small-array and pivot cases.
//! - A temporary ordinal sidecar preserves equal-element stability without boxing floats.
//! - Comparisons use the shared PHP comparator, and every target uses the same sort graph.

mod asm;
mod range;
mod small;
#[cfg(test)]
mod tests;

use asm::{Asm, Cond};
use crate::codegen_support::{abi, emit::Emitter};

/// Emits the float-sort entry point and its private comparison and partition helpers.
pub fn emit_float_sort(emitter: &mut Emitter) {
    let mut a = Asm { emitter };
    emit_entry(&mut a, "__rt_sort_float", "__rt_float_compare_slots", "__rt_float_compare_slots_desc");
    emit_entry(&mut a, "__rt_sort_mixed", "__rt_php_compare_slots", "__rt_php_compare_slots_desc");
    emit_compare(&mut a);
    emit_swap(&mut a);
    small::emit_small(&mut a);
    range::emit_range(&mut a);
    range::emit_insert(&mut a);
}

/// Sorts `(array, descending)` while keeping temporary original positions outside the PHP heap.
fn emit_entry(a: &mut Asm<'_>, label: &str, ascending: &str, descending: &str) {
    a.begin(label);
    // -- preserve the receiver and avoid allocating for trivial arrays --
    a.save(8, 0);
    a.cmp_imm(1, 0, Cond::Eq, &format!("{label}_ascending"));
    let comparator = a.reg(3);
    abi::emit_symbol_address(a.emitter, comparator, descending);
    a.jump(&format!("{label}_comparator"));
    a.label(&format!("{label}_ascending"));
    abi::emit_symbol_address(a.emitter, comparator, ascending);
    a.label(&format!("{label}_comparator"));
    a.save(64, 3);
    a.read(1, 0, 0);
    a.save(16, 1);
    a.cmp_imm(1, 2, Cond::Lt, &format!("{label}_done"));
    a.load(0, 16);
    a.shift(0, 3, true);
    a.emitter.bl_c("malloc");                                                   // allocate one ordinal sidecar, without allocating boxed values
    a.save_result(32);
    a.cmp_result(Cond::Ne, &format!("{label}_allocated"));
    a.jump("__rt_heap_allocation_failed");
    a.label(&format!("{label}_allocated"));
    // -- build the shared context and initialize each element's original ordinal --
    a.load(0, 8);
    a.offset(0, 24);
    a.save(80, 0);
    a.load(0, 32);
    a.save(72, 0);
    a.load(0, 32);
    a.imm(1, 0);
    a.label(&format!("{label}_ordinals"));
    a.index(2, 0, 1);
    a.write(1, 2, 0);
    a.offset(1, 1);
    a.load(3, 16);
    a.cmp(1, 3, Cond::Lt, &format!("{label}_ordinals"));
    let reg = a.reg(0);
    abi::emit_frame_slot_address(a.emitter, reg, 80);
    a.imm(1, 0);
    a.load(2, 16);
    abi::emit_call_label(a.emitter, "__rt_float_sort_range");
    // -- dispose the sidecar after every element has reached its final position --
    a.load(0, 32);
    a.emitter.bl_c("free");                                                     // release the independently allocated original-position sidecar
    a.label(&format!("{label}_done"));
    abi::emit_load_int_immediate(a.emitter, abi::int_result_reg(a.emitter), 1);
    a.ret();
}

/// Compares two positions, using original ordinals only when PHP reports equality.
fn emit_compare(a: &mut Asm<'_>) {
    a.begin("__rt_float_sort_compare");
    a.save(8, 0);
    a.save(16, 1);
    a.save(24, 2);
    // -- compare the two raw payloads through the shared scalar ordering --
    a.read(3, 0, 0);
    a.index(4, 3, 1);
    a.read(0, 4, 0);
    a.index(4, 3, 2);
    a.read(1, 4, 0);
    a.load(3, 8);
    a.read(3, 3, 16);
    let comparator = a.reg(3);
    abi::emit_call_reg(a.emitter, comparator);
    a.cmp_result(Cond::Ne, "__rt_float_sort_compare_return");
    // -- equality remains stable in both directions, including signed zeroes --
    a.load(0, 8);
    a.read(3, 0, 8);
    a.load(1, 16);
    a.load(2, 24);
    a.index(4, 3, 1);
    a.read(1, 4, 0);
    a.index(4, 3, 2);
    a.read(2, 4, 0);
    a.cmp(1, 2, Cond::Lt, "__rt_float_sort_compare_less");
    a.cmp(1, 2, Cond::Gt, "__rt_float_sort_compare_greater");
    abi::emit_load_int_immediate(a.emitter, abi::int_result_reg(a.emitter), 0);
    a.jump("__rt_float_sort_compare_return");
    a.label("__rt_float_sort_compare_less");
    abi::emit_load_int_immediate(a.emitter, abi::int_result_reg(a.emitter), -1);
    a.jump("__rt_float_sort_compare_return");
    a.label("__rt_float_sort_compare_greater");
    abi::emit_load_int_immediate(a.emitter, abi::int_result_reg(a.emitter), 1);
    a.label("__rt_float_sort_compare_return");
    a.ret();
}

/// Swaps two payloads and their original ordinals as a single logical element.
fn emit_swap(a: &mut Asm<'_>) {
    a.emitter.blank();
    a.emitter.label_global("__rt_float_sort_swap");
    // -- exchange the raw float words without retaining or releasing either one --
    a.read(3, 0, 0);
    a.index(4, 3, 1);
    a.index(5, 3, 2);
    a.read(1, 4, 0);
    a.read(2, 5, 0);
    a.write(2, 4, 0);
    a.write(1, 5, 0);
    // -- map the same addresses into the parallel ordinal allocation --
    a.read(0, 0, 8);
    a.combine(4, 3, true);
    a.combine(5, 3, true);
    a.combine(4, 0, false);
    a.combine(5, 0, false);
    a.read(1, 4, 0);
    a.read(2, 5, 0);
    a.write(2, 4, 0);
    a.write(1, 5, 0);
    abi::emit_return(a.emitter);
}
