//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of array associative-array helper builtins, including array key exists, in array string, and in array integer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::*;

/// Verifies `ksort()` orders associative keys ascending without mutating a copied alias.
#[test]
fn test_assoc_ksort_orders_keys_and_preserves_copy() {
    let out = compile_and_run(
        r#"<?php
$sorted = ["b" => 2, "a" => 1, "c" => 3];
$original = $sorted;
$result = ksort($sorted);
echo $result ? "true|" : "false|";
foreach ($sorted as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
foreach ($original as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "true|a:1,b:2,c:3,|b:2,a:1,c:3,");
}

/// Verifies `krsort()` orders associative keys descending while preserving their values.
#[test]
fn test_assoc_krsort_orders_keys_descending() {
    let out = compile_and_run(
        r#"<?php
$values = ["b" => 2, "a" => 1, "c" => 3];
$result = krsort($values);
echo $result ? "true|" : "false|";
foreach ($values as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "true|c:3,b:2,a:1,");
}

/// Verifies `ksort()` applies PHP regular comparison to mixed integer and string keys.
#[test]
fn test_assoc_ksort_compares_mixed_integer_and_string_keys_by_value() {
    let out = compile_and_run(
        r#"<?php
$numeric = ["007" => "string", 500 => "integer"];
ksort($numeric);
foreach ($numeric as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
$lexical = [5 => "integer", "!a" => "string"];
ksort($lexical);
foreach ($lexical as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "007:string,500:integer,|!a:string,5:integer,");
}

/// Verifies `krsort()` reverses PHP regular comparison for mixed integer and string keys.
#[test]
fn test_assoc_krsort_compares_mixed_integer_and_string_keys_by_value() {
    let out = compile_and_run(
        r#"<?php
$numeric = ["007" => "string", 500 => "integer"];
krsort($numeric);
foreach ($numeric as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
$lexical = [5 => "integer", "!a" => "string"];
krsort($lexical);
foreach ($lexical as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "500:integer,007:string,|5:integer,!a:string,");
}

/// Verifies mixed native integers and overflowing integer-string keys preserve
/// insertion order when PHP's binary64 comparison considers them equal.
#[test]
fn test_assoc_key_sorts_keep_i64_boundary_mixed_keys_stable() {
    let out = compile_and_run(
        r#"<?php
$positive_int_first = [9223372036854775807 => "int", "9223372036854775808" => "string"];
ksort($positive_int_first);
foreach ($positive_int_first as $value) echo $value . ",";
echo "|";
$positive_string_first = ["9223372036854775808" => "string", 9223372036854775807 => "int"];
ksort($positive_string_first);
foreach ($positive_string_first as $value) echo $value . ",";
echo "|";
$negative_int_first = [-9223372036854775808 => "int", "-9223372036854775809" => "string"];
ksort($negative_int_first);
foreach ($negative_int_first as $value) echo $value . ",";
echo "|";
$negative_string_first = ["-9223372036854775809" => "string", -9223372036854775808 => "int"];
ksort($negative_string_first);
foreach ($negative_string_first as $value) echo $value . ",";
echo "|";
$descending = ["  +09223372036854775808  " => "string", 9223372036854775807 => "int"];
krsort($descending);
foreach ($descending as $value) echo $value . ",";
"#,
    );
    assert_eq!(
        out,
        "int,string,|string,int,|int,string,|string,int,|string,int,"
    );
}

/// Verifies key sorting compares retained numeric-string keys numerically in both directions.
#[test]
fn test_assoc_key_sorts_compare_numeric_string_keys_numerically() {
    let out = compile_and_run(
        r#"<?php
$ascending = ["010" => "ten", "02" => "two"];
ksort($ascending);
foreach ($ascending as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
$descending = ["010" => "ten", "02" => "two"];
krsort($descending);
foreach ($descending as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "02:two,010:ten,|010:ten,02:two,");
}

/// Verifies bounded numeric-key parsing applies signed decimal exponents in both directions.
#[test]
fn test_assoc_key_sorts_compare_negative_exponent_keys_numerically() {
    let out = compile_and_run(
        r#"<?php
$ascending = ["0.1" => "tenth", "1e-2" => "hundredth"];
ksort($ascending);
foreach ($ascending as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
$descending = ["1e-2" => "hundredth", "0.1" => "tenth"];
krsort($descending);
foreach ($descending as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "1e-2:hundredth,0.1:tenth,|0.1:tenth,1e-2:hundredth,");
}

/// Verifies integer-like string keys retain exact ordering beyond binary64 precision.
#[test]
fn test_assoc_key_sorts_keep_large_integer_string_comparisons_exact() {
    let out = compile_and_run(
        r#"<?php
$strings = [];
$strings["+9007199254740993"] = "high";
$strings["+9007199254740992"] = "low";
ksort($strings);
foreach ($strings as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
$mixed = [];
$mixed["+9007199254740993"] = "string";
$mixed[9007199254740992] = "integer";
ksort($mixed);
foreach ($mixed as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
$descending = [];
$descending[9007199254740992] = "integer";
$descending["+9007199254740993"] = "string";
krsort($descending);
foreach ($descending as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(
        out,
        "+9007199254740992:low,+9007199254740993:high,|9007199254740992:integer,+9007199254740993:string,|+9007199254740993:string,9007199254740992:integer,"
    );
}

/// Verifies exact key parsing accepts trailing whitespace without joining separated digits.
#[test]
fn test_assoc_key_sorts_bound_decimal_whitespace() {
    let out = compile_and_run(
        r#"<?php
$trailing = [];
$trailing["+9007199254740993 "] = "string";
$trailing[9007199254740992] = "integer";
ksort($trailing);
foreach ($trailing as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
$internal = ["1 2" => "string", 5 => "integer"];
ksort($internal);
foreach ($internal as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
$signOnly = ["+ " => "string", -1 => "integer"];
ksort($signOnly);
foreach ($signOnly as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(
        out,
        "9007199254740992:integer,+9007199254740993 :string,|1 2:string,5:integer,|+ :string,-1:integer,"
    );
}

/// Verifies key sorting does not treat libc-only numeric spellings as PHP numeric strings.
#[test]
fn test_assoc_key_sorts_reject_libc_only_numeric_string_spellings() {
    let out = compile_and_run(
        r#"<?php
$ascending = [5 => "integer", "0x10" => "hex", "+INF" => "infinity"];
ksort($ascending);
foreach ($ascending as $key => $value) {
    echo $key . ":" . $value . ",";
}
echo "|";
$descending = [5 => "integer", "0x10" => "hex", "+INF" => "infinity"];
krsort($descending);
foreach ($descending as $key => $value) {
    echo $key . ":" . $value . ",";
}
"#,
    );
    assert_eq!(
        out,
        "+INF:infinity,0x10:hex,5:integer,|5:integer,0x10:hex,+INF:infinity,"
    );
}

/// Verifies key sorting handles strings larger than the legacy 4096-byte C-string scratch.
#[test]
fn test_assoc_key_sorts_bound_long_string_keys() {
    let out = compile_and_run(
        r#"<?php
$long = str_repeat("x", 9000);
$values = ["z" => 1, $long => 2];
ksort($values);
foreach ($values as $key => $value) {
    echo strlen($key) . ":" . $value . ",";
}
"#,
    );
    assert_eq!(out, "9000:2,1:1,");
}

/// Verifies regular key sorting matches PHP's correctly rounded numeric-string order,
/// including overflow and numeric keys larger than the legacy scratch buffer.
#[test]
fn test_assoc_key_sorts_match_php_numeric_string_rounding_and_overflow() {
    let out = compile_and_run(
        r#"<?php
$rounding = [
    "1.05414925678100014" => "later",
    "1.05414925678100013" => "earlier",
];
ksort($rounding);
foreach ($rounding as $value) { echo $value . ","; }
echo "|";

$overflow = ["2e309" => "two", "1e309" => "one"];
ksort($overflow);
foreach ($overflow as $value) { echo $value . ","; }
echo "|";

$huge = "1e" . str_repeat("9", 9001);
$long = ["2e309" => "two", $huge => "huge"];
ksort($long);
foreach ($long as $value) { echo $value . ","; }
"#,
    );
    assert_eq!(out, "earlier,later,|one,two,|huge,two,");
}

/// Verifies integer-form string keys beyond the signed 64-bit range use PHP's
/// exact ordering when their binary64 coercions compare equal.
#[test]
fn test_assoc_key_sorts_order_i64_overflow_integer_spellings_exactly() {
    let out = compile_and_run(
        r#"<?php
$decimal = [
    "99999999999999999999" => "ninety-nine",
    "99999999999999999998" => "ninety-eight",
];
ksort($decimal);
foreach ($decimal as $value) { echo $value, ","; }
echo "|";

$unsigned = [
    "+9223372036854775808" => "above-signed",
    "09223372036854775807" => "signed-max",
];
ksort($unsigned);
foreach ($unsigned as $value) { echo $value, ","; }
echo "|";

$same_value = [
    "9223372036854775808" => "canonical",
    "09223372036854775808" => "padded",
];
ksort($same_value);
foreach ($same_value as $value) { echo $value, ","; }
echo "|";

$reverse = [
    "99999999999999999998" => "ninety-eight",
    "99999999999999999999" => "ninety-nine",
];
krsort($reverse);
foreach ($reverse as $value) { echo $value, ","; }
"#,
    );
    assert_eq!(
        out,
        "ninety-eight,ninety-nine,|signed-max,above-signed,|padded,canonical,|ninety-nine,ninety-eight,"
    );
}

/// Verifies bounded numeric-key parsing never copies through the fixed C-string scratch buffer.
#[test]
fn test_assoc_key_numeric_parser_avoids_fixed_c_string_scratch() {
    let runtime_asm = elephc::codegen::generate_runtime_with_features(
        8_388_608,
        target(),
        elephc::codegen::RuntimeFeatures::none(),
    );
    let start = runtime_asm
        .find("__rt_str_looks_like_int_for_coercion:")
        .expect("missing bounded numeric-key parser");
    let tail = &runtime_asm[start..];
    let end = tail.find("\n    ret").expect("missing numeric-key parser return") + 9;
    let parser_asm = &tail[..end];

    assert!(
        !parser_asm.contains("__rt_cstr"),
        "bounded numeric-key parsing must not use the fixed C-string scratch buffer:\n{parser_asm}"
    );
}

/// Verifies numeric-key exponent handling has no exponent-sized multiply or divide backedge
/// in either supported runtime architecture.
#[test]
fn test_assoc_key_numeric_parser_has_constant_time_scale_application() {
    for runtime_target in [
        Target::new(Platform::MacOS, Arch::AArch64),
        Target::new(Platform::Linux, Arch::X86_64),
    ] {
        let runtime_asm = elephc::codegen::generate_runtime_with_features(
            8_388_608,
            runtime_target,
            elephc::codegen::RuntimeFeatures::none(),
        );
        let start = runtime_asm
            .find("__rt_str_looks_like_int_for_coercion:")
            .expect("missing bounded numeric-key parser");
        let parser_asm = &runtime_asm[start..];

        for backedge in [
            "cbnz x14, L__rt_sliic_scale_divide",
            "cbnz x14, L__rt_sliic_scale_multiply",
            "jnz L__rt_sliic_scale_divide_x",
            "jnz L__rt_sliic_scale_multiply_x",
        ] {
            assert!(
                !parser_asm.contains(backedge),
                "{runtime_target:?} numeric-key parsing must not apply an attacker-controlled exponent one step at a time: {backedge}"
            );
        }
    }
}

// --- Associative array function tests ---

/// Verifies array_key_exists() returns true for present keys and false for absent ones.
/// Fixture: two-element string-keyed assoc array, two lookups (one present, one absent).
#[test]
fn test_assoc_array_key_exists() {
    let out = compile_and_run(
        r#"<?php
$m = ["name" => "Alice", "age" => "30"];
if (array_key_exists("name", $m)) { echo "yes"; }
if (array_key_exists("missing", $m)) { echo "bad"; } else { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

/// Verifies in_array() with string needle finds string values in assoc array.
/// Fixture: two-element string-keyed assoc array, string needle "apple" present, "cherry" absent.
#[test]
fn test_assoc_in_array_str() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => "apple", "b" => "banana"];
if (in_array("apple", $m)) { echo "yes"; }
if (in_array("cherry", $m)) { echo "bad"; } else { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

/// Verifies in_array() with integer needle finds integer values in assoc array.
/// Fixture: two-element string-keyed assoc array, integer needle 10 present, 99 absent.
#[test]
fn test_assoc_in_array_int() {
    let out = compile_and_run(
        r#"<?php
$m = ["x" => 10, "y" => 20];
if (in_array(10, $m)) { echo "yes"; }
if (in_array(99, $m)) { echo "bad"; } else { echo "no"; }
"#,
    );
    assert_eq!(out, "yesno");
}

/// Verifies array_search() returns the key for a found string value.
/// Fixture: two-element string-keyed assoc array, searches for "Bob".
#[test]
fn test_assoc_array_search_str() {
    let out = compile_and_run(
        r#"<?php
$m = ["first" => "Alice", "second" => "Bob"];
$key = array_search("Bob", $m);
echo $key;
"#,
    );
    assert_eq!(out, "second");
}

/// Verifies array_search() returns integer keys as integers and string keys as strings.
/// Fixture: assoc array with int key `10` and string key `"02"`, each holding a distinct value.
#[test]
fn test_assoc_array_search_returns_integer_and_string_keys() {
    let out = compile_and_run(
        r#"<?php
$m = [10 => "Alice", "02" => "Bob"];
echo array_search("Alice", $m);
echo "|";
echo array_search("Bob", $m);
"#,
    );
    assert_eq!(out, "10|02");
}

/// Verifies array_search() with an integer key fits the int|bool return type annotation.
/// Fixture: int-keyed assoc array with declared return type `int|bool` on the wrapper function.
#[test]
fn test_assoc_array_search_integer_key_matches_declared_union_return() {
    let out = compile_and_run(
        r#"<?php
function find_key(): int|bool {
    $m = [10 => "Alice", 20 => "Bob"];
    return array_search("Alice", $m);
}

echo find_key();
"#,
    );
    assert_eq!(out, "10");
}

/// Verifies array_search() returns strictly false (not 0 or empty string) when value is absent.
/// Fixture: two-element string-keyed assoc array, searches for "Carol" which is not present.
#[test]
fn test_assoc_array_search_not_found_is_strict_false() {
    let out = compile_and_run(
        r#"<?php
$m = ["first" => "Alice", "second" => "Bob"];
echo array_search("Carol", $m) === false ? "miss" : "hit";
"#,
    );
    assert_eq!(out, "miss");
}

/// Verifies array_keys() returns all keys of an assoc array in insertion order.
/// Fixture: two-element string-keyed assoc array, iterates and echoes keys with spaces.
#[test]
fn test_assoc_array_keys() {
    let out = compile_and_run(
        r#"<?php
$m = ["x" => 1, "y" => 2];
$keys = array_keys($m);
$n = count($keys);
for ($i = 0; $i < $n; $i++) {
    echo $keys[$i] . " ";
}
"#,
    );
    assert_eq!(out, "x y ");
}

/// Verifies array_keys() preserves integer key `1` and string key `"02"` as distinct types.
/// Fixture: assoc array with mixed int/string keys; echoes both keys separated by `|`.
#[test]
fn test_assoc_array_keys_preserves_integer_and_string_keys() {
    let out = compile_and_run(
        r#"<?php
$m = [1 => "one", "02" => "two"];
$keys = array_keys($m);
echo $keys[0] . "|" . $keys[1];
"#,
    );
    assert_eq!(out, "1|02");
}

/// Verifies array_search() returns the first-matching key in insertion order, not the last.
/// Fixture: three-element assoc array where "same" maps to two keys; confirms only first is returned and array size is unchanged.
#[test]
fn test_assoc_array_search_returns_first_inserted_matching_key() {
    let out = compile_and_run(
        r#"<?php
$m = ["first" => "same", "second" => "same", "third" => "other"];
$key = array_search("same", $m);
echo $key;
echo "|";
echo count($m);
"#,
    );
    assert_eq!(out, "first|3");
}

/// Verifies array_values() returns all string values of a string-keyed assoc array.
/// Fixture: two-element string-keyed assoc array, iterates and echoes values with spaces.
#[test]
fn test_assoc_array_values_str() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => "one", "b" => "two"];
$vals = array_values($m);
$n = count($vals);
for ($i = 0; $i < $n; $i++) {
    echo $vals[$i] . " ";
}
"#,
    );
    assert_eq!(out, "one two ");
}

/// Verifies array_values() returns integer values and they can be used in arithmetic.
/// Fixture: three-element string-keyed assoc array with integer values; sums them to confirm int type.
#[test]
fn test_assoc_array_values_int() {
    let out = compile_and_run(
        r#"<?php
$m = ["a" => 10, "b" => 20, "c" => 30];
$vals = array_values($m);
echo $vals[0] + $vals[1] + $vals[2];
"#,
    );
    assert_eq!(out, "60");
}

/// Verifies foreach over a mixed-type assoc array yields correct key/value pairs.
/// Fixture: three-element assoc array with mixed int and string values, echoes key=value; pairs.
#[test]
fn test_assoc_array_mixed_foreach() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
foreach ($m as $key => $value) {
    echo $key;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(out, "id=7;name=Alice;score=12;");
}

/// Verifies array_values() on a mixed-type assoc array returns values in insertion order.
/// Fixture: three-element assoc array with mixed int/string values, echoes val,val,... format.
#[test]
fn test_assoc_array_values_mixed() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
$vals = array_values($m);
$n = count($vals);
for ($i = 0; $i < $n; $i++) {
    echo $vals[$i];
    echo ",";
}
"#,
    );
    assert_eq!(out, "7,Alice,12,");
}

/// Verifies in_array() finds both string and integer values in a mixed-type assoc array.
/// Fixture: three-element assoc array with mixed types, three lookups: string present, int present, string absent.
#[test]
fn test_assoc_in_array_mixed() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
if (in_array("Alice", $m)) { echo "name"; }
if (in_array(12, $m)) { echo " score"; }
if (!in_array("Bob", $m)) { echo " missing"; }
"#,
    );
    assert_eq!(out, "name score missing");
}

/// Verifies array_search() on a mixed-type assoc array returns correct key for string and int values.
/// Fixture: three-element assoc array with mixed types, searches for "Alice" (string value) and 12 (int value).
#[test]
fn test_assoc_array_search_mixed() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
echo array_search("Alice", $m);
echo ":";
echo array_search(12, $m);
"#,
    );
    assert_eq!(out, "name:score");
}

/// Verifies direct array access via string key on a mixed-type assoc array.
/// Fixture: three-element assoc array with mixed types, accesses $m["name"] and echoes result.
#[test]
fn test_assoc_array_access_mixed_echo() {
    let out = compile_and_run(
        r#"<?php
$m = ["id" => 7, "name" => "Alice", "score" => 12];
echo $m["name"];
"#,
    );
    assert_eq!(out, "Alice");
}

/// Verifies array_values() produces a borrowed reference that survives unset of the source array.
/// Regression: array_values must not copy-or-free source data while borrowed; $vals[0] must remain valid after unset($map).
#[test]
fn test_gc_assoc_array_values_borrowed_array_survives_source_unset() {
    let out = compile_and_run(
        r#"<?php
$map = ["nums" => [7, 8, 9]];
$vals = array_values($map);
unset($map);
$saved = $vals[0];
echo $saved[1];
"#,
    );
    assert_eq!(out, "8");
}

/// `array_reverse` over a Mixed-valued hash reverses insertion order and, without `preserve_keys`,
/// renumbers the surviving integer keys from zero, matching PHP for both key policies.
#[test]
fn test_array_reverse_mixed_valued_hash() {
    let out = compile_and_run(
        r#"<?php
$a = ["x" => 1, "y" => "s"];
var_dump(array_reverse($a));
$b = [1 => 1, 2 => "s"];
var_dump(array_reverse($b));
var_dump(array_reverse($a, true));
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [\"y\"]=>\n  string(1) \"s\"\n  [\"x\"]=>\n  int(1)\n}\n\
array(2) {\n  [0]=>\n  string(1) \"s\"\n  [1]=>\n  int(1)\n}\n\
array(2) {\n  [\"y\"]=>\n  string(1) \"s\"\n  [\"x\"]=>\n  int(1)\n}\n"
    );
}

/// `array_shift` over a Mixed-valued hash returns the head value and renumbers the surviving
/// integer keys from zero, while a copied alias keeps its own order and keys.
#[test]
fn test_array_shift_mixed_valued_hash() {
    let out = compile_and_run(
        r#"<?php
$a = ["x" => 1, "y" => "s"];
$copy = $a;
echo array_shift($a), "|", implode(",", $a), "|", implode(",", $copy), "\n";
$b = [1 => 1, 2 => "s", 5 => 3];
echo array_shift($b), "|", implode(",", $b), "\n";
$c = [];
var_dump(array_shift($c));
"#,
    );
    assert_eq!(out, "1|s|1,s\n1|s,3\nNULL\n");
}

/// `array_unshift` over a Mixed-valued hash prepends values and renumbers the surviving integer
/// keys from zero, while a copied alias keeps its own order and keys. With no values it only
/// reports the current count.
#[test]
fn test_array_unshift_mixed_valued_hash() {
    let out = compile_and_run(
        r#"<?php
$a = ["x" => 1, "y" => "s"];
$copy = $a;
echo array_unshift($a, 9, "z"), "|", implode(",", $a), "|", implode(",", $copy), "\n";
$b = [1 => 1, 2 => "s"];
echo array_unshift($b, 9), "|", implode(",", $b), "\n";
$c = ["only" => 1];
echo array_unshift($c), "|", implode(",", $c), "\n";
"#,
    );
    assert_eq!(out, "4|9,z,1,s|1,s\n3|9,1,s\n1|1\n");
}

/// `rsort` over a Mixed-valued hash reorders the values descending and renumbers the keys from
/// zero, while a copied alias keeps its own order and keys.
#[test]
fn test_rsort_mixed_valued_hash() {
    let out = compile_and_run(
        r#"<?php
$a = ["x" => 1, "y" => "s"];
$copy = $a;
rsort($a);
var_dump($a);
var_dump($copy);
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  string(1) \"s\"\n  [1]=>\n  int(1)\n}\n\
array(2) {\n  [\"x\"]=>\n  int(1)\n  [\"y\"]=>\n  string(1) \"s\"\n}\n"
    );
}

/// A mutating argument expression must not leave `array_unshift` using the stale receiver snapshot
/// the EIR took before argument evaluation: the nested call republishes the receiver's table.
#[test]
fn test_array_unshift_mixed_valued_hash_reloads_receiver_after_argument_mutation() {
    let out = compile_and_run(
        r#"<?php
$a = ["k1" => "x", "k2" => "y", "z" => 3];
array_unshift($a, array_shift($a));
echo count($a), "|", implode(",", $a), "\n";
$b = ["k1" => "x", "k2" => "y", "z" => 3];
array_unshift($b, array_pop($b), count($b));
echo count($b), "|", implode(",", $b), "\n";
"#,
    );
    assert_eq!(out, "3|x,y,3\n4|3,2,x,y\n");
}

/// A miss-read receiver keeps the array shape but holds PHP's null; the four builtins must raise
/// PHP's TypeError instead of dereferencing the in-band null-container sentinel.
#[test]
fn test_mixed_valued_hash_builtins_reject_null_receiver() {
    let out = compile_and_run(
        r#"<?php
$conf = ["a" => ["x" => 1, "y" => "s"]];
$opts = $conf["missing"];
try { array_unshift($opts, 1); } catch (TypeError $e) { echo "u\n"; }
try { array_unshift($opts); } catch (TypeError $e) { echo "u0\n"; }
try { array_shift($opts); } catch (TypeError $e) { echo "s\n"; }
try { $r = array_reverse($opts); } catch (TypeError $e) { echo "r\n"; }
try { rsort($opts); } catch (TypeError $e) { echo "k\n"; }
"#,
    );
    assert_eq!(out, "u\nu0\ns\nr\nk\n");
}

/// The concrete hash `array_pop` and link sorts must also reject a null receiver with PHP's
/// TypeError rather than silently succeeding.
#[test]
fn test_mixed_valued_hash_pop_and_link_sorts_reject_null_receiver() {
    let out = compile_and_run(
        r#"<?php
$conf = ["a" => ["x" => 1, "y" => "s"]];
$opts = $conf["missing"];
try { $r = array_pop($opts); } catch (TypeError $e) { echo "p\n"; }
try { ksort($opts); } catch (TypeError $e) { echo "ks\n"; }
try { asort($opts); } catch (TypeError $e) { echo "as\n"; }
try { krsort($opts); } catch (TypeError $e) { echo "krs\n"; }
try { arsort($opts); } catch (TypeError $e) { echo "ars\n"; }
"#,
    );
    assert_eq!(out, "p\nks\nas\nkrs\nars\n");
}
