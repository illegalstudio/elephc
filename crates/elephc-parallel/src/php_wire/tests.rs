//! Purpose:
//! Unit tests for conversion between PHP's safe serialized-value subset and `EPV1`.
//!
//! Called from:
//! - `cargo test -p elephc-parallel` through `crate::php_wire`.
//!
//! Key details:
//! - Safe scalar and array bytes round-trip without borrowing PHP runtime storage.
//! - Object, reference, malformed, trailing, and cancellation payloads fail closed.

use super::*;
use crate::{
    elephc_parallel_transfer_free, elephc_parallel_value_validate,
    elephc_parallel_value_write_cancellation,
};
use crate::job::{
    elephc_parallel_job_cancel, elephc_parallel_job_cancellation_requested,
    elephc_parallel_job_mark_running, elephc_parallel_job_output_copy, elephc_parallel_job_release,
    elephc_parallel_job_wait, PARALLEL_JOB_COMPLETED,
};

/// Encodes test bytes through the C ABI and checks that failed conversion leaves an empty output.
fn from_php(source: &[u8]) -> Result<ParallelTransferBlob, i32> {
    let mut output = ParallelTransferBlob::EMPTY;
    let status = unsafe {
        elephc_parallel_value_from_php_serialized(source.as_ptr(), source.len(), &mut output)
    };
    if status == PARALLEL_TRANSFER_OK {
        Ok(output)
    } else {
        assert!(output.ptr.is_null());
        Err(status)
    }
}

#[test]
/// Ensures an impossible native buffer capacity returns null instead of unwinding through the ABI.
fn php_transfer_buffer_allocation_fails_closed_on_capacity_overflow() {
    assert!(elephc_parallel_php_buffer_alloc(usize::MAX).is_null());
}

/// Copies a decoded serialization into Rust-owned bytes and releases its temporary transfer blob.
fn to_php(blob: ParallelTransferBlob) -> Result<Vec<u8>, i32> {
    let mut output = ParallelTransferBlob::EMPTY;
    let status = unsafe { elephc_parallel_value_to_php_serialized(blob, &mut output) };
    if status != PARALLEL_TRANSFER_OK {
        assert!(output.ptr.is_null());
        return Err(status);
    }
    let payload = unsafe {
        std::slice::from_raw_parts(
            output.ptr.add(TRANSFER_HEADER_LEN),
            output.len - TRANSFER_HEADER_LEN,
        )
    }
    .to_vec();
    unsafe { elephc_parallel_transfer_free(output) };
    Ok(payload)
}

