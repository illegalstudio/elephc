//! Purpose:
//! Exercises range-domain soundness and static expression graph complexity.
//!
//! Called from:
//! - The integer-range pass unit-test harness.
//!
//! Key details:
//! - Unsupported shared expressions must be cached as unknown.
//! - Deep graphs must not consume the native call stack.
//! - Scalar top and literal facts stay implicit instead of inflating CFG states.
//! - Iterative fact-availability queries must match the original dominance analysis.

use super::*;
use crate::ir::Builder;

/// Emits a scalar operation for generated concrete-trace fixtures.
fn emit_scalar_binop(builder: &mut Builder<'_>, op: Op, lhs: ValueId, rhs: ValueId) -> ValueId {
    builder.emit(op, vec![lhs, rhs], None, IrType::I64, PhpType::Int, Ownership::NonHeap).unwrap()
}

/// Emits a signed comparison for generated concrete-trace fixtures.
fn emit_icmp(builder: &mut Builder<'_>, lhs: ValueId, rhs: ValueId, predicate: CmpPredicate) -> ValueId {
    builder.emit(Op::ICmp, vec![lhs, rhs], Some(Immediate::CmpPredicate(predicate)),
        IrType::I64, PhpType::Bool, Ownership::NonHeap).unwrap()
}

/// Builds a shared unsupported expression graph and requires unknown results to be memoized.
#[test]
fn shared_unknown_expression_is_memoized() {
    let mut function = Function::new("shared".to_string(), IrType::F64, PhpType::Float);
    let result;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let mut value = builder.emit_const_f64(1.0);
        for _ in 0..16 {
            value = builder.emit(
                Op::FAdd, vec![value, value], None,
                IrType::F64, PhpType::Float, Ownership::NonHeap,
            ).expect("float addition");
        }
        result = value;
        builder.terminate(Terminator::Return { value: Some(value) });
    }
    let mut memo = HashMap::new();
    assert_eq!(static_value_range(
        &function, result, &HashMap::new(), &mut memo,
    ), None);
    assert_eq!(memo.len(), 17, "unknown subexpressions must be memoized too");
}

/// A long linear dependency chain uses the heap worklist instead of recursive stack frames.
#[test]
fn deep_expression_uses_bounded_call_stack() {
    let mut function = Function::new("deep".to_string(), IrType::I64, PhpType::Int);
    let result;
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let one = builder.emit_const_i64(1);
        let mut value = one;
        for _ in 0..20_000 {
            value = builder.emit(
                Op::IAdd, vec![value, one], None,
                IrType::I64, PhpType::Int, Ownership::NonHeap,
            ).expect("integer addition");
        }
        result = value;
        builder.terminate(Terminator::Return { value: Some(value) });
    }
    let mut memo = HashMap::new();
    assert_eq!(static_value_range(
        &function, result, &HashMap::new(), &mut memo,
    ), Some(IntRange::point(20_001)));
    assert_eq!(memo.len(), 20_001);
}

/// A large sequence of unconstrained scalars does not populate every outgoing edge state.
#[test]
fn unconstrained_scalars_and_literals_keep_entry_states_sparse() {
    let mut function = Function::new("sparse_unknowns".to_string(), IrType::Void, PhpType::Void);
    let (next, one, unknown);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        next = builder.create_named_block("next", vec![(IrType::I64, PhpType::Int)]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let local = builder.add_local(Some("unknown".to_string()), IrType::I64,
            PhpType::Int, crate::ir::LocalKind::PhpLocal);
        for _ in 0..1_024 {
            builder.emit_load_local(local, IrType::I64, PhpType::Int);
        }
        unknown = builder.emit_load_local(local, IrType::I64, PhpType::Int);
        one = builder.emit_const_i64(1);
        let sum = emit_scalar_binop(&mut builder, Op::ICheckedAddToInt, unknown, one);
        builder.terminate(Terminator::Br { target: next, args: vec![sum] });
        builder.position_at_end(next);
        builder.terminate(Terminator::Return { value: None });
    }
    validate_function(&function).unwrap();
    let states = analyze_block_entries(&function, &HashMap::new()).unwrap();
    let state = states[next.as_raw() as usize].as_ref().unwrap();
    assert!(state.is_empty(), "redundant facts inflated the state: {}", state.len());
    assert_eq!(range_for_value(&function, state, unknown), Some(IntRange::full()));
    assert_eq!(range_for_value(&function, state, one), Some(IntRange::point(1)));
    assert!(collect_safe_candidates(&function, &states).is_empty());
}

