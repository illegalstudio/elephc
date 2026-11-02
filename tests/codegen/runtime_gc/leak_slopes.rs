//! Purpose:
//! Ownership regressions found while bringing the `preg_*` array forms up, each measured as a
//! heap SLOPE: the live blocks after one iteration must equal those after forty.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A fixture's baseline is not always zero -- a class instance or a cached value can outlive
//!   the loop -- so each asserts that what stays live does not GROW with the iteration count.
//! - Expected stdout is `php -n` 8.5's own output at both iteration counts.

use crate::support::{
    compile_and_run_with_heap_debug, compile_and_run_with_heap_debug_and_regex, ProgramOutput,
};

/// Live heap blocks the heap-debug summary reports at exit; `clean` is zero.
fn live_blocks(out: &ProgramOutput) -> usize {
    if out.stderr.contains("HEAP DEBUG: leak summary: clean") {
        return 0;
    }
    let marker = "HEAP DEBUG: leak summary: live_blocks=";
    let start = out
        .stderr
        .find(marker)
        .unwrap_or_else(|| panic!("no leak summary in: {}", out.stderr))
        + marker.len();
    out.stderr[start..]
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|digits| digits.parse().ok())
        .unwrap_or_else(|| panic!("unreadable leak summary in: {}", out.stderr))
}

/// Runs `source` with `__N__` at 1 and at 40 and asserts the extra iterations left nothing live.
fn assert_no_leak_slope(source: &str, once_expected: &str, many_expected: &str, regex: bool) {
    let run = |n: &str| {
        let source = source.replace("__N__", n);
        if regex {
            compile_and_run_with_heap_debug_and_regex(&source)
        } else {
            compile_and_run_with_heap_debug(&source)
        }
    };
    let once = run("1");
    let many = run("40");
    assert_eq!(once.stdout, once_expected, "stderr: {}", once.stderr);
    assert_eq!(many.stdout, many_expected, "stderr: {}", many.stderr);
    assert_eq!(
        live_blocks(&once),
        live_blocks(&many),
        "live blocks grew with the iteration count\nonce: {}\nforty times: {}",
        once.stderr,
        many.stderr
    );
}

/// `return (string) $x;` over a `mixed` or untyped parameter. The alias summary sees the cast as
/// returning `$x`, but a boxed value becomes a string only as a COPY, so the caller kept the box it
/// made at the call boundary -- and, for a variable argument, the result -- alive forever.
#[test]
fn test_string_cast_of_a_mixed_parameter_does_not_leak() {
    assert_no_leak_slope(
        r#"<?php
function asMixed(mixed $x) { return (string) $x; }
function asTyped(mixed $x): string { return (string) $x; }
function untyped($x) { return (string) $x; }
$s = str_repeat('s', 100);
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += strlen(asMixed('abc')) + strlen(asTyped('abc')) + strlen(asMixed(12));
    $total += strlen(untyped('abc')) + strlen(untyped($s));
}
echo $total, "\n";
"#,
        r#"111
"#,
        r#"4440
"#,
        false,
    );
}

/// `$out = []; $out['a'] = 1;` promotes the empty list to a hash, and the promotion must CONSUME
/// the slot's reference; a slot load was not provably owned, so the old array leaked every call.
#[test]
fn test_first_string_key_write_does_not_leak_the_empty_array() {
    assert_no_leak_slope(
        r#"<?php
function build() { $out = []; $out['a'] = 1; $out['b'] = 'x'; return $out; }
function buildString() { $out = []; $out['a'] = 'x'; return $out; }
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += count(build()) + count(buildString());
}
echo $total, "\n";
"#,
        r#"3
"#,
        r#"120
"#,
        false,
    );
}

/// A local holding a string before a loop that stores something else into it is boxed at loop
/// entry; the entry conversion loaded the string as an unboxing COPY and boxed it again, and the
/// first copy leaked every call.
#[test]
fn test_loop_widened_string_local_does_not_leak() {
    assert_no_leak_slope(
        r#"<?php
function mk(int $i) { return $i % 2 ? null : 'z'; }
function wg(string $s, array $ps) { $r = $s; foreach ($ps as $p) { $r = $p; } return 1; }
function wc(array $ps) { $r = 'lit'; foreach ($ps as $p) { $r = mk($p); } return 1; }
function wd(string $s, array $ps) { $r = $s; foreach ($ps as $p) { $r = mk($p); } return $r; }
$s = str_repeat('s', 100);
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += wg($s, [1, 2]) + wc([1, 2]) + strlen((string) wd($s, [1, 2]));
}
echo $total, "\n";
"#,
        r#"3
"#,
        r#"120
"#,
        false,
    );
}

