//! Purpose:
//! Checks ownership of fresh JSON containers, child cells, and source pins.
//!
//! Called from:
//! - Focused `codegen_tests` runtime GC integration tests.
//!
//! Key details:
//! - Every fixture requires a balanced heap with IR optimization enabled and disabled.
//! - PHP comparison is optional locally and uses actual CLI output when requested.

use crate::support::*;

/// Runs a JSON ownership fixture in both optimizer modes and checks the optional PHP oracle.
fn check_json_owners(source: &str, expected: &str) {
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let dir = make_cli_test_dir("elephc_json_decode_owners");
        let php = dir.join("main.php");
        fs::write(&php, source).expect("write JSON ownership fixture");
        let compile = elephc_cli_command(&dir).args([mode, "--heap-debug"])
            .arg(&php).output().expect("compile JSON ownership fixture");
        if !compile.status.success() {
            fs::remove_dir_all(&dir).expect("remove failed JSON fixture");
            panic!("{mode}: {}", String::from_utf8_lossy(&compile.stderr));
        }
        let output = run_binary(&dir.join("main"), &dir);
        let oracle = std::env::var_os("ELEPHC_PHP_CHECK").map(|_| {
            Command::new("php").arg("-n").arg(&php).output().expect("run PHP oracle")
        });
        fs::remove_dir_all(&dir).expect("remove JSON ownership fixture");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{mode}: {stderr}");
        assert_eq!(output.stdout, expected.as_bytes(), "{mode}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {stderr}");
        if let Some(php) = oracle {
            assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
            assert_eq!(output.stdout, php.stdout, "PHP mismatch with {mode}");
        }
    }
}

/// Nested arrays release fresh child boxes and their raw container owners.
#[test]
fn test_json_decode_owners_nested_arrays() {
    check_json_owners(r#"<?php
$source = '[[1,2],[3,4],[],[true,false,null,"x"]]';
$value = json_decode($source, true);
echo json_encode($value);
unset($value);
unset($source);
"#, "[[1,2],[3,4],[],[true,false,null,\"x\"]]");
}

/// Associative objects release temporary key boxes while hash entries adopt value boxes.
#[test]
fn test_json_decode_owners_associative_objects_and_duplicate_keys() {
    check_json_owners(r#"<?php
$source = '{"a":[1,2],"a":{"b":3},"0":"zero","empty":{}}';
$value = json_decode($source, true);
echo json_encode($value);
unset($value);
unset($source);
"#, "{\"a\":{\"b\":3},\"0\":\"zero\",\"empty\":[]}");
}

/// Default object decoding transfers the hash into stdClass and then into its Mixed owner.
#[test]
fn test_json_decode_owners_stdclass_and_empty_containers() {
    check_json_owners(r#"<?php
$source = '{"a":{"b":3},"0":"zero","empty":{},"array":[]}';
$value = json_decode($source);
echo json_encode($value);
unset($value);
unset($source);
"#, "{\"a\":{\"b\":3},\"0\":\"zero\",\"empty\":{},\"array\":[]}");
}

/// A copied decoded array keeps its children alive until its final alias is released.
#[test]
fn test_json_decode_owners_cow_alias() {
    check_json_owners(r#"<?php
$source = '[[1,2],[3,4]]';
$value = json_decode($source, true);
$copy = $value;
unset($value);
echo json_encode($copy);
unset($copy);
unset($source);
"#, "[[1,2],[3,4]]");
}

/// String values remain independent of the decoder scratch buffer and the released source.
#[test]
fn test_json_decode_owners_escaped_strings() {
    check_json_owners(r#"<?php
$source = '["a\u0000b","\u00e9","line\nend",[]]';
$value = json_decode($source, true);
unset($source);
echo bin2hex($value[0]), ':', bin2hex($value[1]), ':', bin2hex($value[2]);
unset($value);
"#, "610062:c3a9:6c696e650a656e64");
}

/// Named and unpacked argument plans preserve source lifetime while returning independent graphs.
#[test]
fn test_json_decode_owners_named_and_unpacked_sources() {
    check_json_owners(r#"<?php
$source = '{"a":[1,2]}';
$value = json_decode(associative: true, json: $source);
echo json_encode($value), '|';
unset($value);
$value = json_decode(...[$source, true]);
echo json_encode($value);
unset($value);
unset($source);
"#, "{\"a\":[1,2]}|{\"a\":[1,2]}");
}

/// Nonthrowing parse errors release partial arrays and keys not yet inserted into hashes.
#[test]
fn test_json_decode_owners_partial_errors() {
    check_json_owners(r#"<?php
$array = '[[1,2],[3,]]';
$value = json_decode($array, true);
echo gettype($value), ':', json_last_error(), '|';
unset($value);
$object = '{"a":[1,2],"b":}';
$value = json_decode($object, true);
echo gettype($value), ':', json_last_error();
unset($value);
unset($array);
unset($object);
"#, "NULL:4|NULL:4");
}

/// Throwing parse errors unwind partial containers before publishing the exception.
#[test]
fn test_json_decode_owners_throwing_partial_error() {
    check_json_owners(r#"<?php
$source = '{"a":[1,2],"b":{"c":[3,]}}';
try {
    $value = json_decode($source, true, 512, JSON_THROW_ON_ERROR);
    echo 'miss';
} catch (JsonException $error) {
    echo $error->getCode();
}
unset($source);
"#, "4");
}

/// Repeated decode and release reuse a small heap instead of retaining each parsed graph.
#[test]
fn test_json_decode_owners_repeated_decode_release() {
    check_json_owners(r#"<?php
$source = '[[1,2,3,4,5],[6,7,8,9,10]]';
for ($i = 0; $i < 100 + $argc; $i++) {
    $value = json_decode($source, true);
    unset($value);
}
echo $i;
unset($source);
"#, "101");
}

/// Later argument failures retire a fresh source and its persisted evaluation pin once each.
#[test]
fn test_json_decode_owners_later_argument_throws() {
    check_json_owners(r#"<?php
function failJsonAssoc(): bool { throw new RuntimeException('stop'); }
try {
    $value = json_decode(str_repeat('x', 20 + $argc), failJsonAssoc());
    echo 'miss';
} catch (RuntimeException $error) {
    echo $error->getMessage();
}
"#, "stop");
}

/// A depth failure releases containers built before the failing child and permits another decode.
#[test]
fn test_json_decode_owners_depth_error_then_success() {
    check_json_owners(r#"<?php
$source = '[[1],[[2]]]';
try {
    $value = json_decode($source, true, 2, JSON_THROW_ON_ERROR);
    echo 'miss';
} catch (JsonException $error) {
    echo $error->getCode(), '|';
}
$value = json_decode($source, true);
echo json_encode($value);
unset($value);
unset($source);
"#, "1|[[1],[[2]]]");
}