/// An implicit literal still supplies a precise range through an explicit block argument.
#[test]
fn implicit_literal_arguments_preserve_checked_arithmetic_proofs() {
    let mut function = Function::new("literal_argument".to_string(), IrType::I64, PhpType::Int);
    let (next, literal, parameter, result);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        next = builder.create_named_block("next", vec![(IrType::I64, PhpType::Int)]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        literal = builder.emit_const_i64(7);
        builder.terminate(Terminator::Br { target: next, args: vec![literal] });
        builder.position_at_end(next);
        parameter = builder.block_param(next, 0);
        let one = builder.emit_const_i64(1);
        result = emit_scalar_binop(&mut builder, Op::ICheckedAddToInt, parameter, one);
        builder.terminate(Terminator::Return { value: Some(result) });
    }
    validate_function(&function).unwrap();
    let states = analyze_block_entries(&function, &HashMap::new()).unwrap();
    let state = states[next.as_raw() as usize].as_ref().unwrap();
    assert_eq!(state.len(), 1);
    assert_eq!(state.get(&parameter), Some(&IntRange::point(7)));
    assert_eq!(range_for_value(&function, state, literal), Some(IntRange::point(7)));
    assert!(IntegerRange.run(&mut function, &mut DataPool::default()));
    let ValueDef::Instruction { inst, .. } = function.value(result).unwrap().def else {
        panic!("arithmetic result is not an instruction");
    };
    assert_eq!(function.instruction(inst).unwrap().op, Op::IAdd);
    validate_function(&function).unwrap();
}

/// A full-domain boxed fact still proves an integer tag, unlike an absent boxed fact.
#[test]
fn boxed_full_domain_facts_are_not_discarded() {
    let mut function = Function::new("boxed_full".to_string(), IrType::Void, PhpType::Void);
    let (next, boxed);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        next = builder.create_named_block("next", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let local = builder.add_local(Some("unknown".to_string()), IrType::I64,
            PhpType::Int, crate::ir::LocalKind::PhpLocal);
        let unknown = builder.emit_load_local(local, IrType::I64, PhpType::Int);
        let zero = builder.emit_const_i64(0);
        boxed = builder.emit(Op::ICheckedAdd, vec![unknown, zero], None,
            IrType::Heap(crate::ir::IrHeapKind::Mixed), PhpType::Mixed, Ownership::Owned).unwrap();
        builder.terminate(Terminator::Br { target: next, args: vec![] });
        builder.position_at_end(next);
        builder.emit(Op::Release, vec![boxed], None, IrType::Void, PhpType::Void, Ownership::NonHeap);
        builder.terminate(Terminator::Return { value: None });
    }
    validate_function(&function).unwrap();
    let states = analyze_block_entries(&function, &HashMap::new()).unwrap();
    let state = states[next.as_raw() as usize].as_ref().unwrap();
    assert_eq!(state.len(), 1);
    assert_eq!(state.get(&boxed), Some(&IntRange::full()));
    assert_eq!(range_for_value(&function, &RangeState::new(), boxed), None);
}

/// Iterative dominance intervals agree with the original queries on deep and unreachable blocks.
#[test]
fn value_availability_matches_deep_dominance() {
    let mut function = Function::new("deep_dominance".to_string(), IrType::Void, PhpType::Void);
    let blocks;
    {
        let mut builder = Builder::new(&mut function);
        blocks = (0..513).map(|index| builder.create_named_block(format!("block_{index}"), vec![])).collect::<Vec<_>>();
        builder.set_entry(blocks[0]);
        for pair in blocks[..512].windows(2) {
            builder.position_at_end(pair[0]);
            builder.terminate(Terminator::Br { target: pair[1], args: vec![] });
        }
        for &block in &blocks[511..] {
            builder.position_at_end(block);
            builder.terminate(Terminator::Return { value: None });
        }
    }
    validate_function(&function).unwrap();
    let dominance = compute_dominance(&function);
    let availability = ValueAvailability::new(&function, &dominance);
    for &anchor in &[blocks[0], blocks[3], blocks[511], blocks[512]] {
        for &block in &blocks {
            assert_eq!(availability.dominates(anchor, block), dominance.dominates(anchor, block));
            assert_eq!(availability.dominates(block, anchor), dominance.dominates(block, anchor));
        }
    }
}

/// Samples narrow and wide intervals around zero, powers of two, and signed overflow limits.
fn sample_ranges() -> Vec<IntRange> {
    let points = [i64::MIN, i64::MIN + 1, -256, -16, -1, 0, 1, 16, 255, i64::MAX - 1, i64::MAX];
    let mut ranges: Vec<_> = points.iter().copied().map(IntRange::point).collect();
    ranges.extend(points.windows(2).map(|ends| IntRange { lo: ends[0], hi: ends[1] }));
    ranges.push(IntRange::full());
    ranges
}

