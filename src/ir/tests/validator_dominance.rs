//! Purpose:
//! Checks dominance validation across bitset word boundaries and dead continuations.
//!
//! Called from:
//! - The EIR data model test module.
//!
//! Key details:
//! - A reachable diamond must reject a value defined on only one branch.
//! - Connected dead blocks retain their cross-block-use exception.

use crate::ir::{validate_function, Builder, Function, IrType, Op, Ownership, Terminator, ValidationError};
use crate::types::PhpType;

/// Dense dominator storage preserves reachable and unreachable rules beyond one machine word.
#[test]
fn validator_dominance_preserves_diamonds_and_dead_blocks_across_word_boundaries() {
    let mut function = Function::new("wide_dominance".into(), IrType::I64, PhpType::Int);
    let (join, dominating, branch_only) = {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", vec![]);
        let chain: Vec<_> = (0..70).map(|index|
            builder.create_named_block(&format!("chain_{index}"), vec![])).collect();
        let left = builder.create_named_block("left", vec![]);
        let right = builder.create_named_block("right", vec![]);
        let join = builder.create_named_block("join", vec![]);
        let dead_first = builder.create_named_block("dead_first", vec![]);
        let dead_second = builder.create_named_block("dead_second", vec![]);
        builder.set_entry(entry);
        builder.position_at_end(entry);
        builder.terminate(Terminator::Br { target: chain[0], args: vec![] });
        for pair in chain.windows(2) {
            builder.position_at_end(pair[0]);
            builder.terminate(Terminator::Br { target: pair[1], args: vec![] });
        }
        builder.position_at_end(*chain.last().unwrap());
        let dominating = builder.emit_const_i64(7);
        let condition = builder.emit_const_bool(true);
        builder.terminate(Terminator::CondBr {
            cond: condition, then_target: left, then_args: vec![],
            else_target: right, else_args: vec![],
        });
        builder.position_at_end(left);
        let branch_only = builder.emit_const_i64(9);
        builder.terminate(Terminator::Br { target: join, args: vec![] });
        builder.position_at_end(right);
        builder.terminate(Terminator::Br { target: join, args: vec![] });
        builder.position_at_end(join);
        builder.terminate(Terminator::Return { value: Some(dominating) });
        builder.position_at_end(dead_first);
        let dead_value = builder.emit_const_i64(11);
        builder.terminate(Terminator::Br { target: dead_second, args: vec![] });
        builder.position_at_end(dead_second);
        let _ = builder.emit_with_effects(
            Op::EchoValue, vec![dead_value], None, IrType::Void, PhpType::Void,
            Ownership::NonHeap, Op::EchoValue.default_effects(), None,
        );
        builder.terminate(Terminator::Unreachable);
        (join, dominating, branch_only)
    };
    assert_eq!(validate_function(&function), Ok(()));
    function.blocks[join.as_raw() as usize].terminator = Some(Terminator::Return { value: Some(branch_only) });
    assert_eq!(validate_function(&function), Err(ValidationError::UseNotDominated {
        value: branch_only, used_in: join,
    }));
    function.blocks[join.as_raw() as usize].terminator = Some(Terminator::Return { value: Some(dominating) });
    assert_eq!(validate_function(&function), Ok(()));
}
