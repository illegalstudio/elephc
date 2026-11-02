//! Purpose:
//! Integration tests for `?ptr<T>` values: a nullable typed pointer is one raw pointer word whose
//! null is address 0, across returns, parameters, locals, properties and call results.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Covers `=== null`, `!== null`, `is_null()`, `isset()`, `??`, `echo` and `gettype()` on null
//!   and set pointers, packed record addresses flowing into `?ptr<T>` sinks, instance and static
//!   property defaults (literal copy and initializer thunk), method, static method and closure
//!   returns, ternary and match merges with `null`, extern calls behind a null guard, and the
//!   null-pointer dereference fatal (task 6da89f60).
//! - Also covers boxing into `mixed` (globals, yields, arrays, hashes, dynamic properties,
//!   iterators, interface wrappers and the eval bridges), where address 0 must be PHP null.
//! - Undeclared locals that hold `null` and packed record addresses or pointers (in sequence,
//!   across `if` arms, around loops that start from `null`, and bound to `?ptr<T>` by-reference
//!   parameters) get the same pointer-word storage (task 1af74866). The branch fixtures read the
//!   local after a second `if`, so DCE does not copy the read into the arm that holds `null`.

use crate::support::*;

/// Verifies a `?ptr<T>` returned by a function or passed as a parameter keeps the pointer and
/// reads as PHP null when it is null, through the comparison, null and output builtins.
#[test]
fn test_nullable_pointer_returns_and_parameters_compare_with_null() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v, bool $ok): ?ptr<Vec2> { return $ok ? $v[0] : null; }
function take(?ptr<Vec2> $p): int { return $p === null ? -1 : $p->x; }
$v = buffer_new<Vec2>(1);
$v[0]->x = 7;
$p = first($v, true);
if ($p !== null) { echo $p->x, "|"; }
$q = first($v, false);
echo $q === null ? "null" : "set", "|", is_null($q) ? "null" : "set", "|";
echo isset($q) ? "set" : "unset", "|", isset($p) ? "set" : "unset", "\n";
$r = $q ?? first($v, true);
echo $r->x, "|", take(null), "|", take($v[0]), "|", take(ptr_null()), "\n";
echo "[", $q, "]|", gettype($q), "|", gettype($p), "\n";
$local = first($v, true);
$local = null;
echo is_null($local) ? "cleared" : "kept", "\n";
"#,
    );
    assert_eq!(out, "7|null|null|unset|set\n7|-1|7|-1\n[]|NULL|integer\ncleared\n");
}

/// Verifies `?ptr<T>` instance and static properties start null (through the literal default
/// copy and through the class initializer thunk), store a packed record address, and clear.
#[test]
fn test_nullable_pointer_properties_default_store_and_clear() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
final class Holder { public ?ptr<Vec2> $p = null; }
final class Lazy { public ?ptr<Vec2> $p = null; public bool $wide = PHP_INT_SIZE > 4 ? true : false; }
final class Box { public static ?ptr<Vec2> $s = null; }
$v = buffer_new<Vec2>(1);
$v[0]->x = 9;
$h = new Holder();
$l = new Lazy();
echo $h->p === null ? "empty" : "full", "|", $l->p === null ? "empty" : "full", "|";
echo Box::$s === null ? "empty" : "full", "\n";
$h->p = $v[0];
Box::$s = $v[0];
echo $h->p === null ? "empty" : $h->p->x, "|", Box::$s === null ? "empty" : Box::$s->x, "\n";
$h->p = null;
Box::$s = null;
echo is_null($h->p) ? "empty" : "full", "|", is_null(Box::$s) ? "empty" : "full", "\n";
"#,
    );
    assert_eq!(out, "empty|empty|empty\n9|9\nempty|empty\n");
}

/// Verifies method, static method and closure calls returning `?ptr<T>`, and ternary and match
/// merges of a record address with `null`, keep the null member of the result type.
#[test]
fn test_nullable_pointer_method_closure_and_merge_results() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
final class Finder {
    public function __construct(private buffer<Vec2> $v) {}
    public function at(bool $ok): ?ptr<Vec2> { return $ok ? $this->v[0] : null; }
    public static function none(): ?ptr<Vec2> { return null; }
}
$v = buffer_new<Vec2>(1);
$v[0]->x = 5;
$f = new Finder($v);
$a = $f->at(true);
$b = $f->at(false);
echo $a === null ? "null" : $a->x, "|", $b === null ? "null" : "set", "|";
echo Finder::none() === null ? "null" : "set", "\n";
$get = function (bool $ok) use ($v): ?ptr<Vec2> { return $ok ? $v[0] : null; };
$c = $get(false);
$d = $get(true);
echo $c === null ? "null" : "set", "|", $d === null ? "null" : $d->x, "\n";
$t = $argc > 5 ? $v[0] : null;
$m = match ($argc) { 1 => null, default => $v[0] };
echo $t === null ? "null" : "set", "|", $m === null ? "null" : "set", "\n";
"#,
    );
    assert_eq!(out, "5|null|null\nnull|5\nnull|null\n");
}

/// Verifies a guarded `?ptr<T>` local passes to an extern `ptr<T>` parameter as the raw address.
#[test]
fn test_nullable_pointer_passes_to_extern_after_null_guard() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
extern function memset(ptr<Vec2> $target, int $byte, u64 $size): ptr;
function first(buffer<Vec2> $v, bool $ok): ?ptr<Vec2> { return $ok ? $v[0] : null; }
$v = buffer_new<Vec2>(1);
$v[0]->x = 3;
$p = first($v, true);
if ($p !== null) { memset($p, 0, 16); }
echo $v[0]->x, "\n";
"#,
    );
    assert_eq!(out, "0\n");
}

/// Verifies writing through a null `?ptr<T>` reports the null-pointer dereference fatal.
#[test]
fn test_nullable_pointer_null_dereference_reports_runtime_error() {
    let err = compile_and_run_expect_failure(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v, bool $ok): ?ptr<Vec2> { return $ok ? $v[0] : null; }
$v = buffer_new<Vec2>(1);
$q = first($v, false);
$q->x = 1;
"#,
    );
    assert!(err.contains("Fatal error: null pointer dereference"));
}