/// Chooses endpoints and an overflow-safe midpoint from one inclusive interval.
fn samples(range: IntRange) -> [i64; 3] {
    [range.lo, ((range.lo as i128 + range.hi as i128) / 2) as i64, range.hi]
}

/// Checks that a mathematical result lies in the interval asserted by the analysis.
fn assert_contains(range: IntRange, value: i128) {
    assert!(range.lo as i128 <= value && value <= range.hi as i128, "{value} outside {range:?}");
}

/// Arithmetic and bitwise transfer facts contain every sampled concrete result.
#[test]
fn sampled_transfer_ranges_are_sound() {
    let ranges = sample_ranges();
    for &lhs in &ranges {
        for &rhs in &ranges {
            for x in samples(lhs) {
                for y in samples(rhs) {
                    for (range, actual) in [
                        (checked_add(lhs, rhs), x as i128 + y as i128),
                        (checked_sub(lhs, rhs), x as i128 - y as i128),
                        (checked_mul(lhs, rhs), x as i128 * y as i128),
                        (bitand_range(lhs, rhs), (x & y) as i128),
                        (bitor_range(lhs, rhs), (x | y) as i128),
                        (bitxor_range(lhs, rhs), (x ^ y) as i128),
                    ] {
                        if let Some(range) = range {
                            assert_contains(range, actual);
                        }
                    }
                }
            }
        }
        for count in 0..=63 {
            for value in samples(lhs) {
                if let Some(range) = shift_left_range(lhs, IntRange::point(count)) {
                    assert_contains(range, value.wrapping_shl(count as u32) as i128);
                }
                if let Some(range) = shift_right_range(lhs, IntRange::point(count)) {
                    assert_contains(range, (value >> count) as i128);
                }
            }
        }
    }
}