/// `preg_match($p, $s, $m)` into a fresh local of a function boxes the matches array into a
/// `mixed` slot, and boxing RETAINS an array: the call's own reference leaked the whole matches
/// array on every call -- ten blocks for `preg_match_all()`.
#[test]
fn test_matches_into_a_fresh_function_local_do_not_leak() {
    assert_no_leak_slope(
        r#"<?php
function all(string $pattern, string $subject) { preg_match_all($pattern, $subject, $plain); return count($plain); }
function one(string $pattern, string $subject) { preg_match($pattern, $subject, $m); return count($m); }
function flagged(string $subject) { preg_match_all('/(a)(x)?/', $subject, $sets, PREG_SET_ORDER | PREG_OFFSET_CAPTURE); return count($sets); }
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += all('/(a)(x)?/', 'za') + one('/(a)(x)?/', 'za') + flagged('zaza');
}
echo $total, "\n";
"#,
        r#"7
"#,
        r#"280
"#,
        true,
    );
}

/// `preg_replace()` returned its subject ITSELF when the pattern failed, so its result was declared
/// to alias its arguments and none of them was ever released -- a `(string)` pattern copy leaked
/// on every call, failing or not.
#[test]
fn test_replacement_with_a_variable_pattern_releases_its_arguments() {
    assert_no_leak_slope(
        r#"<?php
function viaMixed(mixed $p) { return preg_replace((string) $p, 'x', 'bcd'); }
function viaTyped(string $p, string $s) { $r = preg_replace($p, 'x', $s); return $r; }
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += strlen(viaMixed('/a/')) + strlen(viaTyped('/b/', 'abc')) + (int) is_null(@viaTyped('/(/', 'abc'));
}
echo $total, "\n";
"#,
        r#"7
"#,
        r#"280
"#,
        true,
    );
}

/// `$out[$key] = (string) $value` stores a string by copying it, locally or into a property hash,
/// so the cast's own copy is the write's to drop; neither path dropped it.
#[test]
fn test_cast_values_written_under_mixed_keys_do_not_leak() {
    assert_no_leak_slope(
        r#"<?php
class Holder { public array $a = []; public function put($k, $v) { $this->a[$k] = (string) $v; } }
function copyAll(mixed $subject) { $out = []; foreach ($subject as $key => $value) { $out[$key] = (string) $value; } return $out; }
$h = new Holder;
$subjects = ['a' => 'apple', 'b' => 'berry'];
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += count(copyAll($subjects));
    $h->put($i % 3 ? 'k' : 1, 'v' . $i);
    $total += count($h->a);
}
echo $total, "\n";
"#,
        r#"3
"#,
        r#"159
"#,
        false,
    );
}

/// A string a CALL returns is taken over by the Mixed box it is written into -- only a copied
/// string (the cast above) is the write's to release. Releasing the call's string as well freed
/// what the container now held: `$out[$i] = label($raw, $i)` read back as garbage bytes, and
/// PDO's `FETCH_NAMED` rows came back with corrupted column names. `label` recurses so the
/// inliner leaves it a real call.
#[test]
fn test_call_results_written_under_mixed_keys_stay_owned_by_the_array() {
    assert_no_leak_slope(
        r#"<?php
class Holder { public array $a = []; public function name(int $i): string { return strtoupper(chr(97 + $i) . "x"); } public function put(int $n) { for ($i = 0; $i < $n; $i++) { $this->a[$i] = $this->name($i); } } }
function label(array $raw, int $i): string { if ($i < 0) { return label($raw, 0); } return strtoupper($raw[$i]); }
function names(array $raw) { $out = []; for ($i = 0; $i < count($raw); $i++) { $out[$i] = label($raw, $i); } return $out; }
$h = new Holder;
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $row = names(['ab', 'cd', 'ef']);
    $total += strlen(implode('', $row));
    $h->put(3);
}
echo $total, ' ', implode(',', names(['x', 'y'])), ' ', implode(',', $h->a), "\n";
"#,
        r#"6 X,Y AX,BX,CX
"#,
        r#"240 X,Y AX,BX,CX
"#,
        false,
    );
}

