//! Purpose:
//! Checks strip_tags allow-list normalization after integration with main.
//!
//! Called from:
//! - The string encoding codegen integration tests.
//!
//! Key details:
//! - Boxed indexed and associative arrays normalize their values before joining.
//! - Owned normalization copies are released after each allow-list rendering.
//! - Weak scalar acceptance and strict rejection share the same per-file caller profile.

use crate::support::*;

/// Weak scalar allow-lists work through direct, Mixed, and callable argument paths.
#[test]
fn strip_tags_review_weak_scalar_allow_lists() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function stripScalar(mixed $allow): string {
    return strip_tags(str_repeat('<b>z</b>', 2), $allow);
}
echo strip_tags('<b>z</b>', 1), '|', strip_tags('<b>z</b>', 1.5), '|';
echo strip_tags('<b>z</b>', false), '|', strip_tags('<b>z</b>', true), '|';
foreach ([1, 1.5, false, true] as $allow) {
    echo stripScalar($allow), '|';
    echo call_user_func('strip_tags', '<b>z</b>', $allow), '|';
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("z|z|z|z|{}", "zz|z|".repeat(4)));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Eval scalar allow-lists preserve weak coercion and retire their argument cells.
#[test]
fn strip_tags_review_eval_weak_scalar_allow_lists() {
    let out = compile_and_run_with_heap_debug(r#"<?php
eval('foreach ([1, 1.5, false, true] as $allow) {
    echo strip_tags("<b>z</b>", $allow), "|";
    echo call_user_func("strip_tags", "<b>z</b>", $allow), "|";
}');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "z|z|".repeat(4));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Direct eval calls must not keep the foreach source array alive through operand binding.
#[test]
fn strip_tags_review_eval_direct_operand_ownership() {
    let out = compile_and_run_with_heap_debug(r#"<?php
eval('foreach ([1, 1.5, false, true] as $allow) {
    echo strip_tags("<b>z</b>", $allow), "|";
}');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "z|".repeat(4));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Callable eval binding releases scalar argument cells and the foreach source array.
#[test]
fn strip_tags_review_eval_callable_operand_ownership() {
    let out = compile_and_run_with_heap_debug(r#"<?php
eval('foreach ([1, 1.5, false, true] as $allow) {
    echo call_user_func("strip_tags", "<b>z</b>", $allow), "|";
}');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "z|".repeat(4));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Opaque eval keeps the literal foreach receiver alive until its body has finished.
#[test]
fn strip_tags_review_eval_foreach_ownership_control() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$source = $argc > 0 ? 'foreach ([1, 1.5, false, true] as $allow) {
    echo strlen($allow), "|", call_user_func("strlen", $allow), "|";
}' : '';
eval($source);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "1|1|3|3|0|0|1|1|");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Opaque eval retires temporary foreach arrays after break, return, and an exception.
#[test]
fn strip_tags_review_eval_foreach_unwind_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$source = $argc > 0 ? '
foreach ([str_repeat("b", 2)] as $value) { echo $value, "|"; break; }
unset($value);
function escaped(bool $fail): string {
    foreach ([str_repeat("x", 2)] as $value) {
        if ($fail) { throw new Exception("loop"); }
        return $value;
    }
    return "";
}
echo escaped(false), "|";
try { escaped(true); } catch (Exception $error) { echo $error->getMessage(); }
unset($error);
' : '';
eval($source);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bb|xx|loop");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A borrowed foreach receiver stays pinned when the body replaces its source local.
#[test]
fn strip_tags_review_eval_foreach_source_replacement() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$source = $argc > 0 ? '
$items = ["a", "b"];
foreach ($items as $value) { $items = ["new"]; echo $value; }
echo ":", $items[0];
unset($items, $value);
' : '';
eval($source);
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "ab:new");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Eval allow-list normalization retires iterator keys, read values, and converted strings.
#[test]
fn strip_tags_review_eval_array_allow_lists() {
    let out = compile_and_run_with_heap_debug(r#"<?php
eval('for ($i = 0; $i < 4; $i++) {
    echo strip_tags("<p>A</p><b>B</b>", ["p"]), "|";
    echo call_user_func("strip_tags", "<p>A</p><b>B</b>", ["tag" => "b"]), "|";
}');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "<p>A</p>B|A<b>B</b>|".repeat(4));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Eval array entries execute Stringable callbacks and retire the returned string owners.
#[test]
fn strip_tags_review_eval_array_callbacks() {
    let out = compile_and_run_with_heap_debug(r#"<?php
eval('class AllowedTag {
    public function __toString(): string { echo "T"; return str_repeat("p", 1); }
}
echo strip_tags("<p>x</p>", [new AllowedTag()]), "|";
echo call_user_func("strip_tags", "<p>x</p>", [new AllowedTag()]);');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "T<p>x</p>|T<p>x</p>");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A failing eval allow-list callback releases partial normalization and argument leases.
#[test]
fn strip_tags_review_eval_array_callback_throw() {
    let out = compile_and_run_with_heap_debug(r#"<?php
eval('class RejectedTag {
    public function __toString(): string { throw new Exception("tag"); }
}
try { echo strip_tags("<p>x</p>", ["p", new RejectedTag()]); }
catch (Exception $error) { echo $error->getMessage(), "|"; }
unset($error);
try { echo call_user_func("strip_tags", "<p>x</p>", [new RejectedTag()]); }
catch (Exception $error) { echo $error->getMessage(); }
unset($error);');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "tag|tag");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Native and eval loops can share caller locals without retaining weak argument cells.
#[test]
fn strip_tags_review_combined_weak_scalar_allow_lists() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function stripScalar(mixed $allow): string {
    return strip_tags(str_repeat('<b>z</b>', 2), $allow);
}
echo strip_tags('<b>z</b>', 1), '|', strip_tags('<b>z</b>', 1.5), '|';
echo strip_tags('<b>z</b>', false), '|', strip_tags('<b>z</b>', true), '|';
foreach ([1, 1.5, false, true] as $allow) {
    echo stripScalar($allow), '|';
    echo call_user_func('strip_tags', '<b>z</b>', $allow), '|';
}
eval('foreach ([1, 1.5, false, true] as $allow) {
    echo strip_tags("<b>z</b>", $allow), "|";
    echo call_user_func("strip_tags", "<b>z</b>", $allow), "|";
}');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("z|z|z|z|{}{}", "zz|z|".repeat(4), "z|z|".repeat(4)));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A strict caller still rejects scalar allow-lists with PHP's precise argument kind.
#[test]
fn strip_tags_review_strict_scalar_allow_lists() {
    let out = compile_and_run_with_heap_debug(r#"<?php
declare(strict_types=1);
function strictStripScalar(mixed $allow): string {
    return strip_tags('<b>z</b>', $allow);
}
foreach ([1, 1.5, false, true] as $allow) {
    try { echo strictStripScalar($allow); }
    catch (TypeError $error) { echo $error->getMessage(), '|'; }
}
try { strip_tags('<b>z</b>', 1); echo 'bad'; }
catch (TypeError $error) { echo $error->getMessage(), '|'; }
"#);
    assert!(out.success, "{}", out.stderr);
    let expected = ["int", "float", "false", "true", "int"].map(|kind|
        format!("strip_tags(): Argument #2 ($allowed_tags) must be of type array|string|null, {kind} given|")
    ).concat();
    assert_eq!(out.stdout, expected);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Direct, callable, and eval dispatch share PHP's XML-offset and embedded-NUL boundaries.
#[test]
fn strip_tags_review_xml_and_nul_boundaries() {
    let out = compile_and_run_with_heap_debug(r#"<?php
echo '[', strip_tags('<?xml version="1.0"><b>keep</b>'), ']|';
echo strip_tags('x<?xml version="1.0"><b>keep</b>'), '|';
echo strip_tags('<b>x</b><p>y</p>', "<p>\0<b>"), '|';
echo call_user_func('strip_tags', '<b>x</b><p>y</p>', "<p>\0<b>"), '|';
eval('echo "[", strip_tags("<?xml version=\"1.0\"><b>keep</b>"), "]|";
echo strip_tags("x<?xml version=\"1.0\"><b>keep</b>"), "|";
echo strip_tags("<b>x</b><p>y</p>", "<p>\0<b>"), "|";
echo call_user_func("strip_tags", "<b>x</b><p>y</p>", "<p>\0<b>");');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[]|xkeep|x<p>y</p>|x<p>y</p>|[]|xkeep|x<p>y</p>|x<p>y</p>");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A Mixed allow-list reaches the same checked dense-array renderer as a concrete one.
#[test]
fn strip_tags_review_boxed_array_allow_lists() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function stripWith(mixed $allow): string {
    return strip_tags('<p class="x">A</p><b>B</b>', $allow);
}
for ($i = 0; $i < 8; $i++) {
    echo stripWith(['p']), '|', stripWith(['key' => 'p']), '|';
}
echo stripWith(null), '|', stripWith('<b>');
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("{}AB|A<b>B</b>", "<p class=\"x\">A</p>B|<p class=\"x\">A</p>B|".repeat(8)));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Concrete associative allow-lists use values rather than reading their hash header as an array.
#[test]
fn strip_tags_review_concrete_assoc_allow_list() {
    let out = compile_and_run_with_heap_debug(r#"<?php
for ($i = 0; $i < 8; $i++) {
    echo strip_tags('<p>A</p><b>B</b>', ['key' => 'p']), '|';
}
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "<p>A</p>B|".repeat(8));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A single empty rendered array element allows `<>`, unlike an empty array.
#[test]
fn strip_tags_review_single_empty_allow_element() {
    let out = compile_and_run_with_heap_debug(r#"<?php
function stripEmpty(mixed $allow): string { return strip_tags('<>x', $allow); }
function callEmpty(callable $callback, mixed $allow): string { return $callback('<>x', $allow); }
echo strip_tags('<>x', []), '|', strip_tags('<>x', ['']), '|';
echo strip_tags('<>x', [null]), '|', strip_tags('<>x', [false]), '|';
echo strip_tags('<>x', ['', '']), '|', strip_tags('<>x', ['tag' => '']), '|';
foreach ([[], [''], [null], [false], ['', ''], ['tag' => '']] as $allow) {
    echo strip_tags('<>x', $allow), '|', stripEmpty($allow), '|';
    echo callEmpty(strip_tags(...), $allow), '|';
}
$source = $argc > 0 ? 'foreach ([[], [""], [null], [false], ["", ""], ["tag" => ""]] as $allow) {
    echo strip_tags("<>x", $allow), "|", call_user_func("strip_tags", "<>x", $allow), "|";
}' : '';
eval($source);
"#);
    assert!(out.success, "{}", out.stderr);
    let direct = format!("x|{}", "<>x|".repeat(5));
    let native = format!("x|x|x|{}", "<>x|".repeat(15));
    let eval = format!("x|x|{}", "<>x|".repeat(10));
    assert_eq!(out.stdout, format!("{direct}{native}{eval}"));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Empty element preservation also holds without EIR optimization.
#[test]
fn strip_tags_review_single_empty_allow_element_no_ir_opt() {
    without_ir_opt(strip_tags_review_single_empty_allow_element);
}
