//! Purpose:
//! Checks frame storage for checked arithmetic selected by `min()` / `max()` and for a numeric
//! local written by a selection and printed afterwards.
//!
//! Called from:
//! - The library test harness through `codegen::stack_numeric`.
//!
//! Key details:
//! - A selection over numeric operands observes them and produces a numeric cell itself.
//! - A nonnumeric operand makes the selection an unknown consumer, so nothing keeps a cell.
//! - A direct `echo` of a numeric local is an observation; returning the pointer is not.

use super::*;
use crate::ir::{Builder, MinMaxRule, Terminator};

/// Builds `cast(mixed_min_max(counter, checked))` with a numeric or boolean left operand.
fn selection(rule: MinMaxRule, numeric: bool) -> (Function, ValueId, ValueId) {
    let mut function = Function::new("selected_numeric".into(), IrType::F64, PhpType::Float);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let one = builder.emit_const_i64(1);
    let checked = builder.emit(Op::ICheckedSub, vec![one, one], None,
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    let left = if numeric { one } else { builder.emit_const_bool(true) };
    let selected = builder.emit(Op::MixedMinMax, vec![left, checked],
        Some(Immediate::I64(rule.immediate())),
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    builder.emit(Op::Release, vec![checked], None, IrType::Void, PhpType::Void,
        Ownership::NonHeap);
    let result = builder.emit(Op::Cast, vec![selected], Some(Immediate::CastTarget(IrType::F64)),
        IrType::F64, PhpType::Float, Ownership::NonHeap).unwrap();
    builder.emit(Op::Release, vec![selected], None, IrType::Void, PhpType::Void,
        Ownership::NonHeap);
    builder.terminate(Terminator::Return { value: Some(result) });
    crate::ir::validate_function(&function).unwrap();
    (function, checked, selected)
}

/// Every rule keeps both the checked operand and the selected result in frame cells.
#[test]
fn stack_numeric_min_max_selection_keeps_operand_and_result_cells() {
    for rule in [MinMaxRule::MaxPair, MinMaxRule::MaxFold, MinMaxRule::MinPair, MinMaxRule::MinFold] {
        let (function, checked, selected) = selection(rule, true);
        assert_eq!(plan(&function).cells, vec![checked, selected], "{rule:?}");
    }
}

/// A boolean operand can win the selection, so neither value may use a numeric cell.
#[test]
fn stack_numeric_min_max_selection_rejects_nonnumeric_operands() {
    let (function, _, _) = selection(MinMaxRule::MaxPair, false);
    assert!(plan(&function).cells.is_empty());
}

/// Builds a numeric local updated by a selection and then echoed or returned.
fn printed_local(escape: bool) -> (Function, LocalSlotId) {
    let mut function = Function::new("printed_numeric".into(), IrType::Void, PhpType::Void);
    let slot = function.add_local(Some("best".into()), IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed, LocalKind::PhpLocal);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let seed = builder.emit_const_f64(0.5);
    builder.emit_store_local(slot, seed);
    let old = builder.emit_load_local(slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
    let one = builder.emit_const_i64(1);
    let selected = builder.emit(Op::MixedMinMax, vec![old, one],
        Some(Immediate::I64(MinMaxRule::MaxPair.immediate())),
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    builder.emit(Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(slot)), IrType::Void,
        PhpType::Void, Ownership::NonHeap);
    builder.emit_store_local(slot, selected);
    let current = builder.emit_load_local(slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
    builder.emit(Op::EchoValue, vec![current], None, IrType::Void, PhpType::Void,
        Ownership::NonHeap);
    if escape {
        builder.terminate(Terminator::Return { value: Some(current) });
        function.return_type = IrType::Heap(IrHeapKind::Mixed);
        function.return_php_type = PhpType::Mixed;
    } else {
        builder.terminate(Terminator::Return { value: None });
    }
    (function, slot)
}

/// Echoing a numeric local keeps its frame storage; returning its pointer does not.
#[test]
fn stack_numeric_echo_observes_a_selected_numeric_local() {
    let (function, slot) = printed_local(false);
    crate::ir::validate_function(&function).unwrap();
    assert!(plan(&function).locals.contains(&slot));
    let (function, slot) = printed_local(true);
    assert!(!plan(&function).locals.contains(&slot));
}
