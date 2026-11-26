//! Purpose:
//! Verifies sparse callable facts retain conservative CFG behavior and bounded storage.
//!
//! Called from:
//! - The library unit-test harness through `codegen::callable_reachability`.
//!
//! Key details:
//! - An independent dense lattice oracle checks joins, unknowns and saturation.
//! - Synthetic EIR covers loops, handler roots, reference writes and opaque scope writes.
//! - A large unknown-only graph catches per-block allocation of every local slot.

use super::*;
use crate::codegen::platform::Target;
use crate::ir::{Builder, IrType, LocalKind, Ownership};
use crate::types::PhpType;

/// Advances a deterministic stream without adding a random-number dependency.
fn next_seed(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    *seed
}

/// Compares finite slots with an independent dense unknown-or-name-set model.
#[test]
fn sparse_local_joins_match_dense_lattice() {
    let mut seed = 73;
    for _ in 0..512 {
        let mut left = CallableLocalState::new(64);
        let mut right = CallableLocalState::new(64);
        let mut dense_left = vec![None::<BTreeSet<String>>; 64];
        let mut dense_right = dense_left.clone();
        for (state, dense) in [(&mut left, &mut dense_left), (&mut right, &mut dense_right)] {
            for (index, expected) in dense.iter_mut().enumerate() {
                if next_seed(&mut seed) % 3 == 0 {
                    continue;
                }
                let count = (next_seed(&mut seed) % 32 + 1) as usize;
                let names = (0..count).map(|_| {
                    format!("callback_{}", next_seed(&mut seed) % 97)
                }).collect::<BTreeSet<_>>();
                *expected = Some(names.clone());
                state.set(LocalSlotId::from_raw(index as u32), CallableNameSet::Known(names));
            }
        }
        let original = dense_left.clone();
        for (left, right) in dense_left.iter_mut().zip(&dense_right) {
            *left = match (left.as_ref(), right.as_ref()) {
                (Some(left), Some(right)) => {
                    let names = left.union(right).cloned().collect::<BTreeSet<_>>();
                    (names.len() <= 32).then_some(names)
                }
                _ => None,
            };
        }
        assert_eq!(left.merge_from(&right), dense_left != original);
        for (index, names) in dense_left.iter().enumerate() {
            assert_eq!(left.get(LocalSlotId::from_raw(index as u32))
                .and_then(CallableNameSet::finite_names),
                names.as_ref().map(|names| names.iter().cloned().collect::<Vec<_>>()));
        }
        assert!(!left.merge_from(&right));
    }
}

/// Unknown stores, invalid slots and opaque invalidation keep absent facts canonical.
#[test]
fn sparse_local_unknowns_and_bounds_remain_open() {
    let slot = LocalSlotId::from_raw(0);
    let invalid = LocalSlotId::from_raw(1);
    let mut state = CallableLocalState::new(1);
    state.set(invalid, CallableNameSet::singleton("STRTOUPPER"));
    assert_eq!(state, CallableLocalState::new(1));
    state.set(slot, CallableNameSet::singleton("STRTOUPPER"));
    assert_eq!(state.get(slot).unwrap().finite_names(), Some(vec!["strtoupper".into()]));
    state.set(slot, CallableNameSet::Unknown);
    assert_eq!(state, CallableLocalState::new(1));
    state.set(slot, CallableNameSet::singleton("strtolower"));
    state.clear();
    assert_eq!(state, CallableLocalState::new(1));
    let mut unknown = CallableLocalState::new(1);
    state.set(slot, CallableNameSet::singleton("strtolower"));
    assert!(!unknown.merge_from(&state));
    assert!(state.merge_from(&unknown));
    assert_eq!(state, unknown);
}

/// A 32-name union remains finite, while a 33rd candidate removes the sparse fact.
#[test]
fn sparse_callable_name_limit_opens_saturated_slots() {
    let slot = LocalSlotId::from_raw(0);
    let names = (0..32).map(|index| format!("callback_{index}")).collect::<BTreeSet<_>>();
    let mut state = CallableLocalState::new(1);
    state.set(slot, CallableNameSet::Known(names.clone()));
    let mut incoming = CallableLocalState::new(1);
    incoming.set(slot, CallableNameSet::Known(names));
    assert!(!state.merge_from(&incoming));
    assert_eq!(state.get(slot).unwrap().finite_names().unwrap().len(), 32);
    incoming.set(slot, CallableNameSet::singleton("callback_32"));
    assert!(state.merge_from(&incoming));
    assert_eq!(state, CallableLocalState::new(1));
}

