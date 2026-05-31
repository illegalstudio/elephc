//! Purpose:
//! End-to-end tests for RegexIterator and RecursiveRegexIterator.
//! Covers SPL declaration metadata, regex modes, key matching flags, and recursive child propagation.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the SPL test module.
//!
//! Key details:
//! - Regex capture modes reuse the runtime preg_replace_callback capture array path.
//! - RecursiveRegexIterator must preserve pattern/mode/flags state across child iterators.

use crate::support::*;

/// Verifies that regex iterator classes are declared and implement expected contracts.
#[test]
fn test_regex_iterator_classes_are_declared_and_implement_contracts() {
    let out = compile_and_run(
        r#"<?php
function has_name(array $names, string $target): bool {
    foreach ($names as $name) {
        if ($name === $target) {
            return true;
        }
    }
    return false;
}

var_dump(class_exists("RegexIterator"));
var_dump(class_exists("RecursiveRegexIterator"));
$names = spl_classes();
var_dump(has_name($names, "RegexIterator"));
var_dump(has_name($names, "RecursiveRegexIterator"));
var_dump(RegexIterator::USE_KEY);
var_dump(RegexIterator::INVERT_MATCH);
var_dump(RegexIterator::MATCH);
var_dump(RegexIterator::GET_MATCH);
var_dump(RegexIterator::ALL_MATCHES);
var_dump(RegexIterator::SPLIT);
var_dump(RegexIterator::REPLACE);
$it = new RegexIterator(new ArrayIterator([]), "/a/");
var_dump($it instanceof FilterIterator);
var_dump($it instanceof OuterIterator);
$recursive = new RecursiveRegexIterator(new RecursiveArrayIterator([]), "/a/");
var_dump($recursive instanceof RegexIterator);
var_dump($recursive instanceof RecursiveIterator);
"#,
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "int(1)\n",
            "int(2)\n",
            "int(0)\n",
            "int(1)\n",
            "int(2)\n",
            "int(3)\n",
            "int(4)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
            "bool(true)\n",
        )
    );
}

/// Verifies RegexIterator filtering and mode-specific current values.
#[test]
fn test_regex_iterator_modes() {
    let out = compile_and_run(
        r#"<?php
$source = new ArrayIterator([
    "one" => "foo-12 bar-34",
    "two" => "skip",
]);

$match = new RegexIterator($source, "/([a-z]+)-([0-9]+)/");
foreach ($match as $key => $value) {
    echo "m:";
    echo $key;
    echo "=";
    echo $value;
    echo ";";
}

$get = new RegexIterator($source, "/([a-z]+)-([0-9]+)/", RegexIterator::GET_MATCH);
foreach ($get as $key => $value) {
    echo "g:";
    echo $key;
    echo "=";
    echo $value[0];
    echo "/";
    echo $value[1];
    echo "/";
    echo $value[2];
    echo ";";
}

$all = new RegexIterator($source, "/([a-z]+)-([0-9]+)/", RegexIterator::ALL_MATCHES);
foreach ($all as $key => $value) {
    echo "a:";
    echo $key;
    echo "=";
    echo $value[0][0];
    echo ",";
    echo $value[0][1];
    echo "/";
    echo $value[1][0];
    echo ",";
    echo $value[1][1];
    echo "/";
    echo $value[2][0];
    echo ",";
    echo $value[2][1];
    echo ";";
}

$split = new RegexIterator($source, "/[ -]+/", RegexIterator::SPLIT);
foreach ($split as $key => $value) {
    echo "s:";
    echo $key;
    echo "=";
    echo $value[0];
    echo "/";
    echo $value[1];
    echo "/";
    echo $value[2];
    echo ";";
}

$replace = new RegexIterator($source, "/([a-z]+)-([0-9]+)/", RegexIterator::REPLACE);
$replace->replacement = '$1:$2';
foreach ($replace as $key => $value) {
    echo "r:";
    echo $key;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "m:one=foo-12 bar-34;",
            "g:one=foo-12/foo/12;",
            "a:one=foo-12,bar-34/foo,bar/12,34;",
            "s:one=foo/12/bar;",
            "r:one=foo:12 bar:34;",
        )
    );
}