/// References taken inside a loop body are lowered once and run every iteration. Each rebinding
/// must drop the share the previous iteration took (`$r = &$v`, the by-reference `foreach` that
/// shares its elements), and a local already promoted to a cell must not be re-boxed or have its
/// cell's value overwritten without a release (`$a[$i] = &$v`, `[&$v]`). Every one of these leaked
/// one block per extra iteration.
#[test]
fn test_references_taken_in_loops_do_not_leak() {
    assert_no_leak_slope(
        r#"<?php
function aliases() { $sum = 0; for ($i = 0; $i < 3; $i++) { $v = $i; $r = &$v; $r = $r + 1; $sum += $v; } return $sum; }
function elements() { $a = [0, 0, 0]; for ($i = 0; $i < 3; $i++) { $v = $i; $a[$i] = &$v; } return array_sum($a); }
function literals() { $pool = []; foreach ([1, 2, 3] as $k) { $v = $k; $pool[] = [&$v]; } return count($pool) + $pool[0][0]; }
function shared() { $s = []; $vars = ['x' => 1, 'y' => 2]; foreach ($vars as $name => &$value) { $s[$name] = &$value; } unset($value); return $vars['x'] + $vars['y'] + count($s); }
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += aliases() + elements() + literals() + shared();
}
echo $total, "\n";
"#,
        r#"23
"#,
        r#"920
"#,
        false,
    );
}

/// The eval bridge's owner lookup reads an array callable's first element and never released the
/// key or the element, so every `is_callable([$object, 'method'])` on a `mixed` value leaked two
/// blocks in a program that links the bridge.
#[test]
fn test_is_callable_on_an_array_callable_does_not_leak() {
    assert_no_leak_slope(
        r#"<?php
class K { public function low($m) { return 1; } public static function st($m) { return 1; } }
function check(mixed $cb) { return is_callable($cb) ? 1 : 0; }
$k = new K;
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += check([$k, 'low']) + check([$k, 'nope']) + check(['K', 'st']) + check([1, 2]);
}
eval('$unused = 1;');
echo $total, "\n";
"#,
        r#"2
"#,
        r#"80
"#,
        false,
    );
}

/// Every array and flag form, run in a loop, keeps its heap flat.
#[test]
fn test_preg_array_forms_do_not_leak() {
    assert_no_leak_slope(
        r#"<?php
class Lower { public function low($m) { return strtolower($m[0]); } }
function viaPattern($cb, $p) { return preg_replace_callback($p, $cb, 'APPLE pie'); }
$subjects = ['a' => 'apple', 'b' => 'berry', 'c' => 'cherry'];
$k = new Lower;
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += count(preg_replace('/e/', 'E', $subjects));
    $total += strlen(preg_replace(['/a/', '/e/'], ['A', 'E'], 'apple pie'));
    $total += count(preg_replace_callback('/r+/', fn($m) => strtoupper($m[0]), $subjects, -1, $n)) + $n;
    $total += strlen(preg_replace_callback('/(a)(x)?/', fn($m) => json_encode($m), 'za', -1, $c, PREG_OFFSET_CAPTURE));
    $total += strlen(preg_replace_callback('/(a)(x)?/', fn($m) => json_encode($m), 'zaza', -1, $c, PREG_UNMATCHED_AS_NULL));
    $total += count(preg_filter(['/a/', '/b/'], 'x', ['aa', 'cc', 'bb', 'k' => 'ab'], 1, $c)) + $c;
    $total += strlen((string) preg_filter('/a/', 'x', 'bcd'));
    $total += count(preg_replace_callback_array(['/a/' => fn($m) => 'A', '/b/' => [$k, 'low']], ['x' => 'ab', 'y' => 'cc'], 1, $c));
    $total += strlen(viaPattern([$k, 'low'], '/P/')) + strlen(viaPattern(fn($m) => '#', '/e/'));
    $total += count(@preg_replace(['/a/', '/(/'], 'x', ['k' => 'ab', 'j' => 'cd'], -1, $c)) + $c;
}
echo $total, "\n";
"#,
        r#"93
"#,
        r#"3720
"#,
        true,
    );
}

