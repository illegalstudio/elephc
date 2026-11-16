//! Purpose:
//! Verifies owner accounting when boxed branch results are stored in scalar merge slots.
//!
//! Called from:
//! - The AST-to-EIR ownership suite on every supported target.
//!
//! Key details:
//! - Tagged scalar stores borrow their boxed input and retire it after conversion.
//! - Heap-backed merge stores retain the owner transferred to their result.

use crate::ir::{Immediate, LocalKind, Op};
use crate::types::PhpType;

/// Scalar merge stores avoid an untransferable retain while string merges keep their owner.
fn verify_converted_merge_owner(target: &str) {
    let source = format!("{}\n{}",
        include_str!("../../../tests/fixtures/conditional_indexed_accumulation.php"),
        r#"function transferredMergeString(bool $choose, int $length): string {
            return $choose ? str_repeat('a', $length) : str_repeat('b', $length);
        }
        echo transferredMergeString($argc > 1, $argc);"#);
    let module = super::lower_source_at_for_target(&source, std::path::Path::new("main.php"),
        std::path::Path::new("."), crate::codegen::platform::Target::parse(target).unwrap());
    let function = module.functions.iter().find(|function| function.name == "conditionalIndexedSum")
        .expect("conditional accumulation function");
    let mut converted = 0;
    for (index, store) in function.instructions.iter().enumerate() {
        if store.op != Op::StoreLocal { continue; }
        let Some(Immediate::LocalSlot(slot)) = store.immediate else { continue; };
        let local = &function.locals[slot.as_raw() as usize];
        if local.kind != LocalKind::OwnedTemp || local.php_type.codegen_repr() != PhpType::TaggedScalar {
            continue;
        }
        let input = store.operands[0];
        if function.value(input).unwrap().php_type.codegen_repr() != PhpType::Mixed { continue; }
        let producer = function.instructions.iter().find(|inst| inst.result == Some(input)).unwrap();
        assert_ne!(producer.op, Op::Acquire, "{target}: inline slots cannot take an extra boxed owner");
        let retirement = &function.instructions[index + 1];
        assert_eq!(retirement.op, Op::Release, "{target}: retire the boxed conversion input");
        assert_eq!(retirement.operands, [input]);
        converted += 1;
    }
    assert_eq!(converted, 2, "{target}: both ternary arms convert a boxed read to the scalar slot");
    let string = module.functions.iter().find(|function| function.name == "transferredMergeString").unwrap();
    let stores = string.instructions.iter().filter(|inst| inst.op == Op::StoreLocal
        && matches!(inst.immediate, Some(Immediate::LocalSlot(slot))
            if string.locals[slot.as_raw() as usize].kind == LocalKind::OwnedTemp)).collect::<Vec<_>>();
    assert_eq!(stores.len(), 2);
    for store in stores {
        let producer = string.instructions.iter().find(|inst| inst.result == Some(store.operands[0])).unwrap();
        assert_eq!(producer.op, Op::Acquire, "{target}: heap merge slots retain their incoming owner");
    }
    crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
}

supported_target_cases!(converted_merge_owners, verify_converted_merge_owner);