/// Verifies GET_MATCH returns every capture group instead of a fixed ten-slot window.
#[test]
fn test_regex_iterator_get_match_keeps_more_than_ten_captures() {
    let out = compile_and_run(
        r#"<?php
$it = new RegexIterator(
    new ArrayIterator(["abcdefghijkl"]),
    "/(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)(k)(l)/",
    RegexIterator::GET_MATCH
);
foreach ($it as $match) {
    echo count($match);
    echo ":";
    echo $match[11];
    echo $match[12];
}
"#,
    );
    assert_eq!(out, "13:kl");
}

/// Verifies ALL_MATCHES keeps capture groups and occurrences beyond the old ten-slot grid.
#[test]
fn test_regex_iterator_all_matches_keeps_dynamic_captures_and_occurrences() {
    let out = compile_and_run(
        r#"<?php
$source = "abcdefghijkl abcdefghijkl abcdefghijkl abcdefghijkl abcdefghijkl abcdefghijkl abcdefghijkl abcdefghijkl abcdefghijkl abcdefghijkl abcdefghijkl";
$it = new RegexIterator(
    new ArrayIterator([$source]),
    "/(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)(k)(l)/",
    RegexIterator::ALL_MATCHES
);
foreach ($it as $matches) {
    echo count($matches);
    echo ":";
    echo count($matches[0]);
    echo ":";
    echo $matches[11][10];
    echo $matches[12][10];
}
"#,
    );
    assert_eq!(out, "13:11:kl");
}

/// Verifies RegexIterator exposes PHP preg constants and GET_MATCH offset captures.
#[test]
fn test_regex_iterator_get_match_supports_offset_capture() {
    let out = compile_and_run(
        r#"<?php
echo PREG_OFFSET_CAPTURE;
echo ":";
echo PREG_SPLIT_OFFSET_CAPTURE;
echo ";";
$it = new RegexIterator(
    new ArrayIterator(["a12b34"]),
    "/([a-z])([0-9]+)/",
    RegexIterator::GET_MATCH,
    0,
    PREG_OFFSET_CAPTURE
);
foreach ($it as $match) {
    echo $match[0][0];
    echo "@";
    echo $match[0][1];
    echo "/";
    echo $match[1][0];
    echo "@";
    echo $match[1][1];
    echo "/";
    echo $match[2][0];
    echo "@";
    echo $match[2][1];
}
"#,
    );
    assert_eq!(out, "256:4;a12@0/a@0/12@1");
}

/// Verifies ALL_MATCHES honors PREG_SET_ORDER together with PREG_OFFSET_CAPTURE.
#[test]
fn test_regex_iterator_all_matches_supports_set_order_and_offsets() {
    let out = compile_and_run(
        r#"<?php
$it = new RegexIterator(
    new ArrayIterator(["a12b34"]),
    "/([a-z])([0-9]+)/",
    RegexIterator::ALL_MATCHES,
    0,
    PREG_SET_ORDER | PREG_OFFSET_CAPTURE
);
foreach ($it as $rows) {
    foreach ($rows as $row) {
        echo $row[0][0];
        echo "@";
        echo $row[0][1];
        echo "/";
        echo $row[1][0];
        echo "@";
        echo $row[1][1];
        echo "/";
        echo $row[2][0];
        echo "@";
        echo $row[2][1];
        echo ";";
    }
}
"#,
    );
    assert_eq!(out, "a12@0/a@0/12@1;b34@3/b@3/34@4;");
}

/// Verifies SPLIT mode applies the no-empty preg split flag.
#[test]
fn test_regex_iterator_split_supports_preg_split_flags() {
    let out = compile_and_run(
        r#"<?php
$offsets = new RegexIterator(
    new ArrayIterator(["a12--b34"]),
    "/([a-z])([0-9]+)/",
    RegexIterator::SPLIT,
    0,
    PREG_SPLIT_NO_EMPTY
);
foreach ($offsets as $pieces) {
    foreach ($pieces as $piece) {
        echo $piece;
        echo ";";
    }
}
"#,
    );
    assert_eq!(out, "--;");
}

