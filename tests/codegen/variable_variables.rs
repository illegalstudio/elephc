//! Purpose:
//! End-to-end coverage for variable variables (`$$name`, `${expr}`) and literal-name
//! `compact()` / `extract()`, desugared over the frame's compile-time locals.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Fixtures are the shapes vendor code uses (Carbon `CarbonInterval`, `Date::set`,
//!   `Creator::createSafe`, `Localization::translateTimeString`; Laravel `compact()` and
//!   `extract(EXTR_SKIP)`), plus the explicit runtime `Error`s for names elephc cannot write.

use crate::support::*;

/// `$value = $$unit` over a literal list of parameter names (CarbonInterval.php:561).
#[test]
fn test_varvar_read_params_from_literal_foreach() {
    let out = compile_and_run(
        r#"<?php
function f($years = null, $months = null, $days = null) {
    foreach (['years', 'months', 'days'] as $unit) {
        $value = $$unit;
        echo $unit, '=', var_export($value, true), ';';
    }
}
f(1, 'x', 4);
f(null, -2, null);
"#,
    );
    assert_eq!(out, "years=1;months='x';days=4;years=NULL;months=-2;days=NULL;");
}

/// A read through a name the frame does not hold warns "Undefined variable" and yields null.
#[test]
fn test_varvar_read_unknown_name_warns_and_yields_null() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $no, string $message): bool { echo "[$message]"; return true; });
function f(string $n) { $known = 'v'; return $$n; }
var_dump(f('known'), f('nope'));
"#,
    );
    assert_eq!(out, "[Undefined variable $nope]string(1) \"v\"\nNULL\n");
}

/// `$$name = ...` inside a `switch ($name)` case group writes the named local (Date.php:1451).
#[test]
fn test_varvar_write_narrowed_by_switch_case_group() {
    let out = compile_and_run(
        r#"<?php
function set(string $name, int $v): string {
    [$year, $month, $day] = [2024, 5, 17];
    switch ($name) {
        case 'year':
        case 'month':
        case 'day':
            $$name = $v;
            break;
    }
    return "$year-$month-$day";
}
echo set('year', 2030), ' ', set('month', 11), ' ', set('day', 3), ' ', set('week', 1);
"#,
    );
    assert_eq!(out, "2030-5-17 2024-11-17 2024-5-3 2024-5-17");
}

