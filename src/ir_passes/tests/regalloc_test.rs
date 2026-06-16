//! Purpose:
//! Tests for the linear-scan register allocator over EIR functions.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Functions are built by hand with `crate::ir::Builder`. Tests target the
//!   AArch64 pool (eight integer, seven float callee-saved registers) so pool
//!   sizes are deterministic.

use crate::codegen::platform::{Arch, Platform, Target};
use crate::ir::{Builder, Function, Immediate, IrType, Op, Ownership, Terminator, ValueId};
use crate::ir_passes::allocate_registers;
use crate::types::PhpType;

/// The AArch64 target used by these tests, giving a fixed pool size.
fn aarch64() -> Target {
    Target::new(Platform::Linux, Arch::AArch64)
}

/// Emits a float constant in the current block and returns its value.
fn emit_const_f64(builder: &mut Builder<'_>, value: f64) -> ValueId {
    builder
        .emit(
            Op::ConstF64,
            vec![],
            Some(Immediate::F64(value)),
            IrType::F64,
            PhpType::Float,
            Ownership::NonHeap,
        )
        .expect("const_f64 produces a value")
}

/// A purely arithmetic, call-free function assigns every temporary a
/// caller-saved register and records no callee-saved usage, so a leaf function
/// needs no prologue/epilogue save/restore.
#[test]
fn straight_line_integers_use_caller_saved_registers() {
    let mut function = Function::new("ints".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let v0 = builder.emit_const_i64(1);
        let v1 = builder.emit_const_i64(2);
        let v2 = builder.emit_iadd(v0, v1);
        builder.terminate(Terminator::Return { value: Some(v2) });
    }

    let allocation = allocate_registers(&function, aarch64());

    let caller_int = ["x12", "x13", "x14", "x15"];
    for raw in 0..3 {
        let reg = allocation.register_of(ValueId::from_raw(raw));
        assert!(reg.is_some(), "value v{raw} should receive a register");
        assert!(
            caller_int.contains(&reg.unwrap()),
            "call-free integer value v{raw} should use a caller-saved register, got {:?}",
            reg
        );
    }
    assert!(
        allocation.used_callee_saved().is_empty(),
        "a call-free function must not save any callee-saved registers"
    );
}

/// With more simultaneously-live values than the pool holds, the allocator
/// spills at least one value rather than assigning a register twice, and never
/// reports using more registers than the pool provides.
#[test]
fn register_pressure_forces_spills() {
    // AArch64 call-free integer pool holds 12 (4 caller-saved + 8 callee-saved).
    const COUNT: u32 = 16;
    let mut function = Function::new("pressure".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);

        let consts: Vec<ValueId> = (0..COUNT).map(|i| builder.emit_const_i64(i as i64)).collect();
        // Fold left so every constant stays live until its add: the first add
        // sees all later constants still pending, creating peak pressure.
        let mut acc = consts[0];
        for &c in &consts[1..] {
            acc = builder.emit_iadd(acc, c);
        }
        builder.terminate(Terminator::Return { value: Some(acc) });
    }

    let allocation = allocate_registers(&function, aarch64());

    let spilled = (0..COUNT)
        .filter(|&i| allocation.register_of(ValueId::from_raw(i)).is_none())
        .count();
    assert!(spilled >= 1, "more live values than registers must spill some");
    assert!(
        allocation.used_callee_saved().len() <= 8,
        "cannot use more than the eight-register callee-saved integer pool"
    );
}

/// Integer and float values draw from separate register pools.
#[test]
fn integers_and_floats_use_separate_pools() {
    let mut function = Function::new("mixed".to_string(), IrType::I64, PhpType::Int);
    let (int_val, float_val) = {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let i = builder.emit_const_i64(1);
        let f = emit_const_f64(&mut builder, 2.5);
        builder.terminate(Terminator::Return { value: Some(i) });
        (i, f)
    };

    let allocation = allocate_registers(&function, aarch64());

    assert!(
        allocation.register_of(int_val).unwrap().starts_with('x'),
        "integer value uses an x-register"
    );
    assert!(
        allocation.register_of(float_val).unwrap().starts_with('d'),
        "float value uses a d-register"
    );
}

