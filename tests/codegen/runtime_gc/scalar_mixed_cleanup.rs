//! Purpose:
//! Checks scalar Mixed release alongside aliases and potentially throwing cleanup.
//!
//! Called from:
//! - Focused `codegen_tests` runtime ownership regression runs.
//!
//! Key details:
//! - Both optimizer modes must match PHP and finish with balanced heap ownership.
//! - Scalar cleanup must preserve pending exceptions and the generic destructor path.

use crate::support::*;

/// Runs one cleanup fixture with explicit optimizer, heap and optional PHP oracles.
fn check_scalar_cleanup(source: &str, expected: &str) {
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let dir = make_cli_test_dir("elephc_scalar_mixed_cleanup");
        let php = dir.join("main.php");
        fs::write(&php, source).expect("write scalar cleanup fixture");
        let compile = elephc_cli_command(&dir).args([mode, "--heap-debug"])
            .arg(&php).output().expect("compile scalar cleanup fixture");
        if !compile.status.success() {
            fs::remove_dir_all(&dir).expect("remove failed scalar cleanup fixture");
            panic!("{mode}: {}", String::from_utf8_lossy(&compile.stderr));
        }
        let output = run_binary(&dir.join("main"), &dir);
        let oracle = std::env::var_os("ELEPHC_PHP_CHECK").map(|_| {
            Command::new("php").arg("-n").arg(&php).output()
        });
        fs::remove_dir_all(&dir).expect("remove scalar cleanup fixture");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{mode}: {stderr}");
        assert_eq!(output.stdout, expected.as_bytes(), "{mode}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {stderr}");
        if let Some(php) = oracle {
            let php = php.expect("run PHP oracle");
            assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
            assert_eq!(output.stdout, php.stdout, "PHP mismatch with {mode}");
        }
    }
}

/// All scalar tags and shared containers retain their types until the final array owner releases them.
#[test]
fn test_scalar_mixed_cleanup_preserves_aliases_and_runtime_types() {
    check_scalar_cleanup(r#"<?php
function scalarOwner(mixed $value): mixed { return $value; }
$values = [scalarOwner(10), scalarOwner(1.5), scalarOwner(true), scalarOwner(null),
    scalarOwner(false), scalarOwner(-0.0), scalarOwner(PHP_INT_MAX), scalarOwner(PHP_INT_MIN)];
$alias = $values;
unset($values);
echo gettype($alias[0]), ':', gettype($alias[1]), ':', gettype($alias[2]), ':', gettype($alias[3]);
unset($alias);
"#, "integer:double:boolean:NULL");
}

/// An active outer exception survives scalar cleanup in a finally block.
#[test]
fn test_scalar_mixed_cleanup_preserves_outer_exception() {
    check_scalar_cleanup(r#"<?php
function scalarOwner(mixed $value): mixed { return $value; }
try {
    try { throw new RuntimeException('outer'); }
    finally {
        $values = [scalarOwner(1), scalarOwner(2.5), scalarOwner(false), scalarOwner(null)];
        unset($values);
    }
} catch (RuntimeException $error) { echo $error->getMessage(); }
gc_collect_cycles();
"#, "outer");
}

/// Scalar siblings cannot erase the exception accumulated by an earlier throwing destructor.
#[test]
fn test_scalar_mixed_cleanup_preserves_throwing_sibling() {
    check_scalar_cleanup(r#"<?php
function scalarOwner(mixed $value): mixed { return $value; }
class ScalarCleanupBomb {
    public function __destruct() { echo 'drop:'; throw new RuntimeException('cleanup'); }
}
$values = [scalarOwner(new ScalarCleanupBomb()), scalarOwner(1), scalarOwner(2.5),
    scalarOwner(false), scalarOwner(null)];
try { unset($values); }
catch (RuntimeException $error) { echo $error->getMessage(); }
gc_collect_cycles();
"#, "drop:cleanup");
}

/// Strings, nested containers and ordinary objects keep their established release semantics.
#[test]
fn test_scalar_mixed_cleanup_keeps_non_scalar_children_protected() {
    check_scalar_cleanup(r#"<?php
function scalarOwner(mixed $value): mixed { return $value; }
class ScalarCleanupLeaf {
    public static int $freed = 0;
    public function __destruct() { self::$freed++; }
}
$values = [scalarOwner('text'), scalarOwner([1, 2]), scalarOwner(new ScalarCleanupLeaf()),
    scalarOwner(3), scalarOwner(null)];
$alias = $values;
unset($values);
echo ScalarCleanupLeaf::$freed, ':';
unset($alias);
echo ScalarCleanupLeaf::$freed;
gc_collect_cycles();
"#, "0:1");
}

/// Scalar release stays safe inside a destructor while the collector owns a cyclic snapshot.
#[test]
fn test_scalar_mixed_cleanup_during_cycle_destructor() {
    check_scalar_cleanup(r#"<?php
function scalarOwner(mixed $value): mixed { return $value; }
class ScalarCleanupCycle {
    public mixed $next = null;
    public static int $freed = 0;
    public function __destruct() {
        $values = [scalarOwner(1), scalarOwner(2.5), scalarOwner(true), scalarOwner(null)];
        unset($values);
        self::$freed++;
    }
}
$value = new ScalarCleanupCycle();
$value->next = $value;
unset($value);
gc_collect_cycles();
echo ScalarCleanupCycle::$freed;
"#, "1");
}

/// Empty typed array arguments, aliases and reference cells retain their ordinary owner counts.
#[test]
fn test_scalar_mixed_cleanup_empty_array_aliases_and_arguments() {
    check_scalar_cleanup(r#"<?php
function emptyCount(array $value): int { return count($value); }
$empty = [];
$copy = $empty;
echo emptyCount($empty), ':';
unset($empty);
echo count($copy), ':';
$reference =& $copy;
unset($copy);
echo count($reference);
unset($reference);
"#, "0:0:0");
}

/// Empty release preserves an outer throw while nonempty arrays still catch child exceptions.
#[test]
fn test_scalar_mixed_cleanup_empty_array_pending_and_nonempty_fallback() {
    check_scalar_cleanup(r#"<?php
class EmptyCleanupBomb {
    public function __destruct() { echo 'drop:'; throw new RuntimeException('cleanup'); }
}
try {
    try { throw new RuntimeException('outer'); }
    finally { $empty = []; $alias = $empty; unset($empty); unset($alias); }
} catch (RuntimeException $outerError) { echo $outerError->getMessage(), '|'; }
$values = [new EmptyCleanupBomb()];
try { unset($values); }
catch (RuntimeException $cleanupError) { echo $cleanupError->getMessage(); }
gc_collect_cycles();
"#, "outer|drop:cleanup");
}

/// Empty arrays created during a cycle destructor preserve the collector's suppression state.
#[test]
fn test_scalar_mixed_cleanup_empty_array_during_cycle_destructor() {
    check_scalar_cleanup(r#"<?php
class EmptyCleanupCycle {
    public mixed $next = null;
    public static int $freed = 0;
    public function __destruct() {
        $empty = [];
        $alias = $empty;
        unset($empty);
        unset($alias);
        self::$freed++;
    }
}
$value = new EmptyCleanupCycle();
$value->next = $value;
unset($value);
gc_collect_cycles();
echo EmptyCleanupCycle::$freed;
"#, "1");
}
