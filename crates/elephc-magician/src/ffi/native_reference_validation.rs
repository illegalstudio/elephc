//! Purpose:
//! Prepares independently coerced candidates for a shared native property reference.
//!
//! Called from:
//! - The versioned native reference validation ABI, before a publisher changes storage.
//!
//! Key details:
//! - Every source sees the original RHS; no candidate feeds another property's coercion.
//! - Native callbacks contain PHP throws and keep all borrowed inputs alive across re-entry.
//! - Records and candidates are copied outside metadata borrows; only matched sources are cached.

use std::collections::BTreeMap;
use std::ffi::c_void;

use super::native_reference_sources::{snapshot_reference_sources, NativeReferenceTypeSource};

/// Version of this native-only operation table, independent of the eval execution status ABI.
const VALIDATION_ABI_VERSION: u64 = 1;
/// A protected callback completed normally.
const SUCCESS: i32 = 0;
/// A malformed operation or failed native callback cannot publish a replacement.
const FAILURE: i32 = 1;
/// Two live property sources produced different PHP values from the same original RHS.
const CONFLICT: i32 = 3;

/// Protected native callbacks used without keeping a Rust metadata borrow across PHP.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeReferenceValidationOps {
    pub abi_version: u64,
    pub struct_size: u64,
    /// Pure physical-shape probe: one for a current match, zero for a miss, negative for failure.
    pub is_current: Option<unsafe extern "C" fn(*mut c_void, *const NativeReferenceTypeSource, usize) -> i32>,
    /// Status zero transfers one candidate owner; identity acceptance retains the original cell.
    /// Status two reports a contained native Throwable.
    pub coerce: Option<unsafe extern "C" fn(*mut c_void, usize, usize, *mut usize) -> i32>,
    /// Pure PHP identity comparison: status zero writes a zero-or-one result without running PHP.
    pub identical: Option<unsafe extern "C" fn(*mut c_void, usize, usize, *mut i32) -> i32>,
    /// Protected cleanup preserves an existing exception and never starts a collection safe point.
    pub release: Option<unsafe extern "C" fn(*mut c_void, usize) -> i32>,
}

/// Success transfers `candidate` when nonzero; zero leaves the caller's original RHS authoritative.
/// A conflict reports copied source metadata so the publisher can raise its property diagnostic.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeReferenceValidationOutcome {
    pub candidate: usize,
    pub first: NativeReferenceTypeSource,
    pub second: NativeReferenceTypeSource,
}

/// A successfully coerced property candidate owns one native value until transfer or cleanup.
struct Candidate {
    source: NativeReferenceTypeSource,
    value: usize,
}

/// Owns every candidate even if metadata access panics while a callback has re-entered PHP.
struct Candidates {
    values: BTreeMap<u64, Candidate>,
    context: *mut c_void,
    release: unsafe extern "C" fn(*mut c_void, usize) -> i32,
}

impl Candidates {
    /// Retires discarded results before transferring the chosen owner to the native publisher.
    fn finish(&mut self, selected: Option<u64>) -> Result<usize, i32> {
        let discarded = self.values.keys().copied().filter(|token| Some(*token) != selected)
            .collect::<Vec<_>>();
        let mut failure = SUCCESS;
        for token in discarded {
            let value = self.values.remove(&token).unwrap().value;
            let status = unsafe { (self.release)(self.context, value) };
            if failure == SUCCESS && status != SUCCESS { failure = callback_status(status); }
        }
        if failure != SUCCESS { return Err(failure); }
        Ok(selected.and_then(|token| self.values.remove(&token)).map_or(0, |candidate| candidate.value))
    }
}

impl Drop for Candidates {
    /// Candidate cleanup cannot replace an already pending native exception.
    fn drop(&mut self) {
        for (_, candidate) in std::mem::take(&mut self.values) {
            unsafe { (self.release)(self.context, candidate.value); }
        }
    }
}

/// Accepts only the protected callback status namespace: success, failure, or native Throwable.
fn callback_status(status: i32) -> i32 {
    if status == 2 { 2 } else if status == SUCCESS { SUCCESS } else { FAILURE }
}

