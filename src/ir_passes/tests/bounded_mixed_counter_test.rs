//! Purpose:
//! Tests whole-slot integer counter proofs before changing Mixed representation.
//!
//! Called from:
//! - The focused EIR pass library test harness.
//!
//! Key details:
//! - Accepted recurrences need every update proven safe; overflow and pointer escapes reject.

use crate::ir::{
    validate_function, Builder, CmpPredicate, DataPool, Function, FunctionParam, Immediate,
    IrHeapKind, IrType, LocalKind, MixedNumericOp, Op, Ownership, RuntimeCallTarget, RuntimeFnId,
    Terminator,
};
use crate::ir_passes::bounded_mixed_counter::BoundedMixedCounter;
use crate::ir_passes::driver::IrPass;
use crate::types::PhpType;

/// Selects the boxed opcode a counter update uses in the fixture.
#[derive(Clone, Copy)]
enum CounterUpdate {
    /// `$i++` / `$i--`, PHP's string-aware increment opcode.
    StrIncDec,
    /// `$i += 1` / `$i -= 1`, the ordinary numeric compound assignment.
    CompoundAssign,
}

/// Builds a boxed counter with the same increment ownership scaffold as closure loops.
pub(super) fn counter(predicate: CmpPredicate, delta: i64, seed: i64, escape: bool) -> Function {
    counter_with_update(predicate, delta, seed, escape, CounterUpdate::StrIncDec)
}