/// Verifies freeing an object whose `?ptr<T>` property still holds an interior record address
/// leaves the memory before that address alone. The slot used to be described as a Mixed cell,
/// so object teardown decref'd the address and, on AArch64, rewrote the preceding record; the
/// property then appears in `json_encode` as its integer address like a `ptr` property.
#[test]
fn test_nullable_pointer_property_teardown_does_not_release_the_address() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
final class Holder { public ?ptr<Vec2> $p = null; }
function plant(buffer<Vec2> $v): void {
    $h = new Holder();
    $h->p = $v[1];
}
$v = buffer_new<Vec2>(2);
$v[0]->x = 0;
$v[0]->y = 1;
plant($v);
echo $v[0]->x, "|", $v[0]->y, "|", json_encode(new Holder()), "\n";
"#,
    );
    assert_eq!(out, "0|1|{\"p\":0}\n");
}

/// Verifies a `?ptr<T>` boxed into `mixed` (a parameter or a property) boxes a null pointer as
/// PHP null and a set one as its integer address, and that string conversions of a null
/// pointer give the empty string `echo` prints.
#[test]
fn test_nullable_pointer_boxes_and_converts_null_as_php_null() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
final class Bag { public mixed $m = 1; }
function show(mixed $m): void { echo $m === null ? "null" : gettype($m), "|"; }
function first(buffer<Vec2> $v, bool $ok): ?ptr<Vec2> { return $ok ? $v[0] : null; }
$v = buffer_new<Vec2>(1);
show(first($v, false));
show(first($v, true));
$b = new Bag();
$b->m = first($v, false);
echo $b->m === null ? "null" : "set", "|";
$b->m = first($v, true);
echo $b->m === null ? "null" : "set", "\n";
$q = first($v, false);
echo "x{$q}y|", "a" . $q . "b|", (string)$q, "|", sprintf("%s", $q), "|", strlen((string)$q), "|";
echo strlen("a" . first($v, true)) > 3 ? "address" : "empty", "\n";
"#,
    );
    assert_eq!(out, "null|integer|null|set\nxy|ab|||0|address\n");
}

/// Verifies `(bool)`, `boolval()`, `xor` and `empty()` test a `?ptr<T>` word against 0 instead
/// of reading the record it points to as a Mixed cell (a live record of zeros read as falsy,
/// and one starting with 7 faulted).
#[test]
fn test_nullable_pointer_truthiness_and_empty_test_the_address() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v, bool $ok): ?ptr<Vec2> { return $ok ? $v[0] : null; }
function second(buffer<Vec2> $v): ?ptr<Vec2> { return $v[1]; }
$v = buffer_new<Vec2>(2);
$v[1]->x = 7;
$v[1]->y = 1;
$zero = first($v, true);
$live = second($v);
$none = first($v, false);
echo ((bool)$zero) ? "y" : "n", boolval($live) ? "y" : "n", ((bool)$none) ? "y" : "n", "|";
echo empty($zero) ? "e" : "s", empty($live) ? "e" : "s", empty($none) ? "e" : "s", "|";
echo ($zero xor $none) ? "xor" : "same", "\n";
"#,
    );
    assert_eq!(out, "yyn|sse|xor\n");
}

/// Verifies a merge whose null arm is a `?ptr<T>` local, instance property or static property
/// (ternary, `??` and `?:`) stays nullable, so `=== null` and `echo` see PHP null.
#[test]
fn test_nullable_pointer_merges_keep_null_from_locals_and_properties() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
final class Holder { public ?ptr<Vec2> $p = null; public static ?ptr<Vec2> $s = null; }
function first(buffer<Vec2> $v, bool $ok): ?ptr<Vec2> { return $ok ? $v[0] : null; }
$v = buffer_new<Vec2>(1);
$v[0]->x = 3;
$q = first($v, false);
$t = $argc > 5 ? $v[0] : $q;
echo $t === null ? "null" : "set", "[", $t, "]|";
$h = new Holder();
$u = $argc > 5 ? $v[0] : $h->p;
echo $u === null ? "null" : "set", "|";
$s = $argc > 5 ? $v[0] : Holder::$s;
echo $s === null ? "null" : "set", "|";
$w = $q ?? $q;
echo $w === null ? "null" : "set", "|";
$x = $q ?: null;
echo $x === null ? "null" : "set", "|";
$y = $q ?? $v[0];
echo $y === null ? "null" : $y->x, "\n";
"#,
    );
    assert_eq!(out, "null[]|null|null|null|null|3\n");
}

/// Verifies `??` and `??=` with a literal `null` on either side of a `?ptr<T>` stay nullable,
/// so the result is PHP null (`=== null`, `isset()` false, empty echo) rather than a pointer.
#[test]
fn test_nullable_pointer_coalesce_with_null_arms_stays_null() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
final class Holder { public ?ptr<Vec2> $p = null; }
function first(buffer<Vec2> $v, bool $ok): ?ptr<Vec2> { return $ok ? $v[0] : null; }
$v = buffer_new<Vec2>(1);
$q = first($v, false);
$h = new Holder();
$a = $q ?? null;
$b = null ?? $q;
$c = $h->p ?? null;
$d = first($v, false) ?? null;
$e = $q;
$e ??= null;
echo $a === null ? "n" : "s", $b === null ? "n" : "s", $c === null ? "n" : "s";
echo $d === null ? "n" : "s", $e === null ? "n" : "s", "|";
echo isset($a) ? "set" : "unset", "|[", $a, "]\n";
"#,
    );
    assert_eq!(out, "nnnnn|unset|[]\n");
}

/// Verifies `==`, `!=` and `switch` compare a `?ptr<T>` through PHP's loose table as the value
/// it stands for: null for address 0, its integer address otherwise.
#[test]
fn test_nullable_pointer_loose_comparisons_and_switch() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v, bool $ok): ?ptr<Vec2> { return $ok ? $v[0] : null; }
$v = buffer_new<Vec2>(1);
$q = first($v, false);
$p = first($v, true);
echo $q == null ? "eq" : "ne", $q == 0 ? "0" : "-", $q == false ? "f" : "-", $q == "" ? "e" : "-";
echo $q != null ? "!" : ".", $p == null ? "!" : ".", $p != null ? "set" : "null", "|";
switch ($q) { case null: echo "q-null"; break; default: echo "q-default"; }
echo "|";
switch ($p) { case null: echo "p-null"; break; default: echo "p-default"; }
echo "\n";
"#,
    );
    assert_eq!(out, "eq0fe..set|q-null|p-default\n");
}