/// Independently coerces the current source set and rejects incompatible candidates before a store.
///
/// Returns zero for success, one for failure, two for a pending native Throwable, or three for a
/// property-source conflict. No destination is changed. On success a nonzero outcome candidate
/// transfers one owner to the caller; the caller keeps ownership of the original RHS in all cases.
///
/// # Safety
/// `ops` and `output` must point to complete readable/writable ABI records. Every callback and
/// `context` must remain valid through this call, including cleanup. The caller owns a detached
/// concrete RHS and keeps it and the destination reference alive through every callback. Native
/// binding hooks keep the weak wrapper index current. Shape probes and identity comparison run no
/// PHP. Coercers contain every native throw before returning across Rust. Cleanup consumes its
/// candidate owner, preserves pending exceptions, and runs no PHP or collection safe point. These
/// cleanup rules are possible because the caller's RHS keeps accepted object/array payloads alive;
/// property coercions otherwise create scalar candidates. Temporary keepalives must not change
/// PHP-visible reference sharing. The publisher must re-resolve a direct property's destination
/// after coercion rather than treating this reference-level API as a property-place snapshot.
#[no_mangle]
pub unsafe extern "C" fn __elephc_reference_validate_v1(
    reference: usize, original: usize, context: *mut c_void,
    ops: *const NativeReferenceValidationOps, output: *mut NativeReferenceValidationOutcome,
) -> i32 {
    if ops.is_null() || output.is_null() || reference == 0 || original == 0 { return FAILURE; }
    unsafe { output.write(NativeReferenceValidationOutcome::default()); }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let ops = unsafe { ops.read() };
        if ops.abi_version != VALIDATION_ABI_VERSION
            || ops.struct_size != std::mem::size_of::<NativeReferenceValidationOps>() as u64
        { return FAILURE; }
        let (Some(is_current), Some(coerce), Some(identical), Some(release)) =
            (ops.is_current, ops.coerce, ops.identical, ops.release) else { return FAILURE; };
        let mut candidates = Candidates { values: BTreeMap::new(), context, release };
        'rescan: loop {
            let mut matched = Vec::new();
            for source in snapshot_reference_sources(reference) {
                match unsafe { is_current(context, &source, reference) } {
                    0 => continue,
                    1 => {}
                    _ => return FAILURE,
                }
                if !candidates.values.contains_key(&source.token) {
                    let mut value = 0;
                    let status = unsafe { coerce(context, source.validator, original, &mut value) };
                    if status != SUCCESS { return callback_status(status); }
                    if value == 0 { return FAILURE; }
                    candidates.values.insert(source.token, Candidate { source, value });
                    // A callback can remove sources, add a clone, or promote an older unmatched
                    // cell. Restart with copied metadata, retaining only successful coercions.
                    continue 'rescan;
                }
                matched.push(source.token);
            }
            let selected = matched.first().copied();
            if let Some(first) = selected {
                let first = &candidates.values[&first];
                for token in matched.iter().skip(1) {
                    let second = &candidates.values[token];
                    // Unchanged input acceptance must not compare NaN against itself or walk
                    // an accepted recursive array. Both owners already name the same PHP value.
                    if first.value == second.value { continue; }
                    let mut same = 0;
                    let status = unsafe { identical(context, first.value, second.value, &mut same) };
                    if status != SUCCESS { return callback_status(status); }
                    if same == 0 {
                        let first = first.source;
                        let second = second.source;
                        if let Err(status) = candidates.finish(None) { return status; }
                        unsafe { output.write(NativeReferenceValidationOutcome {
                            candidate: 0, first, second,
                        }); }
                        return CONFLICT;
                    }
                    if same != 1 { return FAILURE; }
                }
            }
            return match candidates.finish(selected) {
                Ok(candidate) => {
                    unsafe { output.write(NativeReferenceValidationOutcome { candidate,
                        ..NativeReferenceValidationOutcome::default() }); }
                    SUCCESS
                }
                Err(status) => status,
            };
        }
    })).unwrap_or(FAILURE)
}

#[cfg(test)]
mod tests;
