//! Purpose:
//! Regression coverage for the runtime class named by boxed array-edge argument errors.
//!
//! Called from:
//! - The codegen runtime-GC integration suite.
//!
//! Key details:
//! - All four edge builtins must report the actual namespaced class, including subclasses.
//! - Repeated rejected temporaries and dynamic exception messages must leave the heap clean.

use crate::support::*;

/// Names runtime classes in every array-edge TypeError without retaining rejected objects.
#[test]
fn test_array_edge_type_errors_name_runtime_classes_and_release_messages() {
    let output = compile_and_run_with_heap_debug(
        r#"<?php
namespace ArrayEdgeReview;
class ParentPayload {}
class ChildPayload extends ParentPayload {}
function rejected_child(): mixed { return new ChildPayload(); }
function rejected_parent(): mixed { return new ParentPayload(); }
for ($i = 0; $i < 192; $i++) {
    try { array_first(rejected_child()); }
    catch (\TypeError $error) { echo $error->getMessage(), "\n"; unset($error); }
    try { array_last(rejected_parent()); }
    catch (\TypeError $error) { echo $error->getMessage(), "\n"; unset($error); }
    try { array_key_first(rejected_child()); }
    catch (\TypeError $error) { echo $error->getMessage(), "\n"; unset($error); }
    try { array_key_last(rejected_parent()); }
    catch (\TypeError $error) { echo $error->getMessage(), "\n"; unset($error); }
}
"#,
    );
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    let expected = concat!(
        "array_first(): Argument #1 ($array) must be of type array, ArrayEdgeReview\\ChildPayload given\n",
        "array_last(): Argument #1 ($array) must be of type array, ArrayEdgeReview\\ParentPayload given\n",
        "array_key_first(): Argument #1 ($array) must be of type array, ArrayEdgeReview\\ChildPayload given\n",
        "array_key_last(): Argument #1 ($array) must be of type array, ArrayEdgeReview\\ParentPayload given\n",
    );
    assert_eq!(output.stdout, expected.repeat(192));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Successful key calls detach their exception pins and retire temporary source arrays once.
#[test]
fn test_array_edge_key_success_releases_temporary_boxed_sources() {
    let output = compile_and_run_with_heap_debug(r#"<?php
function temporary_edge_rows(): mixed { return ["first" => 1, "last" => 2]; }
for ($i = 0; $i < 24; $i++) {
    echo array_key_first(temporary_edge_rows()), ":", array_key_last(temporary_edge_rows()), "|";
}
"#);
    assert!(output.success, "stdout: {}\nstderr: {}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "first:last|".repeat(24));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}