/// Verifies a null `?ptr<T>` stored into `mixed` storage (a static property, a by-reference
/// parameter, a static local) is PHP null and a live one is not, while a plain `ptr_null()`
/// merged with a string stays the integer 0.
#[test]
fn test_nullable_pointer_null_boxes_as_null_in_mixed_storage() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
final class Box { public static mixed $m = 1; }
function first(buffer<Vec2> $v, bool $ok): ?ptr<Vec2> { return $ok ? $v[0] : null; }
function set(mixed &$m, ?ptr<Vec2> $p): void { $m = $p; }
function keep(?ptr<Vec2> $p): bool { static $m = null; $m = $p; return $m === null; }
$v = buffer_new<Vec2>(1);
$q = first($v, false);
$p = first($v, true);
Box::$m = $q;
echo Box::$m === null ? "n" : "s";
Box::$m = $p;
echo Box::$m === null ? "n" : "s", "|";
$m = 1;
set($m, $q);
echo $m === null ? "n" : "s", "|";
echo keep($q) ? "n" : "s", keep($p) ? "n" : "s", "|";
$y = $argc > 5 ? "none" : ptr_null();
echo "[", $y, "]", $y === null ? "n" : "s", "\n";
"#,
    );
    // A plain `ptr` is not nullable: its null address boxes as the integer 0.
    assert_eq!(out, "ns|n|ns|[0]s\n");
}

/// Verifies builtins and casts read a `?ptr<T>` as the PHP value it stands for: `json_encode`,
/// `serialize`, `var_dump`, `print_r`, `is_numeric`, `intval`/`floatval` and `(int)`/`(float)`
/// see null for address 0 and an integer for a live address.
#[test]
fn test_nullable_pointer_builtins_and_casts_see_php_null() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v, bool $ok): ?ptr<Vec2> { return $ok ? $v[0] : null; }
$v = buffer_new<Vec2>(1);
$q = first($v, false);
$p = first($v, true);
echo json_encode($q), "|", json_encode($p) === "null" ? "null" : "int", "|";
echo (int)$q, "|", (float)$q, "|", intval($q), "|", floatval($q), "|", (int)$p > 0 ? "addr" : "zero", "|";
echo is_numeric($q) ? "num" : "not", is_numeric($p) ? "num" : "not", "|";
var_dump($q);
print_r($q);
echo "|", serialize($q), "|", serialize($p) === "N;" ? "N" : "i", "\n";
"#,
    );
    assert_eq!(out, "null|int|0|0|0|0|addr|notnum|NULL\n|N;|i\n");
}

/// Verifies builtins reached through `call_user_func`, a variable function, a first-class
/// callable and a pipe see a `?ptr<T>` the way a direct call does (boxed: null or an integer).
#[test]
fn test_nullable_pointer_builtins_through_static_callables() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v, bool $ok): ?ptr<Vec2> { return $ok ? $v[0] : null; }
$v = buffer_new<Vec2>(1);
$q = first($v, false);
$p = first($v, true);
echo call_user_func('json_encode', $p) === "null" ? "null" : "int", "|";
call_user_func('var_dump', $q);
echo call_user_func('serialize', $q), "|", call_user_func('is_numeric', $p) ? "num" : "not", "|";
call_user_func('print_r', $q);
echo "|", call_user_func('intval', $q, 10), "|";
$f = 'json_encode';
echo $f($q), "|";
$g = json_encode(...);
echo $g($q), "|", $q |> $g, "\n";
var_dump($q, $q);
"#,
    );
    assert_eq!(out, "int|NULL\nN;|num||0|null|null|null\nNULL\nNULL\n");
}

/// Verifies a literal `null` or an omitted `= null` default bound to a `?ptr<T>` parameter is
/// the null pointer in the callee on every call surface: methods, static methods, promoted
/// constructor parameters, closures and `call_user_func` with a static or a runtime name.
#[test]
fn test_nullable_pointer_parameters_receive_null_on_every_call_surface() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
final class Box {
    public function __construct(public ?ptr<Vec2> $p = null) {}
    public function take(?ptr<Vec2> $p = null): string { return $p === null ? "null" : "set"; }
    public static function stake(?ptr<Vec2> $p): string { return $p === null ? "null" : "set"; }
}
function take(?ptr<Vec2> $p): string { return $p === null ? "null" : "set"; }
$v = buffer_new<Vec2>(1);
$b = new Box();
echo $b->take(null), $b->take(), $b->take($v[0]), "|", Box::stake(null), "|", $b->p === null ? "null" : "set", "|";
echo call_user_func('take', null), "|";
$f = function (?ptr<Vec2> $p): string { return $p === null ? "null" : "set"; };
echo $f(null), $f($v[0]), "|";
$name = $argc > 5 ? "nope" : "take";
echo call_user_func($name, null), "\n";
"#,
    );
    assert_eq!(out, "nullnullset|null|null|null|nullset|null\n");
}

/// Verifies a `null` bound by a method call to a `?ptr<T>` parameter that the body later
/// reassigns (which widens its slot to `mixed`) still reads as null, while a real address in
/// the same function stays set.
#[test]
fn test_nullable_pointer_parameter_reassigned_in_body_keeps_null() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
final class Box {
    public function take(?ptr<Vec2> $p): string {
        $seen = $p === null ? "null" : "set:" . $p->x;
        $p = null;
        return $seen;
    }
}
$v = buffer_new<Vec2>(1);
$v[0]->x = 4;
$b = new Box();
echo $b->take(null), "|", $b->take($v[0]), "\n";
"#,
    );
    assert_eq!(out, "null|set:4\n");
}

/// Verifies a first-class method callable returning `?ptr<T>`, invoked directly, hands back the
/// pointer word: null for a null return and the record for a live one.
#[test]
fn test_nullable_pointer_result_of_invoked_method_callable() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
final class Finder {
    public function __construct(private buffer<Vec2> $v) {}
    public function none(): ?ptr<Vec2> { return null; }
    public function first(): ?ptr<Vec2> { return $this->v[0]; }
}
$v = buffer_new<Vec2>(1);
$v[0]->x = 3;
$f = new Finder($v);
$n = ($f->none(...))();
$p = ($f->first(...))();
echo $n === null ? "null" : "set", "|", $p === null ? "null" : $p->x, "\n";
"#,
    );
    assert_eq!(out, "null|3\n");
}

/// Verifies `?ptr<T>` locals hold address 0 for null (a typed `= null` declaration and a later
/// `$x = null`), so `=== null`, `is_null()` and `?ptr<T>` by-reference parameters (reading and
/// clearing) all see PHP null, while a live address stays set.
#[test]
fn test_nullable_pointer_locals_and_by_ref_parameters() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function take(?ptr<Vec2> &$p): string { $r = $p === null ? "null" : "set"; return $r; }
function clear(?ptr<Vec2> &$p): void { $p = null; }
$v = buffer_new<Vec2>(1);
?ptr<Vec2> $x = null;
echo $x === null ? "null" : "set", is_null($x) ? "null" : "set", "|", take($x), "|";
$y = $v[0];
echo take($y), "|";
?ptr<Vec2> $z = $v[0];
$z = null;
echo $z === null ? "null" : "set", "|", take($z), "|";
?ptr<Vec2> $w = $v[0];
clear($w);
echo $w === null ? "null" : "set", "\n";
"#,
    );
    assert_eq!(out, "nullnull|null|set|null|null|null\n");
}