/// A write to a name the function never spells throws an explicit Error instead of PHP's
/// implicit new local.
#[test]
fn test_varvar_write_unknown_name_throws_error() {
    let out = compile_and_run(
        r#"<?php
function w(string $n) { $a = 0; $$n = 5; return $a; }
echo w('a'), "\n";
try { w('fresh'); } catch (Error $e) { echo $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(
        out,
        "5\nVariable variable $fresh: creating a new local through a variable variable is not supported (elephc); only locals the function names at compile time can be written\n"
    );
}

/// An unbounded write never retypes a parameter: naming one throws instead.
#[test]
fn test_varvar_unbounded_write_to_parameter_throws_error() {
    let out = compile_and_run(
        r#"<?php
function w(string $n, int $p) { $a = 0; $$n = 7; return "$a/$p"; }
echo w('a', 1), "\n";
try { w('p', 1); } catch (Error $e) { echo substr($e->getMessage(), 0, 53), "\n"; }
"#,
    );
    assert_eq!(out, "7/1\nVariable variable $p: this local is outside the names\n");
}

/// `foreach ($$name as &$v)` iterates the named local itself (Localization.php:287).
#[test]
fn test_varvar_foreach_by_reference() {
    let out = compile_and_run(
        r#"<?php
function f(): string {
    $months = ['a', 'b'];
    $weekdays = ['c'];
    foreach (['months', 'weekdays'] as $variable) {
        foreach ($$variable as $i => &$name) {
            $name .= $i;
        }
        unset($name);
    }
    return implode(',', $months) . '|' . implode(',', $weekdays);
}
echo f();
"#,
    );
    assert_eq!(out, "a0,b1|c0");
}

/// `$$key . 'Translations'` writes resolve through string-valued definitions (Localization.php:303).
#[test]
fn test_varvar_write_through_concatenated_name() {
    let out = compile_and_run(
        r#"<?php
function f(): string {
    $fromTranslations = [];
    $toTranslations = [];
    foreach (['from', 'to'] as $key) {
        $translationKey = $key . 'Translations';
        $$translationKey = [$key, strlen($key)];
    }
    return json_encode([$fromTranslations, $toTranslations]);
}
echo f();
"#,
    );
    assert_eq!(out, r#"[["from",4],["to",2]]"#);
}

/// isset/empty/??/unset through `$$name`, on null, zero, array and unknown locals.
#[test]
fn test_varvar_probes_and_unset() {
    let out = compile_and_run(
        r#"<?php
function p(string $k): string {
    $a = ['x' => 1]; $n = null; $z = 0;
    return (isset($$k) ? 'I' : 'i') . (isset($$k['x']) ? 'X' : 'x') . (empty($$k) ? 'E' : 'e')
        . json_encode($$k ?? 'd');
}
function u(string $k): string { $i = 1; $s = 's'; unset($$k); return var_export(isset($i), true) . var_export(isset($s), true); }
foreach (['a', 'n', 'z', 'q'] as $k) { echo p($k), ' '; }
echo u('i'), ' ', u('s'), ' ', u('q');
"#,
    );
    assert_eq!(out, r#"IXe{"x":1} ixE"d" IxE0 ixE"d" falsetrue truefalse truetrue"#);
}

/// Compound, increment, dimension and property writes through `$$name`.
#[test]
fn test_varvar_compound_dimension_property_writes() {
    let out = compile_and_run(
        r#"<?php
function w(string $k): string {
    $list = ['a' => 1]; $obj = new stdClass(); $count = 5; $text = 'x';
    if ($k === 'list') { $$k['b'] = 2; $$k['a'] += 10; return json_encode($list); }
    if ($k === 'obj') { $$k->p = 7; return json_encode($obj); }
    if ($k === 'count') { $$k++; $$k *= 2; return (string) $count; }
    if ($k === 'text') { $$k .= 'yz'; }
    return $text;
}
echo w('list'), w('obj'), w('count'), w('text');
"#,
    );
    assert_eq!(out, r#"{"a":11,"b":2}{"p":7}12xyz"#);
}

/// `$r = &$$name` binds the named local; the desugared sequence is spliced into the block so
/// reference exposure stays visible to later passes.
#[test]
fn test_varvar_reference_binding() {
    let out = compile_and_run(
        r#"<?php
function b(string $n): string { $left = 'L'; $right = 'R'; $r = &$$n; $r = strtolower($r); return $left . $right; }
echo b('left'), ' ', b('right');
"#,
    );
    assert_eq!(out, "lR Lr");
}

/// Literal names, nested `$$$name`, and `$this` through a variable variable.
#[test]
fn test_varvar_literal_nested_and_this() {
    let out = compile_and_run(
        r#"<?php
final class C {
    public function m(string $n): bool { return $$n === $this; }
}
function f(): string {
    $ab = 'AB'; $ptr = 'ab'; $ptrptr = 'ptr';
    $out = ${'ab'} . ${'a' . 'b'} . $$ptr . $$$ptrptr;
    ${'ab'} = 'x';
    return $out . $ab;
}
echo f(), ' ', var_export((new C())->m('this'), true), var_export((new C())->m('n'), true);
"#,
    );
    assert_eq!(out, "ABABABABx truefalse");
}

/// A read whose name has side effects evaluates it exactly once.
#[test]
fn test_varvar_read_name_with_side_effects_evaluates_once() {
    let out = compile_and_run(
        r#"<?php
function g(): string { echo 'g'; return 'a'; }
function f(): int { $a = 41; return ${g()} + 1; }
echo f();
"#,
    );
    assert_eq!(out, "g42");
}

/// A bounded read over parameters of one declared type keeps that type (no boxing).
#[test]
fn test_varvar_typed_parameter_read() {
    let out = compile_and_run(
        r#"<?php
function t(?string $from, ?string $to): string {
    $out = '';
    foreach (['from', 'to'] as $key) {
        $language = $$key;
        $out .= strtoupper($language ?? 'none') . ';';
    }
    return $out;
}
echo t('en', null);
"#,
    );
    assert_eq!(out, "EN;NONE;");
}

/// Literal-name compact() keeps argument order, includes null locals, and warns for names the
/// frame never spells (Builder.php:1326).
#[test]
fn test_compact_literal_names() {
    let out = compile_and_run(
        r#"<?php
set_error_handler(function (int $no, string $message): bool { echo "[$message]"; return true; });
function w($column, $value, $boolean = 'and') { $type = 'Basic'; return compact('type', ['column', 'value'], 'boolean', 'missing'); }
echo json_encode(w('email', null));
"#,
    );
    assert_eq!(
        out,
        r#"[compact(): Undefined variable $missing]{"type":"Basic","column":"email","value":null,"boolean":"and"}"#
    );
}

/// extract(EXTR_SKIP) before any local, and default-mode extract (PostgresConnector.php:64).
#[test]
fn test_extract_skip_and_overwrite() {
    let out = compile_and_run(
        r#"<?php
function dsn(array $config): string {
    extract($config, EXTR_SKIP);
    $host = isset($host) ? "host={$host};" : '';
    $port = $port ?? 'none';
    return "{$host}port={$port}";
}
function ow(array $vars): string { $a = 'old'; $b = 'keep'; extract($vars); return "$a $b"; }
echo dsn(['host' => 'db', 'port' => 5432, 'config' => 'x']), ' ', dsn([]), ' ', ow(['a' => 'new', 'zz' => 1]);
"#,
    );
    assert_eq!(out, "host=db;port=5432 port=none new keep");
}

/// extract(EXTR_SKIP) inside a closure keeps its `use` captures, by value and by reference,
/// like Filesystem::getRequire; EXTR_OVERWRITE writes through `use (&$y)`.
#[test]
fn test_extract_skip_keeps_closure_captures() {
    let out = compile_and_run(
        r#"<?php
class Box {
    public function getRequire($path, array $data = []) {
        $__path = $path;
        $__data = $data;
        return (static function () use ($__path, $__data) {
            extract($__data, EXTR_SKIP);
            return $__path . '|' . ($name ?? '-');
        })();
    }
}
echo (new Box)->getRequire('a.php', ['__path' => 'b.php', 'name' => 'ana']), ' ';
echo (new Box)->getRequire('a.php', ['__data' => 'x']), ' ';
$x = 1; $y = null;
$c = function () use ($x, &$y) {
    extract(['x' => 9, 'y' => 8, 'z' => 7], EXTR_SKIP);
    echo json_encode([$x, $y, $z]), ' ';
    extract(['x' => 5, 'y' => 6]);
    echo json_encode([$x, $y]), ' ';
};
$c();
echo json_encode([$x, $y]);
"#,
    );
    assert_eq!(out, "a.php|ana a.php|- [1,null,7] [5,6] [1,6]");
}

/// EXTR_SKIP keeps null parameters, ignores a `this` key and invalid names; EXTR_OVERWRITE
/// writes the keys before `this` in array order and then throws.
#[test]
fn test_extract_skip_this_and_overwrite_order() {
    let out = compile_and_run(
        r#"<?php
class M {
    public function m($p) {
        extract(['p' => 'no', 'this' => 1, 'q' => 'si', '1x' => 4, '' => 5, 0 => 6], EXTR_SKIP);
        echo json_encode([$p, $q, $this instanceof M]), ' ';
        try { extract(['z' => 1, 'this' => 2, 'w' => 3]); } catch (Error $e) { echo $e->getMessage(), ' '; }
        echo json_encode([isset($z), isset($w)]), ' ';
    }
}
function f($a, $b = null) { extract(['a' => 1, 'b' => 2, 'c' => 3], EXTR_SKIP); echo json_encode([$a, $b, $c]); }
(new M)->m(null);
f(null);
"#,
    );
    assert_eq!(out, "[null,\"si\",true] Cannot re-assign $this [true,false] [null,null,3]");
}

/// Default-mode extract() of a list keeps typed locals: the `this` check stores no loop value
/// into them.
#[test]
fn test_extract_overwrite_list_keeps_typed_locals() {
    let out = compile_and_run(
        r#"<?php
function f() { $a = 'k'; $rows = [1, 2]; extract($rows); return $a . count($rows); }
function g() { $a = 'k'; extract([1, 2]); return $a; }
function h($d) { $a = 'k'; extract($d); return $a; }
function i($d) { extract($d); return 'ok' . count($d); }
echo f(), g(), h([1, 2, 3]), i([1, 2, 3]);
"#,
    );
    assert_eq!(out, "k2kkok3");
}

/// An unset() after extract() (of an extracted name or of another local) does not block it.
#[test]
fn test_extract_then_later_unset() {
    let out = compile_and_run(
        r#"<?php
function f(array $d) { extract($d); $o = ($a ?? '-'); unset($a); return $o . ($a ?? '-') . ($b ?? '-'); }
function g(array $d) { $tmp = 1; extract($d); unset($tmp); return ($a ?? '-') . ($tmp ?? '-'); }
echo f(['a' => 1, 'b' => 2]), f([]), ' ', g(['a' => 1]), g(['tmp' => 5]);
"#,
    );
    assert_eq!(out, "1-2--- 1---");
}

/// Destructuring targets that are variable variables write the named locals.
#[test]
fn test_varvar_destructuring_targets() {
    let out = compile_and_run(
        r#"<?php
function f(): string {
    $a = 0; $b = 0; $x = 'a'; $y = 'b';
    [$$x, $$y] = [1, 2];
    $out = "$a$b";
    ['k' => $$x] = ['k' => 7];
    list($$x, $b) = [8, 9];
    $out .= "$a$b";
    foreach ([[3, 4]] as [$$x, $b]) {}
    return $out . "$a$b";
}
echo f();
"#,
    );
    assert_eq!(out, "128934");
}

/// extract() with a 'this' key throws before writing.
#[test]
fn test_extract_this_key_throws() {
    let out = compile_and_run(
        r#"<?php
function f(array $d): string { $a = 0; try { extract($d); } catch (Error $e) { return $e->getMessage() . $a; } return "ok$a"; }
echo f(['this' => 1, 'a' => 5]), ' ', f(['a' => 6]);
"#,
    );
    assert_eq!(out, "Cannot re-assign $this0 ok6");
}

/// A dispatched write followed by `return [...]` compiles, and `$$n = v` with
/// `$n === 'this'` raises PHP's own error.
#[test]
fn test_varvar_write_then_return_array_and_this() {
    let out = compile_and_run(
        r#"<?php
function w(string $n): array { $a = 1; $b = 2; $$n = 5; return [$a, $b]; }
function t(string $n): string { $a = 1; try { $$n = 2; } catch (Error $e) { return $e->getMessage(); } return 'no'; }
echo json_encode(w('b')), ' ', t('this');
"#,
    );
    assert_eq!(out, "[1,5] Cannot re-assign $this");
}

/// A switch case that rewrites the subject and falls through, and `$$n` renaming `$n`
/// itself, no longer trust a stale bound.
#[test]
fn test_varvar_bounds_invalidated_by_rewrites() {
    let out = compile_and_run(
        r#"<?php
function fall(string $u): string {
    $a = 1; $b = 2; $c = 3;
    switch ($u) {
        case 'a':
            $u = 'c';
        case 'b':
            $$u = 100;
            break;
    }
    return "$a $b $c";
}
function self_rename(): string { $a = 'x'; $n = 'n'; $$n = 'a'; $$n = $$n . 'y'; return "$n $a"; }
echo fall('a'), '|', fall('b'), '|', self_rename();
"#,
    );
    assert_eq!(out, "1 2 100|1 100 3|a xy");
}