#[test]
/// Checks nested arrays, sparse keys and binary byte strings survive the PHP/EPV1 round trip.
fn safe_nested_values_round_trip_through_epv1() {
    let source = b"a:3:{s:4:\"name\";s:5:\"Ada\0!\";i:4;a:3:{i:0;N;i:1;b:1;i:2;d:-1.5;}s:3:\"bin\";s:3:\"\xff\0x\";}";
    let value = from_php(source).expect("safe PHP value should encode");
    assert_eq!(
        unsafe { elephc_parallel_value_validate(value) },
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(to_php(value).expect("EPV1 value should decode"), source);
    unsafe { elephc_parallel_transfer_free(value) };
}

#[test]
/// Confirms consecutive zero-based PHP keys select indexed EPV1 storage without changing output.
fn sequential_php_array_is_compacted_to_indexed_epv1() {
    let source = b"a:2:{i:0;s:1:\"a\";i:1;s:1:\"b\";}";
    let value = from_php(source).expect("indexed PHP array should encode");
    let mut cursor = 0;
    assert_eq!(
        unsafe { elephc_parallel_value_reader_begin(value, &mut cursor) },
        PARALLEL_TRANSFER_OK
    );
    let mut view = ParallelValueView::EMPTY;
    assert_eq!(
        unsafe { elephc_parallel_value_read(value, &mut cursor, &mut view) },
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(view.tag, PARALLEL_VALUE_INDEXED_ARRAY);
    assert_eq!(view.count, 2);
    assert_eq!(to_php(value).expect("indexed EPV1 should decode"), source);
    unsafe { elephc_parallel_transfer_free(value) };
}

#[test]
/// Checks both infinities and NaN retain PHP's special serialized token spellings.
fn php_special_floats_use_php_tokens() {
    for source in [&b"d:INF;"[..], &b"d:-INF;"[..], &b"d:NAN;"[..]] {
        let value = from_php(source).expect("special float should encode");
        assert_eq!(to_php(value).expect("special float should decode"), source);
        unsafe { elephc_parallel_transfer_free(value) };
    }
}

#[test]
/// Rejects objects, references, incomplete containers, incorrect string lengths and trailing bytes.
fn unsafe_or_malformed_php_payloads_fail_closed() {
    for source in [
        &b"O:8:\"stdClass\":0:{}"[..],
        &b"C:1:\"X\":0:{}"[..],
        &b"r:1;"[..],
        &b"R:1;"[..],
        &b"a:1:{i:0;}"[..],
        &b"N;trailing"[..],
        &b"s:4:\"abc\";"[..],
    ] {
        assert!(matches!(
            from_php(source),
            Err(PARALLEL_TRANSFER_INVALID_ENVELOPE)
        ));
    }
}

#[test]
/// Prevents an EPV1 cancellation capability from being reconstructed through PHP unserialize.
fn cancellation_is_not_routed_through_php_unserialize() {
    let writer = elephc_parallel_value_writer_new();
    assert!(!writer.is_null());
    assert_eq!(
        unsafe { elephc_parallel_value_write_cancellation(writer, 7) },
        PARALLEL_TRANSFER_OK
    );
    let mut value = ParallelTransferBlob::EMPTY;
    assert_eq!(
        unsafe { elephc_parallel_value_writer_finish(writer, &mut value) },
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(to_php(value), Err(PARALLEL_TRANSFER_INVALID_ARGUMENT));
    unsafe { elephc_parallel_transfer_free(value) };
}

#[test]
/// Exercises owned input and result copies across job creation, running, completion and release.
fn php_oriented_job_boundary_owns_and_converts_values() {
    let input = b"a:2:{i:0;s:5:\"input\";i:1;i:42;}";
    let id = unsafe { elephc_parallel_job_create_php_serialized(input.as_ptr(), input.len()) };
    assert_ne!(id, 0);

    let mut worker_input = ParallelTransferBlob::EMPTY;
    assert_eq!(
        unsafe { elephc_parallel_job_input_php_serialized(id, &mut worker_input) },
        PARALLEL_TRANSFER_OK
    );
    let worker_bytes = unsafe {
        std::slice::from_raw_parts(
            worker_input.ptr.add(TRANSFER_HEADER_LEN),
            worker_input.len - TRANSFER_HEADER_LEN,
        )
    };
    assert_eq!(worker_bytes, input);
    unsafe { elephc_parallel_transfer_free(worker_input) };

    assert_eq!(elephc_parallel_job_mark_running(id), 1);
    let result = b"a:1:{s:6:\"result\";s:3:\"yes\";}";
    assert_eq!(
        unsafe {
            elephc_parallel_job_complete_php_serialized(id, result.as_ptr(), result.len())
        },
        1
    );

    let mut parent_result = ParallelTransferBlob::EMPTY;
    assert_eq!(
        unsafe { elephc_parallel_job_result_php_serialized(id, &mut parent_result) },
        PARALLEL_TRANSFER_OK
    );
    let parent_bytes = unsafe {
        std::slice::from_raw_parts(
            parent_result.ptr.add(TRANSFER_HEADER_LEN),
            parent_result.len - TRANSFER_HEADER_LEN,
        )
    };
    assert_eq!(parent_bytes, result);
    unsafe { elephc_parallel_transfer_free(parent_result) };
    assert_eq!(elephc_parallel_job_release(id), 1);
}

#[test]
/// Checks malformed creation fails and an invalid result request clears a stale output descriptor.
fn php_oriented_job_boundary_clears_outputs_on_failure() {
    assert_eq!(
        unsafe { elephc_parallel_job_create_php_serialized(b"O:0:{}".as_ptr(), 6) },
        0
    );

    let mut output = ParallelTransferBlob {
        ptr: std::ptr::dangling_mut::<u8>(),
        len: usize::MAX,
    };
    assert_ne!(
        unsafe { elephc_parallel_job_result_php_serialized(u64::MAX, &mut output) },
        PARALLEL_TRANSFER_OK
    );
    assert!(output.ptr.is_null());
    assert_eq!(output.len, 0);
}

#[test]
/// Verifies prepared bytes are replaced, isolated between threads and released idempotently.
fn prepared_php_blob_is_thread_local_replaceable_and_releasable() {
    elephc_parallel_php_blob_release();
    assert!(elephc_parallel_php_blob_ptr().is_null());
    assert_eq!(elephc_parallel_php_blob_len(), 0);

    let first = b"s:3:\"one\";";
    let first_id =
        unsafe { elephc_parallel_job_create_php_serialized(first.as_ptr(), first.len()) };
    assert_ne!(first_id, 0);
    assert_eq!(
        elephc_parallel_job_input_php_prepare(first_id),
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(elephc_parallel_php_blob_len(), first.len());
    assert_eq!(
        unsafe {
            std::slice::from_raw_parts(
                elephc_parallel_php_blob_ptr(),
                elephc_parallel_php_blob_len(),
            )
        },
        first
    );

    let second = b"s:3:\"two\";";
    let second_id =
        unsafe { elephc_parallel_job_create_php_serialized(second.as_ptr(), second.len()) };
    assert_ne!(second_id, 0);
    assert_eq!(
        elephc_parallel_job_input_php_prepare(second_id),
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(
        unsafe {
            std::slice::from_raw_parts(
                elephc_parallel_php_blob_ptr(),
                elephc_parallel_php_blob_len(),
            )
        },
        second
    );

    let other_thread = std::thread::spawn(|| {
        assert!(elephc_parallel_php_blob_ptr().is_null());
        assert_eq!(elephc_parallel_php_blob_len(), 0);
    });
    other_thread.join().unwrap();

    elephc_parallel_php_blob_release();
    assert!(elephc_parallel_php_blob_ptr().is_null());
    assert_eq!(elephc_parallel_php_blob_len(), 0);
    assert_eq!(crate::job::elephc_parallel_job_cancel(first_id), 1);
    assert_eq!(elephc_parallel_job_release(first_id), 1);
    assert_eq!(crate::job::elephc_parallel_job_cancel(second_id), 1);
    assert_eq!(elephc_parallel_job_release(second_id), 1);
}

#[test]
/// Checks the generated-copy allocator preserves NUL and non-UTF-8 bytes through native storage.
fn php_copy_buffer_preserves_binary_bytes() {
    let bytes = b"A\0B\xff";
    let pointer = elephc_parallel_php_buffer_alloc(bytes.len());
    assert!(!pointer.is_null());
    unsafe { pointer.copy_from_nonoverlapping(bytes.as_ptr(), bytes.len()) };
    assert_eq!(
        unsafe { std::slice::from_raw_parts(pointer, bytes.len()) },
        bytes
    );
    unsafe { elephc_parallel_php_buffer_free(pointer, bytes.len()) };
}

#[test]
/// Uses a publication barrier to show cooperative cancellation preserves an already decoded result.
fn running_cancellation_during_php_result_transfer_keeps_the_terminal_result() {
    let input = b"N;";
    let id = unsafe { elephc_parallel_job_create_php_serialized(input.as_ptr(), input.len()) };
    assert_ne!(id, 0);
    assert_eq!(elephc_parallel_job_mark_running(id), 1);

    arm_completion_transfer_pause(id);
    let result = b"s:4:\"done\";";
    let worker = std::thread::spawn(move || unsafe {
        elephc_parallel_job_complete_php_serialized(id, result.as_ptr(), result.len())
    });
    wait_for_completion_transfer_pause(id);

    // A running cancellation is cooperative. It records the request, then the in-flight,
    // already-decoded worker result publishes atomically as the terminal outcome.
    assert_eq!(elephc_parallel_job_cancel(id), 1);
    assert_eq!(elephc_parallel_job_cancellation_requested(id), 1);
    release_completion_transfer_pause(id);

    assert_eq!(worker.join().expect("completion-transfer worker"), 1);
    assert_eq!(elephc_parallel_job_wait(id), PARALLEL_JOB_COMPLETED);
    assert_eq!(elephc_parallel_job_cancel(id), 0);

    let mut output = ParallelTransferBlob::EMPTY;
    assert_eq!(
        unsafe { elephc_parallel_job_output_copy(id, 1, &mut output) },
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(to_php(output).expect("completed PHP result"), result);
    unsafe { elephc_parallel_transfer_free(output) };
    assert_eq!(elephc_parallel_job_release(id), 1);
}

#[test]
/// Round-trips remote failure fields and frames as data without transferring Throwable pointers.
fn php_failure_fields_round_trip_without_throwable_pointers() {
    let input = b"N;";
    let id = unsafe { elephc_parallel_job_create_php_serialized(input.as_ptr(), input.len()) };
    assert_ne!(id, 0);
    assert_eq!(elephc_parallel_job_mark_running(id), 1);
    let failure = b"a:7:{i:0;i:1;i:1;s:11:\"DomainError\";i:2;s:7:\"bad\0msg\";i:3;i:-7;i:4;s:8:\"task.php\";i:5;i:41;i:6;a:1:{i:0;a:3:{i:0;s:6:\"worker\";i:1;s:8:\"task.php\";i:2;i:41;}}}";
    assert_eq!(
        unsafe {
            elephc_parallel_job_fail_php_serialized(id, failure.as_ptr(), failure.len())
        },
        1
    );
    assert_eq!(
        elephc_parallel_job_failure_php_prepare(id),
        PARALLEL_TRANSFER_OK
    );
    assert_eq!(
        unsafe {
            std::slice::from_raw_parts(
                elephc_parallel_php_blob_ptr(),
                elephc_parallel_php_blob_len(),
            )
        },
        failure
    );
    elephc_parallel_php_blob_release();
    assert_eq!(elephc_parallel_job_release(id), 1);
}