/// Verifies a null `?ptr<T>` variable (declared, or a parameter a guard narrowed to null for
/// its region or for the rest of the body) bound to a `?ptr<T>` by-reference parameter is read
/// as null by the callee, and that a pointer the callee stores through it reaches the caller,
/// including inside the region the guard narrowed (where the variable is typed `?ptr<T>` again,
/// so an untyped callee is not specialized for `null`).
#[test]
fn test_nullable_pointer_by_ref_parameters_accept_null_variables() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function take(?ptr<Vec2> &$p): string { return $p === null ? "null" : "set"; }
function fill(?ptr<Vec2> &$p, buffer<Vec2> $v): void { if ($p === null) { $p = $v[0]; } }
function guarded(?ptr<Vec2> $p): string { if ($p === null) { return take($p); } return "live"; }
function early(?ptr<Vec2> $p): string { if ($p !== null) { return "live"; } return take($p); }
function id($m) { return $m; }
function refill(?ptr<Vec2> $p, buffer<Vec2> $v): string {
    if ($p === null) { fill($p, $v); return is_null($p) ? "null" : "filled" . $p->x; }
    return "live";
}
$v = buffer_new<Vec2>(1);
$v[0]->x = 6;
?ptr<Vec2> $x = null;
echo take($x), "|", guarded(null), "|", guarded($v[0]), "|", early(null), "|";
echo refill(null, $v), "|";
?ptr<Vec2> $y = null;
fill($y, $v);
echo $y === null ? "null" : $y->x, "|";
?ptr<Vec2> $z = null;
if ($z === null) { fill($z, $v); echo id($z) === null ? "null" : "set", "|"; }
echo $z === null ? "null" : $z->x, "\n";
"#,
    );
    assert_eq!(out, "null|null|live|null|filled6|6|set|6\n");
}

/// Verifies plain locals bind to a `?ptr<T>` by-reference parameter once they are pointer words
/// (task 1af74866): one that holds only `null` (it reads as `?ptr<T>` after the call, so a later
/// clear is seen too), one that holds `null` and then a pointer, one that holds `null` in one
/// branch and a pointer in the other, a named argument, and one holding a packed record address.
/// Each sees the callee's write after the call, in functions and at top level; PR #2019 refused
/// all of them because their slots were boxed or held the null sentinel.
#[test]
fn test_nullable_pointer_by_ref_parameters_accept_null_and_pointer_locals() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function fill(?ptr<Vec2> &$p, buffer<Vec2> $v): void { $p = $v[0]; }
function clear(?ptr<Vec2> &$p): void { $p = null; }
function first(buffer<Vec2> $v): ptr<Vec2> { return $v[0]; }
function only_null(buffer<Vec2> $v): string {
    $a = null;
    fill($a, $v);
    $out = $a === null ? "null" : "x" . $a->x;
    clear($a);
    return $out . ($a === null ? ":null" : ":set");
}
function null_then_pointer(buffer<Vec2> $v): string {
    $b = null;
    $b = first($v);
    fill($b, $v);
    return "x" . $b->x;
}
function branches(buffer<Vec2> $v, bool $ok): string {
    if ($ok) { $c = null; } else { $c = first($v); }
    fill($c, $v);
    return $c === null ? "null" : "x" . $c->x;
}
function named(buffer<Vec2> $v): string {
    $d = null;
    fill(p: $d, v: $v);
    return is_null($d) ? "null" : "x" . $d->x;
}
function record(buffer<Vec2> $v): string {
    $e = $v[0];
    fill($e, $v);
    return "x" . $e->x;
}
$v = buffer_new<Vec2>(1);
$v[0]->x = 7;
echo only_null($v), "|", null_then_pointer($v), "|", branches($v, true), "|";
echo branches($v, false), "|", named($v), "|", record($v), "\n";
$a = null;
fill($a, $v);
echo $a === null ? "null" : "x" . $a->x, "|";
$b = null;
$b = first($v);
fill($b, $v);
echo "x", $b->x, "|";
$d = null;
fill(p: $d, v: $v);
echo is_null($d) ? "null" : "x" . $d->x, "|";
if (count($argv) > 5) { $c = null; } else { $c = first($v); }
fill($c, $v);
echo $c === null ? "null" : "x" . $c->x, "\n";
"#,
    );
    assert_eq!(out, "x7:null|x7|x7|x7|x7|x7\nx7|x7|x7|x7\n");
}

/// Verifies a local whose frame slot is still not a pointer word is refused for a `?ptr<T>`
/// by-reference parameter: one that a reference aliases (at top level and in a function) and
/// one that a `global` statement makes program-global. Both keep boxed storage, which would
/// misread the raw pointer word the callee writes back. The function with the `global`
/// statement is called and has an observable body, so reachability pruning keeps it.
#[test]
fn test_nullable_pointer_by_ref_parameters_reject_non_pointer_storage() {
    let prelude = r#"<?php
packed class Vec2 { public int $x; public int $y; }
function fill(?ptr<Vec2> &$p, buffer<Vec2> $v): void { $p = $v[0]; }
$v = buffer_new<Vec2>(1);
"#;
    for body in [
        "$x = null; $r = &$x; fill($x, $v);",
        "function k(buffer<Vec2> $v): void { $x = null; $r = &$x; fill($x, $v); } k($v);",
        "function g(): void { global $x; echo $x; } $x = null; g(); fill($x, $v);",
    ] {
        let error = compile_cli_file_with_flags_expect_failure(&format!("{prelude}{body}"), &[]);
        assert!(
            error.contains("Declare it as `?ptr<Vec2> $x = null;`"),
            "{body}: {error}"
        );
    }
}

/// Verifies a field write through a `?ptr<T>` local that was declared `= null` and is still
/// null at run time reports the null-pointer dereference fatal.
#[test]
fn test_nullable_pointer_typed_null_local_dereference_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
$v = buffer_new<Vec2>(1);
?ptr<Vec2> $x = null;
if ($argc > 5) { $x = $v[0]; }
$x->x = 1;
"#,
    );
    assert!(err.contains("Fatal error: null pointer dereference"));
}

