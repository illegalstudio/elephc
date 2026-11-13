//! Purpose:
//! Keeps string lifetime pins transparent to the inliner's scratch-storage guard.
//!
//! Called from:
//! - The small-function inliner unit suite.
//!
//! Key details:
//! - Stable local strings remain admissible through Acquire and Borrow.
//! - Applying the same wrappers to concat scratch must not make it admissible.

use super::*;
use crate::ir::{Builder, FunctionParam};

/// Lifetime wrappers preserve the underlying stability decision for locals and scratch values.
#[test]
fn inline_string_argument_pins_preserve_storage_stability() {
    for scratch in [false, true] {
        let mut host = Function::new("host".into(), IrType::Void, PhpType::Void);
        let slot = host.add_local(None, IrType::Str, PhpType::Str, LocalKind::PhpLocal);
        {
            let mut builder = Builder::new(&mut host);
            let entry = builder.create_named_block("entry", vec![]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let local = builder.emit(Op::LoadLocal, vec![], Some(Immediate::LocalSlot(slot)),
                IrType::Str, PhpType::Str, Ownership::Borrowed).unwrap();
            let source = if scratch {
                builder.emit(Op::StrConcat, vec![local, local], None,
                    IrType::Str, PhpType::Str, Ownership::Borrowed).unwrap()
            } else { local };
            let pinned = builder.emit(Op::Acquire, vec![source], Some(Immediate::Bool(true)),
                IrType::Str, PhpType::Str, Ownership::Owned).unwrap();
            let borrowed = builder.emit(Op::Borrow, vec![pinned], None,
                IrType::Str, PhpType::Str, Ownership::Borrowed).unwrap();
            let _ = builder.emit(Op::Call, vec![borrowed], Some(Immediate::Data(DataId::from_raw(0))),
                IrType::Void, PhpType::Void, Ownership::NonHeap);
            builder.terminate(Terminator::Return { value: None });
        }
        let mut callee = Function::new("callee".into(), IrType::Void, PhpType::Void);
        callee.params.push(FunctionParam { name: "text".into(), ir_type: IrType::Str,
            php_type: PhpType::Str, by_ref: false, variadic: false });
        let call = host.instructions.iter().find(|inst| inst.op == Op::Call).unwrap();
        assert_eq!(call_string_args_are_stable(&host, call, &callee), !scratch);
    }
}
