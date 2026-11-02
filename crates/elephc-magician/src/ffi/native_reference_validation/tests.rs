//! Purpose:
//! Verifies original-RHS coercion agreement, reentrant source changes and candidate ownership.
//!
//! Called from:
//! - Native reference validation ABI unit tests in the Magician library.
//!
//! Key details:
//! - Opaque value identities model native owners without dereferencing runtime heap memory.
//! - Callback source mutations exercise the same exported metadata APIs as native hooks.

use super::*;
use crate::ffi::native_reference_sources::{
    __elephc_reference_sources_bind_v1, __elephc_reference_sources_reset_v1,
    __elephc_reference_sources_set_reference_v1, __elephc_reference_sources_unbind_v1,
};

/// Scalar PHP values sufficient to distinguish independent coercion from chained coercion.
#[derive(Clone, Debug, PartialEq)]
enum Value { Int(i64), Text(String), Float(f64) }

/// Test-native ownership and physical-binding state behind the opaque callback context.
struct Native {
    values: BTreeMap<usize, Value>,
    owners: BTreeMap<usize, usize>,
    next_value: usize,
    bindings: BTreeMap<usize, (usize, usize)>,
    calls: Vec<usize>,
    released: Vec<usize>,
    action: Option<fn(&mut Native, usize)>,
    pending_throw: bool,
    release_failure: bool,
    comparison_failure: bool,
}

impl Native {
    /// Starts one isolated worker source set and one caller-owned integer RHS.
    fn new() -> Self {
        assert_eq!(__elephc_reference_sources_reset_v1(), SUCCESS);
        Self { values: BTreeMap::from([(1, Value::Int(2))]), owners: BTreeMap::from([(1, 1)]), next_value: 1000,
            bindings: BTreeMap::new(), calls: Vec::new(), released: Vec::new(), action: None,
            pending_throw: false, release_failure: false, comparison_failure: false }
    }

    /// Registers a weak physical binding and its native-authenticated wrapper shape.
    fn bind(&mut self, binding: usize, cell: usize, validator: usize, reference: usize) -> u64 {
        let token = __elephc_reference_sources_bind_v1(binding, cell, validator);
        assert_ne!(token, 0);
        self.bindings.insert(binding, (cell, reference));
        assert_eq!(__elephc_reference_sources_set_reference_v1(cell, reference), SUCCESS);
        token
    }

    /// Calls the complete native ABI without lending any source metadata to a callback.
    fn prepare(&mut self) -> (i32, NativeReferenceValidationOutcome) {
        let mut outcome = NativeReferenceValidationOutcome::default();
        let status = unsafe { __elephc_reference_validate_v1(500, 1,
            (self as *mut Self).cast(), &operations(), &mut outcome) };
        (status, outcome)
    }
}

impl Drop for Native {
    /// Removes the worker's weak addresses before another test reuses its thread.
    fn drop(&mut self) { __elephc_reference_sources_reset_v1(); }
}

/// Checks a copied source against current native physical binding identity without running PHP.
unsafe extern "C" fn current(context: *mut c_void, source: *const NativeReferenceTypeSource, reference: usize) -> i32 {
    let native = unsafe { &mut *context.cast::<Native>() };
    let source = unsafe { source.read() };
    i32::from(native.bindings.get(&source.binding) == Some(&(source.cell, reference)))
}

/// Models two string properties, an int|string property, and a contained coercion throw.
unsafe extern "C" fn coerce(context: *mut c_void, validator: usize, original: usize, output: *mut usize) -> i32 {
    let native = unsafe { &mut *context.cast::<Native>() };
    if original != 1 { return FAILURE; }
    native.calls.push(validator);
    if let Some(action) = native.action { action(native, validator); }
    if validator == 4 { native.pending_throw = true; return 2; }
    if validator == 5 {
        *native.owners.get_mut(&original).unwrap() += 1;
        unsafe { output.write(original); }
        return SUCCESS;
    }
    let value = native.values[&original].clone();
    let value = if validator == 1 || validator == 3 {
        match value { Value::Int(value) => Value::Text(value.to_string()), value => value }
    } else { value };
    let identity = native.next_value;
    native.next_value += 1;
    native.values.insert(identity, value);
    native.owners.insert(identity, 1);
    unsafe { output.write(identity); }
    SUCCESS
}