/// Value-preserving SSA operations retain canonical names through sparse local storage.
#[test]
fn sparse_callable_value_aliases_preserve_names() {
    let mut module = Module::new(Target::detect_host());
    let name = module.data.intern_string("\\STRTOUPPER");
    let mut function = Function::new("aliases".into(), IrType::Void, PhpType::Void);
    let slot = function.add_local(Some("callback".into()), IrType::Str, PhpType::Str, LocalKind::PhpLocal);
    let mut values = Vec::new();
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let mut value = builder.emit_const_str(name);
        values.push(value);
        for op in [Op::Acquire, Op::Borrow, Op::Move, Op::EnsureOwned, Op::StrPersist] {
            value = builder.emit(op, vec![value], None, IrType::Str, PhpType::Str,
                Ownership::MaybeOwned).unwrap();
            values.push(value);
        }
        let boxed = builder.emit(Op::MixedBox, vec![value], None, IrType::from_php(&PhpType::Mixed),
            PhpType::Mixed, Ownership::MaybeOwned).unwrap();
        values.push(boxed);
        value = builder.emit(Op::MixedUnbox, vec![boxed], None, IrType::Str,
            PhpType::Str, Ownership::MaybeOwned).unwrap();
        values.push(value);
        builder.emit(Op::StoreLocal, vec![value], Some(Immediate::LocalSlot(slot)),
            IrType::Void, PhpType::Void, Ownership::NonHeap);
        values.push(builder.emit(Op::LoadLocal, Vec::new(), Some(Immediate::LocalSlot(slot)),
            IrType::Str, PhpType::Str, Ownership::Borrowed).unwrap());
        builder.terminate(Terminator::Return { value: None });
    }
    let analysis = CallableReachabilityAnalysis::new(&module, &function);
    for value in values {
        assert_eq!(analysis.candidates(value), Some(vec!["strtoupper".into()]));
    }
}

/// A loop unions finite names while an implicit catch entry stays unknown on every target.
#[test]
fn sparse_callable_loops_and_handler_roots_preserve_candidates() {
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut module = Module::new(Target::parse(target).unwrap());
        let upper = module.data.intern_string("\\STRTOUPPER");
        let lower = module.data.intern_string("strtolower");
        let mut function = Function::new("loop_names".into(), IrType::Void, PhpType::Void);
        let slot = function.add_local(Some("callback".into()), IrType::Str, PhpType::Str, LocalKind::PhpLocal);
        let (loop_value, after_value, catch_value, unreachable_value);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            let body = builder.create_named_block("loop", Vec::new());
            let after = builder.create_named_block("after", Vec::new());
            let handler = builder.create_named_block("handler", Vec::new());
            let unreachable = builder.create_named_block("unreachable", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let first = builder.emit_const_str(upper);
            builder.emit(Op::StoreLocal, vec![first], Some(Immediate::LocalSlot(slot)),
                IrType::Void, PhpType::Void, Ownership::NonHeap);
            builder.emit(Op::TryPushHandler, Vec::new(), Some(Immediate::I64(handler.as_raw() as i64)),
                IrType::Void, PhpType::Void, Ownership::NonHeap);
            builder.terminate(Terminator::Br { target: body, args: Vec::new() });
            builder.position_at_end(body);
            loop_value = builder.emit(Op::LoadLocal, Vec::new(), Some(Immediate::LocalSlot(slot)),
                IrType::Str, PhpType::Str, Ownership::Borrowed).unwrap();
            let second = builder.emit_const_str(lower);
            builder.emit(Op::StoreLocal, vec![second], Some(Immediate::LocalSlot(slot)),
                IrType::Void, PhpType::Void, Ownership::NonHeap);
            let cond = builder.emit_const_bool(true);
            builder.terminate(Terminator::CondBr { cond, then_target: body, then_args: Vec::new(),
                else_target: after, else_args: Vec::new() });
            builder.position_at_end(after);
            after_value = builder.emit(Op::LoadLocal, Vec::new(), Some(Immediate::LocalSlot(slot)),
                IrType::Str, PhpType::Str, Ownership::Borrowed).unwrap();
            builder.terminate(Terminator::Return { value: None });
            builder.position_at_end(handler);
            catch_value = builder.emit(Op::LoadLocal, Vec::new(), Some(Immediate::LocalSlot(slot)),
                IrType::Str, PhpType::Str, Ownership::Borrowed).unwrap();
            builder.terminate(Terminator::Return { value: None });
            builder.position_at_end(unreachable);
            unreachable_value = builder.emit(Op::LoadLocal, Vec::new(), Some(Immediate::LocalSlot(slot)),
                IrType::Str, PhpType::Str, Ownership::Borrowed).unwrap();
            builder.terminate(Terminator::Return { value: None });
        }
        let analysis = CallableReachabilityAnalysis::new(&module, &function);
        assert_eq!(analysis.candidates(loop_value), Some(vec!["strtolower".into(), "strtoupper".into()]), "{target}");
        assert_eq!(analysis.candidates(after_value), Some(vec!["strtolower".into()]), "{target}");
        assert_eq!(analysis.candidates(catch_value), None, "{target}");
        assert_eq!(analysis.candidates(unreachable_value), None, "{target}");
    }
}