/// Verifies a `?ptr<T>` result of a function called through `call_user_func` with a runtime
/// name (the boxed descriptor invoker path) is PHP null for a null pointer and an integer for a
/// live one, like a direct call.
#[test]
fn test_nullable_pointer_result_through_runtime_named_callable() {
    let out = compile_and_run(
        r#"<?php
extern function malloc(u64 $size): ptr;
extern function free(ptr $p): void;
function none(): ?ptr { return null; }
function some(): ?ptr { return malloc(8); }
$n = $argc > 5 ? "some" : "none";
$s = $argc > 5 ? "none" : "some";
$a = call_user_func($n);
$b = call_user_func($s);
echo $a === null ? "null" : "set", "|", json_encode($a), "|", $b === null ? "null" : gettype($b), "\n";
"#,
    );
    assert_eq!(out, "null|null|integer\n");
}

/// Verifies expression assignments of `null` into `?ptr<T>` locals (inside an expression, chained,
/// and through a by-reference parameter) store address 0, so `=== null` and a later by-reference
/// binding see PHP null.
#[test]
fn test_nullable_pointer_expression_assignments_of_null() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function clear(?ptr<Vec2> &$p): string {
    $q = ($p = null);
    return $p === null ? "null" : "set";
}
function take(?ptr<Vec2> &$p): string { return $p === null ? "null" : "set"; }
$v = buffer_new<Vec2>(1);
?ptr<Vec2> $c = $v[0];
echo clear($c), "|", $c === null ? "null" : "set", "|";
?ptr<Vec2> $b = $v[0];
$a = $b = null;
echo $b === null ? "null" : "set", "|", take($b), "\n";
"#,
    );
    assert_eq!(out, "null|null|null|null\n");
}

/// Verifies `??` keeps a null plain `ptr` on its left side (a plain pointer is never PHP null),
/// even when the default is a `?ptr<T>`.
#[test]
fn test_coalesce_keeps_a_null_plain_pointer_left_side() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function none(): ?ptr { return null; }
$r = ptr_null() ?? none();
echo $r === null ? "null" : "set", "|";
ptr<Vec2> $p = ptr_null();
?ptr<Vec2> $q = null;
echo ($p ?? $q) === null ? "null" : "set", "\n";
"#,
    );
    assert_eq!(out, "set|set\n");
}

/// Verifies a `?ptr<T>` piped into a runtime callable (a `callable` parameter) is boxed as PHP
/// null for address 0, like a direct call.
#[test]
fn test_nullable_pointer_piped_into_runtime_callable() {
    let out = compile_and_run(
        r#"<?php
function show(mixed $v): void { echo $v === null ? "null" : gettype($v), "|"; }
function none(): ?ptr { return null; }
function run(callable $fn): void { none() |> $fn; }
run(show(...));
show(none());
"#,
    );
    assert_eq!(out, "null|null|");
}

/// Verifies `===` between a `?ptr<T>` and a value of another type compares the PHP value the
/// pointer boxes to: a live pointer equals its own `(int)` address, a null one does not equal a
/// plain `ptr_null()` (whose 0 is not PHP null), and a packed record address or another
/// `?ptr<T>` still compares by address.
#[test]
fn test_nullable_pointer_strict_comparisons_with_other_values() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v, bool $ok): ?ptr<Vec2> { return $ok ? $v[0] : null; }
function same(?ptr<Vec2> $p): string { return $p === (int)$p ? "same" : "diff"; }
$v = buffer_new<Vec2>(1);
$n = first($v, false);
$l = first($v, true);
echo same($l), "|", same($n), "|", $n === ptr_null() ? "eq" : "ne", "|", $l === $v[0] ? "eq" : "ne", "|";
echo $n === first($v, false) ? "eq" : "ne", "|", $l !== 5 ? "ne" : "eq", "|", $l === first($v, true) ? "eq" : "ne", "\n";
"#,
    );
    assert_eq!(out, "same|diff|ne|eq|eq|ne|eq\n");
}

/// Verifies a `mixed` static local initialized from a null `?ptr<T>` holds PHP null, not the
/// integer 0, until it is reassigned.
#[test]
fn test_nullable_pointer_null_initializer_of_a_mixed_static_local() {
    let out = compile_and_run(
        r#"<?php
function none(): ?ptr { return null; }
function f(mixed $m, bool $set): string { static $x = none(); $r = json_encode($x); if ($set) { $x = $m; } return $r; }
echo f(1, false), "|", f(2, true), "|", f(3, false), "\n";
"#,
    );
    assert_eq!(out, "null|null|2\n");
}

/// Verifies a null `?ptr<T>` is PHP null, and a live one an integer, wherever it is boxed into
/// `mixed` storage outside a call: a `global` variable, a yielded value, an append to and an
/// element write of a mixed array, a hash write and append, a dynamic property, and array
/// literals.
#[test]
fn test_nullable_pointer_values_box_as_null_in_mixed_storage() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function none(): ?ptr { return null; }
function first(buffer<Vec2> $v): ?ptr<Vec2> { return $v[0]; }
function g(?ptr $p): string { global $gm; $gm = $p; return json_encode($gm) . ($gm === null ? "N" : "S"); }
function gen(?ptr $p): iterable { yield $p; }
$v = buffer_new<Vec2>(1);
$p = none();
$l = first($v);
echo g($p), "|";
foreach (gen($p) as $y) { echo json_encode($y), "|"; }
$a = [1, "x"]; $a[] = $p; $a[0] = $p; echo json_encode($a), "|";
$h = ["k" => "x"]; $h["p"] = $p; $h[] = $p; $h["l"] = gettype($l); echo json_encode($h), "|";
$o = new stdClass(); $o->q = $p; $o->r = $l; echo json_encode($o->q), gettype($o->r), "|";
echo json_encode(["p" => $p, "s" => "x"]), json_encode([$p, "x"]), "\n";
"#,
    );
    assert_eq!(
        out,
        "nullN|null|[null,\"x\",null]|{\"k\":\"x\",\"p\":null,\"0\":null,\"l\":\"integer\"}|nullinteger|{\"p\":null,\"s\":\"x\"}[null,\"x\"]\n"
    );
}

