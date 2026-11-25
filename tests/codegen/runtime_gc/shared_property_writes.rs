//! Purpose:
//! Ownership regressions for literal property writes that adopt an owned Mixed box.
//!
//! Called from:
//! - `cargo test --test codegen_tests_runtime` through the runtime GC module.
//!
//! Key details:
//! - XML capability lowers `$mixed->name .= $text` to an owned Mixed box the store adopts.
//! - A magic `__set` only borrows that box, so the write retires it whether `__set` returns or
//!   throws.
//! - With the eval bridge linked, the eval-object probe runs before the native arms and must hand
//!   the adopted box to them intact.

use crate::support::*;

/// A magic `__set` only borrows the adopted box, so one inline write retires it on success and
/// when `__set` throws.
#[test]
fn test_inline_magic_set_retires_the_adopted_box() {
    if skip_without_xml_native("test_inline_magic_set_retires_the_adopted_box") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
class InlineQuietBlock {
    public function __get(string $name): mixed { return 'quiet-'; }
    public function __set(string $name, mixed $value): void { echo 'kept:', $value, "\n"; }
}
class InlineThrowingBlock {
    public function __get(string $name): mixed { return 'magic-'; }
    public function __set(string $name, mixed $value): void { throw new RuntimeException('set:' . $value); }
}
class InlineFrame { public mixed $block = null; }
function inline_frame(mixed $block): mixed { $frame = new InlineFrame(); $frame->block = $block; return $frame; }
function inline_append(mixed $frame, string $html): void { $frame->block->innerHTML .= $html; }
function inline_run(): void {
    $xml = simplexml_load_string("<root><item>xml</item></root>");
    echo (string) $xml->item, "\n";
    for ($round = 0; $round < 3; $round++) {
        inline_append(inline_frame(new InlineQuietBlock()), "q$round");
        try { inline_append(inline_frame(new InlineThrowingBlock()), "x$round"); } catch (RuntimeException $error) { echo $error->getMessage(), "\n"; }
    }
}
inline_run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "xml\nkept:quiet-q0\nset:magic-x0\nkept:quiet-q1\nset:magic-x1\nkept:quiet-q2\nset:magic-x2\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The inline magic write keeps the same balance without EIR optimization.
#[test]
fn test_inline_magic_set_retires_the_adopted_box_no_ir_opt() {
    without_ir_opt(test_inline_magic_set_retires_the_adopted_box);
}

/// An inline adopted append survives the eval-object probe that a linked eval bridge adds.
#[test]
fn test_eval_linked_mixed_append_keeps_the_adopted_box() {
    if skip_without_xml_native("test_eval_linked_mixed_append_keeps_the_adopted_box") { return; }
    let out = compile_and_run_with_heap_debug(r#"<?php
class ProbeBlock { public mixed $innerHTML = ''; }
class ProbeFrame { public mixed $block = null; }
function probe_frame(mixed $block): mixed { $frame = new ProbeFrame(); $frame->block = $block; return $frame; }
function probe_append(mixed $frame, string $html): void { $frame->block->innerHTML .= $html; }
function probe_run(): void {
    $xml = simplexml_load_string("<root><item>xml</item></root>");
    echo (string) $xml->item, "\n";
    $frame = probe_frame(new ProbeBlock());
    for ($round = 0; $round < 4; $round++) {
        probe_append($frame, "<p>$round</p>");
        $other = str_repeat('z', 24 + $round);
    }
    echo $frame->block->innerHTML, "\n";
}
probe_run();
$code = $argc > 5 ? 'return "eval-a";' : 'return "eval-b";';
echo eval($code), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "xml\n<p>0</p><p>1</p><p>2</p><p>3</p>\neval-b\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The eval-object probe keeps the adopted box without EIR optimization.
#[test]
fn test_eval_linked_mixed_append_keeps_the_adopted_box_no_ir_opt() {
    without_ir_opt(test_eval_linked_mixed_append_keeps_the_adopted_box);
}
