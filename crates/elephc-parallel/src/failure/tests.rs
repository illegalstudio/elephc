//! Purpose:
//! Unit tests for the versioned native Parallel failure envelope.
//!
//! Called from:
//! - `cargo test -p elephc-parallel` through `crate::failure`.
//!
//! Key details:
//! - Covers complete field/frame round trips and fail-closed malformed inputs.

use super::*;
use crate::elephc_parallel_transfer_free;

/// Builds a borrowed C input view whose backing test slice remains alive through encoding.
fn bytes(value: &[u8]) -> ParallelBytes {
    ParallelBytes {
        ptr: value.as_ptr(),
        len: value.len(),
    }
}

/// Produces the null/zero representation of an absent optional failure field.
fn absent() -> ParallelBytes {
    ParallelBytes::EMPTY
}

/// Reads a borrowed test view before its source transfer blob is freed.
fn view_bytes(value: ParallelBytes) -> &'static [u8] {
    unsafe { std::slice::from_raw_parts(value.ptr, value.len) }
}

/// Encodes live test inputs into an owned blob and asserts successful ABI completion.
fn encoded(input: &ParallelFailureInput) -> ParallelTransferBlob {
    let mut output = ParallelTransferBlob::EMPTY;
    assert_eq!(
        unsafe { elephc_parallel_failure_encode(input, &mut output) },
        PARALLEL_TRANSFER_OK
    );
    output
}

/// Wire lengths fail before truncation when a native usize exceeds the u32 header range.
#[test]
fn failure_wire_lengths_are_checked_before_narrowing_to_u32() {
    assert_eq!(wire_u32_len(17), Some(17));
    assert_eq!(wire_u32_len(u32::MAX as usize), Some(u32::MAX));
    if usize::BITS > u32::BITS {
        assert_eq!(wire_u32_len((u32::MAX as usize) + 1), None);
    }
}

/// Failure category, strings, signed code and optional trace-frame fields round-trip without pointers.
#[test]
fn failure_envelope_round_trips_all_fields_and_frames() {
    let frames = [
        ParallelFailureFrameInput {
            function: bytes(b"worker"),
            file: bytes(b"task.php"),
            line: 41,
        },
        ParallelFailureFrameInput {
            function: bytes(b"entry"),
            file: absent(),
            line: 0,
        },
    ];
    let input = ParallelFailureInput {
        kind: PARALLEL_FAILURE_PHP_THROWABLE,
        remote_class: bytes(b"DomainError"),
        message: bytes(b"worker failed"),
        code: -7,
        file: bytes(b"task.php"),
        line: 41,
        frames: frames.as_ptr(),
        frame_count: frames.len(),
    };
    let blob = encoded(&input);
    let mut view = ParallelFailureView::EMPTY;
    assert_eq!(
        unsafe { elephc_parallel_failure_view(blob, &mut view) },
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(view.kind, PARALLEL_FAILURE_PHP_THROWABLE);
    assert_eq!(view_bytes(view.remote_class), b"DomainError");
    assert_eq!(view_bytes(view.message), b"worker failed");
    assert_eq!(view.code, -7);
    assert_eq!(view_bytes(view.file), b"task.php");
    assert_eq!(view.line, 41);
    assert_eq!(view.frame_count, 2);
    let mut frame = ParallelFailureFrameView::EMPTY;
    assert_eq!(
        unsafe { elephc_parallel_failure_frame(blob, 1, &mut frame) },
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(view_bytes(frame.function), b"entry");
    assert!(frame.file.ptr.is_null());
    unsafe { elephc_parallel_transfer_free(blob) };
}

/// Truncated envelopes clear a prepopulated output view rather than exposing partial decoded fields.
#[test]
fn invalid_or_truncated_envelopes_fail_closed_and_clear_views() {
    let input = ParallelFailureInput {
        kind: PARALLEL_FAILURE_INFRASTRUCTURE,
        remote_class: absent(),
        message: bytes(b"broken"),
        code: 0,
        file: absent(),
        line: 0,
        frames: ptr::null(),
        frame_count: 0,
    };
    let blob = encoded(&input);
    let truncated = ParallelTransferBlob {
        ptr: blob.ptr,
        len: blob.len - 1,
    };
    let mut view = ParallelFailureView {
        kind: 99,
        ..ParallelFailureView::EMPTY
    };
    assert_eq!(
        unsafe { elephc_parallel_failure_view(truncated, &mut view) },
        PARALLEL_TRANSFER_INVALID_ENVELOPE
    );
    assert_eq!(view.kind, 0);
    assert!(view.message.ptr.is_null());
    unsafe { elephc_parallel_transfer_free(blob) };
}

/// Unknown categories and invalid null/nonzero input lengths are rejected without publishing a blob.
#[test]
fn unknown_failure_kinds_and_invalid_pointer_lengths_are_rejected() {
    let mut output = ParallelTransferBlob::EMPTY;
    let invalid_kind = ParallelFailureInput {
        kind: 99,
        remote_class: absent(),
        message: bytes(b"no"),
        code: 0,
        file: absent(),
        line: 0,
        frames: ptr::null(),
        frame_count: 0,
    };
    assert_eq!(
        unsafe { elephc_parallel_failure_encode(&invalid_kind, &mut output) },
        PARALLEL_TRANSFER_INVALID_ARGUMENT
    );
    let invalid_message = ParallelFailureInput {
        kind: PARALLEL_FAILURE_PHP_FATAL,
        message: ParallelBytes {
            ptr: ptr::null(),
            len: 1,
        },
        ..invalid_kind
    };
    assert_eq!(
        unsafe { elephc_parallel_failure_encode(&invalid_message, &mut output) },
        PARALLEL_TRANSFER_INVALID_ARGUMENT
    );
    assert!(output.ptr.is_null());
}
