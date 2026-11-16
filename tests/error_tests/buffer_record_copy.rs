//! Purpose:
//! Type errors for whole-record packed buffer assignment.
//!
//! Called from:
//! - The error integration test harness.
//!
//! Key details:
//! - Identical byte layouts do not make different packed classes assignment-compatible.
//! - Dynamic and untyped sources cannot prove the record type to copy.

use super::*;

/// Rejects other packed classes through local, instance and static buffer targets.
#[test]
fn test_error_buffer_record_copy_rejects_different_classes() {
    let prefix = "<?php packed class A { public int $id; } packed class B { public int $id; } ";
    for body in [
        "$a = buffer_new<A>(1); $b = buffer_new<B>(1); $a[0] = $b[0];",
        "class Store { public buffer<A> $data; } $s = new Store(); $s->data = buffer_new<A>(1); $b = buffer_new<B>(1); $s->data[0] = $b[0];",
        "class Store { public static buffer<A> $data; } Store::$data = buffer_new<A>(1); $b = buffer_new<B>(1); Store::$data[0] = $b[0];",
    ] {
        expect_error(&format!("{prefix}{body}"), "Buffer element type mismatch: expected A record, got ptr<B>");
    }
}

/// Requires a typed record address rather than null, mixed or an untyped pointer.
#[test]
fn test_error_buffer_record_copy_requires_typed_address() {
    let prefix = "<?php packed class Record { public int $id; } $dst = buffer_new<Record>(1); ";
    for body in [
        "$dst[0] = null;",
        "ptr $p = ptr_null(); $dst[0] = $p;",
        "function put(buffer<Record> $dst, mixed $value): void { $dst[0] = $value; }",
    ] {
        expect_error(&format!("{prefix}{body}"), "Buffer element type mismatch: expected Record record");
    }
}
