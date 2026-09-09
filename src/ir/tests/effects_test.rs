//! Purpose:
//! Verifies the EIR effect bitset and deterministic effect names.
//!
//! Called from:
//! - `crate::ir::tests`.
//!
//! Key details:
//! - Pure means no bits; observable operations must survive dead-code passes.

use crate::ir::{Effects, Op, RuntimeFnId};

/// Joins may invoke string conversions and destructors even when their string result is discarded.
#[test]
fn implode_effects_preserve_string_conversion_callbacks() {
    assert_eq!(RuntimeFnId::Implode.effects(), Effects::all());
    assert!(RuntimeFnId::Implode.effects().is_observable());
    assert_eq!(RuntimeFnId::Implode.intrinsic_effects(), Effects::all());
}

/// Both explicit collection and automatic safe points can execute arbitrary throwing destructors.
#[test]
fn collection_effects_include_destructor_callbacks() {
    let collect = crate::ir::GcControlOp::Collect.effects();
    assert_eq!(collect, Effects::all());
    assert_eq!(Op::GcCollect.default_effects(), collect);
    assert_eq!(Op::GcControl.default_effects(), collect);
    assert_eq!(crate::ir::GcControlOp::Disable.effects(), Effects::WRITES_GLOBAL);
    assert_eq!(crate::ir::GcControlOp::Enabled.effects(), Effects::READS_GLOBAL);
}

/// Boxed array projections read mutable storage, allocate owners, and may reject invalid runtime tags.
#[test]
fn array_projection_effects_preserve_heap_reads_and_failures() {
    let required = Effects::READS_HEAP | Effects::ALLOC_HEAP | Effects::REFCOUNT_OP
        | Effects::MAY_THROW | Effects::MAY_FATAL;
    for target in [RuntimeFnId::ArrayMerge, RuntimeFnId::ArrayReverse, RuntimeFnId::ArrayValues] {
        assert!(target.effects().contains(required), "{target:?}");
        assert!(!target.effects().is_pure(), "{target:?}");
    }
}

/// Warning-capable operations must not be reordered across state accessed by a user handler.
#[test]
fn warning_handlers_observe_and_mutate_program_state() {
    assert!(Effects::MAY_WARN.may_observe());
    assert!(Effects::MAY_WARN.may_mutate());
    assert!(Effects::MAY_WARN.is_observable());
}

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

/// Curl transfer boundaries identify network work without classifying internal adapters.
#[test]
fn curl_runtime_effects_mark_network_boundaries() {
    let perform = RuntimeFnId::CurlEasyPerform.effects();
    assert!(perform.contains(Effects::NETWORK_IO));
    assert!(perform.contains(Effects::BLOCKING_IO));
    assert!(perform.contains(Effects::MAY_THROW));
    assert!(perform.contains(Effects::OUTPUT));
    assert!(perform.contains(Effects::ALLOC_HEAP));

    let progress = RuntimeFnId::CurlMultiExec.effects();
    assert!(progress.contains(Effects::NETWORK_IO));
    assert!(!progress.contains(Effects::BLOCKING_IO));
    assert!(progress.contains(Effects::MAY_THROW));
    assert!(progress.contains(Effects::OUTPUT));
    assert!(progress.contains(Effects::ALLOC_HEAP));

    let adapter = RuntimeFnId::CurlAdapterAddr.effects();
    assert!(!adapter.contains(Effects::NETWORK_IO));
    assert!(!adapter.contains(Effects::BLOCKING_IO));

    assert!(matches!(
        RuntimeFnId::CurlEasyPerform.monitoring_policy(),
        elephc_monitoring_contract::MonitoringPolicy::Io {
            kind: elephc_monitoring_contract::IoKind::Network,
            wait: elephc_monitoring_contract::WaitPolicy::Measured,
            trace_context: elephc_monitoring_contract::TraceContextPolicy::Automatic,
        }
    ));
}

/// Formatting calls retain arbitrary `__toString()` effects, including throws and globals.
#[test]
fn printf_family_effects_cover_userland_string_conversion() {
    for target in [
        RuntimeFnId::Fprintf,
        RuntimeFnId::Printf,
        RuntimeFnId::Sprintf,
        RuntimeFnId::Vfprintf,
        RuntimeFnId::Vprintf,
        RuntimeFnId::Vsprintf,
    ] {
        let effects = target.effects();
        assert!(effects.contains(Effects::MAY_THROW), "{target:?}");
        assert!(effects.contains(Effects::WRITES_GLOBAL), "{target:?}");
        assert!(effects.contains(Effects::OUTPUT), "{target:?}");
    }
}
