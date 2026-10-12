//! Purpose:
//! Tests constant placement, register costs, and target immediate boundaries.
//!
//! Called from:
//! - The Rust library test harness.
//!
//! Key details:
//! - Hand-built EIR prevents AST folding from hiding allocation decisions.
//! - Every supported target must avoid constant spill slots under pressure.

use std::collections::HashSet;

use crate::codegen::platform::{Arch, Target};
use crate::codegen::value_placement;
use crate::codegen_support::abi::immediate::{aarch64_logical_immediate, select_int_immediate, ImmediateOp};
use crate::ir::{Builder, Function, IrType, Op, Ownership, Terminator, ValueId};
use crate::ir_passes::{allocate_registers, Location, ScalarConstant};
use crate::types::PhpType;

/// Lists the complete supported target matrix.
fn targets() -> impl Iterator<Item = Target> {
    ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"]
        .into_iter().map(|name| Target::parse(name).unwrap())
}

/// Arithmetic immediate boundaries never silently truncate or zero-extend a 64-bit value.
#[test]
fn rematerialize_immediate_encoding_boundaries() {
    for op in [ImmediateOp::Add, ImmediateOp::Sub, ImmediateOp::Compare] {
        for n in [0, 1, 4095, 4096, 4095 << 12, -1, -4095, -4096, -(4095 << 12)] {
            assert!(select_int_immediate(Arch::AArch64, op, n).is_some(), "{op:?}: {n}");
        }
        for n in [4097, 1 << 24, i64::MIN, i64::MAX] {
            assert!(select_int_immediate(Arch::AArch64, op, n).is_none(), "{op:?}: {n}");
        }
    }
    for op in [ImmediateOp::Add, ImmediateOp::Sub, ImmediateOp::Mul, ImmediateOp::And,
               ImmediateOp::Or, ImmediateOp::Xor, ImmediateOp::Compare] {
        for n in [i32::MIN as i64, -1, 0, i32::MAX as i64] {
            assert!(select_int_immediate(Arch::X86_64, op, n).is_some());
        }
        for n in [i32::MIN as i64 - 1, i32::MAX as i64 + 1, 0xffff_ffff, i64::MIN, i64::MAX] {
            assert!(select_int_immediate(Arch::X86_64, op, n).is_none(), "{op:?}: {n}");
        }
    }
    assert!(select_int_immediate(Arch::AArch64, ImmediateOp::Mul, 3).is_none());
    let negative = select_int_immediate(Arch::AArch64, ImmediateOp::Compare, -4096).unwrap();
    assert_eq!((negative.mnemonic, negative.operand.as_str()), ("cmn", "#1, lsl #12"));
}

/// Covers every legal AArch64 logical mask and rejects unrelated 64-bit patterns.
#[test]
fn rematerialize_all_aarch64_logical_immediates() {
    let mut legal = HashSet::new();
    for width in [2, 4, 8, 16, 32, 64] {
        for ones in 1..width {
            let element = (1u64 << ones) - 1;
            let mut repeated = 0;
            for offset in (0..64).step_by(width) { repeated |= element << offset; }
            for rotation in 0..width { legal.insert(repeated.rotate_right(rotation as u32)); }
        }
    }
    assert_eq!(legal.len(), 5334);
    for &bits in &legal { assert!(aarch64_logical_immediate(bits), "{bits:#x}"); }
    for bits in [0, u64::MAX, 0x1234_5678_9abc_def0, 0x8000_0000_0000_0005] {
        assert!(!aarch64_logical_immediate(bits), "{bits:#x}");
    }
    let mut bits = 17u64;
    for _ in 0..10000 {
        bits = bits.wrapping_mul(6364136223846793005).wrapping_add(1);
        assert_eq!(aarch64_logical_immediate(bits), legal.contains(&bits));
    }
}

/// Scalars retain exact recipes and never reserve spill slots, including unused definitions.
#[test]
fn rematerialize_scalars_have_no_spill_slots() {
    let mut function = Function::new("recipes".into(), IrType::F64, PhpType::Float);
    let mut recipes = Vec::new();
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        for n in [0, 1, -1, i64::MIN, i64::MAX] {
            recipes.push((b.emit_const_i64(n), ScalarConstant::Integer(n)));
        }
        recipes.push((b.emit_const_bool(true), ScalarConstant::Integer(1)));
        recipes.push((b.emit_const_null(), ScalarConstant::Integer(0x7fff_ffff_ffff_fffe)));
        for bits in [0, 1 << 63, 0x7ff0_0000_0000_0000, 0x7ff8_0000_0000_1234] {
            recipes.push((b.emit_const_f64(f64::from_bits(bits)), ScalarConstant::Float(bits)));
        }
        b.terminate(Terminator::Return { value: Some(recipes.last().unwrap().0) });
    }
    for target in targets() {
        let allocation = allocate_registers(&function, target);
        let placement = value_placement::allocate(&function, &allocation);
        assert_eq!(placement.total_slot_bytes, 0, "{target:?}");
        for &(id, recipe) in &recipes {
            assert_eq!(allocation.rematerialized(id), Some(recipe), "{target:?}");
            assert_eq!(allocation.location(id), Location::Rematerialized);
        }
    }
    function.flags.is_generator = true;
    for target in targets() {
        let allocation = allocate_registers(&function, target);
        assert_eq!(value_placement::allocate(&function, &allocation).total_slot_bytes, 0);
        assert!(recipes.iter().all(|(id, recipe)| allocation.rematerialized(*id) == Some(*recipe)));
    }
}