/// An element write into a local whose frame slot is BOXED -- because another branch assigns it a
/// `mixed` value -- borrows an unboxed array with its own reference and stores a new box. The old
/// box was never dropped, so `$a = []; $a[0] = 'x';` leaked two blocks every call.
#[test]
fn test_element_write_into_a_boxed_local_does_not_leak() {
    assert_no_leak_slope(
        r#"<?php
function build(mixed $row, int $flags) { if ($flags & 256) { $a = $row; } else { $a = []; $a[0] = 'x'; $a[1] = 'y'; } return count($a); }
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += build([1], 512) + build([1, 2, 3], 256);
}
echo $total, "\n";
"#,
        r#"5
"#,
        r#"200
"#,
        false,
    );
}

/// Every way of invoking a callable that the compiler builds a temporary for. The invoker only
/// borrows its callback, and each shape dropped something: the `[$class, 'm']` array behind
/// `$class::m()` (and the original of the Mixed copy a typed callable array is converted to), an
/// immediately invoked closure, the static-method descriptor behind `$a()` on a local, the Mixed
/// cell a scalar-typed invocation is cast from, and -- in the eval bridge -- the comparison
/// result and the answer of `spl_autoload_register()`. Measured before: 62 blocks per call.
#[test]
fn test_callable_invocation_shapes_do_not_leak() {
    assert_no_leak_slope(
        r#"<?php
class K { static function n() { return 4; } function m() { return "y" . mt_rand(1, 1); } static function s() { return "x" . mt_rand(1, 1); } static function load($c) { } }
function shapes() {
    $c = 'K';
    $a = ['K', 's'];
    $l = ['K', 'load'];
    spl_autoload_register($l);
    spl_autoload_register($l);
    spl_autoload_unregister($l);
    return $c::n() . $c::s() . call_user_func([$c, 'n']) . [new K, 'm']() . $a() . call_user_func($a)
        . (function () { return "z" . mt_rand(1, 1); })() . implode(array_map(['K', 's'], [1]));
}
$out = '';
for ($i = 0; $i < __N__; $i++) { $out = shapes(); }
echo $out, "\n";
"#,
        r#"4x14y1x1x1z1x1
"#,
        r#"4x14y1x1x1z1x1
"#,
        false,
    );
}

/// A reflector over a compiled class, and its eval-dispatched getters. Construction boxed the
/// member objects and attribute arrays it had just created with a retaining box, the bridge
/// never released the key and element of each name it read back, and a scalar getter's result
/// cell was cast and dropped. Measured before: 619 blocks per iteration.
#[test]
fn test_reflectors_over_a_compiled_class_do_not_leak() {
    assert_no_leak_slope(
        r#"<?php
class Base {}
interface Marker {}
final class Shape extends Base implements Marker {
    const A = 1.5;
    public $p = 1;
    public $q;
    function m(int $a = 2, $b = 'x') {}
    static function s() {}
}
function reflect() {
    $class = new ReflectionClass('Shape');
    $object = new ReflectionObject(new Shape());
    $method = new ReflectionMethod('Shape', 'm');
    $property = new ReflectionProperty('Shape', 'p');
    return $class->getName() . $class->getShortName() . ($class->isFinal() ? 'F' : '-')
        . ($class->hasMethod('m') ? 'M' : '-') . $class->getConstant('A')
        . $object->getName() . $method->getNumberOfParameters() . $property->getName();
}
$out = '';
for ($i = 0; $i < __N__; $i++) { $out = reflect(); }
echo $out, "\n";
"#,
        r#"ShapeShapeFM1.5Shape2p
"#,
        r#"ShapeShapeFM1.5Shape2p
"#,
        false,
    );
}

