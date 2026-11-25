//! Purpose:
//! Ownership regressions for literal property writes that adopt an owned Mixed box, inline and
//! shared through one helper body.
//!
//! Called from:
//! - `cargo test --test codegen_tests_runtime` through the runtime GC module.
//!
//! Key details:
//! - XML capability lowers `$mixed->name .= $text` to an owned Mixed box the store adopts, so
//!   repeated sites share an adopting helper body.
//! - A magic `__set` only borrows that box, so the write retires it whether `__set` returns or
//!   throws. Writes that store nothing (refusal, non-object receiver, dispatch miss) retire it
//!   too.
//! - With the eval bridge linked, the eval-object probe runs before the native arms and must hand
//!   the adopted box to them intact.

use crate::support::*;

/// Adopted boxes stay balanced through the shared helper on success and when `__set` throws.
#[test]
fn test_shared_literal_write_adopts_owned_boxes_through_throws() {
    if skip_without_xml_native("test_shared_literal_write_adopts_owned_boxes_through_throws") { return; }
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(r#"<?php
class AdoptBlock { public mixed $innerHTML = ''; }
class AdoptMagicBlock {
    public function __get(string $name): mixed { return 'magic-'; }
    public function __set(string $name, mixed $value): void { throw new RuntimeException('set:' . $value); }
}
class AdoptQuietBlock {
    public function __get(string $name): mixed { return 'quiet-'; }
    public function __set(string $name, mixed $value): void { echo 'kept:', $value, "\n"; }
}
class AdoptFrame { public mixed $block = null; }
class AdoptParser {
    public function one(mixed $frame, string $html): void { $frame->block->innerHTML .= $html; }
    public function two(mixed $frame, string $html): void { $frame->block->innerHTML .= $html; }
}
function adopt_frame(mixed $block): mixed { $frame = new AdoptFrame(); $frame->block = $block; return $frame; }
function adopt_run(): void {
    $xml = simplexml_load_string("<root><item>xml</item></root>");
    echo (string) $xml->item, "\n";
    $parser = new AdoptParser();
    $plain = adopt_frame(new AdoptBlock());
    for ($round = 0; $round < 3; $round++) {
        $parser->one($plain, "a$round|");
        $parser->two($plain, "b$round|");
        $parser->one(adopt_frame(new AdoptQuietBlock()), "q$round");
        try { $parser->one(adopt_frame(new AdoptMagicBlock()), "x$round"); } catch (RuntimeException $error) { echo $error->getMessage(), "\n"; }
        try { $parser->two(adopt_frame(new AdoptMagicBlock()), "y$round"); } catch (RuntimeException $error) { echo $error->getMessage(), "\n"; }
    }
    echo $plain->block->innerHTML, "\n";
    unset($plain, $parser);
}
adopt_run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "xml\nkept:quiet-q0\nset:magic-x0\nset:magic-y0\nkept:quiet-q1\nset:magic-x1\nset:magic-y1\n\
         kept:quiet-q2\nset:magic-x2\nset:magic-y2\na0|b0|a1|b1|a2|b2|\n"
    );
    assert!(assembly.contains("adopt=true"), "the repeated adopted writes must share an adopting body");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The adopting body keeps the same balance without EIR optimization.
#[test]
fn test_shared_literal_write_adopts_owned_boxes_through_throws_no_ir_opt() {
    without_ir_opt(test_shared_literal_write_adopts_owned_boxes_through_throws);
}

/// The adopting body publishes the box before an overwritten value's destructor throws.
#[test]
fn test_shared_literal_write_adopts_through_a_throwing_destructor() {
    if skip_without_xml_native("test_shared_literal_write_adopts_through_a_throwing_destructor") { return; }
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(r#"<?php
class AdoptBlock { public mixed $innerHTML = ''; }
class AdoptBomb {
    public function __construct(private string $label) {}
    public function __toString(): string { return $this->label; }
    public function __destruct() { throw new RuntimeException('destruct:' . $this->label); }
}
class AdoptFrame { public mixed $block = null; }
class AdoptParser {
    public function one(mixed $frame, string $html): void { $frame->block->innerHTML .= $html; }
    public function two(mixed $frame, string $html): void { $frame->block->innerHTML .= $html; }
}
function adopt_frame(mixed $block): mixed { $frame = new AdoptFrame(); $frame->block = $block; return $frame; }
function adopt_run(): void {
    $xml = simplexml_load_string("<root><item>xml</item></root>");
    echo (string) $xml->item, "\n";
    $parser = new AdoptParser();
    $plain = adopt_frame(new AdoptBlock());
    for ($round = 0; $round < 3; $round++) {
        $plain->block->innerHTML = new AdoptBomb("b$round");
        try { $parser->one($plain, "|x$round"); } catch (RuntimeException $error) { echo $error->getMessage(), "\n"; }
        echo $plain->block->innerHTML, "\n";
        try { $parser->two($plain, "|y$round"); } catch (RuntimeException $error) { echo $error->getMessage(), "\n"; }
        echo $plain->block->innerHTML, "\n";
    }
    unset($plain, $parser);
}
adopt_run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "xml\ndestruct:b0\nb0|x0\nb0|x0|y0\ndestruct:b1\nb1|x1\nb1|x1|y1\ndestruct:b2\nb2|x2\nb2|x2|y2\n");
    assert!(assembly.contains("adopt=true"), "the repeated adopted writes must share an adopting body");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// PHP source whose adopted boxes reach writes that store nothing: a refused private property,
/// a scalar receiver and a Closure receiver that matches no dispatch arm.
const UNSTORED_WRITES: &str = r#"<?php
class UnstoredPrivate { private mixed $secret = 'p'; public function __get(string $name): mixed { return 'g'; } }
class UnstoredFrame { public mixed $block = null; }
function unstored_frame(mixed $block): mixed { $frame = new UnstoredFrame(); $frame->block = $block; return $frame; }
function secret_one(mixed $frame, string $html): void { $frame->block->secret .= $html; }
function secret_two(mixed $frame, string $html): void { $frame->block->secret .= $html; }
function label_one(mixed $frame, string $html): void { $frame->block->label .= $html; }
function label_two(mixed $frame, string $html): void { $frame->block->label .= $html; }
function unstored_run(): void {
    $xml = simplexml_load_string("<root><item>xml</item></root>");
    echo (string) $xml->item, "\n";
    for ($round = 0; $round < 3; $round++) {
        $private = unstored_frame(new UnstoredPrivate());
        try { secret_one($private, "a$round"); } catch (Error $error) { echo $error->getMessage(), "\n"; }
        try { secret_two($private, "b$round"); } catch (Error $error) { echo $error->getMessage(), "\n"; }
        $scalar = unstored_frame(5);
        try { label_one($scalar, "c$round"); } catch (Error $error) {}
        try { label_two($scalar, "d$round"); } catch (Error $error) {}
        echo json_encode($scalar->block), "\n";
        $closure = unstored_frame(function () use ($round) { return "closure$round"; });
        try { label_one($closure, "e$round"); } catch (Error $error) {}
        try { label_two($closure, "f$round"); } catch (Error $error) {}
        echo ($closure->block)(), "\n";
    }
}
unstored_run();
"#;

/// Stdout of `UNSTORED_WRITES`, which PHP 8.6 prints the same way.
const UNSTORED_STDOUT: &str = "xml\nCannot access private property UnstoredPrivate::$secret\nCannot access private property UnstoredPrivate::$secret\n5\nclosure0\nCannot access private property UnstoredPrivate::$secret\nCannot access private property UnstoredPrivate::$secret\n5\nclosure1\nCannot access private property UnstoredPrivate::$secret\nCannot access private property UnstoredPrivate::$secret\n5\nclosure2\n";

/// An adopted box is retired when the shared write stores nothing: refusal, non-object receiver
/// and dispatch miss.
#[test]
fn test_adopted_box_is_retired_when_the_write_stores_nothing() {
    if skip_without_xml_native("test_adopted_box_is_retired_when_the_write_stores_nothing") { return; }
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(UNSTORED_WRITES);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, UNSTORED_STDOUT);
    assert!(assembly.contains("adopt=true"), "the repeated adopted writes must share an adopting body");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The non-storing writes keep the same balance without EIR optimization.
#[test]
fn test_adopted_box_is_retired_when_the_write_stores_nothing_no_ir_opt() {
    without_ir_opt(test_adopted_box_is_retired_when_the_write_stores_nothing);
}

/// With the eval bridge linked, the probe hands the box on and the dispatch miss retires it.
#[test]
fn test_eval_linked_adopted_box_is_retired_when_the_write_stores_nothing() {
    if skip_without_xml_native("test_eval_linked_adopted_box_is_retired_when_the_write_stores_nothing") { return; }
    let source = format!("{UNSTORED_WRITES}$code = $argc > 5 ? 'return 1;' : 'return 2;';\necho eval($code), \"\\n\";\n");
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, format!("{UNSTORED_STDOUT}2\n"));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

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