/// Repeated encodable uses never keep an otherwise expensive logical mask in a register.
#[test]
fn rematerialize_immediate_only_loop_mask() {
    let (mut function, constants) = wide_loop(1, false, false);
    let value = constants[0];
    let crate::ir::ValueDef::Instruction { inst, .. } = function.value(value).unwrap().def else { unreachable!() };
    function.instructions[inst.as_raw() as usize].immediate = Some(crate::ir::Immediate::I64(0x00ff_00ff_00ff_00ff));
    for target in targets().filter(|target| target.arch == Arch::AArch64) {
        assert!(allocate_registers(&function, target).rematerialized(value).is_some());
    }
}

/// Long ranges and call preservation can outweigh two uses of a multi-instruction constant.
#[test]
fn rematerialize_live_range_and_call_cost() {
    for (padding, call, keep) in [(0, false, true), (96, false, false), (0, true, false)] {
        let mut function = Function::new("cost".into(), IrType::I64, PhpType::Int);
        let constant;
        {
            let mut b = Builder::new(&mut function);
            let entry = b.create_named_block("entry", vec![(IrType::I64, PhpType::Int)]);
            b.set_entry(entry);
            b.position_at_end(entry);
            constant = b.emit_const_i64(0x1234_5678_9abc_def0);
            for _ in 0..padding {
                b.emit(Op::Nop, vec![], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            if call {
                b.emit(Op::Call, vec![], Some(crate::ir::Immediate::FunctionRef(crate::ir::FunctionId::from_raw(0))),
                    IrType::Void, PhpType::Void, Ownership::NonHeap);
            }
            let first = b.emit_iadd(b.block_param(entry, 0), constant);
            let second = b.emit_iadd(first, constant);
            b.terminate(Terminator::Return { value: Some(second) });
        }
        let target = Target::parse("linux-aarch64").unwrap();
        let allocation = allocate_registers(&function, target);
        assert_eq!(allocation.register_of(constant).is_some(), keep, "padding={padding}, call={call}");
    }
}

/// Builds a loop using one or many wide constants that cannot fold into mask instructions.
fn wide_loop(count: usize, with_call: bool, float: bool) -> (Function, Vec<ValueId>) {
    let ty = if float { IrType::F64 } else { IrType::I64 };
    let php = if float { PhpType::Float } else { PhpType::Int };
    let mut function = Function::new("wide_loop".into(), ty, php.clone());
    let mut constants = Vec::new();
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        let header = b.create_named_block("loop", vec![(ty, php.clone())]);
        b.set_entry(entry);
        b.position_at_end(entry);
        for index in 0..count {
            constants.push(if float { b.emit_const_f64(1.2345 + index as f64) }
                else { b.emit_const_i64(0x1234_5678_9abc_def0 + index as i64) });
        }
        b.terminate(Terminator::Br { target: header, args: vec![constants[0]] });
        b.position_at_end(header);
        if with_call {
            b.emit(Op::ConcatReset, vec![], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
            b.emit(Op::Call, vec![], Some(crate::ir::Immediate::FunctionRef(crate::ir::FunctionId::from_raw(0))),
                IrType::Void, PhpType::Void, Ownership::NonHeap);
        }
        let mut acc = b.block_param(header, 0);
        for &constant in &constants {
            acc = b.emit(if float { Op::FAdd } else { Op::IBitXor }, vec![acc, constant], None,
                ty, php.clone(), Ownership::NonHeap).unwrap();
        }
        b.terminate(Terminator::Br { target: header, args: vec![acc] });
    }
    (function, constants)
}

/// Expensive loop constants stay in registers when their reuse pays for the live range.
#[test]
fn rematerialize_costly_loop_constant_uses_register() {
    for float in [false, true] {
        let (function, constants) = wide_loop(1, false, float);
        for target in targets() {
            let allocation = allocate_registers(&function, target);
            assert!(allocation.register_of(constants[0]).is_some(), "{target:?}, float={float}");
            assert_eq!(value_placement::allocate(&function, &allocation).slot(constants[0]), None);
        }
    }
}

/// Pressure and calls fall back to recipes instead of generating stores and spill reloads.
#[test]
fn rematerialize_evicted_constants_and_call_crossing_floats() {
    for float in [false, true] {
        let (function, constants) = wide_loop(20, true, float);
        for target in targets() {
            let allocation = allocate_registers(&function, target);
            let placement = value_placement::allocate(&function, &allocation);
            assert!(constants.iter().any(|id| allocation.rematerialized(*id).is_some()));
            for &id in &constants { assert_eq!(placement.slot(id), None, "{target:?}"); }
            if float && target.arch == Arch::X86_64 {
                assert!(constants.iter().all(|id| allocation.rematerialized(*id).is_some()));
            }
        }
    }
}