/// Proven induction summaries contain all sampled headers and never permit a wrapping update.
#[test]
fn sampled_induction_summaries_are_sound() {
    let ranges = sample_ranges();
    for &init in &ranges {
        for &bound in &ranges {
            for step in [-16, -3, -1, 1, 3, 16] {
                for predicate in [CmpPredicate::Slt, CmpPredicate::Sle, CmpPredicate::Sgt, CmpPredicate::Sge] {
                    let Some(summary) = induction_summary(init, step, predicate, bound) else {
                        continue;
                    };
                    for initial in samples(init) {
                        for limit in samples(bound) {
                            let mut value = initial;
                            for _ in 0..32 {
                                assert_contains(summary, value as i128);
                                let continues = match predicate {
                                    CmpPredicate::Slt => value < limit,
                                    CmpPredicate::Sle => value <= limit,
                                    CmpPredicate::Sgt => value > limit,
                                    CmpPredicate::Sge => value >= limit,
                                    _ => unreachable!(),
                                };
                                if !continues {
                                    break;
                                }
                                value = value.checked_add(step).expect("proven recurrence overflowed");
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Signed comparison refinements never discard a satisfying pair at domain boundaries.
#[test]
fn sampled_comparison_refinements_are_sound() {
    let ranges = sample_ranges();
    for &lhs in &ranges {
        for &rhs in &ranges {
            for predicate in [
                CmpPredicate::Eq, CmpPredicate::Ne, CmpPredicate::Slt,
                CmpPredicate::Sle, CmpPredicate::Sgt, CmpPredicate::Sge,
            ] {
                for x in samples(lhs) {
                    for y in samples(rhs) {
                        let satisfies = match predicate {
                            CmpPredicate::Eq => x == y,
                            CmpPredicate::Ne => x != y,
                            CmpPredicate::Slt => x < y,
                            CmpPredicate::Sle => x <= y,
                            CmpPredicate::Sgt => x > y,
                            CmpPredicate::Sge => x >= y,
                            _ => unreachable!(),
                        };
                        if satisfies {
                            let (left, right) = refine_ranges(lhs, predicate, rhs)
                                .expect("satisfying comparison marked unreachable");
                            assert_contains(left, x as i128);
                            assert_contains(right, y as i128);
                        }
                    }
                }
            }
        }
    }
}

/// Checks generated cyclic CFGs against concrete traces, including simultaneous parameter swaps.
#[test]
fn generated_loop_proofs_match_concrete_traces() {
    let seeds = [i64::MIN, i64::MIN + 1, -257, -1, 0, 1, 255, i64::MAX - 1, i64::MAX];
    for split_latches in [false, true] {
        for descending in [false, true] {
            for mask in [7, 255, i64::MAX] {
                for x in seeds {
                    for y in seeds {
                        check_loop_trace(x, y, mask, descending, split_latches);
                    }
                }
            }
        }
    }
}

/// Builds a counter loop carrying two evolving integers and compares each rewrite to i128 math.
fn check_loop_trace(mut x: i64, mut y: i64, mask: i64, descending: bool, split_latches: bool) {
    let mut function = Function::new("trace_loop".to_string(), IrType::Void, PhpType::Void);
    let (product, sum, update);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let header = builder.create_named_block("header", vec![(IrType::I64, PhpType::Int); 3]);
        let body = builder.create_named_block("body", vec![]);
        let exit = builder.create_named_block("exit", vec![]);
        let then_block = split_latches.then(|| builder.create_named_block("then", vec![]));
        let else_block = split_latches.then(|| builder.create_named_block("else", vec![]));
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let first = builder.emit_const_i64(if descending { 8 } else { 0 });
        let last = builder.emit_const_i64(if descending { 0 } else { 8 });
        let step = builder.emit_const_i64(if descending { -1 } else { 1 });
        let zero = builder.emit_const_i64(0);
        let one = builder.emit_const_i64(1);
        let three = builder.emit_const_i64(3);
        let mask_value = builder.emit_const_i64(mask);
        let start_x = builder.emit_const_i64(x);
        let start_y = builder.emit_const_i64(y);
        builder.terminate(Terminator::Br { target: header, args: vec![first, start_x, start_y] });
        builder.position_at_end(header);
        let count = builder.block_param(header, 0);
        let current_x = builder.block_param(header, 1);
        let current_y = builder.block_param(header, 2);
        let condition = emit_icmp(&mut builder, count, last,
            if descending { CmpPredicate::Sgt } else { CmpPredicate::Slt });
        builder.terminate(Terminator::CondBr { cond: condition, then_target: body,
            then_args: vec![], else_target: exit, else_args: vec![] });
        builder.position_at_end(body);
        product = emit_scalar_binop(&mut builder, Op::ICheckedMulToInt, current_x, three);
        let shifted = emit_scalar_binop(&mut builder, Op::IShl, current_y, one);
        sum = emit_scalar_binop(&mut builder, Op::ICheckedAddToInt, shifted, current_x);
        let masked = emit_scalar_binop(&mut builder, Op::IBitAnd, current_x, mask_value);
        update = emit_scalar_binop(&mut builder, Op::ICheckedAddToInt, count, step);
        let parity = emit_scalar_binop(&mut builder, Op::IBitAnd, count, one);
        let even = emit_icmp(&mut builder, parity, zero, CmpPredicate::Eq);
        let then_args = vec![update, current_y, masked];
        let else_args = vec![update, shifted, current_x];
        if let (Some(then_block), Some(else_block)) = (then_block, else_block) {
            builder.terminate(Terminator::CondBr { cond: even, then_target: then_block,
                then_args: vec![], else_target: else_block, else_args: vec![] });
            builder.position_at_end(then_block);
            builder.terminate(Terminator::Br { target: header, args: then_args });
            builder.position_at_end(else_block);
            builder.terminate(Terminator::Br { target: header, args: else_args });
        } else {
            builder.terminate(Terminator::CondBr { cond: even, then_target: header,
                then_args, else_target: header, else_args });
        }
        builder.position_at_end(exit);
        builder.terminate(Terminator::Return { value: None });
    }
    assert!(validate_function(&function).is_ok());
    let dominance = compute_dominance(&function);
    let availability = ValueAvailability::new(&function, &dominance);
    for left in &function.blocks {
        for right in &function.blocks {
            assert_eq!(availability.dominates(left.id, right.id), dominance.dominates(left.id, right.id));
        }
    }
    IntegerRange.run(&mut function, &mut DataPool::default());
    assert!(validate_function(&function).is_ok());
    let op = |value| {
        let ValueDef::Instruction { inst, .. } = function.value(value).unwrap().def else {
            unreachable!();
        };
        function.instruction(inst).unwrap().op
    };
    assert_eq!(op(update), Op::IAdd, "bounded counter should specialize");
    for iteration in 0..8 {
        let count = if descending { 8 - iteration } else { iteration };
        let shifted = y.wrapping_shl(1);
        for (value, actual, scalar) in [
            (product, x as i128 * 3, Op::IMul),
            (sum, shifted as i128 + x as i128, Op::IAdd),
        ] {
            if op(value) == scalar {
                assert!(i64::try_from(actual).is_ok(),
                    "false proof: x={x}, y={y}, mask={mask}, count={count}, split={split_latches}");
            }
        }
        (x, y) = if count & 1 == 0 { (y, x & mask) } else { (shifted, x) };
    }
}