/// Block parameters and branch arguments stay in stack slots so the existing
/// slot-based block-parameter moves remain correct.
#[test]
fn block_parameters_and_branch_arguments_stay_spilled() {
    let mut function = Function::new("params".to_string(), IrType::I64, PhpType::Int);
    let (arg, param) = {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let body = builder.create_named_block("body", vec![(IrType::I64, PhpType::Int)]);
        builder.set_entry(entry);

        builder.position_at_end(entry);
        let arg = builder.emit_const_i64(7);
        builder.terminate(Terminator::Br {
            target: body,
            args: vec![arg],
        });

        let param = builder.block_param(body, 0);
        builder.position_at_end(body);
        builder.terminate(Terminator::Return { value: Some(param) });
        (arg, param)
    };

    let allocation = allocate_registers(&function, aarch64());

    assert_eq!(
        allocation.register_of(arg),
        None,
        "a branch argument must stay in its slot"
    );
    assert_eq!(
        allocation.register_of(param),
        None,
        "a block parameter must stay in its slot"
    );
}

/// The x86_64 target used by these tests, exercising the caller-saved float
/// pool (x86_64 has no callee-saved XMM registers).
fn x86_64() -> Target {
    Target::new(Platform::Linux, Arch::X86_64)
}

/// Emits a void-result call that no other value consumes, acting purely as a
/// clobber point between its surrounding instructions.
fn emit_clobber_call(builder: &mut Builder<'_>) {
    builder.emit(
        Op::Call,
        vec![],
        Some(Immediate::I64(0)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
}

/// A value that lives across a call must use a callee-saved register, which
/// survives the call, and the function records that register for save/restore.
#[test]
fn value_live_across_call_uses_callee_saved() {
    let mut function = Function::new("across".to_string(), IrType::I64, PhpType::Int);
    let live = {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let v0 = builder.emit_const_i64(1);
        emit_clobber_call(&mut builder); // clobber point between def and use
        let v1 = builder.emit_iadd(v0, v0); // v0 is live across the call
        builder.terminate(Terminator::Return { value: Some(v1) });
        v0
    };

    let allocation = allocate_registers(&function, aarch64());

    let reg = allocation.register_of(live).expect("cross-call value gets a register");
    let callee_int = ["x21", "x22", "x23", "x24", "x25", "x26", "x27", "x28"];
    assert!(
        callee_int.contains(&reg),
        "a value live across a call must use a callee-saved register, got {reg}"
    );
    assert!(
        allocation.used_callee_saved().contains(&reg),
        "the callee-saved register holding a cross-call value must be recorded"
    );
}

/// Regression: a call's *result* must never use a caller-saved register, even
/// when nothing else clobbers between the call and the result's use. The call
/// lowering runs its argument-cleanup calls (`decref`) AFTER storing the result,
/// which would clobber a caller-saved result register. The defining instruction
/// being a clobber (its position is `start`) must disqualify the value from the
/// caller-saved pool.
#[test]
fn call_result_is_never_caller_saved() {
    let mut function = Function::new("callret".to_string(), IrType::I64, PhpType::Int);
    let call_result = {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        // A call that produces an integer result consumed by a volatile-safe op.
        let result = builder
            .emit(
                Op::Call,
                vec![],
                Some(Immediate::I64(0)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
            )
            .expect("call produces a value");
        let doubled = builder.emit_iadd(result, result);
        builder.terminate(Terminator::Return { value: Some(doubled) });
        result
    };

    let allocation = allocate_registers(&function, aarch64());

    let caller_saved = ["x12", "x13", "x14", "x15", "d16", "d17", "d18", "d19"];
    if let Some(reg) = allocation.register_of(call_result) {
        assert!(
            !caller_saved.contains(&reg),
            "a call result must not live in a caller-saved register (the call's \
             trailing cleanup clobbers it), got {reg}"
        );
    }
}

/// On x86_64, where there are no callee-saved XMM registers, a call-free float
/// value is still register-allocated from the caller-saved XMM pool.
#[test]
fn call_free_float_uses_caller_saved_xmm_on_x86_64() {
    let mut function = Function::new("floats".to_string(), IrType::F64, PhpType::Float);
    let sum = {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let a = emit_const_f64(&mut builder, 1.5);
        let b = emit_const_f64(&mut builder, 2.5);
        let sum = builder
            .emit(
                Op::FAdd,
                vec![a, b],
                None,
                IrType::F64,
                PhpType::Float,
                Ownership::NonHeap,
            )
            .expect("fadd produces a value");
        builder.terminate(Terminator::Return { value: Some(sum) });
        sum
    };

    let allocation = allocate_registers(&function, x86_64());

    let reg = allocation.register_of(sum).expect("call-free float gets a register on x86_64");
    assert!(
        reg.starts_with("xmm"),
        "call-free float should use a caller-saved XMM register, got {reg}"
    );
    assert!(
        allocation.used_callee_saved().is_empty(),
        "caller-saved XMM registers need no save/restore"
    );
}

/// A float value that lives across a call on x86_64 cannot be register-allocated
/// (no callee-saved XMM and the caller-saved pool is clobbered by the call), so
/// it stays spilled.
#[test]
fn float_live_across_call_spills_on_x86_64() {
    let mut function = Function::new("fspill".to_string(), IrType::F64, PhpType::Float);
    let live = {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let a = emit_const_f64(&mut builder, 1.5);
        emit_clobber_call(&mut builder);
        let sum = builder
            .emit(
                Op::FAdd,
                vec![a, a],
                None,
                IrType::F64,
                PhpType::Float,
                Ownership::NonHeap,
            )
            .expect("fadd produces a value");
        builder.terminate(Terminator::Return { value: Some(sum) });
        a
    };

    let allocation = allocate_registers(&function, x86_64());

    assert_eq!(
        allocation.register_of(live),
        None,
        "a float live across a call on x86_64 must stay spilled"
    );
}

/// Under register pressure the use-weighted spill heuristic keeps a
/// frequently-used value in a register and spills a rarely-used one instead.
#[test]
fn spill_heuristic_keeps_frequently_used_value() {
    // Fill the entire call-free integer pool (4 caller + 8 callee = 12) with
    // long-lived constants, then introduce one more long-lived value. The pool
    // is exhausted, so the allocator must spill exactly one of the long-lived
    // intervals. The hot value below is used far more often than the others.
    const FILLERS: u32 = 12;
    let mut function = Function::new("heur".to_string(), IrType::I64, PhpType::Int);
    let (hot, fillers) = {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);

        // One value used many times: defined first, kept live to the end.
        let hot = builder.emit_const_i64(1);
        let fillers: Vec<ValueId> = (0..FILLERS).map(|i| builder.emit_const_i64(i as i64 + 2)).collect();
        // Reference `hot` repeatedly so its use weight dominates.
        let mut acc = hot;
        for _ in 0..6 {
            acc = builder.emit_iadd(acc, hot);
        }
        // Keep every filler live to the same point by folding them in last.
        for &f in &fillers {
            acc = builder.emit_iadd(acc, f);
        }
        builder.terminate(Terminator::Return { value: Some(acc) });
        (hot, fillers)
    };

    let allocation = allocate_registers(&function, aarch64());

    assert!(
        allocation.register_of(hot).is_some(),
        "the most frequently-used value must keep its register under pressure"
    );
    let spilled_fillers = fillers
        .iter()
        .filter(|&&f| allocation.register_of(f).is_none())
        .count();
    assert!(
        spilled_fillers >= 1,
        "a rarely-used value should be spilled before the hot value"
    );
}

/// Generator functions fall back to all-spilled in this first cut, because
/// values must live in the generator state frame across suspends.
#[test]
fn generator_functions_are_all_spilled() {
    let mut function = Function::new("gen".to_string(), IrType::I64, PhpType::Int);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let v0 = builder.emit_const_i64(1);
        builder.terminate(Terminator::Return { value: Some(v0) });
    }
    function.flags.is_generator = true;

    let allocation = allocate_registers(&function, aarch64());

    assert_eq!(allocation.register_of(ValueId::from_raw(0)), None);
    assert!(allocation.used_callee_saved().is_empty());
}