/// Compares candidate PHP values without executing user code or changing source metadata.
unsafe extern "C" fn identical(context: *mut c_void, first: usize, second: usize, output: *mut i32) -> i32 {
    let native = unsafe { &mut *context.cast::<Native>() };
    if native.comparison_failure { return FAILURE; }
    let same = native.values[&first] == native.values[&second];
    unsafe { output.write(i32::from(same)); }
    SUCCESS
}

/// Consumes every candidate once, including when protected cleanup reports a failure.
unsafe extern "C" fn release(context: *mut c_void, value: usize) -> i32 {
    let native = unsafe { &mut *context.cast::<Native>() };
    let Some(owners) = native.owners.get_mut(&value) else { return FAILURE; };
    if *owners == 0 { return FAILURE; }
    *owners -= 1;
    if *owners == 0 { native.owners.remove(&value); native.values.remove(&value); }
    native.released.push(value);
    if native.release_failure { native.release_failure = false; FAILURE } else { SUCCESS }
}

/// Supplies the exact complete operation table generated native publishers will provide.
fn operations() -> NativeReferenceValidationOps {
    NativeReferenceValidationOps { abi_version: VALIDATION_ABI_VERSION,
        struct_size: std::mem::size_of::<NativeReferenceValidationOps>() as u64,
        is_current: Some(current), coerce: Some(coerce), identical: Some(identical), release: Some(release) }
}

/// string and int|string must independently receive integer two and disagree before publication.
#[test]
fn native_reference_validation_does_not_chain_property_coercions() {
    let mut native = Native::new();
    let first = native.bind(10, 100, 1, 500);
    let second = native.bind(20, 200, 2, 500);
    let (status, outcome) = native.prepare();
    assert_eq!(status, CONFLICT);
    assert_eq!((outcome.first.token, outcome.second.token), (first, second));
    assert_eq!(native.calls, [1, 2]);
    assert_eq!(native.values, BTreeMap::from([(1, Value::Int(2))]));
    assert_eq!(native.released.len(), 2);
    assert_eq!(outcome.candidate, 0);
}

/// Cloned bindings each coerce once even when their cell and declaration validator are identical.
#[test]
fn native_reference_validation_keeps_each_clone_source_and_transfers_one_owner() {
    let mut native = Native::new();
    native.bind(10, 100, 1, 500);
    native.bind(20, 100, 1, 500);
    let (status, outcome) = native.prepare();
    assert_eq!(status, SUCCESS);
    assert_eq!(native.calls, [1, 1]);
    assert_eq!(native.values[&outcome.candidate], Value::Text("2".into()));
    assert_eq!(native.released.len(), 1);
    assert_eq!(native.values.len(), 2, "the caller still owns the RHS and receives one candidate");
    assert_eq!(unsafe { release((&mut native as *mut Native).cast(), outcome.candidate) }, SUCCESS);
}

/// Simulates a coercion attaching an older source and unsetting a pending source.
fn attach_older_source(native: &mut Native, validator: usize) {
    if validator != 3 { return; }
    native.action = None;
    native.bindings.insert(10, (100, 500));
    assert_eq!(__elephc_reference_sources_set_reference_v1(100, 500), SUCCESS);
    native.bindings.remove(&30);
    assert_eq!(__elephc_reference_sources_unbind_v1(30), SUCCESS);
}

