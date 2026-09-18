//! Purpose:
//! Verifies scope writeback and pending exception ownership at the native/eval boundary.
//!
//! Called from:
//! - The runtime GC codegen integration suite.
//!
//! Key details:
//! - Opaque eval prevents literal lowering from hiding the bridge status path.
//! - Replacement cleanup must finish publishing scope changes before an exception escapes.

use crate::support::*;

/// Native catches observe eval writes to locals, global aliases and caller references before a throw.
#[test]
fn test_core_eval_throw_publishes_scope_changes_before_native_catch() {
    let source = r#"<?php
function catchEvalScopeChanges(string $source, mixed &$reference): void {
    global $marker;
    $local = "before";
    $gone = "present";
    try { eval($source); }
    catch (Throwable $error) {
        echo $local, ":", $reference, ":", $marker, ":", isset($gone) ? "set" : "unset", ":", $error->getMessage(), "|";
    }
}
$marker = "old";
$reference = $argc > 0 ? "old" : 1;
$source = '$local = "after"; $reference = 42; $marker = "changed";
unset($gone); throw new Exception("stop"); // ' . $argc;
catchEvalScopeChanges($source, $reference);
echo $reference, ":", $marker;
"#;
    assert_eq!(compile_and_run(source), "after:42:changed:unset:stop|42:changed");
}
