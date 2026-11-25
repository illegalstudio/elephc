//! Purpose:
//! Regressions for lazy magic getters that cache a real dynamic property.
//!
//! Called from:
//! - The OOP codegen integration test binary.
//!
//! Key details:
//! - Hash presence, runtime subclass layouts and independent result owners match PHP.

use crate::support::*;

/// Cached dynamic values bypass the getter, including null, on known and boxed receivers.
#[test]
fn test_magic_cached_dynamic_properties() {
    let out = compile_and_run_with_heap_debug(r#"<?php
#[AllowDynamicProperties]
class LazyCache {
    public int $reads = 0;
    public mixed $seed;
    public function __construct(mixed $seed) { $this->seed = $seed; }
    public function __get(string $name): mixed {
        if ($name !== 'cached') { return null; }
        if (++$this->reads > 3) { throw new LogicException('recursive getter'); }
        $this->cached = $this->seed;
        return $this->cached;
    }
}
class ChildCache extends LazyCache { public string $padding = 'extra'; }
function known(LazyCache $value): mixed { return $value->cached; }
function literal(LazyCache $value): mixed { return $value->{'cached'}; }
function boxed(mixed $value): mixed { return $value->cached; }
function run(): void {
    foreach ([null, false, ['x' => null, 'flag' => false]] as $seed) {
        $value = new ChildCache($seed);
        $first = known($value);
        $value->seed = 'changed';
        $second = literal($value);
        $third = boxed($value);
        echo json_encode([$first, $second, $third, $value->reads]), "\n";
        unset($first, $second, $third, $value);
    }
}
run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "[null,null,null,1]\n[false,false,false,1]\n[{\"x\":null,\"flag\":false},{\"x\":null,\"flag\":false},{\"x\":null,\"flag\":false},1]\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The same cache and owner semantics hold without EIR optimization.
#[test]
fn test_magic_cached_dynamic_properties_no_ir_opt() {
    without_ir_opt(test_magic_cached_dynamic_properties);
}

/// Literal dynamic probes preserve get-only coalesce and retire scalar result boxes.
#[test]
fn test_magic_literal_probe_order_and_owners() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class LiteralGetOnly {
    public function __get(string $name): int { echo 'get:', $name, "\n"; return 1; }
}
class LiteralWithIsset {
    public function __get(string $name): int { echo 'get:', $name, "\n"; return 0; }
    public function __isset(string $name): bool { echo 'isset:', $name, "\n"; return true; }
}
function probe(): void {
    $get = new LiteralGetOnly();
    var_dump(isset($get->{'missing'}));
    var_dump($get->{'missing'} ?? 'default');
    var_dump(empty($get->{'missing'}));
    $with = new LiteralWithIsset();
    var_dump(isset($with->{'missing'}));
    var_dump($with->{'missing'} ?? 'default');
    var_dump(empty($with->{'missing'}));
    unset($get, $with);
}
probe();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "bool(false)\nget:missing\nint(1)\nbool(true)\nisset:missing\nbool(true)\nisset:missing\nget:missing\nint(0)\nisset:missing\nget:missing\nbool(true)\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Literal probe ordering and temporary retirement are independent of EIR optimization.
#[test]
fn test_magic_literal_probe_order_and_owners_no_ir_opt() {
    without_ir_opt(test_magic_literal_probe_order_and_owners);
}

/// Literal and named `??` reads keep php's accessor order on known receivers: a stored hash entry
/// (even null) answers first, `__isset` runs only when declared and can veto `__get`, and a
/// get-only class still reads its getter. Every intermediate box is retired.
#[test]
fn test_magic_coalesce_accessor_order_on_known_receivers() {
    let out = compile_and_run_with_heap_debug(r#"<?php
class CoalesceGetOnly {
    public function __get(string $name): int { echo 'get:', $name, "\n"; return 7; }
}
class CoalesceWithIsset {
    public bool $answer = true;
    public function __get(string $name): string { echo 'get:', $name, "\n"; return "v-$name"; }
    public function __isset(string $name): bool { echo 'isset:', $name, "\n"; return $this->answer; }
}
#[AllowDynamicProperties]
class CoalesceCached {
    public function __get(string $name): mixed { echo 'get:', $name, "\n"; return 'from-get'; }
    public function __isset(string $name): bool { echo 'isset:', $name, "\n"; return true; }
}
function run(): void {
    $g = new CoalesceGetOnly();
    var_dump($g->{'a'} ?? 'd1');
    var_dump($g->b ?? 'd2');
    var_dump(isset($g->c), empty($g->d));
    $w = new CoalesceWithIsset();
    var_dump($w->{'e'} ?? 'd3');
    var_dump($w->f ?? 'd4');
    $w->answer = false;
    var_dump($w->{'g'} ?? 'd5');
    var_dump($w->h ?? 'd6');
    $c = new CoalesceCached();
    var_dump($c->{'k'} ?? 'd7');
    $c->k = null;
    var_dump($c->{'k'} ?? 'd8');
    $c->k = 'stored';
    var_dump($c->{'k'} ?? 'd9', $c->k ?? 'd10');
    $n = $g->{'z'} ?? 0;
    echo $n + 1, "\n";
    unset($g, $w, $c);
}
run();
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, concat!(
        "get:a\nint(7)\nget:b\nint(7)\nbool(false)\nbool(true)\n",
        "isset:e\nget:e\nstring(3) \"v-e\"\nisset:f\nget:f\nstring(3) \"v-f\"\n",
        "isset:g\nstring(2) \"d5\"\nisset:h\nstring(2) \"d6\"\n",
        "isset:k\nget:k\nstring(8) \"from-get\"\nstring(2) \"d8\"\nstring(6) \"stored\"\nstring(6) \"stored\"\n",
        "get:z\n8\n",
    ));
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// The coalesce accessor order and box retirement are independent of EIR optimization.
#[test]
fn test_magic_coalesce_accessor_order_on_known_receivers_no_ir_opt() {
    without_ir_opt(test_magic_coalesce_accessor_order_on_known_receivers);
}
