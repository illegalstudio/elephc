//! Purpose:
//! Integration tests for the `zval` pack/unpack bridge extension, covering scalar,
//! string, null, and array (packed + hash, nested) round-trips plus `zval_type` and
//! `zval_free` behavior, builtin arg-count errors, and the legacy callable-wrapper
//! link-regression guard.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each fixture packs an elephc value into a PHP `zval`, then either inspects the
//!   `IS_*` type byte via `zval_type` or unpacks it back and reads the recovered value.
//! - Array fixtures assert recovered CONTENTS (values/keys/count), not just the
//!   `IS_ARRAY` type byte: a type-byte-only check leaves packed-array unpack/pack
//!   defects (stale loop index, COW-flag value_type mask, missing nested-array arms)
//!   invisible, since an array of nulls is still `IS_ARRAY`.

use crate::support::*;

/// Packs an integer and asserts `zval_type` reports `IS_LONG` (4).
#[test]
fn test_zval_type_int() {
    let out = compile_and_run(r#"<?php echo zval_type(zval_pack(42));"#);
    assert_eq!(out, "4");
}

/// Packs a float and asserts `zval_type` reports `IS_DOUBLE` (5).
#[test]
fn test_zval_type_float() {
    let out = compile_and_run(r#"<?php echo zval_type(zval_pack(1.5));"#);
    assert_eq!(out, "5");
}

/// Packs a string and asserts `zval_type` reports `IS_STRING` (6).
#[test]
fn test_zval_type_string() {
    let out = compile_and_run(r#"<?php echo zval_type(zval_pack("hi"));"#);
    assert_eq!(out, "6");
}

/// Packs true and asserts `zval_type` reports `IS_TRUE` (3).
#[test]
fn test_zval_type_true() {
    let out = compile_and_run(r#"<?php echo zval_type(zval_pack(true));"#);
    assert_eq!(out, "3");
}

/// Packs false and asserts `zval_type` reports `IS_FALSE` (2).
#[test]
fn test_zval_type_false() {
    let out = compile_and_run(r#"<?php echo zval_type(zval_pack(false));"#);
    assert_eq!(out, "2");
}

/// Packs null and asserts `zval_type` reports `IS_NULL` (1).
#[test]
fn test_zval_type_null() {
    let out = compile_and_run(r#"<?php echo zval_type(zval_pack(null));"#);
    assert_eq!(out, "1");
}

/// Roundtrips a positive integer through pack/unpack and echoes the value.
#[test]
fn test_zval_roundtrip_int() {
    let out = compile_and_run(r#"<?php echo zval_unpack(zval_pack(42));"#);
    assert_eq!(out, "42");
}

/// Roundtrips a negative integer through pack/unpack and echoes the value.
#[test]
fn test_zval_roundtrip_int_negative() {
    let out = compile_and_run(r#"<?php echo zval_unpack(zval_pack(-7));"#);
    assert_eq!(out, "-7");
}

/// Roundtrips a float through pack/unpack and echoes the value.
#[test]
fn test_zval_roundtrip_float() {
    let out = compile_and_run(r#"<?php echo zval_unpack(zval_pack(1.5));"#);
    assert_eq!(out, "1.5");
}

/// Roundtrips true through pack/unpack; PHP echoes true as `1`.
#[test]
fn test_zval_roundtrip_true() {
    let out = compile_and_run(r#"<?php echo zval_unpack(zval_pack(true));"#);
    assert_eq!(out, "1");
}

/// Roundtrips false through pack/unpack; PHP echoes false as the empty string.
#[test]
fn test_zval_roundtrip_false() {
    let out = compile_and_run(r#"<?php echo zval_unpack(zval_pack(false));"#);
    assert_eq!(out, "");
}

/// Roundtrips null through pack/unpack; PHP echoes null as the empty string.
#[test]
fn test_zval_roundtrip_null() {
    let out = compile_and_run(r#"<?php echo zval_unpack(zval_pack(null));"#);
    assert_eq!(out, "");
}

/// Roundtrips a non-empty string through pack/unpack and echoes it.
#[test]
fn test_zval_roundtrip_string() {
    let out = compile_and_run(r#"<?php echo zval_unpack(zval_pack("hello"));"#);
    assert_eq!(out, "hello");
}

/// Roundtrips an empty string through pack/unpack; PHP echoes it as empty.
#[test]
fn test_zval_roundtrip_empty_string() {
    let out = compile_and_run(r#"<?php echo zval_unpack(zval_pack(""));"#);
    assert_eq!(out, "");
}

/// Roundtrips a string containing spaces and punctuation to exercise byte copy.
#[test]
fn test_zval_roundtrip_string_punct() {
    let out = compile_and_run(r#"<?php echo zval_unpack(zval_pack("a b!c"));"#);
    assert_eq!(out, "a b!c");
}

/// Frees a zval holding a string and confirms control flow continues cleanly.
#[test]
fn test_zval_free_string_no_crash() {
    let out = compile_and_run(
        r#"<?php
$z = zval_pack("hello");
zval_free($z);
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Frees a zval holding an integer and confirms control flow continues cleanly.
#[test]
fn test_zval_free_int_no_crash() {
    let out = compile_and_run(
        r#"<?php
$z = zval_pack(42);
zval_free($z);
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Packs a string, unpacks and echoes it, then frees the zval (full lifecycle).
#[test]
fn test_zval_pack_unpack_free_string() {
    let out = compile_and_run(
        r#"<?php
$z = zval_pack("hello");
echo zval_unpack($z);
zval_free($z);
"#,
    );
    assert_eq!(out, "hello");
}

/// Roundtrips an integer stored in a local variable (not just a literal).
#[test]
fn test_zval_roundtrip_int_from_var() {
    let out = compile_and_run(
        r#"<?php
$x = 40;
$z = zval_pack($x);
echo zval_unpack($z);
"#,
    );
    assert_eq!(out, "40");
}

/// Packs the same value twice to confirm repeated allocation does not corrupt.
#[test]
fn test_zval_pack_twice() {
    let out = compile_and_run(
        r#"<?php
$z1 = zval_pack(7);
$z2 = zval_pack(11);
echo zval_unpack($z1);
echo zval_unpack($z2);
"#,
    );
    assert_eq!(out, "711");
}

/// Asserts a source fails to compile, returning the panic message payload.
fn assert_compile_error(source: &str) -> String {
    let result = std::panic::catch_unwind(|| compile_and_run(source));
    match result {
        Err(payload) => payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "<non-string panic>".to_string()),
        Ok(out) => panic!("expected compile error, got output: {out}"),
    }
}

/// Errors when `zval_pack` is called with no arguments.
#[test]
fn test_zval_pack_no_args_errors() {
    let msg = assert_compile_error(r#"<?php zval_pack();"#);
    assert!(
        msg.contains("zval_pack") || msg.contains("argument"),
        "got: {msg}"
    );
}

/// Errors when `zval_type` is called with no arguments.
#[test]
fn test_zval_type_no_args_errors() {
    let msg = assert_compile_error(r#"<?php zval_type();"#);
    assert!(
        msg.contains("zval_type") || msg.contains("argument"),
        "got: {msg}"
    );
}

// -- Stage 2: packed int/string/nested array pack/unpack --

/// Packs a small indexed int array and asserts `zval_type` reports `IS_ARRAY` (7).
#[test]
fn test_zval_type_packed_array_int() {
    let out = compile_and_run(r#"<?php echo zval_type(zval_pack([1, 2, 3]));"#);
    assert_eq!(out, "7");
}

/// Packs an indexed int array that exceeds the minimum packed table size (8),
/// exercising the next-power-of-two growth of the packed HashTable.
#[test]
fn test_zval_type_packed_array_grows() {
    let out = compile_and_run(r#"<?php echo zval_type(zval_pack([1, 2, 3, 4, 5, 6, 7, 8, 9]));"#);
    assert_eq!(out, "7");
}

/// Packs a string-element indexed array and asserts `zval_type` reports `IS_ARRAY` (7).
#[test]
fn test_zval_type_packed_array_string() {
    let out = compile_and_run(r#"<?php echo zval_type(zval_pack(["a", "b"]));"#);
    assert_eq!(out, "7");
}

/// Frees a zval holding a packed int array and confirms control flow continues.
#[test]
fn test_zval_free_packed_int_no_crash() {
    let out = compile_and_run(
        r#"<?php
$z = zval_pack([1, 2, 3]);
zval_free($z);
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Frees a zval holding a packed string array (each bucket owns a zend_string).
#[test]
fn test_zval_free_packed_string_no_crash() {
    let out = compile_and_run(
        r#"<?php
$z = zval_pack(["aa", "bb", "cc"]);
zval_free($z);
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Roundtrips a packed int array through pack -> unpack -> pack and confirms the
/// rebuilt value re-packs as `IS_ARRAY`, exercising the array rebuild path.
#[test]
fn test_zval_roundtrip_packed_array_type() {
    let out = compile_and_run(
        r#"<?php echo zval_type(zval_pack(zval_unpack(zval_pack([1, 2, 3]))));"#,
    );
    assert_eq!(out, "7");
}

/// Roundtrips a packed string array through pack -> unpack -> pack and confirms
/// the rebuilt value re-packs as `IS_ARRAY` (string-element rebuild path).
#[test]
fn test_zval_roundtrip_packed_string_array_type() {
    let out = compile_and_run(
        r#"<?php echo zval_type(zval_pack(zval_unpack(zval_pack(["x", "y", "z"]))));"#,
    );
    assert_eq!(out, "7");
}

/// Packs, unpacks, re-packs, then frees a packed int array (full nested lifecycle).
#[test]
fn test_zval_pack_unpack_free_packed_array() {
    let out = compile_and_run(
        r#"<?php
$z = zval_pack([1, 2, 3]);
$u = zval_pack(zval_unpack($z));
zval_free($u);
zval_free($z);
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Roundtrips a nested packed array (array of arrays) through pack -> unpack -> pack
/// and confirms the nested rebuilt value re-packs as `IS_ARRAY`.
#[test]
fn test_zval_roundtrip_nested_packed_array_type() {
    let out = compile_and_run(
        r#"<?php echo zval_type(zval_pack(zval_unpack(zval_pack([[1, 2], [3, 4]]))));"#,
    );
    assert_eq!(out, "7");
}

// -- Stage 3: associative (hash) array pack/unpack --

/// Packs a string-keyed assoc array and asserts `zval_type` reports `IS_ARRAY` (7).
#[test]
fn test_zval_type_hash_array() {
    let out = compile_and_run(r#"<?php echo zval_type(zval_pack(["a" => 1, "b" => 2]));"#);
    assert_eq!(out, "7");
}

/// Packs an assoc array exceeding the minimum hash table size (8), exercising
/// the next-power-of-two growth of the hash HashTable.
#[test]
fn test_zval_type_hash_array_grows() {
    let out = compile_and_run(
        r#"<?php echo zval_type(zval_pack(["a" => 1, "b" => 2, "c" => 3, "d" => 4, "e" => 5, "f" => 6, "g" => 7, "h" => 8, "i" => 9]));"#,
    );
    assert_eq!(out, "7");
}

/// Packs a mixed int/string-keyed assoc array and asserts `zval_type` reports `IS_ARRAY` (7).
#[test]
fn test_zval_type_hash_mixed_keys() {
    let out = compile_and_run(r#"<?php echo zval_type(zval_pack([5 => "x", "k" => "y"]));"#);
    assert_eq!(out, "7");
}

/// Frees a zval holding a string-keyed hash and confirms control flow continues.
#[test]
fn test_zval_free_hash_no_crash() {
    let out = compile_and_run(
        r#"<?php
$z = zval_pack(["a" => 1, "b" => 2]);
zval_free($z);
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Frees a zval holding a mixed-key hash with string values (each bucket owns a
/// zend_string key plus a zend_string value) and confirms control flow continues.
#[test]
fn test_zval_free_hash_string_values_no_crash() {
    let out = compile_and_run(
        r#"<?php
$z = zval_pack(["a" => "alpha", "b" => "beta", "c" => "gamma"]);
zval_free($z);
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Roundtrips a string-keyed hash through pack -> unpack -> pack and confirms the
/// rebuilt assoc array re-packs as `IS_ARRAY`.
#[test]
fn test_zval_roundtrip_hash_array_type() {
    let out = compile_and_run(
        r#"<?php echo zval_type(zval_pack(zval_unpack(zval_pack(["a" => 1, "b" => 2]))));"#,
    );
    assert_eq!(out, "7");
}

/// Roundtrips a hash with string values (key + value both zend_string) through
/// pack -> unpack -> pack and confirms it re-packs as `IS_ARRAY`.
#[test]
fn test_zval_roundtrip_hash_string_values_type() {
    let out = compile_and_run(
        r#"<?php echo zval_type(zval_pack(zval_unpack(zval_pack(["a" => "alpha", "b" => "beta"]))));"#,
    );
    assert_eq!(out, "7");
}

/// Roundtrips a mixed int/string-keyed hash through pack -> unpack -> pack and
/// confirms the rebuilt value re-packs as `IS_ARRAY`.
#[test]
fn test_zval_roundtrip_hash_mixed_keys_type() {
    let out = compile_and_run(
        r#"<?php echo zval_type(zval_pack(zval_unpack(zval_pack([5 => "x", "k" => "y"]))));"#,
    );
    assert_eq!(out, "7");
}

/// Packs, unpacks, re-packs, then frees a string-keyed hash (full nested lifecycle).
#[test]
fn test_zval_pack_unpack_free_hash_array() {
    let out = compile_and_run(
        r#"<?php
$z = zval_pack(["a" => 1, "b" => 2, "c" => 3]);
$u = zval_pack(zval_unpack($z));
zval_free($u);
zval_free($z);
echo "ok";
"#,
    );
    assert_eq!(out, "ok");
}

/// Packs a nested hash (hash of hashes) and asserts `zval_type` reports `IS_ARRAY`.
#[test]
fn test_zval_type_nested_hash_array() {
    let out = compile_and_run(
        r#"<?php echo zval_type(zval_pack(["outer" => ["inner" => 1, "k" => 2], "x" => ["y" => 3]]));"#,
    );
    assert_eq!(out, "7");
}

/// Roundtrips a nested hash (hash of hashes) through pack -> unpack -> pack and
/// confirms the rebuilt value re-packs as `IS_ARRAY`.
#[test]
fn test_zval_roundtrip_nested_hash_array_type() {
    let out = compile_and_run(
        r#"<?php echo zval_type(zval_pack(zval_unpack(zval_pack(["outer" => ["inner" => 1], "x" => ["y" => 2]]))));"#,
    );
    assert_eq!(out, "7");
}

/// Roundtrips a hash with more than 8 string keys through pack -> unpack -> pack to
/// exercise DJBX33A collision chains across a grown hash table.
#[test]
fn test_zval_roundtrip_hash_many_keys_type() {
    let out = compile_and_run(
        r#"<?php echo zval_type(zval_pack(zval_unpack(zval_pack(["k1" => 1, "k2" => 2, "k3" => 3, "k4" => 4, "k5" => 5, "k6" => 6, "k7" => 7, "k8" => 8, "k9" => 9, "k10" => 10]))));"#,
    );
    assert_eq!(out, "7");
}

// -- Stage 4: content round-trips. These read the recovered VALUES back (not just
// the IS_ARRAY type byte), which is what catches packed-array unpack/pack defects:
// a stale-loop-index store, a too-wide value_type mask that swallows the COW flag,
// and missing nested-array dispatch arms all leave the type byte intact while
// corrupting every element. Reading the contents back is the only effective guard.

/// Reads back a packed int array's values after a pack/unpack round-trip via implode.
/// Regression guard: a stale loop index or a value_type-mask bug yields empty elements.
#[test]
fn test_zval_roundtrip_packed_int_values() {
    let out = compile_and_run(r#"<?php echo implode(",", zval_unpack(zval_pack([10, 20, 30])));"#);
    assert_eq!(out, "10,20,30");
}

/// Reads back a single-element packed array (index 0 isolates the value_type mask bug
/// from the loop-index bug, since i*8 == 0 regardless of any stale index).
#[test]
fn test_zval_roundtrip_packed_single_value() {
    let out = compile_and_run(
        r#"<?php $u = zval_unpack(zval_pack([99])); echo $u[0];"#,
    );
    assert_eq!(out, "99");
}

/// Reads back packed string-array elements by index after a round-trip.
#[test]
fn test_zval_roundtrip_packed_string_values() {
    let out = compile_and_run(
        r#"<?php $u = zval_unpack(zval_pack(["aa", "bb", "cc"])); echo $u[0], $u[1], $u[2];"#,
    );
    assert_eq!(out, "aabbcc");
}

/// Iterates an unpacked packed array with foreach to confirm every slot holds a value.
#[test]
fn test_zval_roundtrip_packed_foreach() {
    let out = compile_and_run(
        r#"<?php foreach (zval_unpack(zval_pack([1, 2, 3])) as $v) { echo $v, ";"; }"#,
    );
    assert_eq!(out, "1;2;3;");
}

/// Confirms the unpacked packed array reports the correct element count.
#[test]
fn test_zval_roundtrip_packed_count() {
    let out = compile_and_run(
        r#"<?php echo count(zval_unpack(zval_pack([1, 2, 3, 4, 5])));"#,
    );
    assert_eq!(out, "5");
}

/// Reads back string-keyed hash values by key after a pack/unpack round-trip.
#[test]
fn test_zval_roundtrip_hash_values() {
    let out = compile_and_run(
        r#"<?php $u = zval_unpack(zval_pack(["a" => 1, "b" => 2])); echo $u["a"], $u["b"];"#,
    );
    assert_eq!(out, "12");
}

/// Reads back a mixed int/string-keyed hash by both key kinds after a round-trip.
#[test]
fn test_zval_roundtrip_hash_mixed_key_values() {
    let out = compile_and_run(
        r#"<?php $u = zval_unpack(zval_pack([5 => "x", "k" => "y"])); echo $u[5], $u["k"];"#,
    );
    assert_eq!(out, "xy");
}

/// Reads back string values from a string-keyed hash after a round-trip.
#[test]
fn test_zval_roundtrip_hash_string_values() {
    let out = compile_and_run(
        r#"<?php $u = zval_unpack(zval_pack(["a" => "alpha", "b" => "beta"])); echo $u["a"], "-", $u["b"];"#,
    );
    assert_eq!(out, "alpha-beta");
}

/// Reads back inner values of a nested packed array (array of arrays) after a round-trip.
/// Regression guard: missing nested-array (value_type 4) dispatch packs inner elements as null.
#[test]
fn test_zval_roundtrip_nested_packed_values() {
    let out = compile_and_run(
        r#"<?php $u = zval_unpack(zval_pack([[1, 2], [3, 4]])); echo $u[0][0], $u[0][1], $u[1][0], $u[1][1];"#,
    );
    assert_eq!(out, "1234");
}

/// Reads back inner values of a nested hash (hash of hashes) after a round-trip.
#[test]
fn test_zval_roundtrip_nested_hash_values() {
    let out = compile_and_run(
        r#"<?php $u = zval_unpack(zval_pack(["outer" => ["inner" => 1], "x" => ["y" => 2]])); echo $u["outer"]["inner"], $u["x"]["y"];"#,
    );
    assert_eq!(out, "12");
}

/// Reads back a packed array of hashes (value_type 5 nested-hash dispatch arm).
#[test]
fn test_zval_roundtrip_packed_of_hash_values() {
    let out = compile_and_run(
        r#"<?php $u = zval_unpack(zval_pack([["a" => 1], ["b" => 2]])); echo $u[0]["a"], $u[1]["b"];"#,
    );
    assert_eq!(out, "12");
}

/// Reads back a hash of packed arrays after a round-trip.
#[test]
fn test_zval_roundtrip_hash_of_packed_values() {
    let out = compile_and_run(
        r#"<?php $u = zval_unpack(zval_pack(["x" => [1, 2], "y" => [3, 4]])); echo $u["x"][0], $u["y"][1];"#,
    );
    assert_eq!(out, "14");
}

/// Reads back every value of a grown packed array (> 8 elements) after a round-trip,
/// confirming the packed table growth path preserves all element payloads.
#[test]
fn test_zval_roundtrip_packed_grown_values() {
    let out = compile_and_run(
        r#"<?php echo implode(",", zval_unpack(zval_pack([1, 2, 3, 4, 5, 6, 7, 8, 9])));"#,
    );
    assert_eq!(out, "1,2,3,4,5,6,7,8,9");
}

/// Round-trips PHP_INT_MAX, PHP_INT_MIN, and a negative through a packed array to
/// confirm full 64-bit integer payloads survive without truncation or sign loss.
#[test]
fn test_zval_roundtrip_packed_int64_extremes() {
    let out = compile_and_run(
        r#"<?php echo implode(",", zval_unpack(zval_pack([9223372036854775807, -9223372036854775807, -7])));"#,
    );
    assert_eq!(out, "9223372036854775807,-9223372036854775807,-7");
}

/// Round-trips a string containing an embedded NUL byte to confirm zend_string copies
/// by length (not as a C string truncated at the first NUL).
#[test]
fn test_zval_roundtrip_string_embedded_nul() {
    let out = compile_and_run(
        r#"<?php echo strlen(zval_unpack(zval_pack("a\0b")));"#,
    );
    assert_eq!(out, "3");
}

// -- Stage 5: error tests for wrong argument count (one per builtin per CLAUDE.md). --

/// Errors when `zval_unpack` is called with no arguments.
#[test]
fn test_zval_unpack_no_args_errors() {
    let msg = assert_compile_error(r#"<?php zval_unpack();"#);
    assert!(
        msg.contains("zval_unpack") || msg.contains("argument"),
        "got: {msg}"
    );
}

/// Errors when `zval_free` is called with no arguments.
#[test]
fn test_zval_free_no_args_errors() {
    let msg = assert_compile_error(r#"<?php zval_free();"#);
    assert!(
        msg.contains("zval_free") || msg.contains("argument"),
        "got: {msg}"
    );
}

/// Errors when `zval_pack` is called with too many arguments.
#[test]
fn test_zval_pack_too_many_args_errors() {
    let msg = assert_compile_error(r#"<?php zval_pack(1, 2);"#);
    assert!(
        msg.contains("zval_pack") || msg.contains("argument"),
        "got: {msg}"
    );
}

// -- Stage 6: regression guard for the legacy callable-wrapper link fix. --

/// Compiles a program that uses BOTH a dynamic string callback (array_map) AND the
/// zval builtins. Regression guard: before the fix, adding the zval builtins to the
/// catalog made the legacy dynamic-call dispatch emit unresolved `_fn_zval_u_*`
/// wrapper references, breaking the link of any dynamic-callback program. The zval
/// builtins are now excluded from that wrapper, so this must compile, link, and run.
#[test]
fn test_zval_with_dynamic_callback_links() {
    let out = compile_and_run(
        r#"<?php
function ztype($x) { $z = zval_pack($x); $t = zval_type($z); zval_free($z); return $t; }
echo implode(",", array_map('ztype', [1, 1.5, "s"]));
"#,
    );
    // IS_LONG=4, IS_DOUBLE=5, IS_STRING=6
    assert_eq!(out, "4,5,6");
}

/// Regression test: `zval_free` on a packed string must release the owned
/// `zend_string` AND the 16-byte zval block. `__rt_zval_free` stashed the zval
/// pointer in a caller-saved register (`x10`/`r10`) across the
/// `__rt_zval_free_children` call; for a string/array child, `free_children`
/// makes a nested `__rt_heap_free` call that clobbers that register, so the zval
/// block was freed from a garbage pointer (range-rejected) and leaked one
/// 16-byte block per call. Scalars were unaffected because their `free_children`
/// makes no nested call. The pointer is now spilled to the stack frame. Heap
/// must be clean at exit.
#[test]
fn test_zval_free_string_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$z = zval_pack("hello");
zval_free($z);
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: `zval_free` on a packed array must release every owned
/// PHP-shaped block (the zval, the `zend_array`, the data block, each bucket's
/// `zend_string` key and string/array-value children) with no leak. The array is
/// packed from a local so the operand is a borrowed value rather than a fresh
/// owning temporary — the latter exercises a separate, pre-existing general
/// builtin-argument-temporary leak unrelated to the zval path. Heap must be
/// clean at exit.
#[test]
fn test_zval_free_array_from_local_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = ["alpha" => 1, "beta" => "two", "gamma" => [3, 4]];
$z = zval_pack($a);
zval_free($z);
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Regression test: a packed array recovered via `zval_unpack` must be a fully
/// heap-recognized elephc array that `foreach` can iterate. `__rt_zval_unpack_array`
/// rebuilds the array with `__rt_array_new` (which stamps the x86_64 heap ownership
/// marker `0x454C5048` in the kind word's high 32 bits) and then overwrites the kind
/// word to set value_type 7. The x86_64 overwrite dropped the marker, so
/// `__rt_heap_kind` reported kind 0 and `foreach` raised "foreach over iterable with
/// unsupported kind"; the marker is now preserved. arm64 has no such marker and was
/// already correct. (This caught an x86_64-only failure that the macOS suite cannot
/// observe — the kind-0 misclassification only bites where the marker is checked.)
#[test]
fn test_zval_unpack_packed_array_is_heap_recognized_by_foreach() {
    let out = compile_and_run(
        r#"<?php
$a = [1, 2, 3];
$s = "";
foreach (zval_unpack(zval_pack($a)) as $v) {
    $s .= $v . ";";
}
echo $s;
"#,
    );
    assert_eq!(out, "1;2;3;");
}

/// Reads back float elements of a packed array after a round-trip. Regression guard
/// for the packed-pack `value_type 2` (IS_DOUBLE) element dispatch arm, which the
/// int/string/nested fixtures never exercise.
#[test]
fn test_zval_roundtrip_packed_float_values() {
    let out = compile_and_run(
        r#"<?php $u = zval_unpack(zval_pack([1.5, 2.5])); echo $u[0], "|", $u[1];"#,
    );
    assert_eq!(out, "1.5|2.5");
}

/// Reads back boolean elements of a packed array after a round-trip. Regression guard
/// for the packed-pack `value_type 3` (IS_TRUE/IS_FALSE) element dispatch arm.
#[test]
fn test_zval_roundtrip_packed_bool_values() {
    let out = compile_and_run(
        r#"<?php $u = zval_unpack(zval_pack([true, false])); echo ($u[0] ? "T" : "F"), ($u[1] ? "T" : "F");"#,
    );
    assert_eq!(out, "TF");
}

/// Reads back null elements of a packed array after a round-trip. Regression guard
/// for the packed-pack null element arm: a null payload must round-trip as a present
/// `null` entry, not a dropped slot or garbage value.
#[test]
fn test_zval_roundtrip_packed_null_values() {
    let out = compile_and_run(
        r#"<?php $u = zval_unpack(zval_pack([null, null])); echo count($u), (is_null($u[0]) ? "y" : "n"), (is_null($u[1]) ? "y" : "n");"#,
    );
    assert_eq!(out, "2yy");
}

/// Errors when `zval_unpack` receives a non-pointer argument (a string here): the
/// checker's `ensure_pointer_type` rejects it before lowering.
#[test]
fn test_zval_unpack_non_pointer_arg_errors() {
    let msg = assert_compile_error(r#"<?php zval_unpack("x");"#);
    assert!(
        msg.contains("zval_unpack") || msg.contains("pointer"),
        "got: {msg}"
    );
}

/// Errors when `zval_type` receives a non-pointer argument (an int here).
#[test]
fn test_zval_type_non_pointer_arg_errors() {
    let msg = assert_compile_error(r#"<?php zval_type(42);"#);
    assert!(
        msg.contains("zval_type") || msg.contains("pointer"),
        "got: {msg}"
    );
}

/// Errors when `zval_free` receives a non-pointer argument (an int here).
#[test]
fn test_zval_free_non_pointer_arg_errors() {
    let msg = assert_compile_error(r#"<?php zval_free(1);"#);
    assert!(
        msg.contains("zval_free") || msg.contains("pointer"),
        "got: {msg}"
    );
}

/// Regression test: `zval_free` on a packed (indexed) array must release the zval,
/// the `zend_array`, its data block, and each element child with no leak — the
/// packed-bucket free path, exercised here at the top level rather than only as a
/// nested child of a hash. Heap must be clean at exit.
#[test]
fn test_zval_free_packed_array_does_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$a = [10, 20, 30];
$z = zval_pack($a);
zval_free($z);
echo "done";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "done");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}