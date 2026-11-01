//! Purpose:
//! Integration tests for a null-capable string property (`?string` / `string|null`) filled from a
//! caller's boxed union value, then read back after the caller's frame is gone. Pins the ownership
//! contract of the gradual UNION parameter guard, which forwards its operand instead of acquiring
//! it.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A `?string` slot has the Mixed representation: one word holding a boxed Mixed cell pointer,
//!   with the uninitialized marker in the metadata word. It is NOT a TaggedScalar — only a
//!   null-capable INT union takes that inline two-word storage (see `nullable_int_defaults`).
//! - Regression for the use-after-free where `new H($n)` with a declared `?string` parameter and a
//!   branch-retyped `$n` released the caller's boxed cell once too often: the guard borrowed it,
//!   the constructor's `prop_set` incref and the call-argument release cancelled, and the frame's
//!   own release at scope exit freed the cell the object's property still pointed at. Reading it
//!   back printed `int(9223372036854775806)` (the free list overwrote the tag word while the null
//!   sentinel stayed in the payload) or an empty string, and `--heap-debug` reported a bad
//!   refcount.
//! - Expected outputs are `php -n` 8.5.6 output.

use super::*;

/// Verifies the reduced Symfony shape: objects carrying a `?string` built inside a loop, collected
/// into an array the function RETURNS, and read back after that frame is gone. This is
/// `AutowireLocator::__construct()` building `TypedReference`s; the `?string $name` came back as a
/// recycled object cell, so `ResolveBindingsPass` stringified an object PHP never reaches.
#[test]
fn test_nullable_string_property_survives_the_frame_that_built_it() {
    let out = compile_and_run(
        r#"<?php
class Ref { public function __construct(private string $id, private int $behavior = 1) {} }
class TR extends Ref {
    private ?string $name;
    public function __construct(string $id, private string $type, int $behavior = 1, ?string $name = null, private array $attributes = []) {
        $this->name = $type === $id ? $name : null;
        parent::__construct($id, $behavior);
    }
    public function getName(): ?string { return $this->name; }
}
function build(array $services): array {
    $references = [];
    foreach ($services as $key => $type) {
        $behavior = 1;
        if ('?' === $type[0]) { $type = substr($type, 1); $behavior = 3; }
        $name = $key;
        if (\is_int($name)) { $key = $type; $name = null; }
        $references[$key] = new TR($type, $type, $behavior, $name, []);
    }
    return $references;
}
foreach (build(['aliasA' => '?TypeA', 0 => '?TypeB', 'aliasC' => 'TypeC']) as $k => $r) {
    echo $k, '=';
    var_dump($r->getName());
}
"#,
    );
    assert_eq!(
        out,
        "aliasA=string(6) \"aliasA\"\nTypeB=NULL\naliasC=string(6) \"aliasC\"\n"
    );
}

/// Verifies the smallest face of the same defect: a local retyped to null inside a branch is a
/// boxed union, the declared `?string` parameter guards it, and the property must still read back
/// as null once the producing frame has returned. Before the fix the null row printed
/// `int(9223372036854775806)`.
#[test]
fn test_nullable_string_property_from_a_branch_retyped_local_reads_back() {
    let out = compile_and_run(
        r#"<?php
class TR {
    private ?string $name;
    public function __construct(string $id, private string $type, int $behavior = 1, ?string $name = null) {
        $this->name = $type === $id ? $name : null;
    }
    public function getName(): ?string { return $this->name; }
}
function m3build(bool $flag): array {
    $name = 'aliasA';
    if ($flag) { $name = null; }
    return ['k' => new TR('T', 'T', 1, $name)];
}
var_dump(m3build(false)['k']->getName());
var_dump(m3build(true)['k']->getName());
"#,
    );
    assert_eq!(out, "string(6) \"aliasA\"\nNULL\n");
}

