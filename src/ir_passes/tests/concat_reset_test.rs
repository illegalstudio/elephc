//! Purpose:
//! Unit tests for the concat scratch-state optimization pass (`ConcatResetElim`).
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Hand-built EIR functions exercise forward clean-state elimination across
//!   entry blocks, arithmetic loops, diamond branches, concrete array iteration,
//!   and scalar casts, as well as backward coalescing of unobserved resets and
//!   preservation across calls, output, `BoolToStr`, destructors, closures,
//!   cyclic phi nodes, throwing paths, and `eval`.

use crate::ir::{
    validate_function, Builder, CmpPredicate, DataId, DataPool, Effects, Function, Immediate,
    InstId, IrHeapKind, IrType, IterStartMetadata, LocalKind, Op, Ownership, Terminator, ValueId,
};
use crate::ir_passes::concat_reset::ConcatResetElim;
use crate::ir_passes::driver::{run_function_passes, IrPass};
use crate::types::PhpType;

/// Runs one `ConcatResetElim` pass over `function`.
fn run_once(function: &mut Function) -> bool {
    ConcatResetElim.run(function, &mut DataPool::default())
}

/// Runs the fixed-point driver with only `ConcatResetElim` registered.
fn run_to_fixed_point(function: &mut Function) {
    let passes: Vec<Box<dyn IrPass>> = vec![Box::new(ConcatResetElim)];
    run_function_passes(function, &passes, &mut DataPool::default());
}

/// Counts remaining `Op::ConcatReset` instructions in `function`.
fn count_concat_resets(function: &Function) -> usize {
    function
        .instructions
        .iter()
        .filter(|inst| inst.op == Op::ConcatReset)
        .count()
}

/// Returns the `InstId` of the instruction that will be appended next.
fn next_inst_id(builder: &Builder<'_>) -> InstId {
    InstId::from_raw(builder.function().instructions.len() as u32)
}

