//! Purpose:
//! Tests for the peephole's scalar box elision: dead scalar boxes and inline `?int` boxes that
//! only feed ordering comparisons.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are built with `crate::ir::Builder` and re-validated after the pass.
//! - Negative cases keep a box that is stored, or whose comparison partner would lose its
//!   runtime-tagged operand.

use crate::ir::{
    validate_function, Builder, CmpPredicate, DataPool, Function, Immediate, IrHeapKind,
    IrType, LocalKind, Op, Ownership, Terminator, ValueId,
};
use crate::ir_passes::driver::IrPass;
use crate::ir_passes::peephole::Peephole;
use crate::types::PhpType;

/// Shape of the one use a test gives the box besides its release.
#[derive(Clone, Copy)]
enum BoxUse {
    /// Only the box's own release.
    ReleaseOnly,
    /// One `php_rel_cmp` against an integer, then the release.
    Compare,
    /// A store into a `mixed` local, then the release.
    Store,
}

/// Builds `box(source)` with the requested use and returns the function, the box result, and
/// the instruction indexes of the box and of the comparison (when there is one).
fn boxed_function(tagged: bool, use_kind: BoxUse) -> (Function, ValueId, usize, Option<usize>) {
    let mut function = Function::new("tagged_box".to_string(), IrType::I64, PhpType::Int);
    let mut compare = None;
    let boxed;
    {
        let mut builder = Builder::new(&mut function);
        let tagged_slot = builder.add_local(
            Some("key".into()), IrType::TaggedScalar, PhpType::TaggedScalar, LocalKind::PhpLocal,
        );
        let mixed_slot = builder.add_local(
            Some("copy".into()), IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let source = if tagged {
            builder.emit_load_local(tagged_slot, IrType::TaggedScalar, PhpType::TaggedScalar)
        } else {
            builder.emit_const_i64(7)
        };
        let other = builder.emit_const_i64(3);
        boxed = builder
            .emit(
                Op::MixedBox, vec![source], None,
                IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned,
            )
            .expect("box result");
        let mut result = other;
        match use_kind {
            BoxUse::ReleaseOnly => {}
            BoxUse::Compare => {
                compare = Some(builder.function().instructions.len());
                result = builder
                    .emit(
                        Op::PhpRelCmp, vec![other, boxed],
                        Some(Immediate::CmpPredicate(CmpPredicate::Sgt)),
                        IrType::I64, PhpType::Bool, Ownership::NonHeap,
                    )
                    .expect("comparison result");
            }
            BoxUse::Store => builder.emit_store_local(mixed_slot, boxed),
        }
        builder.emit(Op::Release, vec![boxed], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
        builder.terminate(Terminator::Return { value: Some(result) });
    }
    let box_index = function
        .instructions
        .iter()
        .position(|inst| inst.op == Op::MixedBox)
        .expect("box instruction");
    (function, boxed, box_index, compare)
}

/// Returns how many live instructions of `op` remain.
fn count(function: &Function, op: Op) -> usize {
    function.instructions.iter().filter(|inst| inst.op == op).count()
}

/// A scalar box nothing reads is allocate-then-free: the box and its release disappear.
#[test]
fn dead_scalar_box_and_its_release_are_removed() {
    for tagged in [false, true] {
        let (mut function, _, box_index, _) = boxed_function(tagged, BoxUse::ReleaseOnly);
        assert!(Peephole.run(&mut function, &mut DataPool::default()));
        assert_eq!(function.instructions[box_index].op, Op::Nop, "the dead box is neutralized");
        assert_eq!(count(&function, Op::Release), 0, "its release goes with it");
        assert!(validate_function(&function).is_ok());
    }
}

/// An inline `?int` compared through PHP ordering needs no heap cell: the comparison reads the
/// tagged value itself.
#[test]
fn tagged_scalar_box_feeding_an_ordering_compare_is_redirected() {
    let (mut function, boxed, box_index, compare) = boxed_function(true, BoxUse::Compare);
    assert!(Peephole.run(&mut function, &mut DataPool::default()));
    assert_eq!(function.instructions[box_index].op, Op::Nop);
    assert_eq!(count(&function, Op::Release), 0);
    let compare = &function.instructions[compare.expect("comparison index")];
    assert!(!compare.operands.contains(&boxed), "the comparison no longer reads the box");
    assert_eq!(
        function.value(compare.operands[1]).map(|value| value.ir_type),
        Some(IrType::TaggedScalar),
        "the comparison reads the inline tagged scalar"
    );
    assert!(validate_function(&function).is_ok());
}

/// A stored box is a real `mixed` value, and an integer box is the comparison's only tagged
/// operand: both keep their heap cell.
#[test]
fn stored_and_plain_integer_compared_boxes_are_kept() {
    for (tagged, use_kind) in [(true, BoxUse::Store), (false, BoxUse::Compare)] {
        let (mut function, _, box_index, _) = boxed_function(tagged, use_kind);
        Peephole.run(&mut function, &mut DataPool::default());
        assert_eq!(function.instructions[box_index].op, Op::MixedBox);
        assert_eq!(count(&function, Op::Release), 1);
        assert!(validate_function(&function).is_ok());
    }
}