/// Verifies a `?ptr<T>` returned by an `Iterator::current()` implementation is PHP null for
/// address 0 and an integer when live, through a `foreach` over the concrete class, a `foreach`
/// and a direct `current()` call on an `Iterator`-typed parameter (the interface wrapper), and a
/// nullsafe call.
#[test]
fn test_nullable_pointer_iterator_values_box_as_null() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v): ?ptr<Vec2> { return $v[0]; }
class It implements Iterator {
    private int $i = 0;
    public function __construct(private ?ptr $p) {}
    public function current(): ?ptr { return $this->i === 0 ? null : $this->p; }
    public function key(): mixed { return $this->i; }
    public function next(): void { $this->i++; }
    public function rewind(): void { $this->i = 0; }
    public function valid(): bool { return $this->i < 2; }
}
function walk(Iterator $it): void {
    foreach ($it as $v) { echo json_encode($v === null), gettype($v), "|"; }
    $it->rewind();
    echo json_encode($it->current()), "|";
}
function opt(?It $it): string { return json_encode($it?->current()); }
$v = buffer_new<Vec2>(1);
$it = new It(first($v));
foreach ($it as $k => $p) { echo $k, gettype($p), "|"; }
walk($it);
echo opt($it), opt(null), "\n";
"#,
    );
    assert_eq!(out, "0NULL|1integer|trueNULL|falseinteger|null|nullnull\n");
}

/// Verifies `eval` reaches `?ptr<T>` members through the native bridges: a method return and
/// parameter, an instance and a static property read and write, and a constructor parameter all
/// map address 0 to PHP null, and a live pointer reads as an integer.
#[test]
fn test_nullable_pointer_members_cross_the_eval_bridges() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v): ?ptr<Vec2> { return $v[0]; }
class C {
    public static ?ptr $sp = null;
    public ?ptr $ip = null;
    public function __construct(public ?ptr $cp = null) {}
    public function m(?ptr $p): string { return $p === null ? "N" : "S"; }
    public function r(): ?ptr { return null; }
}
function reads(C $o): void { eval('echo json_encode($o->r()), $o->m(null), json_encode($o->ip), json_encode(C::$sp), "|";'); }
function writes(C $o): void { eval('C::$sp = null; $o->ip = null; $n = new C(null); echo json_encode(C::$sp), json_encode($o->ip), json_encode($n->cp), "|";'); }
function live(): void { eval('echo gettype(C::$sp), "\n";'); }
$o = new C();
reads($o);
writes($o);
$v = buffer_new<Vec2>(1);
C::$sp = first($v);
live();
"#,
    );
    assert_eq!(out, "nullNnullnull|nullnullnull|integer\n");
}

/// Verifies a `?ptr<T>` that reaches `mixed` through a generated wrapper is PHP null for address
/// 0 and an integer when live: a `Fiber` closure's declared return, an interface method whose
/// implementation takes `mixed` (the source method adapter boxes the argument), and the result
/// of a string callable.
#[test]
fn test_nullable_pointer_values_box_as_null_through_wrappers() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v): ?ptr<Vec2> { return $v[0]; }
function none(): ?ptr { return null; }
interface Taker { public function take(?ptr $p): string; }
class T implements Taker { public function take(mixed $p): string { return gettype($p); } }
function via(Taker $t, ?ptr $p): string { return $t->take($p); }
$v = buffer_new<Vec2>(1);
$f = new Fiber(function (): ?ptr { return null; });
$f->start();
$g = new Fiber(function () use ($v): ?ptr { return first($v); });
$g->start();
echo gettype($f->getReturn()), gettype($g->getReturn()), "|";
echo via(new T(), none()), via(new T(), first($v)), "|";
$name = "none";
echo json_encode($name()), "\n";
"#,
    );
    assert_eq!(out, "NULLinteger|NULLinteger|null\n");
}

/// Verifies a null `?ptr<T>` parameter is PHP null where a function's own frame slots are read
/// back as values: `debug_backtrace()` arguments (plain and `#[\SensitiveParameter]`),
/// `debug_print_backtrace()`, `get_defined_vars()`, `func_get_args()` and a closure capture.
#[test]
fn test_nullable_pointer_parameters_read_as_null_through_introspection() {
    let out = compile_and_run(
        r#"<?php
function none(): ?ptr { return null; }
function bt(?ptr $p): string { return json_encode(debug_backtrace()[0]["args"]); }
function sens(#[\SensitiveParameter] ?ptr $p): string { return json_encode(debug_backtrace()[0]["args"][0]->getValue()); }
function pbt(?ptr $p): void { debug_print_backtrace(); }
function vars(?ptr $p): string { return json_encode(get_defined_vars()); }
function args(?ptr $p): string { return json_encode(func_get_args()); }
function cap(?ptr $p): string { $f = function () use ($p) { return json_encode($p); }; return $f(); }
echo bt(none()), sens(none()), "|";
ob_start();
pbt(none());
$t = ob_get_clean();
echo str_contains($t, "pbt(NULL)") ? "NULL" : $t, "|";
echo vars(none()), args(none()), cap(none()), "\n";
"#,
    );
    assert_eq!(out, "[null]null|NULL|{\"p\":null}[null]null\n");
}

/// Verifies `?ptr<T>` properties read and written by name at run time keep PHP null as address
/// 0: `ReflectionClass` static property values, `ReflectionProperty` reads and writes, a read and
/// a write through a `mixed` receiver, and a dynamic property name (including one that misses).
#[test]
fn test_nullable_pointer_properties_by_runtime_name() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v): ?ptr<Vec2> { return $v[0]; }
class S { public static ?ptr $sp = null; public ?ptr $ip = null; }
function read(mixed $o): string { return json_encode($o->ip); }
function kind(mixed $o): string { return gettype($o->ip); }
function write(mixed $o, mixed $v): void { $o->ip = $v; }
function named(S $o, string $n): string { return json_encode($o->$n); }
$c = new ReflectionClass('S');
echo json_encode($c->getStaticProperties()), json_encode($c->getStaticPropertyValue('sp')), "|";
$o = new S();
$rp = new ReflectionProperty('S', 'ip');
$rs = new ReflectionProperty('S', 'sp');
echo json_encode($rp->getValue($o)), json_encode($rs->getValue()), "|";
echo read($o), named($o, "ip"), named($o, "nope"), "|";
$v = buffer_new<Vec2>(1);
$o->ip = first($v);
echo gettype($rp->getValue($o)), kind($o), "|";
$rp->setValue($o, null);
echo json_encode($o->ip === null), "|";
$o->ip = first($v);
write($o, null);
echo json_encode($o->ip === null), read($o), "\n";
"#,
    );
    assert_eq!(out, "{\"sp\":null}null|nullnull|nullnullnull|integerinteger|true|truenull\n");
}