/// Calls from compiled code into eval-declared code. The bridge boxes each argument with a
/// reference of its own, so the compiled temporary (`[1, 2, $i]`) was never dropped; and a
/// METHOD activation was torn down without releasing its scope (a function's was), so every
/// method local leaked, and a by-value array parameter's separated copy kept the caller's array
/// alive. Measured before: 8 blocks per iteration.
#[test]
fn test_calls_into_eval_declared_code_do_not_leak() {
    assert_no_leak_slope(
        r#"<?php
class EvalPrelude {}
eval('class Z { static function f($a) { return count($a); } static function h() { $t = 5; $u = [1, 2]; return 1; } } function zf($a) { return count($a); }');
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += Z::f([1, 2, $i]) + zf([1, $i]) + Z::h();
}
echo $total, "\n";
"#,
        r#"6
"#,
        r#"240
"#,
        false,
    );
}

/// Temporaries inside eval-declared code, and the literal forms of `Closure::fromCallable()`.
/// The interpreter never released the literal arguments a builtin evaluated for itself, nor a
/// binary or unary operator's temporary operands (and the zero `-1` is computed against); the
/// bridge's `.` dropped both operands' string casts; and a literal `fromCallable()` reached the
/// bridge, which kept its argument. Measured before: 27 blocks per iteration.
#[test]
fn test_eval_expression_temporaries_do_not_leak() {
    assert_no_leak_slope(
        r#"<?php
class EvalPrelude {}
eval('class Z { static function t($x) { $a = strlen("ab") . strtoupper($x); $b = -1; "a" . "y"; return $a . $b; } }');
class K { function m($v) { return "m$v"; } }
$out = '';
for ($i = 0; $i < __N__; $i++) {
    $out = Z::t("q") . Closure::fromCallable([new K, 'm'])(1) . Closure::fromCallable('strtoupper')("z");
}
echo $out, "\n";
"#,
        r#"2Q-1m1Z
"#,
        r#"2Q-1m1Z
"#,
        false,
    );
}

/// Keyed writes into a local promoted to a hash, whose frame slot is therefore a boxed `mixed`.
/// The load lends the write an unboxed hash holding its own reference; the write dropped the old
/// box and then boxed the result by RETAINING, so that reference was never given back. Every
/// later write saw the hash shared, copied the whole table and leaked the copy it replaced -- set,
/// unset and append alike. Measured before: 9 blocks per call.
#[test]
fn test_hash_writes_into_a_boxed_local_do_not_copy_the_table() {
    assert_no_leak_slope(
        r#"<?php
function build(mixed $k) {
    $h = [];
    $h['a'] = 1;
    $h[$k] = 2;
    $h['c'] = 3;
    unset($h['a']);
    $h[] = 4;
    $h['d'] = 5;
    return count($h);
}
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += build('b' . $i);
}
echo $total, "\n";
"#,
        r#"4
"#,
        r#"160
"#,
        false,
    );
}

/// `isset($h[$k])` read a boxed local's hash with its own reference and never released it, so
/// the write the `isset` guards copied the table and leaked the original. Measured before: 6
/// blocks per call.
#[test]
fn test_isset_before_a_write_does_not_keep_the_hash_shared() {
    assert_no_leak_slope(
        r#"<?php
function build(mixed $k) {
    $h = [];
    $h['a'] = 1;
    if (!isset($h[$k])) { $h[$k] = 2; }
    if (!isset($h['c'])) { $h['c'] = 3; }
    if (!isset($h['a'])) { $h['a'] = 4; }
    return count($h) * 10 + $h['a'];
}
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += build('b' . $i);
}
echo $total, "\n";
"#,
        r#"31
"#,
        r#"1240
"#,
        false,
    );
}

/// A method cache filled in a `foreach`, Twig's `getAttribute()` shape. The loop-entry storage
/// contract loaded the cache before learning it had nothing to convert; that unused load held a
/// reference, so the loop's first write copied the table. Together with the write-back above,
/// this was one table copy per element. Measured before: 26 blocks per call.
#[test]
fn test_method_cache_loop_does_not_copy_the_table_per_element() {
    assert_no_leak_slope(
        r#"<?php
function names(): mixed { return ['getName', 'alpha', 'isValid', 'Zeta', 'getId']; }
function build(mixed $methods): array {
    $cache = [];
    foreach ($methods as $i => $method) {
        $cache[$method] = $method;
        if ($method[0] === 'g') { continue; }
        $cache[strtolower($method)] = $method;
    }
    return $cache;
}
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += count(build(names()));
}
echo $total, "\n";
"#,
        r#"7
"#,
        r#"280
"#,
        false,
    );
}

