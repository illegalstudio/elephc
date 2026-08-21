//! Purpose:
//! Verifies the EIR effect bitset and deterministic effect names.
//!
//! Called from:
//! - `crate::ir::tests`.
//!
//! Key details:
//! - Pure means no bits; observable operations must survive dead-code passes.

use crate::ir::{Effects, Op, RuntimeFnId};

/// The pure effect set is empty and reports itself as pure.
#[test]
fn pure_has_no_bits() {
    assert!(Effects::PURE.is_empty());
    assert!(Effects::PURE.is_pure());
}

/// Heap reads and writes are independent effect categories.
#[test]
fn reads_and_writes_are_orthogonal() {
    let read = Effects::READS_HEAP;
    let write = Effects::WRITES_HEAP;
    assert!(read.may_observe());
    assert!(!read.may_mutate());
    assert!(write.may_mutate());
    assert!(!write.may_observe());
}

/// Combined effects retain all component bits.
#[test]
fn combined_effects_compose() {
    let effects = Effects::READS_HEAP | Effects::MAY_FATAL;
    assert!(effects.contains(Effects::READS_HEAP));
    assert!(effects.contains(Effects::MAY_FATAL));
    assert_eq!(effects.names(), vec!["reads_heap", "may_fatal"]);
}

/// Typed array reads distinguish warning-capable access from silent probing.
#[test]
fn array_read_opcodes_have_precise_warning_contracts() {
    assert_eq!(
        Op::ArrayGet.default_effects(),
        Effects::READS_HEAP | Effects::MAY_WARN
    );
    assert_eq!(Op::ArrayGetSilent.default_effects(), Effects::READS_HEAP);
    assert_eq!(
        Op::HashGet.default_effects(),
        Effects::READS_HEAP | Effects::MAY_WARN
    );
    assert_eq!(Op::HashGetSilent.default_effects(), Effects::READS_HEAP);
}

/// Dynamic instance calls retain a catchable-error bit until target refinement proves otherwise.
#[test]
fn instance_call_opcodes_default_to_may_throw() {
    let expected = Effects::READS_HEAP | Effects::MAY_THROW | Effects::MAY_DEOPT;
    assert_eq!(Op::MethodCall.default_effects(), expected);
    assert_eq!(Op::NullsafeMethodCall.default_effects(), expected);
}

/// Read-only runtime probes avoid the former all-effects fallback.
#[test]
fn runtime_function_probes_expose_targeted_effects() {
    assert_eq!(
        RuntimeFnId::FunctionExists.effects(),
        Effects::READS_GLOBAL
    );
    assert_eq!(RuntimeFnId::GetClass.effects(), Effects::READS_HEAP);
    assert_eq!(RuntimeFnId::Clamp.effects(), Effects::MAY_THROW);
    assert_eq!(
        RuntimeFnId::SplAutoloadExtensions.effects(),
        Effects::READS_GLOBAL | Effects::WRITES_GLOBAL
    );
    assert_eq!(
        RuntimeFnId::Hrtime.effects(),
        Effects::READS_PROCESS | Effects::ALLOC_HEAP
    );
    assert_eq!(
        RuntimeFnId::PhpUname.effects(),
        Effects::READS_PROCESS | Effects::ALLOC_CONCAT | Effects::MAY_FATAL
    );
}

/// Persistent class-name results cannot alias their object arguments.
#[test]
fn class_name_lookup_results_are_argument_independent() {
    use crate::builtins::semantics::BuiltinResultOwnership;

    assert_eq!(
        RuntimeFnId::GetClass.result_ownership(),
        BuiltinResultOwnership::Independent
    );
    assert_eq!(
        RuntimeFnId::GetParentClass.result_ownership(),
        BuiltinResultOwnership::Independent
    );
}
