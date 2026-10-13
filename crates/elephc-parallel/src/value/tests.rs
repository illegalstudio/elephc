//! Purpose:
//! Unit tests for the native Parallel transferable-value wire format.
//!
//! Called from:
//! - `cargo test -p elephc-parallel` through `crate::value`.
//!
//! Key details:
//! - Exercises nested indexed/associative values, cursor reads, and structural rejection.

use super::*;
use crate::{elephc_parallel_transfer_copy, elephc_parallel_transfer_free};

/// Creates an owned test writer and refuses a null allocation before any raw-pointer call.
fn writer() -> *mut ParallelValueWriter {
    let writer = elephc_parallel_value_writer_new();
    assert!(!writer.is_null());
    writer
}

/// Consumes a complete test writer into an owned blob, asserting the ABI success status.
fn finish(writer: *mut ParallelValueWriter) -> ParallelTransferBlob {
    let mut blob = ParallelTransferBlob::EMPTY;
    assert_eq!(
        unsafe { elephc_parallel_value_writer_finish(writer, &mut blob) },
        PARALLEL_TRANSFER_OK
    );
    blob
}

/// Reads one borrowed test view from a live blob and advances its caller-owned cursor.
fn read(blob: ParallelTransferBlob, cursor: &mut usize) -> ParallelValueView {
    let mut view = ParallelValueView::EMPTY;
    assert_eq!(
        unsafe { elephc_parallel_value_read(blob, cursor, &mut view) },
        PARALLEL_TRANSFER_OK
    );
    view
}

/// Exposes test view bytes only while their source blob remains alive.
fn view_bytes(value: ParallelBytes) -> &'static [u8] {
    unsafe { std::slice::from_raw_parts(value.ptr, value.len) }
}

/// Nested key/value, scalar and cancellation records preserve source order and exact payloads.
#[test]
fn nested_transferable_values_round_trip_in_source_order() {
    let writer = writer();
    assert_eq!(
        unsafe { elephc_parallel_value_begin_assoc(writer, 1) },
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(
        unsafe { elephc_parallel_value_write_string(writer, b"items".as_ptr(), 5) },
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(
        unsafe { elephc_parallel_value_begin_indexed(writer, 4) },
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(
        unsafe { elephc_parallel_value_write_int(writer, -7) },
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(
        unsafe { elephc_parallel_value_write_bool(writer, 1) },
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(
        unsafe { elephc_parallel_value_write_float(writer, 1.5f64.to_bits()) },
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(
        unsafe { elephc_parallel_value_write_cancellation(writer, 42) },
        PARALLEL_TRANSFER_OK
    );
    let blob = finish(writer);
    assert_eq!(
        unsafe { elephc_parallel_value_validate(blob) },
        PARALLEL_TRANSFER_OK
    );

    let mut cursor = 0;
    assert_eq!(
        unsafe { elephc_parallel_value_reader_begin(blob, &mut cursor) },
        PARALLEL_TRANSFER_OK
    );
    let assoc = read(blob, &mut cursor);
    assert_eq!(assoc.tag, PARALLEL_VALUE_ASSOC_ARRAY);
    assert_eq!(assoc.count, 1);
    let key = read(blob, &mut cursor);
    assert_eq!(key.tag, PARALLEL_VALUE_STRING);
    assert_eq!(view_bytes(key.bytes), b"items");
    let array = read(blob, &mut cursor);
    assert_eq!(array.tag, PARALLEL_VALUE_INDEXED_ARRAY);
    assert_eq!(array.count, 4);
    let integer = read(blob, &mut cursor);
    assert_eq!(integer.int_value, -7);
    assert_eq!(read(blob, &mut cursor).tag, PARALLEL_VALUE_TRUE);
    let float = read(blob, &mut cursor);
    assert_eq!(f64::from_bits(float.float_bits), 1.5);
    assert_eq!(read(blob, &mut cursor).cancellation_id, 42);
    assert_eq!(cursor, blob.len - TRANSFER_HEADER_LEN);
    unsafe { elephc_parallel_transfer_free(blob) };
}

/// Writers reject incomplete children and extra roots without publishing a partial transfer blob.
#[test]
fn incomplete_or_overfull_writers_fail_closed() {
    let incomplete = writer();
    assert_eq!(
        unsafe { elephc_parallel_value_begin_indexed(incomplete, 1) },
        PARALLEL_TRANSFER_OK
    );
    let mut output = ParallelTransferBlob::EMPTY;
    assert_eq!(
        unsafe { elephc_parallel_value_writer_finish(incomplete, &mut output) },
        PARALLEL_TRANSFER_INVALID_ENVELOPE
    );
    assert!(output.ptr.is_null());

    let overfull = writer();
    assert_eq!(
        unsafe { elephc_parallel_value_write_null(overfull) },
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(
        unsafe { elephc_parallel_value_write_null(overfull) },
        PARALLEL_TRANSFER_INVALID_ARGUMENT
    );
    unsafe { elephc_parallel_value_writer_free(overfull) };
}

/// Unknown tags, trailing records and zero cancellation ids fail envelope validation.
#[test]
fn malformed_tags_trailing_bytes_and_zero_cancellation_ids_are_rejected() {
    for payload in [
        [&VALUE_MAGIC[..], &[255]].concat(),
        [&VALUE_MAGIC[..], &[PARALLEL_VALUE_NULL as u8, 0]].concat(),
        [
            &VALUE_MAGIC[..],
            &[PARALLEL_VALUE_CANCELLATION as u8],
            &0u64.to_le_bytes(),
        ]
        .concat(),
    ] {
        let mut blob = ParallelTransferBlob::EMPTY;
        assert_eq!(
            unsafe {
                elephc_parallel_transfer_copy(payload.as_ptr(), payload.len(), &mut blob)
            },
            PARALLEL_TRANSFER_OK
        );
        assert_eq!(
            unsafe { elephc_parallel_value_validate(blob) },
            PARALLEL_TRANSFER_INVALID_ENVELOPE
        );
        unsafe { elephc_parallel_transfer_free(blob) };
    }
}
