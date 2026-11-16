//! Purpose:
//! Check ownership across guarded interface property writes and removals.
//!
//! Called from:
//! - The runtime GC codegen suite.
//!
//! Key details:
//! - Fresh payloads, throwing RHS evaluation and removed slots must leave a clean heap.

use crate::support::{compile_and_run_with_heap_debug, without_ir_opt};

/// Guarded writes and unset retire fresh strings, arrays and objects on success and exceptions.
#[test]
fn test_guarded_interface_mutation_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
interface Root {}
final class Token {
    public static int $freed = 0;
    public function __destruct() { self::$freed++; }
}
final class Label implements Root {
    public string $text = "seed";
    public array $items = [];
    public ?Token $token = null;
}
final class Plain implements Root {}
function failValue(): string { echo "R"; throw new Error("stop"); }
function mutate(Root $root, string $name): void {
    if (!($root instanceof Label)) { echo "P"; return; }
    $root->text = str_repeat("x", 64);
    $root->items = [str_repeat("y", 64)];
    $root->token = new Token();
    $root->$name = str_repeat("z", 64);
    try { $root->text = failValue(); } catch (Error $error) { echo "E"; }
    echo strlen($root->text), ":", count($root->items), ":";
    unset($root->$name, $root->items, $root->token);
    echo isset($root->text) ? "bad" : "U";
}
for ($i = 0; $i < 4; $i++) { mutate(new Label(), "text"); }
mutate(new Plain(), "text");
echo ":", Token::$freed;
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "RE64:1:URE64:1:URE64:1:URE64:1:UP:4", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The same guarded ownership paths remain balanced without EIR optimization.
#[test]
fn test_guarded_interface_mutation_owners_without_ir_opt() {
    without_ir_opt(test_guarded_interface_mutation_owners);
}
