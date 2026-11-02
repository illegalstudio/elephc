//! Purpose:
//! Regression tests for the borrowed global snapshot the eval bridge publishes at a native/eval
//! boundary.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_global_sync` through Rust's test harness.
//!
//! Key details:
//! - The native-to-eval flush stores a BORROWED cell in the eval global scope. A native method
//!   the fragment calls can replace that native cell and release it, so the eval-to-native
//!   reload must skip entries the fragment never wrote. Replaying one wrote freed memory back
//!   over the live global; Symfony's cold `cache:warmup` died on exactly that: `$_ENV` became a
//!   recycled `ServiceLocator` cell, and `EnvVarProcessor::getEnv()` indexed it as an array.
//! - The assembly assertion pins the half that is observable in every eval program: the
//!   native-to-eval flush marks the entries it just published clean. Without it the reload has
//!   no way to tell an untouched borrowed snapshot from one the fragment wrote.

use crate::support::*;

/// Returns the emitted helper whose label contains `label_fragment`, up to its `ret`.
fn sync_helper_body<'a>(asm: &'a str, label_fragment: &str) -> &'a str {
    let mut offset = 0;
    for line in asm.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.ends_with(':') && trimmed.contains(label_fragment) {
            let rest = &asm[offset..];
            let end = rest
                .find("\n    ret\n")
                .map_or(rest.len().min(4096), |index| index + 9);
            return &rest[..end];
        }
        offset += line.len();
    }
    panic!("missing eval sync helper {label_fragment}");
}

/// Verifies a native call that replaces a superglobal survives the eval boundary.
#[test]
fn test_eval_native_superglobal_replacement_survives_scope_reload() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$_ENV = ['tag' => 'before'];
class GlobalSyncReplacer {
    public function run(): void {
        $_ENV = ['tag' => 'after'];
    }
}
function global_sync_probe(): string {
    return $_ENV['tag'];
}
eval('$replacer = new GlobalSyncReplacer(); $replacer->run();');
echo global_sync_probe(), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "after\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "{}",
        out.stderr
    );
}

/// Verifies the native-to-eval flush clears the dirty bits it just published.
#[test]
fn test_eval_global_flush_marks_snapshot_clean() {
    let (_out, asm) = compile_and_run_with_heap_debug_and_asm(
        r#"<?php
$_ENV = ['tag' => 'before'];
class GlobalSyncFlushProbe {
    public function run(): void { $_ENV = ['tag' => 'after']; }
}
eval('$p = new GlobalSyncFlushProbe(); $p->run();');
echo $_ENV['tag'], "\n";
"#,
    );
    let flush = sync_helper_body(&asm, "eval_globals_from_native");
    assert!(
        flush.contains("__elephc_eval_scope_clear_dirty"),
        "the native-to-eval flush must mark its borrowed snapshot clean:\n{flush}"
    );
}
