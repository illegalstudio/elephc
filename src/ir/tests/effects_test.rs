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

/// Nullable indexed and associative reads retain possible boxing/copy allocation
/// while distinguishing warning-producing access from silent probing.
#[test]
fn array_read_opcodes_have_precise_warning_contracts() {
    assert_eq!(
        Op::ArrayGet.default_effects(),
        Effects::READS_HEAP | Effects::ALLOC_HEAP | Effects::MAY_WARN
    );
    assert_eq!(
        Op::ArrayGetSilent.default_effects(),
        Effects::READS_HEAP | Effects::ALLOC_HEAP
    );
    assert_eq!(
        Op::HashGet.default_effects(),
        Effects::READS_HEAP | Effects::ALLOC_HEAP | Effects::MAY_WARN | Effects::MAY_FATAL
    );
    assert_eq!(
        Op::HashGetSilent.default_effects(),
        Effects::READS_HEAP | Effects::ALLOC_HEAP | Effects::MAY_WARN | Effects::MAY_FATAL
    );
}

/// Associative-key probes and writes remain observable because invalid or
/// lossy PHP keys can warn, deprecate, or terminate even in silent reads.
#[test]
fn associative_key_operations_preserve_diagnostic_effects() {
    for op in [
        Op::HashGet,
        Op::HashGetSilent,
        Op::HashIsset,
        Op::HashSet,
        Op::HashUnset,
        Op::HashAppend,
        Op::ArrayKeyExists,
    ] {
        let effects = op.default_effects();
        assert!(effects.contains(Effects::MAY_WARN), "{op:?}");
        assert!(effects.contains(Effects::MAY_FATAL), "{op:?}");
        assert!(effects.is_observable(), "{op:?}");
    }
    let runtime_effects = RuntimeFnId::ArrayKeyExists.effects();
    assert!(runtime_effects.contains(Effects::MAY_WARN));
    assert!(runtime_effects.contains(Effects::MAY_FATAL));
    assert!(runtime_effects.is_observable());
}

/// Float-to-int conversion remains observable because PHP can emit a
/// precision-loss warning or reject a non-representable value.
#[test]
fn float_to_int_conversion_preserves_diagnostic_effects() {
    let effects = Op::FToI.default_effects();
    assert!(effects.contains(Effects::MAY_WARN));
    assert!(effects.contains(Effects::MAY_FATAL));
    assert!(effects.is_observable());
}

/// Float and Mixed truthiness retain the PHP 8.5 NAN warning boundary.
#[test]
fn truthiness_preserves_diagnostic_effects() {
    let effects = Op::IsTruthy.default_effects();
    assert!(effects.contains(Effects::MAY_WARN));
    assert!(effects.contains(Effects::MAY_FATAL));
    assert!(effects.is_observable());
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