/// `substr()` answers a view into its subject. Over a `mixed` argument the subject is the copy
/// the string cast made, which the view kept from being released, and releasing the view could
/// not free it past its first byte. Measured before: 1 block per call.
#[test]
fn test_substr_of_a_mixed_argument_does_not_leak_the_cast() {
    assert_no_leak_slope(
        r#"<?php
function tail(mixed $m) { $n = substr($m, 1); return strlen($n); }
function head(mixed $m) { return strlen(substr(strtoupper($m), 0, 2)); }
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += tail('getName' . $i) + head('isValid');
}
echo $total, "\n";
"#,
        r#"9
"#,
        r#"390
"#,
        false,
    );
}

/// A callable array passed to a `callable` parameter is normalized into a fresh bound
/// descriptor that retains its receiver, and the call boundary must release it. The normalized
/// value was left `maybe_owned`, so nobody did: `apply([$obj, 'm'], 7)` leaked the 96-byte
/// descriptor and one reference to `$obj` per call, for a function and a static method alike.
#[test]
fn test_callable_array_argument_descriptor_is_released_after_the_call() {
    assert_no_leak_slope(
        r#"<?php
class LeakSlopeTwice { public function m($v) { return $v * 2; } }
class LeakSlopeSvc { public static function apply(callable $c, $v) { return $c($v); } }
function leak_slope_apply(callable $c, $v) { return $c($v); }
$t = new LeakSlopeTwice();
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += leak_slope_apply([$t, 'm'], 7) + LeakSlopeSvc::apply([$t, 'm'], 1);
}
echo $total, "\n";
"#,
        "16\n",
        "640\n",
        false,
    );
}

/// A `string` parameter that a call to an unknown callable may bind by reference gets boxed
/// frame storage, and a read of it lowered BEFORE that call was a borrowed load the IR never
/// releases -- while codegen read the box through `__rt_mixed_cast_string`, which copies a string
/// payload. Laravel Prompts' key handler leaked one string per keystroke on each of its paths:
/// `$key[0]` before `$ignore($key)`, and `... . $key . ...` through the representation check.
#[test]
fn test_reads_of_a_promotable_string_parameter_do_not_copy_it() {
    assert_no_leak_slope(
        r#"<?php
function leak_slope_key(string $key, ?Closure $ignore): int {
    $first = $key[0];
    if ($ignore !== null) {
        return $ignore($key) ? 1 : 0;
    }
    return strlen($first . $key . $first);
}
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += leak_slope_key(str_repeat("k", 3), null);
}
echo $total, "\n";
"#,
        "5\n",
        "200\n",
        false,
    );
}

/// `unset($trace[$i]['args'])` over a `debug_backtrace()` result rewrote the frame as a fresh
/// boxed hash and stored it back with `array_set`, which RETAINS an already-boxed value; the
/// nested-unset lowering still counted the box as consumed and never released it, so every call
/// leaked the whole frame (its hash, strings and argument array). Symfony's
/// `ErrorHandler::cleanTrace()` runs it for each deprecation.
#[test]
fn test_nested_unset_over_a_backtrace_frame_releases_the_rewritten_frame() {
    assert_no_leak_slope(
        r#"<?php
function leak_slope_trace(int $n): int {
    $trace = debug_backtrace(DEBUG_BACKTRACE_PROVIDE_OBJECT);
    for ($i = 0; isset($trace[$i]); ++$i) {
        unset($trace[$i]['args'], $trace[$i]['object']);
    }
    return count($trace) + $n;
}
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += leak_slope_trace(0);
}
echo $total, "\n";
"#,
        "1\n",
        "40\n",
        false,
    );
}

