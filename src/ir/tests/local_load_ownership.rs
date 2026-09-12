//! Purpose:
//! Verifies provisional local-load releases against final storage and ownership.
//!
//! Called from:
//! - The EIR builder unit suite.
//!
//! Key details:
//! - Both ordinary and static locals borrow same-storage Callable and Mixed views.
//! - Explicit owners and compiler temporary slots are outside the borrowing proof.

use crate::ir::{Builder, Function, Immediate, IrType, LocalKind, Op, Ownership, Terminator};
use crate::types::PhpType;

/// Builds one local-load release and verifies its state after finalization.
fn assert_release_survives(
    storage: PhpType,
    result: PhpType,
    kind: LocalKind,
    ownership: Ownership,
    survives: bool,
) {
    let diagnostic = format!("{kind:?}: {storage:?} -> {result:?}, {ownership:?}");
    let mut function = Function::new("local_release".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("value".to_string()), IrType::from_php(&storage), storage, kind,
        );
        let op = if kind == LocalKind::StaticLocal { Op::LoadStaticLocal } else { Op::LoadLocal };
        let load = builder.emit(
            op, Vec::new(), Some(Immediate::LocalSlot(slot)),
            IrType::from_php(&result), result, ownership,
        ).expect("a local load returns a value");
        builder.emit(
            Op::Release, vec![load], None, IrType::Void, PhpType::Void, Ownership::NonHeap,
        );
        builder.prune_borrowed_local_load_release_ops();
        builder.terminate(Terminator::Return { value: None });
    }
    let release = &function.instructions[1];
    assert_eq!(release.op, if survives { Op::Release } else { Op::Nop }, "{diagnostic}");
    if !survives {
        assert!(release.operands.is_empty(), "{diagnostic}");
        assert_eq!(release.immediate, None, "{diagnostic}");
        assert_eq!(release.effects, Op::Nop.default_effects(), "{diagnostic}");
    }
}

/// Callable/Mixed borrowed views prune while boxing and unboxing conversions keep cleanup.
#[test]
fn local_load_release_pruning_follows_the_storage_to_result_matrix() {
    let array = PhpType::Array(Box::new(PhpType::Int));
    let rows = [
        (PhpType::Callable, PhpType::Callable, false),
        (PhpType::Mixed, PhpType::Callable, true),
        (PhpType::Mixed, PhpType::Mixed, false),
        (array.clone(), PhpType::Mixed, true),
        (PhpType::Int, PhpType::Mixed, true),
        (PhpType::Str, PhpType::Mixed, true),
        (PhpType::Mixed, array.clone(), true),
        (PhpType::Mixed, PhpType::Str, true),
        (PhpType::Str, PhpType::Str, false),
        (array.clone(), array, false),
        (PhpType::php_array(), PhpType::Mixed, false),
    ];
    for kind in [LocalKind::PhpLocal, LocalKind::StaticLocal] {
        for (storage, result, survives) in &rows {
            assert_release_survives(
                storage.clone(), result.clone(), kind, Ownership::MaybeOwned, *survives,
            );
        }
    }
}

/// An explicit owning value cannot be made borrowed merely from its storage shape.
#[test]
fn owned_local_load_releases_are_not_pruned() {
    for kind in [LocalKind::PhpLocal, LocalKind::StaticLocal] {
        for ty in [PhpType::Callable, PhpType::Mixed] {
            assert_release_survives(ty.clone(), ty, kind, Ownership::Owned, true);
        }
    }
}

/// Compiler temporary owners are outside the ordinary PHP-local borrowing proof.
#[test]
fn compiler_temp_load_releases_are_not_pruned() {
    for kind in [LocalKind::OwnedTemp, LocalKind::HiddenTemp] {
        for ty in [PhpType::Callable, PhpType::Mixed] {
            assert_release_survives(ty.clone(), ty, kind, Ownership::MaybeOwned, true);
        }
    }
}
