use crate::support::*;

#[test]
fn test_return_type_from_foreach() {
    let out = compile_and_run(
        r#"<?php
function find($arr, $target) {
    foreach ($arr as $v) {
        if ($v === $target) { return "found"; }
    }
    return "not found";
}
echo find([1, 2, 3], 2);
"#,
    );
    assert_eq!(out, "found");
}

#[test]
fn test_return_type_mixed_branches() {
    let out = compile_and_run(
        r#"<?php
function describe($n) {
    if ($n > 0) { return "positive"; }
    return 0;
}
$r = describe(5);
echo $r;
"#,
    );
    assert_eq!(out, "positive");
}

#[test]
fn test_return_type_switch_foreach() {
    let out = compile_and_run(
        r#"<?php
function classify($items) {
    foreach ($items as $item) {
        switch ($item) {
            case 0: return "zero";
            default: return "nonzero";
        }
    }
    return "empty";
}
echo classify([0]);
"#,
    );
    assert_eq!(out, "zero");
}

#[test]
fn test_return_string_from_else() {
    let out = compile_and_run(
        r#"<?php
function check($x) {
    if ($x > 10) {
        return "big";
    } else {
        return "small";
    }
}
echo check(5) . "|" . check(15);
"#,
    );
    assert_eq!(out, "small|big");
}

#[test]
fn test_array_return_type_survives_indexing() {
    let out = compile_and_run(
        r#"<?php
function getColor(): array {
    return [255, 128, 0];
}

$color = getColor();
echo $color[0] . "," . $color[1] . "," . $color[2];
"#,
    );
    assert_eq!(out, "255,128,0");
}

#[test]
fn test_string_array_element_keeps_string_type() {
    let out = compile_and_run(
        r#"<?php
function paint(string $name): string {
    return $name;
}

function pickSecond(array $names): string {
    return paint($names[1]);
}

echo pickSecond(["foo", "bar"]);
"#,
    );
    assert_eq!(out, "bar");
}

#[test]
fn test_string_array_return_type_keeps_string_elements() {
    let out = compile_and_run(
        r#"<?php
function paint(string $name): string {
    return $name;
}

function loadNames(): array {
    return ["foo", "bar"];
}

$names = loadNames();
echo paint($names[1]);
"#,
    );
    assert_eq!(out, "bar");
}