/// Verifies SPLIT mode applies delimiter and offset capture preg split flags.
#[test]
fn test_regex_iterator_split_supports_delimiter_and_offset_capture() {
    let out = compile_and_run(
        r#"<?php
$offsets = new RegexIterator(
    new ArrayIterator(["a12--b34"]),
    "/([a-z])([0-9]+)/",
    RegexIterator::SPLIT,
    0,
    PREG_SPLIT_NO_EMPTY | PREG_SPLIT_DELIM_CAPTURE | PREG_SPLIT_OFFSET_CAPTURE
);
foreach ($offsets as $pieces) {
    foreach ($pieces as $piece) {
        echo $piece[0];
        echo "@";
        echo $piece[1];
        echo ";";
    }
}
"#,
    );
    assert_eq!(out, "a@0;12@1;--@3;b@5;34@6;");
}

/// Verifies RegexIterator SPLIT mode keeps delimiter captures beyond the old fixed
/// 99-capture runtime window inherited from preg_split.
#[test]
fn test_regex_iterator_split_keeps_delimiter_captures_beyond_ninety_nine() {
    let out = compile_and_run(
        r#"<?php
$pattern = "/";
$subject = "";
for ($i = 1; $i <= 105; $i = $i + 1) {
    $pattern = $pattern . "(.)";
    $subject = $subject . ($i === 105 ? "z" : "a");
}
$pattern = $pattern . "/";
$it = new RegexIterator(
    new ArrayIterator([$subject]),
    $pattern,
    RegexIterator::SPLIT,
    0,
    PREG_SPLIT_NO_EMPTY | PREG_SPLIT_DELIM_CAPTURE
);
foreach ($it as $pieces) {
    echo count($pieces);
    echo ":";
    echo $pieces[104];
}
"#,
    );
    assert_eq!(out, "105:z");
}

/// Verifies RegexIterator flags, setters, getters, and key replacement behavior.
#[test]
fn test_regex_iterator_flags_and_accessors() {
    let out = compile_and_run(
        r#"<?php
$it = new RegexIterator(
    new ArrayIterator(["a1" => "valueA", "b2" => "valueB", "skip" => "valueC"]),
    "/([a-z])([0-9])/",
    RegexIterator::GET_MATCH,
    RegexIterator::USE_KEY
);
echo $it->getRegex();
echo "|";
echo $it->getMode();
echo "|";
echo $it->getFlags();
echo "|";
echo $it->getPregFlags();
echo ";";
foreach ($it as $key => $value) {
    echo "g:";
    echo $key;
    echo "=";
    echo $value[0];
    echo "/";
    echo $value[1];
    echo "/";
    echo $value[2];
    echo ";";
}

$it->setMode(RegexIterator::REPLACE);
$it->replacement = 'K$1';
foreach ($it as $key => $value) {
    echo "r:";
    echo $key;
    echo "=";
    echo $value;
    echo ";";
}

$it->setFlags(RegexIterator::USE_KEY | RegexIterator::INVERT_MATCH);
$it->setPregFlags(7);
$it->setMode(RegexIterator::MATCH);
echo "p:";
echo $it->getPregFlags();
echo ";";
foreach ($it as $key => $value) {
    echo "i:";
    echo $key;
    echo "=";
    echo $value;
    echo ";";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "/([a-z])([0-9])/|1|1|0;",
            "g:a1=a1/a/1;",
            "g:b2=b2/b/2;",
            "r:Ka=valueA;",
            "r:Kb=valueB;",
            "p:7;",
            "i:skip=valueC;",
        )
    );
}

/// Verifies RecursiveRegexIterator filters recursive children with preserved state.
#[test]
fn test_recursive_regex_iterator_preserves_state_for_children() {
    let out = compile_and_run(
        r#"<?php
$filter = new RecursiveRegexIterator(
    new RecursiveArrayIterator([
        "keep" => ["apple" => 1, "skip" => 2],
        "drop" => ["banana" => 3],
        "tail" => "apple",
    ]),
    "/keep|apple|tail/",
    RecursiveRegexIterator::MATCH,
    RecursiveRegexIterator::USE_KEY
);
$tree = new RecursiveIteratorIterator($filter, RecursiveIteratorIterator::SELF_FIRST);
foreach ($tree as $key => $value) {
    echo $tree->getDepth();
    echo ":";
    echo $key;
    echo "=";
    echo gettype($value) === "array" ? "array" : $value;
    echo ";";
}
"#,
    );
    assert_eq!(out, "0:keep=array;1:apple=1;0:tail=apple;");
}