/// Verifies the value reaches every whole-object reader — `var_dump`, `print_r`, `var_export`,
/// `json_encode`, `serialize`/`unserialize`, `get_object_vars`, the `(array)` cast — plus `isset`
/// and `??`, for both the set and the null state of the slot.
#[test]
fn test_nullable_string_property_readers_after_the_frame_returned() {
    let out = compile_and_run(
        r#"<?php
class H {
    public ?string $name;
    public function __construct(?string $name) { $this->name = $name; }
}
function mk(bool $flag): H {
    $n = 'set';
    if ($flag) { $n = null; }
    return new H($n);
}
foreach ([false, true] as $flag) {
    $h = mk($flag);
    var_dump($h->name);
    print_r($h->name);
    echo "\n";
    var_export($h->name);
    echo "\n";
    echo json_encode($h), "\n";
    $s = serialize($h);
    echo $s, "\n";
    var_dump(unserialize($s)->name);
    print_r(get_object_vars($h));
    print_r((array) $h);
    var_dump(isset($h->name));
    var_dump($h->name ?? 'fallback');
    echo "--\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "string(3) \"set\"\n",
            "set\n",
            "'set'\n",
            "{\"name\":\"set\"}\n",
            "O:1:\"H\":1:{s:4:\"name\";s:3:\"set\";}\n",
            "string(3) \"set\"\n",
            "Array\n(\n    [name] => set\n)\n",
            "Array\n(\n    [name] => set\n)\n",
            "bool(true)\n",
            "string(3) \"set\"\n",
            "--\n",
            "NULL\n",
            "\n",
            "NULL\n",
            "{\"name\":null}\n",
            "O:1:\"H\":1:{s:4:\"name\";N;}\n",
            "NULL\n",
            "Array\n(\n    [name] => \n)\n",
            "Array\n(\n    [name] => \n)\n",
            "bool(false)\n",
            "string(8) \"fallback\"\n",
            "--\n",
        )
    );
}

/// Verifies a NON-nullable `string` property filled from a local that dies with its frame. This
/// slot takes the `Str` representation and its own persisted block, so it never reached the union
/// guard; the test pins that the fix did not disturb it.
#[test]
fn test_string_property_from_a_dying_local_reads_back() {
    let out = compile_and_run(
        r#"<?php
class S {
    private string $name;
    public function __construct(string $n) { $this->name = $n; }
    public function get(): string { return $this->name; }
}
function mk(): array {
    $s = 'hello' . 'world';
    return ['k' => new S($s)];
}
$r = mk();
var_dump($r['k']->get());
"#,
    );
    assert_eq!(out, "string(10) \"helloworld\"\n");
}

/// Verifies the ownership contract in BOTH directions with one heap-debug fixture.
///
/// The first half feeds the guard a BORROWED local load: nothing may release it after the call,
/// because the caller's frame still owns that cell and the new object's property now points at it.
/// The second half feeds the guard an OWNING temporary (a `mixed` call result): its reference
/// transfers through the forwarding guard and the call-argument cleanup must still release it, or
/// the cell leaks. Before the fix the first half aborted with
/// `Fatal error: heap debug detected bad refcount`.
#[test]
fn test_nullable_string_param_guard_balances_borrowed_and_owning_sources() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
class H {
    private ?string $name;
    public function __construct(?string $name) { $this->name = $name; }
    public function get(): ?string { return $this->name; }
}
function src(): mixed { $n = 'kept'; return $n; }
function mk(bool $flag): array {
    $n = 'set';
    if ($flag) { $n = null; }
    return ['k' => new H($n)];
}
$a = mk(true);
var_dump($a['k']->get());
unset($a);
$b = ['k' => new H(src())];
var_dump($b['k']->get());
unset($b);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "NULL\nstring(4) \"kept\"\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// Verifies the `string|null` spelling behaves as `?string`, and that a repeated build/read cycle
/// stays stable once the freed cell would have been recycled by the next allocation — the shape
/// that turned the property into a boxed OBJECT of an unrelated class in the Symfony binary.
#[test]
fn test_explicit_string_null_property_repeated_build_and_read() {
    let out = compile_and_run(
        r#"<?php
class H {
    private string|null $name;
    public function __construct(string|null $name) { $this->name = $name; }
    public function get(): string|null { return $this->name; }
}
function mk(bool $flag): array {
    $n = 'row';
    if ($flag) { $n = null; }
    return ['k' => new H($n)];
}
$kept = [];
for ($i = 0; $i < 4; $i++) {
    $kept[] = mk($i % 2 === 1);
    $noise = str_repeat('x', 16);
}
foreach ($kept as $row) {
    var_dump($row['k']->get());
}
"#,
    );
    assert_eq!(
        out,
        "string(3) \"row\"\nNULL\nstring(3) \"row\"\nNULL\n"
    );
}
