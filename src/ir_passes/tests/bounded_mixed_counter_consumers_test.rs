//! Purpose:
//! Checks comparison dispatch before numeric storage replay retains an integer counter.
//!
//! Called from:
//! - The focused EIR pass library test harness.
//!
//! Key details:
//! - A proven integer range alone cannot authorize a different PHP comparison rule.

use std::collections::HashMap;
use crate::ir::{validate_function, Builder, CmpPredicate, DataPool, IrHeapKind, IrType,
    LocalKind, LocalSlotId, Op, Ownership};
use crate::types::PhpType;
use super::bounded_mixed_counter_test::counter;

/// Storage trials reject representation-sensitive partners while preserving ordinary comparisons.
#[test]
fn bounded_mixed_counter_storage_preserves_comparison_partners() {
    for (op, unsafe_partner) in [(Op::LooseEq, PhpType::Mixed), (Op::LooseNotEq, PhpType::Mixed),
        (Op::Spaceship, PhpType::Bool), (Op::StrictEq, PhpType::Pointer(None)),
        (Op::StrictNotEq, PhpType::Pointer(None))] {
        for (php, accepted) in [(unsafe_partner, false), (PhpType::Int, true)] {
            let mut function = counter(CmpPredicate::Slt, 1, 0, false);
            let entry = function.blocks[0].id;
            let body = function.blocks[2].id;
            let entry_term = function.blocks[0].terminator.take().unwrap();
            let body_term = function.blocks[2].terminator.take().unwrap();
            {
                let mut b = Builder::new(&mut function);
                b.position_at_end(entry);
                let ty = if php == PhpType::Mixed { IrType::Heap(IrHeapKind::Mixed) } else { IrType::I64 };
                let other = b.add_local(Some("other".into()), ty, php.clone(), LocalKind::PhpLocal);
                let zero = if php == PhpType::Bool { b.emit_const_bool(false) } else { b.emit_const_i64(0) };
                b.emit_store_local(other, zero);
                b.terminate(entry_term);
                b.position_at_end(body);
                let value = b.emit_load_local(LocalSlotId::from_raw(1), IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);
                let partner = b.emit_load_local(other, ty, php.clone());
                let result_php = if op == Op::Spaceship { PhpType::Int } else { PhpType::Bool };
                b.emit(op, vec![value, partner], None, IrType::I64, result_php, Ownership::NonHeap);
                b.terminate(body_term);
            }
            assert!(validate_function(&function).is_ok(), "{:?}", validate_function(&function));
            let proven = crate::ir_passes::prove_integer_slots(&function, &mut DataPool::default(), &HashMap::new());
            assert_eq!(proven.slots.contains(&LocalSlotId::from_raw(1)), accepted, "{op:?} with {php:?}");
        }
    }
}