/// Verifies reflection writes into `?ptr<T>` properties compile and store the pointer word: PHP
/// null and a `mixed` null become address 0, a `mixed` live pointer its address, through
/// `ReflectionProperty::setValue()` and `ReflectionClass::setStaticPropertyValue()` on a static
/// property, and a packed buffer element written into an instance property is its address.
#[test]
fn test_nullable_pointer_properties_written_through_reflection() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
class S { public static ?ptr $sp = null; public ?ptr<Vec2> $ip = null; }
function first(buffer<Vec2> $v): ?ptr<Vec2> { return $v[0]; }
function mixnull(): mixed { return null; }
function mixlive(buffer<Vec2> $v): mixed { return first($v); }
$v = buffer_new<Vec2>(1);
$v[0]->x = 7;
$rs = new ReflectionProperty('S', 'sp');
$c = new ReflectionClass('S');
S::$sp = first($v); $rs->setValue(null, null); echo json_encode(S::$sp === null), "|";
S::$sp = first($v); $c->setStaticPropertyValue('sp', null); echo json_encode(S::$sp === null), "|";
S::$sp = first($v); $rs->setValue(null, mixnull()); echo json_encode(S::$sp === null), "|";
$rs->setValue(null, mixlive($v)); echo gettype(S::$sp), "|";
$c->setStaticPropertyValue('sp', mixnull()); echo json_encode(S::$sp === null), "|";
$o = new S();
(new ReflectionProperty('S', 'ip'))->setValue($o, $v[0]);
$p = $o->ip;
echo $p === null ? "null" : $p->x, "\n";
"#,
    );
    assert_eq!(out, "true|true|true|integer|true|7\n");
}

/// Verifies a `mixed` call result written into a static `?ptr<T>` property through
/// `ReflectionProperty::setValue()` or `ReflectionClass::setStaticPropertyValue()` releases its
/// boxed cell once the address is copied out.
///
/// Creating the reflection objects leaves blocks live on its own, so the writes are measured
/// against the same program without them: they must not add a single live block.
#[test]
fn test_nullable_pointer_static_reflection_writes_release_mixed_sources() {
    let prelude = r#"<?php
packed class Vec2 { public int $x; public int $y; }
class S { public static ?ptr $sp = null; }
function first(buffer<Vec2> $v): ?ptr<Vec2> { return $v[0]; }
function mixnull(): mixed { return null; }
function mixlive(buffer<Vec2> $v): mixed { return first($v); }
$v = buffer_new<Vec2>(1);
$rs = new ReflectionProperty('S', 'sp');
$c = new ReflectionClass('S');
"#;
    let writes = r#"$rs->setValue(null, mixlive($v));
$c->setStaticPropertyValue('sp', mixnull());
$c->setStaticPropertyValue('sp', mixlive($v));
$rs->setValue(null, mixnull());
$rs->setValue(null, mixlive($v));
$c->setStaticPropertyValue('sp', mixnull());
"#;
    let tail = "echo json_encode(S::$sp === null), \"\\n\";\nunset($rs, $c);\n";
    let baseline = compile_and_run_with_heap_debug(&format!("{prelude}{tail}"));
    let written = compile_and_run_with_heap_debug(&format!("{prelude}{writes}{tail}"));
    let summary = |stderr: &str| {
        stderr
            .lines()
            .find(|line| line.contains("HEAP DEBUG: leak summary"))
            .map(str::to_string)
    };
    assert_eq!(written.stdout, "true\n", "{}", written.stderr);
    assert!(summary(&baseline.stderr).is_some(), "{}", baseline.stderr);
    assert_eq!(summary(&written.stderr), summary(&baseline.stderr), "{}", written.stderr);
}

/// Verifies a local assigned `null` and then a packed record address or a `ptr<T>` keeps one
/// pointer word, in a function and at top level, and that a record local later assigned `null`
/// reads as null (task 1af74866: the slot used to widen to boxed `mixed`, and the record load
/// from it failed in codegen).
#[test]
fn test_null_then_record_locals_keep_pointer_word_storage() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v): ptr<Vec2> { return $v[0]; }
function seq(buffer<Vec2> $v): int { $x = null; $x = $v[1]; return $x->x; }
function seq_ptr(buffer<Vec2> $v): int { $x = null; $x = first($v); return $x->x; }
function back_to_null(buffer<Vec2> $v): string { $x = $v[1]; $x = null; return $x === null ? "null" : "set"; }
$v = buffer_new<Vec2>(2);
$v[0]->x = 5;
$v[1]->x = 6;
$x = null;
$x = $v[0];
echo $x->x, "|", seq($v), "|", seq_ptr($v), "|", back_to_null($v), "\n";
"#,
    );
    assert_eq!(out, "5|6|5|null\n");
}

/// Verifies a local holding `null` in one `if` arm and a record address or pointer in the other
/// (in both orders, with an implicit `else`, and at top level) is a `?ptr<T>` after the merge,
/// so `=== null`, `!== null` and `is_null()` test the address and fields read through it.
#[test]
fn test_null_or_record_branches_join_to_nullable_pointer() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v): ptr<Vec2> { return $v[0]; }
function record_or_null(buffer<Vec2> $v, bool $ok): string {
    if ($ok) { $x = $v[1]; } else { $x = null; }
    if ($x === null) { return "null"; }
    return "x" . $x->x;
}
function null_or_record(buffer<Vec2> $v, bool $ok): string {
    if ($ok) { $x = null; } else { $x = $v[1]; }
    if ($x !== null) { return "x" . $x->x; }
    return "null";
}
function null_then_pointer(buffer<Vec2> $v, bool $ok): string {
    $x = null;
    if ($ok) { $x = first($v); }
    if (is_null($x)) { return "null"; }
    return "x" . $x->x;
}
$v = buffer_new<Vec2>(2);
$v[0]->x = 3;
$v[1]->x = 4;
echo record_or_null($v, true), record_or_null($v, false), "|";
echo null_or_record($v, true), null_or_record($v, false), "|";
echo null_then_pointer($v, true), null_then_pointer($v, false), "|";
if ($argc > 5) { $y = null; } else { $y = $v[1]; }
if ($y !== null) { echo "x", $y->x; } else { echo "null"; }
echo "\n";
"#,
    );
    assert_eq!(out, "x4null|nullx4|x3null|x4\n");
}

