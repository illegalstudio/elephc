//! Purpose:
//! Checks frame storage for checked arithmetic selected by `min()` / `max()` and for a numeric
//! local written by a selection and printed afterwards.
//!
//! Called from:
//! - The library test harness through `codegen::stack_numeric`.
//!
//! Key details:
//! - A selection over numeric operands observes them and produces a numeric cell itself.
//! - A nonnumeric operand makes the selection an unknown consumer, so nothing keeps a numeric
//!   cell; a selection that is only read in its block gets a guarded cell instead.
//! - The assignment copy of a numeric selection observes it without boxing it.
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
    // A fold rule selects among three or more operands, a pair rule between exactly two.
    let operands = if rule.is_pair() { vec![left, checked] } else { vec![left, checked, one] };
    let selected = builder.emit(Op::MixedMinMax, operands,
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

/// A selection over an unproven operand that is only cast and released keeps a guarded cell;
/// its checked operand, read by a consumer the numeric proof does not know, stays on the heap.
#[test]
fn stack_numeric_min_max_selection_over_unproven_operands_is_guarded() {
    let (function, _, selected) = selection(MinMaxRule::MaxPair, false);
    let frame = plan(&function);
    assert!(frame.cells.is_empty());
    assert_eq!(frame.guarded, vec![selected]);
    let (function, _, selected) = selection(MinMaxRule::MaxPair, true);
    let frame = plan(&function);
    assert!(frame.cells.contains(&selected));
    assert!(frame.guarded.is_empty());
}

/// Builds `store(clone(mixed_min_max(left, 1)))`, the by-value assignment of a selection,
/// optionally returning the selection itself.
fn assigned_selection(numeric: bool, escape: bool) -> (Function, ValueId) {
    let mut function = Function::new("assigned_selection".into(), IrType::Void, PhpType::Void);
    let slot = function.add_local(Some("picked".into()), IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed, LocalKind::PhpLocal);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let one = builder.emit_const_i64(1);
    let left = if numeric { builder.emit_const_f64(0.5) } else { builder.emit_const_bool(true) };
    let selected = builder.emit(Op::MixedMinMax, vec![left, one],
        Some(Immediate::I64(MinMaxRule::MaxPair.immediate())),
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    let copy = builder.emit(Op::MixedClone, vec![selected], None,
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    builder.emit_store_local(slot, copy);
    if escape {
        builder.terminate(Terminator::Return { value: Some(selected) });
        function.return_type = IrType::Heap(IrHeapKind::Mixed);
        function.return_php_type = PhpType::Mixed;
    } else {
        builder.emit(Op::Release, vec![selected], None, IrType::Void, PhpType::Void,
            Ownership::NonHeap);
        builder.terminate(Terminator::Return { value: None });
    }
    (function, selected)
}

/// An assignment copy reads a selection without boxing it: a numeric one keeps its numeric
/// cell, a runtime-typed one its guarded cell, and a returned one neither.
#[test]
fn stack_numeric_assignment_copy_observes_a_selection() {
    let (function, selected) = assigned_selection(true, false);
    crate::ir::validate_function(&function).unwrap();
    assert!(plan(&function).cells.contains(&selected));
    let (function, selected) = assigned_selection(false, false);
    let frame = plan(&function);
    assert!(!frame.cells.contains(&selected));
    assert_eq!(frame.guarded, vec![selected]);
    for numeric in [true, false] {
        let (function, selected) = assigned_selection(numeric, true);
        let frame = plan(&function);
        assert!(!frame.cells.contains(&selected) && frame.guarded.is_empty(), "numeric={numeric}");
    }
}

/// Builds `cast(mixed_min_max(1, pin(checked), pin(box(raw)), 0.5))`: the lifetime pins call
/// lowering takes on computed arguments that are not the last one.
fn pinned_operands() -> (Function, ValueId, ValueId, ValueId) {
    let mut function = Function::new("pinned_operands".into(), IrType::F64, PhpType::Float);
    let mut builder = Builder::new(&mut function);
    let entry = builder.create_named_block("entry", vec![]);
    builder.set_entry(entry);
    builder.position_at_end(entry);
    let one = builder.emit_const_i64(1);
    let half = builder.emit_const_f64(0.5);
    let checked = builder.emit(Op::ICheckedSub, vec![one, one], None,
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    let pinned = builder.emit(Op::Acquire, vec![checked], Some(Immediate::Bool(true)),
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    builder.emit(Op::Release, vec![checked], None, IrType::Void, PhpType::Void,
        Ownership::NonHeap);
    let selected = builder.emit(Op::MixedMinMax, vec![one, pinned, half],
        Some(Immediate::I64(MinMaxRule::MaxFold.immediate())),
        IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
    builder.emit(Op::Release, vec![pinned], None, IrType::Void, PhpType::Void,
        Ownership::NonHeap);
    let result = builder.emit(Op::Cast, vec![selected], Some(Immediate::CastTarget(IrType::F64)),
        IrType::F64, PhpType::Float, Ownership::NonHeap).unwrap();
    builder.emit(Op::Release, vec![selected], None, IrType::Void, PhpType::Void,
        Ownership::NonHeap);
    builder.terminate(Terminator::Return { value: Some(result) });
    crate::ir::validate_function(&function).unwrap();
    (function, checked, pinned, selected)
}

/// A pinned argument forwards its frame cell: the checked operand and the selection keep their
/// cells, and the pin itself is recognized as that stack value by the instruction lowering.
#[test]
fn stack_numeric_pinned_call_operand_keeps_its_frame_cell() {
    let (function, checked, pinned, selected) = pinned_operands();
    let frame = plan(&function);
    assert_eq!(frame.cells, vec![checked, selected]);
    let cells: HashMap<ValueId, usize> = frame.cells.iter().map(|&value| (value, 24)).collect();
    assert!(is_stack_value(&function, &cells, pinned));
}
