//! Purpose:
//! Exercises exact and saturated incoming-edge counts in native cycle collection.
//!
//! Called from:
//! - Focused `codegen_tests` runtime GC regressions on executable targets.
//!
//! Key details:
//! - PHP output and balanced ownership must agree with IR optimization on and off.
//! - High fan-in crosses every boundary around the x86 counter saturation sentinel.

use crate::support::*;

#[path = "gc_incoming_cache/dynamic_hash.rs"]
mod dynamic_hash;

/// Compiles one collector fixture in both optimizer modes with heap and optional PHP checks.
fn check_gc_cache(source: &str, expected: &str) {
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let dir = make_cli_test_dir("elephc_gc_incoming_cache");
        let php = dir.join("main.php");
        fs::write(&php, source).expect("write incoming-edge fixture");
        let compile = elephc_cli_command(&dir).args([mode, "--heap-debug"])
            .arg(&php).output().expect("compile incoming-edge fixture");
        if !compile.status.success() {
            fs::remove_dir_all(&dir).expect("remove failed collector fixture");
            panic!("{mode}: {}", String::from_utf8_lossy(&compile.stderr));
        }
        let output = run_binary(&dir.join("main"), &dir);
        let oracle = std::env::var_os("ELEPHC_PHP_CHECK").map(|_| {
            Command::new("php").arg("-n").arg(&php).output()
        });
        fs::remove_dir_all(&dir).expect("remove incoming-edge fixture");
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

/// Array, hash, object, and boxed-property edges keep their live descendants reachable.
#[test]
fn test_gc_incoming_cache_rooted_container_graph() {
    check_gc_cache(r#"<?php
class CacheLeaf { public int $value = 7; public mixed $child = null; }
$first = new CacheLeaf();
$second = new CacheLeaf();
$first->child = $second;
$array = [$first, $second];
$hash = ['first' => $first, 'second' => $second];
gc_collect_cycles();
$second->value = 9;
echo $hash['first']->child->value, ':', count($array), '|';
gc_collect_cycles();
echo $array[0]->child->value;
"#, "9:2|9");
}

/// A collector pass reclaims a dead cycle while a live cycle retains every owned child.
#[test]
fn test_gc_incoming_cache_live_and_dead_cycles() {
    check_gc_cache(r#"<?php
class CacheCycle {
    public mixed $next = null;
    public int $value = 7;
    public static int $freed = 0;
    public function __destruct() { self::$freed++; }
}
$live = new CacheCycle();
$live->next = $live;
$dead = new CacheCycle();
$dead->next = $dead;
unset($dead);
gc_collect_cycles();
echo CacheCycle::$freed, ':', $live->next->value, '|';
$live->next = null;
unset($live);
gc_collect_cycles();
echo CacheCycle::$freed;
"#, "1:7|2");
}

/// A managed reference cell is counted as an owned graph node rather than shortcut to its payload.
#[test]
fn test_gc_incoming_cache_reference_cell_edges() {
    check_gc_cache(r#"<?php
class CacheRefLeaf { public int $value = 7; }
$values = [new CacheRefLeaf(), new CacheRefLeaf()];
foreach ($values as &$value) {}
gc_collect_cycles();
$value->value = 9;
echo $values[0]->value, ':', $values[1]->value;
unset($value);
unset($values);
"#, "7:9");
}

/// Checks an incoming count near saturation with the original local root removed.
fn check_gc_cache_fan_in(count: usize) {
    let source = format!(r#"<?php
class CacheSharedLeaf {{ public int $value = 7; }}
$value = new CacheSharedLeaf();
$n = {count} + $argc - 1;
$aliases = [];
for ($i = 0; $i < $n; $i++) {{ $aliases[] = $value; }}
unset($value);
gc_collect_cycles();
echo count($aliases), ':', $aliases[$n - 1]->value;
unset($aliases);
"#);
    check_gc_cache(&source, &format!("{count}:7"));
}

/// A value immediately below saturation uses its exact cached incoming count.
#[test]
fn test_gc_incoming_cache_fan_in_8190() {
    check_gc_cache_fan_in(8190);
}

/// The saturation sentinel itself must select a precise fallback rather than root by approximation.
#[test]
fn test_gc_incoming_cache_fan_in_8191() {
    check_gc_cache_fan_in(8191);
}

/// Crossing the packed count capacity preserves a child whose only owners are heap edges.
#[test]
fn test_gc_incoming_cache_fan_in_8192() {
    check_gc_cache_fan_in(8192);
}

/// Larger incoming totals cannot wrap into the heap marker or misclassify a live child.
#[test]
fn test_gc_incoming_cache_fan_in_9000() {
    check_gc_cache_fan_in(9000);
}

/// Saturation cannot invent an external root and strand a high-fan-in unreachable cycle.
#[test]
fn test_gc_incoming_cache_saturated_cycle_is_reclaimed() {
    check_gc_cache(r#"<?php
class CacheFanCycle {
    public mixed $links = null;
    public static int $freed = 0;
    public function __destruct() { self::$freed++; }
}
$value = new CacheFanCycle();
$aliases = [];
for ($i = 0; $i < 9000; $i++) { $aliases[] = $value; }
$value->links = $aliases;
unset($aliases);
unset($value);
gc_collect_cycles();
echo CacheFanCycle::$freed;
"#, "1");
}