/// Builds the counter fixture with the requested update opcode shape.
fn counter_with_update(
    predicate: CmpPredicate,
    delta: i64,
    seed: i64,
    escape: bool,
    update_kind: CounterUpdate,
) -> Function {
    let mut function = Function::new("counter".into(), IrType::I64, PhpType::Int);
    function.params.push(FunctionParam {
        name: "n".into(),
        ir_type: IrType::I64,
        php_type: PhpType::Int,
        by_ref: false,
        variadic: false,
    });
    let mut b = Builder::new(&mut function);
    let n = b.add_local(
        Some("n".into()),
        IrType::I64,
        PhpType::Int,
        LocalKind::PhpLocal,
    );
    let i = b.add_local(
        Some("i".into()),
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
        LocalKind::PhpLocal,
    );
    let entry = b.create_named_block("entry", vec![]);
    let cond = b.create_named_block("cond", vec![]);
    let body = b.create_named_block("body", vec![]);
    let exit = b.create_named_block("exit", vec![]);
    b.set_entry(entry);
    b.position_at_end(entry);
    let initial = b.emit_const_i64(seed);
    b.emit_store_local(i, initial);
    b.terminate(Terminator::Br {
        target: cond,
        args: vec![],
    });
    b.position_at_end(cond);
    let value = b.emit_load_local(i, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
    let limit = b.emit_load_local(n, IrType::I64, PhpType::Int);
    let cmp = b
        .emit(
            Op::PhpRelCmp,
            vec![value, limit],
            Some(Immediate::CmpPredicate(predicate)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
        )
        .unwrap();
    b.terminate(Terminator::CondBr {
        cond: cmp,
        then_target: body,
        then_args: vec![],
        else_target: exit,
        else_args: vec![],
    });
    b.position_at_end(body);
    let old = b.emit_load_local(i, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
    if escape {
        b.terminate(Terminator::Return { value: Some(old) });
    } else {
        let update = match update_kind {
            CounterUpdate::StrIncDec => b
                .emit(
                    Op::StrIncDec,
                    vec![old],
                    Some(Immediate::I64(delta)),
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .unwrap(),
            CounterUpdate::CompoundAssign => {
                let magnitude = if delta < 0 { -delta } else { delta };
                let step = b.emit_const_i64(magnitude);
                let opcode = if delta < 0 { MixedNumericOp::Sub } else { MixedNumericOp::Add };
                b.emit(
                    Op::MixedNumericBinop,
                    vec![old, step],
                    Some(Immediate::MixedNumericOp(opcode)),
                    IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed,
                    Ownership::Owned,
                )
                .unwrap()
            }
        };
        let copy = b
            .emit(
                Op::MixedClone,
                vec![update],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
            .unwrap();
        let acquired = b
            .emit(
                Op::Acquire,
                vec![copy],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
            .unwrap();
        b.emit(
            Op::ReleaseLocalSlot,
            vec![],
            Some(Immediate::LocalSlot(i)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        b.emit_store_local(i, acquired);
        for value in [update, copy] {
            b.emit(
                Op::Release,
                vec![value],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
        }
        b.terminate(Terminator::Br {
            target: cond,
            args: vec![],
        });
    }
    b.position_at_end(exit);
    let zero = b.emit_const_i64(0);
    b.terminate(Terminator::Return { value: Some(zero) });
    function
}

/// Strict comparison bounds make ascending and descending updates fit at both integer limits.
#[test]
fn bounded_mixed_counter_promotes_strict_bounded_recurrences() {
    for (cmp, delta) in [(CmpPredicate::Slt, 1), (CmpPredicate::Sgt, -1)] {
        let mut f = counter(cmp, delta, 0, false);
        assert!(validate_function(&f).is_ok(), "{:?}", validate_function(&f));
        assert!(BoundedMixedCounter::default().run(&mut f, &mut DataPool::default()));
        assert_eq!(f.locals[1].ir_type, IrType::I64);
        assert!(!f.instructions.iter().any(|inst| inst.op == Op::StrIncDec));
        assert!(validate_function(&f).is_ok(), "{:?}", validate_function(&f));
        assert!(!BoundedMixedCounter::default().run(&mut f, &mut DataPool::default()));
    }
}

/// `$i += 1` and `$i -= 1` are counter updates just like `$i++` and `$i--`.
///
/// The compound form lowers to `mixed_numeric_binop` with a constant step, not `str_inc_dec`,
/// so it needs the same bounded-recurrence proof to become a raw integer slot.
#[test]
fn bounded_mixed_counter_promotes_compound_assignment_recurrences() {
    for (cmp, delta) in [(CmpPredicate::Slt, 1), (CmpPredicate::Sgt, -1)] {
        let mut f = counter_with_update(cmp, delta, 0, false, CounterUpdate::CompoundAssign);
        assert!(validate_function(&f).is_ok(), "{:?}", validate_function(&f));
        assert!(BoundedMixedCounter::default().run(&mut f, &mut DataPool::default()));
        assert_eq!(f.locals[1].ir_type, IrType::I64);
        assert!(!f.instructions.iter().any(|inst| inst.op == Op::MixedNumericBinop));
        assert!(validate_function(&f).is_ok(), "{:?}", validate_function(&f));
        assert!(!BoundedMixedCounter::default().run(&mut f, &mut DataPool::default()));
    }
}

/// A compound update with an unprovable bound keeps its boxed recurrence untouched.
#[test]
fn bounded_mixed_counter_rejects_compound_overflow() {
    for (cmp, delta) in [(CmpPredicate::Sle, 1), (CmpPredicate::Sge, -1)] {
        let mut f = counter_with_update(cmp, delta, 0, false, CounterUpdate::CompoundAssign);
        let before = format!("{f:?}");
        assert!(!BoundedMixedCounter::default().run(&mut f, &mut DataPool::default()));
        assert_eq!(format!("{f:?}"), before);
    }
}

/// Numeric consumers preserve their Mixed opcode while proven counters become raw integers.
#[test]
fn bounded_mixed_counter_allows_numeric_reads_only_after_range_proof() {
    use crate::ir::{LocalSlotId, MixedNumericOp};
    for op in [MixedNumericOp::Add, MixedNumericOp::Sub, MixedNumericOp::Mul] {
        for (predicate, accepted) in [(CmpPredicate::Slt, true), (CmpPredicate::Sle, false)] {
            let mut f = counter(predicate, 1, 0, false);
            let body = f.blocks[2].id;
            let term = f.blocks[2].terminator.take().unwrap();
            let result = {
                let mut builder = Builder::new(&mut f);
                builder.position_at_end(body);
                let value = builder.emit_load_local(LocalSlotId::from_raw(1),
                    IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
                let one = builder.emit_const_i64(1);
                let result = builder.emit(Op::MixedNumericBinop, vec![value, one],
                    Some(Immediate::MixedNumericOp(op)), IrType::Heap(IrHeapKind::Mixed),
                    PhpType::Mixed, Ownership::Owned).unwrap();
                builder.emit(Op::Release, vec![result], None, IrType::Void,
                    PhpType::Void, Ownership::NonHeap);
                builder.terminate(term);
                result
            };
            validate_function(&f).unwrap();
            let before = format!("{f:?}");
            assert_eq!(BoundedMixedCounter::default().run(&mut f, &mut DataPool::default()), accepted);
            validate_function(&f).unwrap();
            if accepted {
                assert_eq!(f.locals[1].ir_type, IrType::I64);
                let crate::ir::ValueDef::Instruction { inst, .. } = f.value(result).unwrap().def else { panic!("numeric result"); };
                let math = f.instruction(inst).unwrap();
                assert_eq!(math.op, Op::MixedNumericBinop);
                assert_eq!(f.value(math.operands[0]).unwrap().php_type, PhpType::Int);
            } else {
                assert_eq!(format!("{f:?}"), before);
            }
        }
    }
}

/// An inclusive maximum can overflow, so a failed trial must leave the entire function alone.
#[test]
fn bounded_mixed_counter_rejects_possible_overflow() {
    for (cmp, delta, seed) in [(CmpPredicate::Sle, 1, 0), (CmpPredicate::Sge, -1, 0)] {
        let mut f = counter(cmp, delta, seed, false);
        let before = format!("{f:?}");
        assert!(!BoundedMixedCounter::default().run(&mut f, &mut DataPool::default()));
        assert_eq!(format!("{f:?}"), before);
    }
}

/// Pointer returns, reference storage and an exposed main frame cannot be scalarized.
#[test]
fn bounded_mixed_counter_rejects_escapes_and_exposed_main_frame() {
    let mut escape = counter(CmpPredicate::Slt, 1, 0, true);
    assert!(!BoundedMixedCounter::default().run(&mut escape, &mut DataPool::default()));
    for kind in [
        LocalKind::RefCell,
        LocalKind::EvalScope,
        LocalKind::GeneratorState,
    ] {
        let mut f = counter(CmpPredicate::Slt, 1, 0, false);
        f.add_local(None, IrType::I64, PhpType::Int, kind);
        assert!(!BoundedMixedCounter::default().run(&mut f, &mut DataPool::default()));
    }
    let mut private_main = counter(CmpPredicate::Slt, 1, 0, false);
    private_main.flags.is_main = true;
    assert!(BoundedMixedCounter::default().run(&mut private_main, &mut DataPool::default()));

    let mut exposed = counter(CmpPredicate::Slt, 1, 0, false);
    exposed.flags.is_main = true;
    let body = exposed.blocks[2].id;
    let term = exposed.blocks[2].terminator.take().unwrap();
    let mut data = DataPool::default();
    let label = data.intern_string("include once=false required=false");
    {
        let mut builder = Builder::new(&mut exposed);
        builder.position_at_end(body);
        assert!(builder.emit(
            Op::RuntimeCall,
            Vec::new(),
            Some(Immediate::Data(label)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        ).is_none());
        builder.terminate(term);
    }
    assert!(!BoundedMixedCounter::default().run(&mut exposed, &mut data));
}

/// Adds a call of data[0] to the boxed-counter fixture after its update.
fn counter_with_call() -> Function {
    use crate::ir::LocalSlotId;
    let mut f = counter(CmpPredicate::Slt, 1, 0, false);
    let body = f.blocks[2].id;
    let term = f.blocks[2].terminator.take().unwrap();
    {
        let mut b = Builder::new(&mut f);
        b.position_at_end(body);
        let value = b.emit_load_local(
            LocalSlotId::from_raw(1),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        b.emit(
            Op::Call,
            vec![value],
            Some(Immediate::Data(crate::ir::DataId::from_raw(0))),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        b.terminate(term);
    }
    f
}

/// A call consumer needs authoritative physical Mixed parameter metadata, not a guessed type.
#[test]
fn bounded_mixed_counter_preserves_typed_parameter_guards() {
    use crate::codegen::platform::{Arch, Platform, Target};
    use crate::ir::Module;
    for (parameter, by_ref, accepted) in [
        (PhpType::Mixed, false, true),
        (PhpType::False, false, false),
        (PhpType::Int, false, false),
        (PhpType::Mixed, true, false),
    ] {
        let mut f = counter_with_call();
        let mut module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
        let mut callee = Function::new("callee".into(), IrType::Void, PhpType::Void);
        callee.params.push(FunctionParam {
            name: "x".into(),
            ir_type: if parameter == PhpType::Mixed {
                IrType::Heap(IrHeapKind::Mixed)
            } else {
                IrType::I64
            },
            php_type: parameter,
            by_ref,
            variadic: false,
        });
        module.functions.push(callee);
        module.data.function_names.push("callee".into());
        assert_eq!(
            BoundedMixedCounter::new(&module).run(&mut f, &mut module.data),
            accepted
        );
        assert!(validate_function(&f).is_ok(), "{:?}", validate_function(&f));
    }
}

/// Missing initialization must not become an integer merely because a backedge writes an update.
#[test]
fn bounded_mixed_counter_requires_definite_ssa_initialization() {
    let mut f = counter(CmpPredicate::Slt, 1, 0, false);
    let store = f.blocks[0]
        .instructions
        .iter()
        .copied()
        .find(|id| f.instruction(*id).unwrap().op == Op::StoreLocal)
        .unwrap();
    crate::ir_passes::rewrite::neutralize_to_nop(f.instruction_mut(store).unwrap());
    let before = format!("{f:?}");
    assert!(!BoundedMixedCounter::default().run(&mut f, &mut DataPool::default()));
    assert_eq!(format!("{f:?}"), before);
}

/// Canonical callee lookup must preserve the backend's first target, including unsafe targets.
#[test]
fn bounded_mixed_counter_uses_canonical_first_callee_metadata() {
    use crate::codegen::platform::{Arch, Platform, Target};
    use crate::ir::Module;
    for (parameter, by_ref, later_by_ref, accepted) in [
        (PhpType::Mixed, true, Some(false), false),
        (PhpType::False, false, Some(false), false),
        (PhpType::Mixed, false, None, true),
        (PhpType::Mixed, false, Some(true), true),
    ] {
        let mut f = counter_with_call();
        let mut module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
        let mut first = Function::new("\\CaLlEe".into(), IrType::Void, PhpType::Void);
        first.params.push(FunctionParam {
            name: "x".into(),
            ir_type: if parameter == PhpType::Mixed {
                IrType::Heap(IrHeapKind::Mixed)
            } else {
                IrType::I64
            },
            php_type: parameter,
            by_ref,
            variadic: false,
        });
        module.functions.push(first);
        if let Some(by_ref) = later_by_ref {
            let mut later = Function::new("callee".into(), IrType::Void, PhpType::Void);
            later.flags.is_closure = true;
            later.params.push(FunctionParam {
                name: "x".into(),
                ir_type: IrType::Heap(IrHeapKind::Mixed),
                php_type: PhpType::Mixed,
                by_ref,
                variadic: false,
            });
            module.closures.push(later);
        }
        module.data.function_names.push("callee".into());
        assert_eq!(
            BoundedMixedCounter::new(&module).run(&mut f, &mut module.data),
            accepted
        );
        assert!(validate_function(&f).is_ok(), "{:?}", validate_function(&f));
    }
}

/// Joint storage trials prove every mutually observed recurrence before accepting either slot.
#[test]
fn bounded_mixed_counter_storage_joint_checks_every_update() {
    use std::collections::HashMap;
    use crate::ir::LocalSlotId;
    for (seed, accepted) in [(0, true), (i64::MAX, false)] {
        let mut function = counter(CmpPredicate::Slt, 1, 0, false);
        let first = LocalSlotId::from_raw(1);
        let second = function.add_local(Some("other".into()), IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed, LocalKind::PhpLocal);
        let entry = function.blocks[0].id;
        let body = function.blocks[2].id;
        let entry_term = function.blocks[0].terminator.take().unwrap();
        let body_term = function.blocks[2].terminator.take().unwrap();
        {
            let mut builder = Builder::new(&mut function);
            builder.position_at_end(entry);
            let seed = builder.emit_const_i64(seed);
            builder.emit_store_local(second, seed);
            builder.terminate(entry_term);
            builder.position_at_end(body);
            let left = builder.emit_load_local(first, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
            let right = builder.emit_load_local(second, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
            builder.emit(Op::PhpRelCmp, vec![left, right], Some(Immediate::CmpPredicate(CmpPredicate::Sgt)),
                IrType::I64, PhpType::Bool, Ownership::NonHeap);
            let update = builder.emit(Op::StrIncDec, vec![right], Some(Immediate::I64(1)),
                IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
            let owner = builder.emit(Op::Acquire, vec![update], None,
                IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
            builder.emit_store_local(second, owner);
            builder.emit(Op::Release, vec![update], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            builder.terminate(body_term);
        }
        assert!(validate_function(&function).is_ok());
        let original = crate::ir::print_function(&function);
        let slots = crate::ir_passes::prove_integer_slots(&function, &mut DataPool::default(), &HashMap::new()).slots;
        assert_eq!(slots.contains(&first), accepted);
        assert_eq!(slots.contains(&second), accepted);
        assert_eq!(crate::ir::print_function(&function), original, "analysis must not commit its trial");
    }
}


/// A numeric temporary's computed store initializes it, but a prior uninitialized read rejects.
#[test]
fn stack_numeric_domain_requires_initialized_numeric_temporaries() {
    for initialized in [false, true] {
        let mut function = counter(CmpPredicate::Slt, 1, 0, false);
        let slot = function.add_local(Some("square".into()), IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed, LocalKind::PhpLocal);
        let body = function.blocks[2].id;
        let term = function.blocks[2].terminator.take().unwrap();
        let read = {
            let mut builder = Builder::new(&mut function); builder.position_at_end(body);
            if !initialized { builder.emit_load_local(slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed); }
            let input = builder.emit_load_local(crate::ir::LocalSlotId::from_raw(1),
                IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
            let square = builder.emit(Op::MixedNumericBinop, vec![input, input],
                Some(Immediate::MixedNumericOp(crate::ir::MixedNumericOp::Mul)),
                IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
            builder.emit_store_local(slot, square);
            let read = builder.emit_load_local(slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
            builder.terminate(term); read
        };
        validate_function(&function).unwrap();
        assert_eq!(crate::ir_passes::numeric_domain::inputs(&function, &[read]), initialized);
    }
}

/// Another reference local does not alias a closed counter, while its own storage never narrows.
#[test]
fn bounded_mixed_counter_storage_ignores_unrelated_reference_slots() {
    let mut function = counter(CmpPredicate::Slt, 1, 0, false);
    let unrelated = function.add_local(Some("reference".into()), IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed, LocalKind::RefCell);
    validate_function(&function).unwrap();
    let slots = crate::ir_passes::prove_integer_slots(&function, &mut DataPool::default(),
        &std::collections::HashMap::new()).slots;
    assert!(slots.contains(&crate::ir::LocalSlotId::from_raw(1)));
    assert!(!slots.contains(&unrelated));
}

/// A value-only typed arithmetic runtime call keeps its boxed operand by value, so a strict
/// bounded counter feeding `intdiv()` still promotes to raw SSA instead of staying boxed.
#[test]
fn bounded_mixed_counter_promotes_across_value_only_runtime_call() {
    let mut function = counter(CmpPredicate::Slt, 1, 0, false);
    let body = function.blocks[2].id;
    let terminator = function.blocks[2].terminator.take().unwrap();
    {
        let mut builder = Builder::new(&mut function);
        builder.position_at_end(body);
        let value = builder.emit_load_local(
            crate::ir::LocalSlotId::from_raw(1),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        let divisor = builder.emit_const_i64(3);
        builder
            .emit(
                Op::RuntimeCall,
                vec![value, divisor],
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                    RuntimeFnId::Intdiv,
                ))),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .unwrap();
        builder.terminate(terminator);
    }
    validate_function(&function).unwrap();
    assert!(BoundedMixedCounter::default().run(&mut function, &mut DataPool::default()));
    assert_eq!(function.locals[1].ir_type, IrType::I64);
    assert!(validate_function(&function).is_ok());
}

/// Constants and retired bookkeeping in an already optimized preheader do not block a seed guard.
#[test]
fn bounded_mixed_counter_seed_guard_accepts_pure_suffix() {
    use crate::ir::{LocalSlotId, Module};
    use crate::ir_passes::bounded_mixed_counter::LoopCounterVersioning;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut function = counter(CmpPredicate::Sge, -1, 0, false);
        let entry = function.blocks[0].id;
        let old_store = function.blocks[0].instructions.pop().unwrap();
        assert_eq!(function.instructions[old_store.as_raw() as usize].op, Op::StoreLocal);
        crate::ir_passes::rewrite::neutralize_to_nop(&mut function.instructions[old_store.as_raw() as usize]);
        let bound = function.blocks[1].instructions[1];
        let inst = &mut function.instructions[bound.as_raw() as usize];
        assert_eq!(inst.op, Op::LoadLocal);
        inst.op = Op::ConstI64;
        inst.immediate = Some(Immediate::I64(0));
        inst.effects = Op::ConstI64.default_effects();
        let terminator = function.blocks[0].terminator.take().unwrap();
        {
            let mut b = Builder::new(&mut function);
            b.position_at_end(entry);
            let base = b.emit_load_local(LocalSlotId::from_raw(0), IrType::I64, PhpType::Int);
            let one = b.emit_const_i64(1);
            let seed = b.emit(Op::ICheckedSub, vec![base, one], None,
                IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
            b.emit_store_local(LocalSlotId::from_raw(1), seed);
            b.emit(Op::Nop, vec![], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            b.emit_const_i64(17);
            b.emit_const_bool(true);
            b.terminate(terminator);
        }
        assert!(validate_function(&function).is_ok(), "{target}: {:?}", validate_function(&function));
        let module = Module::new(crate::codegen::platform::Target::parse(target).unwrap());
        let pass = LoopCounterVersioning::new(&module);
        assert!(pass.run(&mut function, &mut DataPool::default()), "{target}");
        assert!(function.blocks.iter().any(|block| block.name == "counter.seed.inrange"),
            "{target}: {}", crate::ir::print_function(&function));
        assert!(!function.blocks.iter().any(|block| block.name.contains(".counter.fast")), "{target}");
        assert!(!pass.run(&mut function, &mut DataPool::default()), "{target}: guard converges");
    }
}