/// Verifies the record search Ivory writes (`$best = null;` before a loop that stores
/// `$best = $buf[$i];`) keeps the record addresses: `for`, `foreach` and `while` loops in
/// functions and at top level, a loop that finds nothing, and one that never runs, all read
/// `null` or the last stored record after the loop.
#[test]
fn test_null_entering_loops_keep_record_addresses() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function widest(buffer<Vec2> $v): string {
    $best = null;
    for ($i = 0; $i < 4; $i++) {
        if ($best === null || $v[$i]->x > $best->x) { $best = $v[$i]; }
    }
    if ($best !== null) { return "x" . $best->x; }
    return "none";
}
function over(buffer<Vec2> $v, int $limit): string {
    $hit = null;
    foreach ([0, 1, 2, 3] as $i) {
        if ($v[$i]->x > $limit) { $hit = $v[$i]; }
    }
    return $hit === null ? "none" : "x" . $hit->x;
}
function first_over(buffer<Vec2> $v, int $limit): string {
    $hit = null;
    $i = 0;
    while ($hit === null && $i < 4) {
        if ($v[$i]->x > $limit) { $hit = $v[$i]; }
        $i++;
    }
    return isset($hit) ? "x" . $hit->x : "none";
}
function never_runs(buffer<Vec2> $v, int $n): string {
    $hit = null;
    for ($i = 0; $i < $n; $i++) { $hit = $v[$i]; }
    return $hit === null ? "none" : "x" . $hit->x;
}
$v = buffer_new<Vec2>(4);
$v[0]->x = 3;
$v[1]->x = 9;
$v[2]->x = 4;
$v[3]->x = 1;
echo widest($v), "|", over($v, 5), "|", over($v, 20), "|", first_over($v, 3), "|";
echo first_over($v, 20), "|", never_runs($v, 0), "|", never_runs($v, 2), "\n";
$best = null;
for ($i = 0; $i < 4; $i++) {
    if ($best === null || $v[$i]->x < $best->x) { $best = $v[$i]; }
}
if ($best !== null) { echo "min x", $best->x, "\n"; }
"#,
    );
    assert_eq!(out, "x9|x9|none|x9|none|none|x9\nmin x1\n");
}

/// Verifies `=== null`, `is_null()`, `isset()`, `!== null`, `(string)`, concatenation and `echo`
/// on a local that holds `null` or a record address answer by the address after the merge.
#[test]
fn test_null_or_record_locals_answer_null_predicates_and_conversions() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function probe(buffer<Vec2> $v, bool $ok): string {
    $x = null;
    if ($ok) { $x = $v[0]; }
    $tests = "";
    if ($x === null) { $tests .= "n"; } else { $tests .= "s"; }
    $tests .= (is_null($x) ? "n" : "s") . (isset($x) ? "s" : "n") . ($x !== null ? "s" : "n");
    return $tests . ":" . (strlen((string) $x) > 2 ? "addr" : "[" . $x . "]");
}
$v = buffer_new<Vec2>(1);
echo probe($v, false), "|", probe($v, true), "|";
$z = null;
if ($argc > 5) { $z = $v[0]; }
if ($z === null) { echo "[", $z, "]"; }
echo "\n";
"#,
    );
    assert_eq!(out, "nnnn:[]|ssss:addr|[]\n");
}

/// Verifies a local that holds `null` or a record address passes to `?ptr<T>` and (after a null
/// guard) `ptr<T>` parameters, including an extern one, and returns from a `?ptr<T>` function.
#[test]
fn test_null_or_record_locals_pass_and_return_as_pointers() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
extern function memset(ptr<Vec2> $target, int $byte, u64 $size): ptr;
function take(?ptr<Vec2> $p): string { return $p === null ? "null" : "x" . $p->x; }
function field(ptr<Vec2> $p): int { return $p->x; }
function back(buffer<Vec2> $v, bool $ok): ?ptr<Vec2> {
    $x = null;
    if ($ok) { $x = $v[1]; }
    return $x;
}
function pass(buffer<Vec2> $v, bool $ok): string {
    $x = null;
    if ($ok) { $x = $v[1]; }
    $out = take($x);
    if ($x !== null) { $out .= ":" . field($x); }
    return $out;
}
function clear(buffer<Vec2> $v): void {
    $x = null;
    $x = $v[0];
    memset($x, 0, 16);
}
function clear_guarded(buffer<Vec2> $v, bool $ok): void {
    $x = null;
    if ($ok) { $x = $v[1]; }
    if ($x !== null) { memset($x, 0, 16); }
}
$v = buffer_new<Vec2>(2);
$v[0]->x = 3;
$v[1]->x = 4;
echo take(back($v, true)), "|", take(back($v, false)), "|", pass($v, true), "|", pass($v, false), "|";
clear($v);
clear_guarded($v, false);
echo $v[0]->x, ":", $v[1]->x, "|";
clear_guarded($v, true);
echo $v[1]->x, "\n";
"#,
    );
    assert_eq!(out, "x4|null|x4:4|null|0:4|0\n");
}

/// Verifies a by-value capture of a `null` local that the closure body assigns pointers in a
/// loop starts as address 0. The capture is a hidden `null` parameter with no ABI value, so a
/// call whose loop never runs, made right after one that stored a pointer in the same frame,
/// must still read PHP null rather than the stale slot.
#[test]
fn test_null_capture_assigned_pointers_in_a_loop_starts_null() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v): ptr<Vec2> { return $v[0]; }
$v = buffer_new<Vec2>(1);
$v[0]->x = 7;
$c = null;
$f = function (buffer<Vec2> $v, int $n) use ($c): string {
    for ($i = 0; $i < $n; $i++) { $c = first($v); }
    return $c === null ? "null" : "x" . $c->x;
};
echo $f($v, 1), "|", $f($v, 0), "|", $f($v, 2), "|", $f($v, 0), "\n";
"#,
    );
    assert_eq!(out, "x7|null|x7|null\n");
}

/// Verifies a local that a reference aliases keeps reading PHP null when it is also assigned
/// pointers: after a zero-trip loop that starts from `null`, and after an `if` whose taken arm
/// stores `null`, through the local and through the alias. The pointer-word retype applies only
/// to ordinary frame slots, because the reference cell keeps the `null` it was given.
#[test]
fn test_reference_aliased_null_locals_keep_reading_null() {
    let out = compile_and_run(
        r#"<?php
packed class Vec2 { public int $x; public int $y; }
function first(buffer<Vec2> $v): ptr<Vec2> { return $v[0]; }
function probe(buffer<Vec2> $v, int $n): string {
    $p = null;
    $alias = &$p;
    $p = null;
    for ($i = 0; $i < $n; $i++) { $p = first($v); }
    return (is_null($p) ? "null" : "set") . (is_null($alias) ? ":null" : ":set");
}
function branch(buffer<Vec2> $v, bool $ok): string {
    $p = null;
    $alias = &$p;
    if ($ok) { $p = first($v); } else { $p = null; }
    if ($p === null) { return "null"; }
    return "set";
}
$v = buffer_new<Vec2>(1);
echo probe($v, 0), "|", branch($v, false), "\n";
"#,
    );
    assert_eq!(out, "null:null|null\n");
}