/// Reentrant metadata changes force a fresh scan without repeating a successful source coercion.
#[test]
fn native_reference_validation_rescans_after_coercion_and_skips_retired_sources() {
    let mut native = Native::new();
    native.bind(10, 100, 1, 600);
    native.bind(20, 200, 3, 500);
    native.bind(30, 300, 4, 500);
    native.action = Some(attach_older_source);
    let (status, outcome) = native.prepare();
    assert_eq!(status, SUCCESS);
    assert_eq!(native.calls, [3, 1]);
    assert!(!native.pending_throw);
    assert_eq!(native.values[&outcome.candidate], Value::Text("2".into()));
    assert_eq!(unsafe { release((&mut native as *mut Native).cast(), outcome.candidate) }, SUCCESS);
}

/// A contained native coercion throw retires prior candidates without transferring a result.
#[test]
fn native_reference_validation_releases_candidates_on_a_later_throw() {
    let mut native = Native::new();
    native.bind(10, 100, 1, 500);
    native.bind(20, 200, 4, 500);
    let (status, outcome) = native.prepare();
    assert_eq!(status, 2);
    assert!(native.pending_throw);
    assert_eq!(outcome.candidate, 0);
    assert_eq!(native.released.len(), 1);
    assert_eq!(native.values, BTreeMap::from([(1, Value::Int(2))]));
}

/// Stale positive index entries never authorize a coercion without native physical confirmation.
#[test]
fn native_reference_validation_rechecks_physical_shape_and_handles_cleanup_failure() {
    let mut native = Native::new();
    native.bind(10, 100, 1, 500);
    native.bindings.insert(10, (100, 600));
    let (status, outcome) = native.prepare();
    assert_eq!((status, outcome.candidate), (SUCCESS, 0));
    assert!(native.calls.is_empty());
    native.bindings.insert(10, (100, 500));
    native.bind(20, 200, 3, 500);
    native.release_failure = true;
    let (status, outcome) = native.prepare();
    assert_eq!(status, FAILURE);
    assert_eq!(outcome.candidate, 0);
    assert_eq!(native.released.len(), 2, "a failed discard must also retire the selected candidate");
}

/// Every supported native target uses fixed 64-bit records and rejects incomplete callback tables.
#[test]
fn native_reference_validation_abi_rejects_incomplete_operations() {
    assert_eq!(std::mem::size_of::<NativeReferenceValidationOps>(), 48);
    assert_eq!(std::mem::size_of::<NativeReferenceValidationOutcome>(), 72);
    assert_eq!(std::mem::offset_of!(NativeReferenceValidationOutcome, first), 8);
    assert_eq!(std::mem::offset_of!(NativeReferenceValidationOutcome, second), 40);
    let mut native = Native::new();
    let mut ops = operations();
    ops.coerce = None;
    let mut outcome = NativeReferenceValidationOutcome::default();
    let status = unsafe { __elephc_reference_validate_v1(500, 1,
        (&mut native as *mut Native).cast(), &ops, &mut outcome) };
    assert_eq!(status, FAILURE);
    assert!(native.calls.is_empty());
}

/// Identical accepted input owners agree without applying PHP's non-reflexive NaN comparison.
#[test]
fn native_reference_validation_preserves_unchanged_nan_input_identity() {
    let mut native = Native::new();
    native.values.insert(1, Value::Float(f64::NAN));
    native.bind(10, 100, 5, 500);
    native.bind(20, 200, 5, 500);
    native.comparison_failure = true;
    let (status, outcome) = native.prepare();
    assert_eq!(status, SUCCESS);
    assert_eq!(outcome.candidate, 1);
    assert_eq!(native.calls, [5, 5]);
    assert_eq!(native.owners[&1], 2, "the caller's RHS owner and the transferred candidate remain");
    assert!(matches!(native.values[&1], Value::Float(value) if value.is_nan()));
    assert_eq!(unsafe { release((&mut native as *mut Native).cast(), outcome.candidate) }, SUCCESS);
    assert_eq!(native.owners[&1], 1);
}
