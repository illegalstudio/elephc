//! Purpose:
//! Checks native argument adapter ownership when post-eval methods use dynamic dispatch.
//!
//! Called from:
//! - Runtime GC integration tests and the focused fileinfo CI gates.
//!
//! Key details:
//! - Optional parameters force opaque receivers through the eval method bridge.
//! - Both successful calls and exceptions retire adapters before scope teardown.

use crate::support::compile_and_run_with_heap_debug;

/// Opaque post-eval method dispatch retires adapter boxes before normal or throwing status checks.
#[test]
fn test_fileinfo_eval_method_adapter_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class EvalMethodAdapterOwners {
    public function value(string $text, string $suffix = "!"): string { return $text . $suffix; }
    public function fail(string $text, int $unused = 0): string { throw new Error($text); }
    public function __destruct() { echo "destroyed|"; }
}
function evalThisOwnerArgument(int $flag): int {
    if ($flag > 0) { throw new Error("argument"); }
    return 0;
}
class EvalThisArgumentOwner {
    public function target(int $argument): void {}
    public function guarded(int $flag, string $source): void {
        $this->target(evalThisOwnerArgument($flag));
        eval($source);
    }
    public function __destruct() { echo "guarded-destroyed|"; }
}
$object = new EvalMethodAdapterOwners();
$code = $argc > 0 ? '$sentinel = 1;' : '';
eval($code);
for ($i = 0; $i < 3; $i++) {
    echo $object->value("owned"), "|";
    try { $object->fail("failed"); } catch (Error $error) { echo $error->getMessage(), "|"; }
}
try { (new EvalThisArgumentOwner())->guarded($argc, ""); }
catch (Error $error) { echo $error->getMessage(), "|"; }
unset($object, $code, $sentinel, $error);
"#);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, format!("{}guarded-destroyed|argument|destroyed|", "owned!|failed|".repeat(3)), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The unoptimized backend preserves the same post-eval method adapter ownership.
#[test]
fn test_fileinfo_eval_method_adapter_owners_without_ir_opt() {
    crate::support::without_ir_opt(test_fileinfo_eval_method_adapter_owners);
}
