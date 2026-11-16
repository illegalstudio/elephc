//! Purpose:
//! Verifies native coroutine and SPL owner edges under immediate cycle collection.
//!
//! Called from:
//! - Focused CLI runtime GC regression tests.
//!
//! Key details:
//! - Shared PHP fixtures exercise unstarted, suspended and terminated Generators.
//! - Heap debug catches missing edges and duplicate teardown of selected children.

use crate::support::*;

const OWNERS: &str = include_str!("../../fixtures/native_object_cycles.php");

/// All native owner shapes release detached cycles and their payloads exactly once.
#[test]
fn test_gc_native_object_cycles_release_generator_and_spl_owners() {
    let source = format!(r#"{OWNERS}
function makeNativeCycle(string $kind): WeakReference {{
    $owner = new NativeCycleOwner($kind);
    return WeakReference::create($owner);
}}
$weak = [];
foreach (['/fresh', '/started', '/returned', '/closure', '/closure-started', '/fixed', '/stack', '/queue', '/list'] as $kind) {{
    $weak[] = makeNativeCycle($kind);
}}
gc_collect_cycles();
echo NativeCycleOwner::$destroyed, ':';
foreach ($weak as $observer) {{ echo $observer->get() === null ? 'dead|' : 'alive|'; }}
unset($observer, $weak);
gc_collect_cycles();
"#);
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, format!("9:{}", "dead|".repeat(9)), "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// External roots, resurrection and throwing native cycles preserve balanced cleanup.
#[test]
fn test_gc_native_object_cycles_preserve_roots_and_recount_after_destructors() {
    let source = format!(r#"{OWNERS}
$owner = new NativeCycleOwner('/started');
$weak = WeakReference::create($owner);
gc_collect_cycles();
echo strlen($owner->payload), ':', NativeCycleOwner::$destroyed, '|';
unset($owner);
gc_collect_cycles();
echo $weak->get() === null ? 'dead|' : 'alive|';
unset($weak);
$owner = new NativeCycleOwner('/resurrect');
$weak = WeakReference::create($owner);
unset($owner);
gc_collect_cycles();
echo NativeCycleOwner::$destroyed, ':', strlen(NativeCycleOwner::$saved->payload), '|';
NativeCycleOwner::$saved = null;
gc_collect_cycles();
echo $weak->get() === null ? 'dead|' : 'alive|';
unset($weak);
$owner = new NativeCycleOwner('/throw');
$weak = WeakReference::create($owner);
try {{ unset($owner); gc_collect_cycles(); }} catch (RuntimeException $error) {{ echo $error->getMessage(), '|'; }}
echo NativeCycleOwner::$destroyed, ':', $weak->get() === null ? 'dead' : 'alive';
unset($weak, $error);
"#);
    let out = compile_and_run_with_heap_debug(&source);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "4096:0|dead|2:4096|dead|native cycle cleanup|3:dead", "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