/// Emits one `Op::ConcatReset` instruction at the builder's current insertion point.
fn emit_concat_reset(builder: &mut Builder<'_>) -> InstId {
    let inst_id = next_inst_id(builder);
    builder.emit(
        Op::ConcatReset,
        vec![],
        None,
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
    inst_id
}

/// Emits an integer comparison instruction (`Op::ICmp`).
fn emit_icmp(
    builder: &mut Builder<'_>,
    predicate: CmpPredicate,
    lhs: ValueId,
    rhs: ValueId,
) -> ValueId {
    builder
        .emit(
            Op::ICmp,
            vec![lhs, rhs],
            Some(Immediate::CmpPredicate(predicate)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
        )
        .expect("icmp produces a value")
}

/// Resets at function entry and across pure scalar statements are eliminated
/// because the function prologue already establishes `_concat_off == frame_base`.
#[test]
fn clean_entry_and_scalar_statements_remove_all_resets() {
    let mut function = Function::new("scalar_only".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let slot = builder.add_local(
            Some("x".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);

        emit_concat_reset(&mut builder);
        let one = builder.emit_const_i64(1);
        builder.emit_store_local(slot, one);

        emit_concat_reset(&mut builder);
        let loaded = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        let two = builder.emit_const_i64(2);
        let sum = builder.emit_iadd(loaded, two);
        builder.emit_store_local(slot, sum);

        emit_concat_reset(&mut builder);
        let final_val = builder.emit_load_local(slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return {
            value: Some(final_val),
        });
    }

    assert!(run_once(&mut function));
    assert_eq!(count_concat_resets(&function), 0);
    assert!(validate_function(&function).is_ok());
}

/// After a scratch-dirtying operation (`StrConcat`), the first subsequent
/// `ConcatReset` is kept to reclaim the scratch buffer, while later consecutive
/// resets across concat-inert instructions are removed.
#[test]
fn dirty_concat_keeps_first_reset_and_removes_subsequent_clean_resets() {
    let mut function = Function::new("after_concat".to_string(), IrType::I64, PhpType::Int);
    let (r0, r1, r2);
    {
        let mut builder = Builder::new(&mut function);
        let str_slot = builder.add_local(
            Some("s".to_string()),
            IrType::Str,
            PhpType::Str,
            LocalKind::PhpLocal,
        );
        let int_slot = builder.add_local(
            Some("n".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);

        r0 = emit_concat_reset(&mut builder);
        let a = builder.emit_const_str(DataId::from_raw(0));
        let b = builder.emit_const_str(DataId::from_raw(1));
        let joined = builder
            .emit(
                Op::StrConcat,
                vec![a, b],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("str_concat produces a value");
        let owned = builder
            .emit(
                Op::Acquire,
                vec![joined],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Owned,
            )
            .expect("acquire produces a value");
        builder.emit_store_local(str_slot, owned);

        // First reset after StrConcat must survive.
        r1 = emit_concat_reset(&mut builder);
        let ten = builder.emit_const_i64(10);
        builder.emit_store_local(int_slot, ten);

        // Second reset sees clean state and is eliminated.
        r2 = emit_concat_reset(&mut builder);
        let loaded = builder.emit_load_local(int_slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return {
            value: Some(loaded),
        });
    }

    assert!(run_once(&mut function));
    assert_eq!(function.instruction(r0).unwrap().op, Op::Nop);
    assert_eq!(function.instruction(r1).unwrap().op, Op::ConcatReset);
    assert_eq!(function.instruction(r2).unwrap().op, Op::Nop);
    assert!(validate_function(&function).is_ok());
}

/// An arithmetic-only loop preceded by a dirty string operation keeps the single
/// pre-loop reset and eliminates every reset inside the loop header, body, and
/// update blocks.
#[test]
fn arithmetic_loop_after_concat_eliminates_in_loop_resets() {
    let mut function = Function::new("arith_loop".to_string(), IrType::I64, PhpType::Int);
    let (pre_loop_reset, body_reset, update_reset, exit_reset);
    {
        let mut builder = Builder::new(&mut function);
        let i_slot = builder.add_local(
            Some("i".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        let header = builder.create_named_block("for.cond", vec![]);
        let body = builder.create_named_block("for.body", vec![]);
        let update = builder.create_named_block("for.update", vec![]);
        let exit = builder.create_named_block("for.exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let num = builder.emit_const_i64(42);
        let _scratch = builder
            .emit(
                Op::IToStr,
                vec![num],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("itostr produces a value");
        pre_loop_reset = emit_concat_reset(&mut builder);
        let zero = builder.emit_const_i64(0);
        builder.emit_store_local(i_slot, zero);
        builder.terminate(Terminator::Br {
            target: header,
            args: vec![],
        });

        builder.position_at_end(header);
        let i_val = builder.emit_load_local(i_slot, IrType::I64, PhpType::Int);
        let bound = builder.emit_const_i64(100);
        let cmp = emit_icmp(&mut builder, CmpPredicate::Slt, i_val, bound);
        builder.terminate(Terminator::CondBr {
            cond: cmp,
            then_target: body,
            then_args: vec![],
            else_target: exit,
            else_args: vec![],
        });

        builder.position_at_end(body);
        body_reset = emit_concat_reset(&mut builder);
        builder.terminate(Terminator::Br {
            target: update,
            args: vec![],
        });

        builder.position_at_end(update);
        update_reset = emit_concat_reset(&mut builder);
        let cur = builder.emit_load_local(i_slot, IrType::I64, PhpType::Int);
        let one = builder.emit_const_i64(1);
        let next = builder.emit_iadd(cur, one);
        builder.emit_store_local(i_slot, next);
        builder.terminate(Terminator::Br {
            target: header,
            args: vec![],
        });

        builder.position_at_end(exit);
        exit_reset = emit_concat_reset(&mut builder);
        let res = builder.emit_load_local(i_slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(res) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instruction(pre_loop_reset).unwrap().op,
        Op::ConcatReset
    );
    assert_eq!(function.instruction(body_reset).unwrap().op, Op::Nop);
    assert_eq!(function.instruction(update_reset).unwrap().op, Op::Nop);
    assert_eq!(function.instruction(exit_reset).unwrap().op, Op::Nop);
    assert!(validate_function(&function).is_ok());
}

/// A loop whose body allocates concat scratch (`StrConcat`) keeps its in-loop
/// `ConcatReset` because the back-edge reaches the loop header with a dirty state.
#[test]
fn loop_with_concat_in_body_preserves_loop_reset() {
    let mut function = Function::new("concat_loop".to_string(), IrType::I64, PhpType::Int);
    let body_reset;
    {
        let mut builder = Builder::new(&mut function);
        let i_slot = builder.add_local(
            Some("i".to_string()),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        let header = builder.create_named_block("while.cond", vec![]);
        let body = builder.create_named_block("while.body", vec![]);
        let exit = builder.create_named_block("while.exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let zero = builder.emit_const_i64(0);
        builder.emit_store_local(i_slot, zero);
        builder.terminate(Terminator::Br {
            target: header,
            args: vec![],
        });

        builder.position_at_end(header);
        let i_val = builder.emit_load_local(i_slot, IrType::I64, PhpType::Int);
        let bound = builder.emit_const_i64(10);
        let cmp = emit_icmp(&mut builder, CmpPredicate::Slt, i_val, bound);
        builder.terminate(Terminator::CondBr {
            cond: cmp,
            then_target: body,
            then_args: vec![],
            else_target: exit,
            else_args: vec![],
        });

        builder.position_at_end(body);
        body_reset = emit_concat_reset(&mut builder);
        let a = builder.emit_const_str(DataId::from_raw(0));
        let b = builder.emit_const_str(DataId::from_raw(1));
        let _joined = builder
            .emit(
                Op::StrConcat,
                vec![a, b],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("str_concat produces a value");
        let one = builder.emit_const_i64(1);
        let next = builder.emit_iadd(i_val, one);
        builder.emit_store_local(i_slot, next);
        builder.terminate(Terminator::Br {
            target: header,
            args: vec![],
        });

        builder.position_at_end(exit);
        let res = builder.emit_load_local(i_slot, IrType::I64, PhpType::Int);
        builder.terminate(Terminator::Return { value: Some(res) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instruction(body_reset).unwrap().op,
        Op::ConcatReset
    );
    assert!(validate_function(&function).is_ok());
}

/// Backward coalescing removes an earlier `ConcatReset` in one branch when the
/// merge block immediately performs another `ConcatReset` that is required by
/// the sibling dirty branch.
#[test]
fn backward_coalescing_removes_branch_reset_postdominated_by_merge_reset() {
    let mut function = Function::new("branch_coalesce".to_string(), IrType::I64, PhpType::Int);
    let (then_reset, merge_reset);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let then_bb = builder.create_named_block("then", vec![]);
        let else_bb = builder.create_named_block("else", vec![]);
        let merge_bb = builder.create_named_block("merge", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let cond = builder.emit_const_bool(true);
        let _dirty = builder
            .emit(
                Op::IToStr,
                vec![cond],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("itostr produces a value");
        builder.terminate(Terminator::CondBr {
            cond,
            then_target: then_bb,
            then_args: vec![],
            else_target: else_bb,
            else_args: vec![],
        });

        builder.position_at_end(then_bb);
        then_reset = emit_concat_reset(&mut builder);
        builder.terminate(Terminator::Br {
            target: merge_bb,
            args: vec![],
        });

        builder.position_at_end(else_bb);
        let n = builder.emit_const_i64(7);
        let _else_dirty = builder
            .emit(
                Op::IToStr,
                vec![n],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("itostr produces a value");
        builder.terminate(Terminator::Br {
            target: merge_bb,
            args: vec![],
        });

        builder.position_at_end(merge_bb);
        merge_reset = emit_concat_reset(&mut builder);
        let ret = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(ret) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instruction(then_reset).unwrap().op,
        Op::Nop,
        "then-branch reset is post-dominated by merge reset with no intervening observation"
    );
    assert_eq!(
        function.instruction(merge_reset).unwrap().op,
        Op::ConcatReset,
        "merge reset must remain for the dirty else-branch"
    );
    assert!(validate_function(&function).is_ok());
}

/// Calls, output, `eval`, and exception handler entries are treated as barriers
/// that preserve required `ConcatReset` instructions.
#[test]
fn calls_output_eval_and_handlers_preserve_concat_resets() {
    let mut function = Function::new("barriers".to_string(), IrType::I64, PhpType::Int);
    let (reset_before_echo, reset_after_echo, reset_after_eval, handler_reset);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let handler = builder.create_named_block("catch", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        builder.emit(
            Op::TryPushHandler,
            vec![],
            Some(Immediate::I64(handler.as_raw() as i64)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        let num = builder.emit_const_i64(5);
        let _dirty = builder
            .emit(
                Op::IToStr,
                vec![num],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("itostr produces a value");
        // Must survive before EchoValue because EchoValue observes/dirties concat scratch.
        reset_before_echo = emit_concat_reset(&mut builder);
        builder.emit(
            Op::EchoValue,
            vec![num],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        // Must survive after EchoValue and before EvalLiteralCall because both are concat barriers.
        reset_after_echo = emit_concat_reset(&mut builder);
        let code = builder.emit_const_str(DataId::from_raw(0));
        let _eval_res = builder
            .emit(
                Op::EvalLiteralCall,
                vec![code],
                Some(Immediate::Data(DataId::from_raw(0))),
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Owned,
            )
            .expect("eval_literal_call produces a value");
        // Must survive after EvalLiteralCall because eval can dirty concat scratch.
        reset_after_eval = emit_concat_reset(&mut builder);
        builder.emit(
            Op::TryPopHandler,
            vec![],
            Some(Immediate::I64(handler.as_raw() as i64)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.terminate(Terminator::Br {
            target: exit,
            args: vec![],
        });

        builder.position_at_end(handler);
        // Handler entry is conservatively dirty, so its first ConcatReset must survive.
        handler_reset = emit_concat_reset(&mut builder);
        builder.terminate(Terminator::Br {
            target: exit,
            args: vec![],
        });

        builder.position_at_end(exit);
        let zero = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(zero) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instruction(reset_before_echo).unwrap().op,
        Op::ConcatReset
    );
    assert_eq!(
        function.instruction(reset_after_echo).unwrap().op,
        Op::ConcatReset
    );
    assert_eq!(
        function.instruction(reset_after_eval).unwrap().op,
        Op::ConcatReset
    );
    assert_eq!(
        function.instruction(handler_reset).unwrap().op,
        Op::ConcatReset
    );
    assert!(validate_function(&function).is_ok());
}

/// Loading a `PhpType::Str` from a `PhpType::Mixed` local slot invokes
/// `__rt_mixed_cast_string` in codegen, which may write `_concat_buf`, so the
/// subsequent `ConcatReset` must be preserved.
#[test]
fn mixed_to_string_local_load_marks_concat_state_dirty() {
    let mut function = Function::new("mixed_load".to_string(), IrType::I64, PhpType::Int);
    let reset_after_load;
    {
        let mut builder = Builder::new(&mut function);
        let mixed_slot = builder.add_local(
            Some("m".to_string()),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);

        let _coerced = builder.emit_load_local(mixed_slot, IrType::Str, PhpType::Str);
        reset_after_load = emit_concat_reset(&mut builder);
        let zero = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(zero) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instruction(reset_after_load).unwrap().op,
        Op::ConcatReset
    );
    assert!(validate_function(&function).is_ok());
}

/// Even when effect refinement marks a user call `Effects::PURE`, `ConcatResetElim`
/// treats `Op::Call` as a concat boundary so resets before and after calls are
/// not coalesced across the call.
#[test]
fn pure_refined_call_still_acts_as_concat_barrier() {
    let mut function = Function::new("call_barrier".to_string(), IrType::I64, PhpType::Int);
    let (reset_before_call, reset_after_call);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);

        let one = builder.emit_const_i64(1);
        let _dirty = builder
            .emit(
                Op::IToStr,
                vec![one],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("itostr produces a value");
        reset_before_call = emit_concat_reset(&mut builder);
        let _call_res = builder
            .emit_with_effects(
                Op::Call,
                vec![],
                Some(Immediate::FunctionRef(crate::ir::FunctionId::from_raw(0))),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
                Effects::PURE,
                None,
            )
            .expect("call produces a value");
        reset_after_call = emit_concat_reset(&mut builder);
        builder.terminate(Terminator::Return { value: Some(one) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instruction(reset_before_call).unwrap().op,
        Op::ConcatReset
    );
    assert_eq!(
        function.instruction(reset_after_call).unwrap().op,
        Op::ConcatReset
    );
    assert!(validate_function(&function).is_ok());
}

/// `Op::BoolToStr` lowers to `__rt_itoa` for `true` in codegen, which advances
/// `_concat_off` by 21 bytes without a capacity check, so `ConcatResetElim` must
/// treat `Op::BoolToStr` as a concat-dirtying barrier.
#[test]
fn bool_to_str_is_treated_as_concat_dirtying_barrier() {
    let mut function = Function::new("bool_to_str_barrier".to_string(), IrType::I64, PhpType::Int);
    let (reset_before, reset_after);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);

        let one = builder.emit_const_i64(1);
        let _dirty = builder
            .emit(
                Op::IToStr,
                vec![one],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("itostr produces a value");
        reset_before = emit_concat_reset(&mut builder);
        let flag = builder.emit_const_bool(true);
        let _bstr = builder
            .emit(
                Op::BoolToStr,
                vec![flag],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("bool_to_str produces a value");
        reset_after = emit_concat_reset(&mut builder);
        builder.terminate(Terminator::Return { value: Some(one) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instruction(reset_before).unwrap().op,
        Op::ConcatReset,
        "reset before BoolToStr must not be coalesced across BoolToStr"
    );
    assert_eq!(
        function.instruction(reset_after).unwrap().op,
        Op::ConcatReset,
        "reset after BoolToStr must be kept because BoolToStr writes _concat_buf"
    );
    assert!(validate_function(&function).is_ok());
}

/// Releasing a `PhpType::Callable`, `PhpType::Object`, or `PhpType::Resource`, or
/// overwriting a promoted ref-cell slot holding a potentially destructor-bearing
/// payload, is treated as a concat barrier because `__rt_callable_descriptor_release`
/// or object destruction can run user `__destruct` code.
#[test]
fn callable_object_resource_and_promoted_ref_cell_stores_act_as_barriers() {
    let mut function = Function::new("dtor_barriers".to_string(), IrType::I64, PhpType::Int);
    let (reset_after_callable_release, reset_after_ref_cell_store, reset_after_slot_release);
    {
        let mut builder = Builder::new(&mut function);
        let callable_slot = builder.add_local(
            Some("fn".to_string()),
            IrType::I64,
            PhpType::Callable,
            LocalKind::PhpLocal,
        );
        let obj_slot = builder.add_local(
            Some("obj".to_string()),
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object("Boxed".to_string()),
            LocalKind::PhpLocal,
        );
        let cell_owner = builder.add_local(
            None,
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object("Boxed".to_string()),
            LocalKind::RefCell,
        );
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);

        let callable_val =
            builder.emit_load_local(callable_slot, IrType::I64, PhpType::Callable);
        builder.emit(
            Op::Release,
            vec![callable_val],
            None,
            IrType::Void,
            PhpType::Callable,
            Ownership::NonHeap,
        );
        reset_after_callable_release = emit_concat_reset(&mut builder);

        builder.emit(
            Op::PromoteLocalRefCell,
            vec![],
            Some(Immediate::LocalSlotPair {
                first: obj_slot,
                second: cell_owner,
            }),
            IrType::Void,
            PhpType::Object("Boxed".to_string()),
            Ownership::NonHeap,
        );
        let new_obj =
            builder.emit_load_local(obj_slot, IrType::Heap(IrHeapKind::Object), PhpType::Object("Boxed".to_string()));
        builder.emit_store_local(obj_slot, new_obj);
        reset_after_ref_cell_store = emit_concat_reset(&mut builder);

        builder.emit(
            Op::ReleaseLocalSlot,
            vec![],
            Some(Immediate::LocalSlot(callable_slot)),
            IrType::Void,
            PhpType::Callable,
            Ownership::NonHeap,
        );
        reset_after_slot_release = emit_concat_reset(&mut builder);

        let zero = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(zero) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function
            .instruction(reset_after_callable_release)
            .unwrap()
            .op,
        Op::ConcatReset
    );
    assert_eq!(
        function.instruction(reset_after_ref_cell_store).unwrap().op,
        Op::ConcatReset
    );
    assert_eq!(
        function.instruction(reset_after_slot_release).unwrap().op,
        Op::ConcatReset
    );
    assert!(validate_function(&function).is_ok());
}

/// A cyclic `Mixed` block-parameter phi (or cyclic `Mixed` local copy) without a
/// destructor-free base is not treated as destructor-free when released.
#[test]
fn cyclic_mixed_phi_release_is_not_treated_as_destructor_free() {
    let mut function = Function::new("cyclic_phi".to_string(), IrType::I64, PhpType::Int);
    let reset_after_release;
    {
        let mut builder = Builder::new(&mut function);
        let mixed_slot = builder.add_local(
            Some("in".to_string()),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        let loop_bb = builder.create_named_block(
            "loop",
            vec![(IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed)],
        );
        let exit_bb = builder.create_named_block("exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let init_val = builder.emit_load_local(
            mixed_slot,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        builder.terminate(Terminator::Br {
            target: loop_bb,
            args: vec![init_val],
        });

        let phi_val = builder.function().block(loop_bb).unwrap().params[0];
        builder.position_at_end(loop_bb);
        builder.emit(
            Op::Release,
            vec![phi_val],
            None,
            IrType::Void,
            PhpType::Mixed,
            Ownership::NonHeap,
        );
        reset_after_release = emit_concat_reset(&mut builder);
        builder.emit_store_local(mixed_slot, phi_val);
        let cond = builder.emit_const_bool(false);
        builder.terminate(Terminator::CondBr {
            cond,
            then_target: loop_bb,
            then_args: vec![phi_val],
            else_target: exit_bb,
            else_args: vec![],
        });

        builder.position_at_end(exit_bb);
        let zero = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(zero) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instruction(reset_after_release).unwrap().op,
        Op::ConcatReset,
        "releasing a cyclic Mixed phi without a destructor-free seed must keep its following reset"
    );
    assert!(validate_function(&function).is_ok());
}

/// Throwing an exception without an in-frame handler (`Terminator::Throw` or a
/// `MAY_THROW` instruction before a later reset) preserves the preceding
/// `ConcatReset` so scratch space used to construct the exception is reclaimed.
#[test]
fn unhandled_throw_preserves_preceding_concat_reset() {
    let mut function = Function::new("throw_no_handler".to_string(), IrType::I64, PhpType::Int);
    let reset_before_throw;
    {
        let mut builder = Builder::new(&mut function);
        let exc_slot = builder.add_local(
            Some("e".to_string()),
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object("Exception".to_string()),
            LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);

        let num = builder.emit_const_i64(99);
        let _dirty = builder
            .emit(
                Op::IToStr,
                vec![num],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("itostr produces a value");
        reset_before_throw = emit_concat_reset(&mut builder);
        let exc = builder.emit_load_local(
            exc_slot,
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object("Exception".to_string()),
        );
        builder.terminate(Terminator::Throw { value: exc });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instruction(reset_before_throw).unwrap().op,
        Op::ConcatReset
    );
    assert!(validate_function(&function).is_ok());
}

/// By-value iteration (`IterStart`, `IterNext`, `IterCurrentValue`, `IterEnd`) over
/// a concrete `int[]` array and inline scalar `Op::Cast` (`Str` -> `I64`) preserve
/// clean concat state and allow in-loop `ConcatReset` elimination.
#[test]
fn concrete_array_iterator_and_scalar_cast_preserve_clean_concat_state() {
    let mut function = Function::new("iter_and_cast".to_string(), IrType::I64, PhpType::Int);
    let (body_reset, exit_reset);
    {
        let mut builder = Builder::new(&mut function);
        let arr_slot = builder.add_local(
            Some("arr".to_string()),
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
            LocalKind::PhpLocal,
        );
        let iter_state = builder.add_local(
            None,
            IrType::Heap(IrHeapKind::Iterable),
            PhpType::Iterable,
            LocalKind::IteratorState,
        );
        let entry = builder.create_named_block("entry", vec![]);
        let header = builder.create_named_block("foreach.cond", vec![]);
        let body = builder.create_named_block("foreach.body", vec![]);
        let exit = builder.create_named_block("foreach.exit", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let s = builder.emit_const_str(DataId::from_raw(0));
        let _cast_int = builder
            .emit(
                Op::Cast,
                vec![s],
                Some(Immediate::CastTarget(IrType::I64)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .expect("cast produces a value");
        let arr = builder.emit_load_local(
            arr_slot,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
        );
        let iter_handle = builder
            .emit(
                Op::IterStart,
                vec![arr],
                Some(Immediate::IterStart(IterStartMetadata::new(
                    iter_state, false, None, None,
                ))),
                IrType::Heap(IrHeapKind::Iterable),
                PhpType::Iterable,
                Ownership::Owned,
            )
            .expect("iter_start produces a handle");
        builder.terminate(Terminator::Br {
            target: header,
            args: vec![],
        });

        builder.position_at_end(header);
        let has_next = builder
            .emit(
                Op::IterNext,
                vec![iter_handle],
                None,
                IrType::I64,
                PhpType::Bool,
                Ownership::NonHeap,
            )
            .expect("iter_next produces a bool");
        builder.terminate(Terminator::CondBr {
            cond: has_next,
            then_target: body,
            then_args: vec![],
            else_target: exit,
            else_args: vec![],
        });

        builder.position_at_end(body);
        let _elem = builder
            .emit(
                Op::IterCurrentValue,
                vec![iter_handle],
                None,
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .expect("iter_current_value produces a value");
        body_reset = emit_concat_reset(&mut builder);
        builder.terminate(Terminator::Br {
            target: header,
            args: vec![],
        });

        builder.position_at_end(exit);
        builder.emit(
            Op::IterEnd,
            vec![],
            Some(Immediate::LocalSlot(iter_state)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        exit_reset = emit_concat_reset(&mut builder);
        let zero = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(zero) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(function.instruction(body_reset).unwrap().op, Op::Nop);
    assert_eq!(function.instruction(exit_reset).unwrap().op, Op::Nop);
    assert!(validate_function(&function).is_ok());
}

/// `Op::FToI` with `Immediate::FloatKeyDiagnostic` and `Op::ArrayGet` format warning
/// diagnostics into `_concat_buf` at the current `_concat_off` without a capacity check,
/// so both must act as concat barriers and prevent backward coalescing of a preceding reset.
#[test]
fn warning_producing_float_key_and_array_get_act_as_concat_barriers() {
    let mut function = Function::new("warn_scratch_barriers".to_string(), IrType::I64, PhpType::Int);
    let (then_reset_before_ftoi, then_reset_before_get, merge_reset);
    {
        let mut builder = Builder::new(&mut function);
        let arr_slot = builder.add_local(
            Some("arr".to_string()),
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
            LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        let then_b = builder.create_named_block("then", vec![]);
        let else_b = builder.create_named_block("else", vec![]);
        let merge_b = builder.create_named_block("merge", vec![]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let cond = builder.emit_const_bool(true);
        builder.terminate(Terminator::CondBr {
            cond,
            then_target: then_b,
            then_args: vec![],
            else_target: else_b,
            else_args: vec![],
        });

        // `then` dirties scratch, resets, runs `FToI(FloatKeyDiagnostic)`, resets again,
        // and runs `ArrayGet` before jumping to `merge`.
        builder.position_at_end(then_b);
        let n1 = builder.emit_const_i64(1);
        let _s1 = builder
            .emit(
                Op::IToStr,
                vec![n1],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("itostr produces a value");
        then_reset_before_ftoi = emit_concat_reset(&mut builder);
        let fkey = builder.emit_const_f64(1.5);
        let ikey = builder
            .emit_with_effects(
                Op::FToI,
                vec![fkey],
                Some(Immediate::FloatKeyDiagnostic),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
                Effects::MAY_WARN,
                None,
            )
            .expect("ftoi produces a value");
        then_reset_before_get = emit_concat_reset(&mut builder);
        let arr = builder.emit_load_local(
            arr_slot,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
        );
        let _elem = builder
            .emit(
                Op::ArrayGet,
                vec![arr, ikey],
                None,
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .expect("array_get produces a value");
        builder.terminate(Terminator::Br {
            target: merge_b,
            args: vec![],
        });

        // `else` leaves concat state dirty going into `merge`.
        builder.position_at_end(else_b);
        let n2 = builder.emit_const_i64(2);
        let _s2 = builder
            .emit(
                Op::IToStr,
                vec![n2],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("itostr produces a value");
        builder.terminate(Terminator::Br {
            target: merge_b,
            args: vec![],
        });

        builder.position_at_end(merge_b);
        merge_reset = emit_concat_reset(&mut builder);
        let out = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(out) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instruction(then_reset_before_ftoi).unwrap().op,
        Op::ConcatReset,
        "reset before FToI(FloatKeyDiagnostic) must not coalesce across the warning formatter"
    );
    assert_eq!(
        function.instruction(then_reset_before_get).unwrap().op,
        Op::ConcatReset,
        "reset before ArrayGet must not coalesce across the undefined-key warning formatter"
    );
    assert_eq!(
        function.instruction(merge_reset).unwrap().op,
        Op::ConcatReset
    );
    assert!(validate_function(&function).is_ok());
}

/// Mutual recursion between an array slot `$dst`, an element slot `$v`, and
/// `Op::IterCurrentValue` over `$dst` (`$dst[$k] = $v; foreach ($dst as $k => $v)`)
/// terminates without infinite recursion and fails closed (`false`).
#[test]
fn cyclic_iterator_over_mutated_array_slot_terminates_and_stays_conservative() {
    let mut function = Function::new("cyclic_iter_slot".to_string(), IrType::I64, PhpType::Int);
    let reset_after_iter;
    {
        let mut builder = Builder::new(&mut function);
        let dst_slot = builder.add_local(
            Some("dst".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
            },
            LocalKind::PhpLocal,
        );
        let val_slot = builder.add_local(
            Some("v".to_string()),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::PhpLocal,
        );
        let iter_state = builder.add_local(
            None,
            IrType::Heap(IrHeapKind::Iterable),
            PhpType::Iterable,
            LocalKind::IteratorState,
        );
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);

        let init_hash = builder
            .emit(
                Op::HashNew,
                vec![],
                Some(Immediate::I64(0)),
                IrType::Heap(IrHeapKind::Hash),
                PhpType::AssocArray {
                    key: Box::new(PhpType::Str),
                    value: Box::new(PhpType::Mixed),
                },
                Ownership::Owned,
            )
            .expect("hash_new produces a value");
        builder.emit_store_local(dst_slot, init_hash);

        let dst_loaded = builder.emit_load_local(
            dst_slot,
            IrType::Heap(IrHeapKind::Hash),
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
            },
        );
        let key = builder.emit_const_str(DataId::from_raw(0));
        let prev_v =
            builder.emit_load_local(val_slot, IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
        builder.emit(
            Op::HashSet,
            vec![dst_loaded, key, prev_v],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );

        let iter_handle = builder
            .emit(
                Op::IterStart,
                vec![dst_loaded],
                Some(Immediate::IterStart(IterStartMetadata::new(
                    iter_state, false, None, None,
                ))),
                IrType::Heap(IrHeapKind::Iterable),
                PhpType::Iterable,
                Ownership::Owned,
            )
            .expect("iter_start produces a handle");
        let cur_v = builder
            .emit(
                Op::IterCurrentValue,
                vec![iter_handle],
                None,
                IrType::Heap(IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Borrowed,
            )
            .expect("iter_current_value produces a value");
        builder.emit_store_local(val_slot, cur_v);
        reset_after_iter = emit_concat_reset(&mut builder);
        let zero = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(zero) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instruction(reset_after_iter).unwrap().op,
        Op::ConcatReset
    );
    assert!(validate_function(&function).is_ok());
}

/// Spreading an associative array whose elements may hold `__destruct` objects via
/// `Op::HashSpread` records the spread source as a dependency of the `Op::HashNew`
/// allocation, so releasing the spread result or its local slot preserves the
/// following `Op::ConcatReset`.
#[test]
fn hash_spread_of_destructor_bearing_source_preserves_reset_after_release() {
    let mut function = Function::new("hash_spread_dtor".to_string(), IrType::I64, PhpType::Int);
    let (reset_after_spread, reset_after_slot_release, reset_after_val_release);
    {
        let mut builder = Builder::new(&mut function);
        let dtor_assoc_ty = PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Object("Dtor".to_string())),
        };
        let mixed_assoc_ty = PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Object("Dtor".to_string())),
        };
        let src_slot = builder.add_local(
            Some("src".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            dtor_assoc_ty.clone(),
            LocalKind::PhpLocal,
        );
        let copy_slot = builder.add_local(
            Some("copy".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            mixed_assoc_ty.clone(),
            LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);

        let hash = builder
            .emit(
                Op::HashNew,
                vec![],
                Some(Immediate::I64(0)),
                IrType::Heap(IrHeapKind::Hash),
                mixed_assoc_ty.clone(),
                Ownership::Owned,
            )
            .expect("hash_new produces a value");
        let src = builder.emit_load_local(
            src_slot,
            IrType::Heap(IrHeapKind::Hash),
            dtor_assoc_ty.clone(),
        );
        builder.emit(
            Op::HashSpread,
            vec![hash, src],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.emit_store_local(copy_slot, hash);

        reset_after_spread = emit_concat_reset(&mut builder);
        builder.emit(
            Op::ReleaseLocalSlot,
            vec![],
            Some(Immediate::LocalSlot(copy_slot)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        reset_after_slot_release = emit_concat_reset(&mut builder);

        let hash2 = builder
            .emit(
                Op::HashNew,
                vec![],
                Some(Immediate::I64(0)),
                IrType::Heap(IrHeapKind::Hash),
                mixed_assoc_ty,
                Ownership::Owned,
            )
            .expect("hash_new produces a value");
        let src2 = builder.emit_load_local(src_slot, IrType::Heap(IrHeapKind::Hash), dtor_assoc_ty);
        builder.emit(
            Op::HashSpread,
            vec![hash2, src2],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        let _clean_before_val_release = emit_concat_reset(&mut builder);
        builder.emit(
            Op::Release,
            vec![hash2],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        reset_after_val_release = emit_concat_reset(&mut builder);

        let n = builder.emit_const_i64(7);
        let _s = builder
            .emit(
                Op::IToStr,
                vec![n],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("itostr produces a value");
        let zero = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(zero) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instruction(reset_after_spread).unwrap().op,
        Op::ConcatReset
    );
    assert_eq!(
        function.instruction(reset_after_slot_release).unwrap().op,
        Op::ConcatReset,
        "ConcatReset after releasing a HashSpread local slot that may hold destructors must stay"
    );
    assert_eq!(
        function.instruction(reset_after_val_release).unwrap().op,
        Op::ConcatReset,
        "ConcatReset after releasing a HashSpread SSA value that may hold destructors must stay"
    );
    assert!(validate_function(&function).is_ok());
}

/// Spreading a destructor-free `int[]` promoted via `Op::ArrayToHash` keeps the
/// resulting `Op::HashNew` allocation destructor-free so releasing it preserves clean state.
#[test]
fn hash_spread_of_destructor_free_array_to_hash_allows_clean_release_reset_elimination() {
    let mut function =
        Function::new("hash_spread_int_clean".to_string(), IrType::I64, PhpType::Int);
    let (reset_after_spread, reset_after_release);
    {
        let mut builder = Builder::new(&mut function);
        let int_arr_ty = PhpType::Array(Box::new(PhpType::Int));
        let int_hash_ty = PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: Box::new(PhpType::Int),
        };
        let src_slot = builder.add_local(
            Some("src".to_string()),
            IrType::Heap(IrHeapKind::Array),
            int_arr_ty.clone(),
            LocalKind::PhpLocal,
        );
        let copy_slot = builder.add_local(
            Some("copy".to_string()),
            IrType::Heap(IrHeapKind::Hash),
            int_hash_ty.clone(),
            LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);

        let init_arr = builder
            .emit(
                Op::ArrayNew,
                vec![],
                Some(Immediate::I64(1)),
                IrType::Heap(IrHeapKind::Array),
                int_arr_ty.clone(),
                Ownership::Owned,
            )
            .expect("array_new produces a value");
        let one = builder.emit_const_i64(1);
        builder.emit(
            Op::ArrayPush,
            vec![init_arr, one],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.emit_store_local(src_slot, init_arr);

        let hash = builder
            .emit(
                Op::HashNew,
                vec![],
                Some(Immediate::I64(0)),
                IrType::Heap(IrHeapKind::Hash),
                int_hash_ty.clone(),
                Ownership::Owned,
            )
            .expect("hash_new produces a value");
        let src = builder.emit_load_local(src_slot, IrType::Heap(IrHeapKind::Array), int_arr_ty);
        let promoted = builder
            .emit(
                Op::ArrayToHash,
                vec![src],
                None,
                IrType::Heap(IrHeapKind::Hash),
                int_hash_ty,
                Ownership::Owned,
            )
            .expect("array_to_hash produces a value");
        builder.emit(
            Op::HashSpread,
            vec![hash, promoted],
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        builder.emit_store_local(copy_slot, hash);

        reset_after_spread = emit_concat_reset(&mut builder);
        builder.emit(
            Op::ReleaseLocalSlot,
            vec![],
            Some(Immediate::LocalSlot(copy_slot)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        reset_after_release = emit_concat_reset(&mut builder);
        let n = builder.emit_const_i64(7);
        let _s = builder
            .emit(
                Op::IToStr,
                vec![n],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("itostr produces a value");
        let zero = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(zero) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instruction(reset_after_spread).unwrap().op,
        Op::ConcatReset
    );
    assert_eq!(
        function.instruction(reset_after_release).unwrap().op,
        Op::Nop,
        "releasing a HashSpread slot populated only from destructor-free int[] should stay clean"
    );
    assert!(validate_function(&function).is_ok());
}

/// Passing an empty-array local (`Array(Never)`) to an opaque runtime/user call
/// disqualifies the slot from being treated as destructor-free on subsequent release.
#[test]
fn opaque_call_use_of_empty_array_local_disqualifies_slot_release() {
    let mut function =
        Function::new("opaque_empty_array_escape".to_string(), IrType::I64, PhpType::Int);
    let reset_after_release;
    {
        let mut builder = Builder::new(&mut function);
        let empty_arr_ty = PhpType::Array(Box::new(PhpType::Never));
        let arr_slot = builder.add_local(
            Some("arr".to_string()),
            IrType::Heap(IrHeapKind::Array),
            empty_arr_ty.clone(),
            LocalKind::PhpLocal,
        );
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);

        let init_arr = builder
            .emit(
                Op::ArrayNew,
                vec![],
                Some(Immediate::I64(0)),
                IrType::Heap(IrHeapKind::Array),
                empty_arr_ty.clone(),
                Ownership::Owned,
            )
            .expect("array_new produces a value");
        builder.emit_store_local(arr_slot, init_arr);

        let loaded =
            builder.emit_load_local(arr_slot, IrType::Heap(IrHeapKind::Array), empty_arr_ty);
        builder.emit_with_effects(
            Op::Call,
            vec![loaded],
            Some(Immediate::FunctionRef(crate::ir::FunctionId::from_raw(0))),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Effects::PURE,
            None,
        );
        let _reset_after_call = emit_concat_reset(&mut builder);
        builder.emit(
            Op::ReleaseLocalSlot,
            vec![],
            Some(Immediate::LocalSlot(arr_slot)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
        reset_after_release = emit_concat_reset(&mut builder);
        let n = builder.emit_const_i64(7);
        let _s = builder
            .emit(
                Op::IToStr,
                vec![n],
                None,
                IrType::Str,
                PhpType::Str,
                Ownership::Borrowed,
            )
            .expect("itostr produces a value");
        let zero = builder.emit_const_i64(0);
        builder.terminate(Terminator::Return { value: Some(zero) });
    }

    run_to_fixed_point(&mut function);
    assert_eq!(
        function.instruction(reset_after_release).unwrap().op,
        Op::ConcatReset,
        "releasing an array slot that escaped to a call must preserve the following ConcatReset"
    );
    assert!(validate_function(&function).is_ok());
}