/// A Mixed value stored through a `string &` parameter (`$file = $trace[$i]['file']`) was cast
/// with `__rt_mixed_cast_string`, which persists a string payload into a fresh copy, but the box
/// the store consumes was never released: the box and its string leaked per assignment.
#[test]
fn test_mixed_value_stored_through_a_string_reference_releases_its_box() {
    assert_no_leak_slope(
        r#"<?php
function leak_slope_set(array $t, string &$file): void { $file = $t[0]['file']; }
function leak_slope_frame(string $file): int {
    leak_slope_set([['file' => str_repeat('f', 3), 'line' => 3]], $file);
    return strlen($file);
}
$total = 0;
for ($i = 0; $i < __N__; $i++) {
    $total += leak_slope_frame("in");
}
echo $total, "\n";
"#,
        "3\n",
        "120\n",
        false,
    );
}

/// An exception subclass with its own constructor is allocated by the generic object path,
/// which seeds `$previous` with a boxed null; the inlined `parent::__construct()` overwrote that
/// slot without releasing it, one box per exception. A previous exception passed through the
/// subclass must still be retained and released with its owner.
#[test]
fn test_exception_subclass_parent_constructor_releases_the_default_previous() {
    assert_no_leak_slope(
        r#"<?php
class E extends Exception {
    public function __construct(string $m, ?Throwable $p = null) { parent::__construct($m, 0, $p); }
}
class F extends Exception {
    public function __construct() { parent::__construct("f"); }
}
$n = 0;
for ($i = 0; $i < __N__; $i++) {
    $e = new E("x$i");
    $w = new E("w$i", new F());
    $n += strlen($e->getMessage()) + ($w->getPrevious() instanceof F ? 1 : 0);
}
echo $n, "\n";
"#,
        r#"3
"#,
        r#"150
"#,
        false,
    );
}

/// Carbon's shape: a trait's untyped `add($unit, ...)` overrides `DateTime::add(DateInterval)`
/// and is called both directly with strings and through a `DateTime`-typed site. The override
/// once adopted the builtin's `DateInterval` storage (refusing the string calls), the prelude
/// body `parent::add()` reaches was never emitted, and `return parent::add(...)` through the
/// `static` return guard leaked the object.
#[test]
fn test_untyped_override_of_a_builtin_datetime_method_compiles_and_releases() {
    assert_no_leak_slope(
        r#"<?php
trait Units {
    public function add($unit, $value = 1): static {
        if ($unit instanceof DateInterval) {
            return parent::add($unit);
        }
        return parent::add(new DateInterval('P' . $value . strtoupper(substr($unit, 0, 1))));
    }
}
class C extends DateTime { use Units; }
function viaBase(DateTime $d): int {
    return (int) $d->add(new DateInterval('P2D'))->format('d');
}
$n = 0;
for ($i = 0; $i < __N__; $i++) {
    $c = new C('2024-01-01');
    $c->add('day', 3);
    $n += (int) $c->format('d') + viaBase($c);
}
echo $n, "\n";
"#,
        r#"10
"#,
        r#"400
"#,
        false,
    );
}

/// `$this->tags[$name][] = $attributes` on a bare `array` property (Symfony's
/// `Definition::addTag`). The property is stored as a boxed `mixed`, so the nested-append lowering
/// falls back to the generic read/push/write-back desugar, but its key probe had already read the
/// property: that owned box was never released, and with it the whole tag table it held. Under
/// Symfony's container passes the extra share later let an unrelated teardown free a table that
/// a live definition still pointed to, and `lint:container` crashed.
#[test]
fn test_nested_append_on_a_mixed_array_property_does_not_leak() {
    assert_no_leak_slope(
        r#"<?php
class Def {
    private array $tags = [];
    public function setTags(array $tags): static { $this->tags = $tags; return $this; }
    public function addTag(string $name, array $attributes = []): static { $this->tags[$name][] = $attributes; return $this; }
    public function count(): int { return count($this->tags); }
}
function run(int $i): int {
    $d = new Def();
    $d->setTags(['k' => [['e' => 'x' . $i]]]);
    $d->addTag('k', ['a' => 1]);
    $d->addTag('p');
    return $d->count();
}
$total = 0;
for ($i = 0; $i < __N__; $i++) { $total += run($i); }
echo $total, "\n";
"#,
        r#"2
"#,
        r#"80
"#,
        false,
    );
}