/// Ref-cell and opaque writes discard exactly the same local facts as dense states.
#[test]
fn sparse_callable_reference_and_scope_writes_invalidate_facts() {
    let module = Module::new(Target::detect_host());
    let first = LocalSlotId::from_raw(0);
    let second = LocalSlotId::from_raw(1);
    let value_names = vec![CallableNameSet::singleton("strtoupper")];
    for (op, immediate, clear_second) in [
        (Op::UnsetLocal, Some(Immediate::LocalSlot(first)), false),
        (Op::BindRefCellPtr, Some(Immediate::LocalSlot(first)), false),
        (Op::IterCurrentValueRef, Some(Immediate::LocalSlot(first)), false),
        (Op::PromoteLocalRefCell, Some(Immediate::LocalSlotPair { first, second }), true),
        (Op::PromoteLocalRefCell, Some(Immediate::PropertyRefCell { first, second, suffix_and_type: 0 }), true),
        (Op::AliasLocalRefCell, Some(Immediate::LocalSlotPair { first, second }), true),
        (Op::RetainLocalRefCell, Some(Immediate::LocalSlotPair { first, second }), true),
        (Op::BindRefCellPtr, Some(Immediate::LocalSlotPair { first, second }), true),
        (Op::AdoptRefCellPtr, Some(Immediate::LocalSlotPair { first, second }), true),
        (Op::EvalLiteralCall, None, true), (Op::EvalScopeSet, None, true), (Op::ListUnpack, None, true),
    ] {
        let mut state = CallableLocalState::new(2);
        state.set(first, CallableNameSet::singleton("strtoupper"));
        state.set(second, CallableNameSet::singleton("strtolower"));
        let mut function = Function::new("invalidate".into(), IrType::Void, PhpType::Void);
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.emit(op, Vec::new(), immediate, IrType::Void, PhpType::Void, Ownership::NonHeap);
        let inst = function.instruction(function.blocks[0].instructions[0]).unwrap();
        apply_local_transfer(inst, &mut state, &value_names);
        assert!(state.get(first).is_none(), "{op:?}");
        assert_eq!(state.get(second).is_none(), clear_second, "{op:?}");
        assert_eq!(instruction_result_names(&module, inst, &state, &value_names), CallableNameSet::Unknown);
    }
}

/// Many unknown temporaries and blocks must not allocate a dense local vector per block.
#[test]
fn sparse_callable_large_unknown_graph_has_bounded_state() {
    let module = Module::new(Target::detect_host());
    let mut function = Function::new("large_unknown".into(), IrType::Void, PhpType::Void);
    let mut last = LocalSlotId::from_raw(0);
    for _ in 0..32768 {
        last = function.add_local(None, IrType::Str, PhpType::Str, LocalKind::HiddenTemp);
    }
    let mut loads = Vec::new();
    {
        let mut builder = Builder::new(&mut function);
        let blocks = (0..1024).map(|index| builder.create_named_block(format!("b{index}"), Vec::new()))
            .collect::<Vec<_>>();
        builder.set_entry(blocks[0]);
        for (index, block) in blocks.iter().enumerate() {
            builder.position_at_end(*block);
            loads.push(builder.emit(Op::LoadLocal, Vec::new(), Some(Immediate::LocalSlot(last)),
                IrType::Str, PhpType::Str, Ownership::Borrowed).unwrap());
            builder.terminate(if let Some(next) = blocks.get(index + 1) {
                Terminator::Br { target: *next, args: Vec::new() }
            } else {
                Terminator::Return { value: None }
            });
        }
    }
    let analysis = CallableReachabilityAnalysis::new(&module, &function);
    assert!(loads.into_iter().all(|value| analysis.candidates(value).is_none()));
}
