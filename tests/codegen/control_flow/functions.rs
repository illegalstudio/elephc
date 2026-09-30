//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of control flow functions, including function call integer, function call string, and function void.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use super::*;

/// Compiles a function returning the sum of two integers and verifies the result.
#[test]
fn test_function_call_int() {
    let out = compile_and_run("<?php function add($a, $b) { return $a + $b; } echo add(10, 32);");
    assert_eq!(out, "42");
}

/// Compiles a function returning a concatenated string and verifies the output.
#[test]
fn test_function_call_string() {
    let out = compile_and_run(
        "<?php function greet($name) { return \"Hello, \" . $name; } echo greet(\"World\");",
    );
    assert_eq!(out, "Hello, World");
}

/// Verifies that string concatenation inside a function return is preserved when
/// the returned value is used in further concatenation operations.
#[test]
fn test_function_returned_concat_survives_outer_concat() {
    let out = compile_and_run(
        r#"<?php
function label($name) { return "[" . $name . "]"; }
echo label("title") . "|" . label("slug");
"#,
    );
    assert_eq!(out, "[title]|[slug]");
}

/// Verifies that a function returning a builtin-produced string persists it
/// before the caller starts a new concat expression.
#[test]
fn test_function_returned_builtin_string_survives_caller_concat() {
    let out = compile_and_run(
        r#"<?php
function query_name(): string {
    return urldecode(substr("name=elephc", 5));
}

$name = query_name();
echo $name . "\n";
echo "Hello, " . $name . "!\n";
echo "Hello, " . query_name() . "!\n";
"#,
    );
    assert_eq!(out, "elephc\nHello, elephc!\nHello, elephc!\n");
}

/// Compiles a void function that echoes a value and returns early, then verifies
/// the side effect occurs correctly when the function is called as a statement.
#[test]
fn test_function_void() {
    let out = compile_and_run("<?php function say() { echo \"hi\"; return; } say();");
    assert_eq!(out, "hi");
}

/// Verifies that variables inside a function body do not leak to the outer scope,
/// and that the global variable remains unchanged after the function call.
#[test]
fn test_function_local_scope() {
    let out = compile_and_run(
        "<?php $x = 1; function get_two() { $x = 2; return $x; } echo $x . \" \" . get_two();",
    );
    assert_eq!(out, "1 2");
}

/// Compiles a recursive function computing factorial and verifies correct evaluation
/// of 5! = 120.
#[test]
fn test_function_recursive() {
    let out = compile_and_run(
        "<?php function fact($n) { if ($n <= 1) { return 1; } return $n * fact($n - 1); } echo fact(5);",
    );
    assert_eq!(out, "120");
}

/// Verifies that a function can be called multiple times with different arguments
/// and each call returns the correct independent result.
#[test]
fn test_function_multiple_calls() {
    let out = compile_and_run(
        "<?php function double($x) { return $x * 2; } echo double(3) . \" \" . double(7);",
    );
    assert_eq!(out, "6 14");
}

/// Verifies that the return value of a function can be passed directly as an
/// argument to another function call, with correct evaluation order.
#[test]
fn test_function_as_argument() {
    let out = compile_and_run(
        "<?php function add($a, $b) { return $a + $b; } echo add(add(1, 2), add(3, 4));",
    );
    assert_eq!(out, "10");
}

/// Compiles a function with no parameters that returns a constant integer.
#[test]
fn test_function_no_args() {
    let out = compile_and_run("<?php function answer() { return 42; } echo answer();");
    assert_eq!(out, "42");
}

/// A bare PHP `array` parameter keeps object elements dynamically typed across sibling
/// call sites, allowing the query-builder use case without pinning the callee to one class.
#[test]
fn test_array_element_object_sibling_covariance_at_param() {
    let out = compile_and_run(
        "<?php interface CondA {} interface CondB {} final readonly class QCond implements CondA { public function __construct(public string $sql) {} } final readonly class CCond implements CondB { public function __construct(public string $sql) {} } final class QB { /** @var list<QCond|CCond> */ private array $conditions; public function __construct() { $this->conditions = []; } private function withConditions(array $conditions): self { $c = new QB(); $c->conditions = $conditions; return $c; } public function where(string $s): self { return $this->withConditions([new QCond($s)]); } public function compound(string $s): self { return $this->withConditions([new CCond($s)]); } public function size(): int { return count($this->conditions); } public function firstSql(): string { $c = $this->conditions[0]; return $c->sql; } } function main(): void { $qb = new QB(); $a = $qb->where('a = 1'); $b = $qb->compound('b = 2'); echo $a->size(), ':', $a->firstSql(), '|', $b->size(), ':', $b->firstSql(); } main();",
    );
    assert_eq!(out, "1:a = 1|1:b = 2");
}

/// A free-function bare `array` parameter must not read a sibling object's same-offset
/// property under the first call site's class. The missing property warns and returns null.
#[test]
fn test_array_object_sibling_missing_property_does_not_read_same_offset() {
    let out = compile_and_run_capture(
        r#"<?php
final class BarkDog {
    public function __construct(public string $bark) {}
}
final class MeowCat {
    public function __construct(public string $meow) {}
}
function firstBark(array $items): mixed {
    return $items[0]->bark;
}
echo firstBark([new BarkDog('woof')]), '|';
$missing = firstBark([new MeowCat('mew')]);
echo $missing === null ? 'null' : $missing;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "woof|null");
    assert!(
        out.diagnostics
            .contains("Warning: Undefined property: MeowCat::$bark"),
        "missing PHP-style undefined-property warning: {}",
        out.diagnostics
    );
}

/// An instance-method bare `array` parameter dispatches property reads by runtime class
/// when sibling objects have incompatible layouts, instead of loading a fixed offset.
#[test]
fn test_array_object_sibling_layout_mismatch_does_not_crash() {
    let out = compile_and_run_capture(
        r#"<?php
final class AgedDog {
    public function __construct(public int $age, public string $bark) {}
}
final class ShortCat {
    public function __construct(public string $meow) {}
}
final class BarkReader {
    public function first(array $items): mixed {
        return $items[0]->bark;
    }
}
$reader = new BarkReader();
echo $reader->first([new AgedDog(4, 'woof')]), '|';
$missing = $reader->first([new ShortCat('mew')]);
echo $missing === null ? 'null' : $missing;
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "woof|null");
    assert!(
        out.diagnostics
            .contains("Warning: Undefined property: ShortCat::$bark"),
        "missing PHP-style undefined-property warning: {}",
        out.diagnostics
    );
}

/// Associative bare-array parameters erase concrete object value classes while preserving
/// hash storage, so sibling call sites remain valid and dispatch their shared property safely.
#[test]
fn test_assoc_array_object_sibling_params_preserve_hash_shape() {
    let out = compile_and_run(
        r#"<?php
final class LeftCondition {
    public function __construct(public string $sql) {}
}
final class RightCondition {
    public function __construct(public string $sql) {}
}
function firstSql(array $conditions): string {
    return $conditions['first']->sql;
}
echo firstSql(['first' => new LeftCondition('a = 1')]), '|';
echo firstSql(['first' => new RightCondition('b = 2')]);
"#,
    );
    assert_eq!(out, "a = 1|b = 2");
}

// --- Logical operators ---

/// EC-8 (#491): `if ($x === false) { throw; } return $x;` narrows an `int|false` value to `int`
/// after the divergent guard, so the `: int` return matches. Byte-parity vs PHP 8.5.
#[test]
fn test_strict_false_guard_narrowing() {
    let out = compile_and_run(
        "<?php final class G { public static function requireInt(int|false $v): int { if ($v === false) { throw new \\RuntimeException('no'); } return $v; } } echo G::requireInt(42), ':', G::requireInt(7);",
    );
    assert_eq!(out, "42:7");
}

/// EC-8 (#491): `if ($x === null) { throw; } return $x;` narrows a nullable value to non-null
/// after the divergent guard (elephc models `?T`'s null as Void), so `?string`→string and
/// `?self`→self. Byte-parity vs PHP 8.5.
#[test]
fn test_strict_null_guard_narrowing() {
    let out = compile_and_run(
        "<?php function req(?string $x): string { if ($x === null) { throw new \\Exception('no'); } return $x; } echo req('hi');",
    );
    assert_eq!(out, "hi");
}

/// EC-8 (#491): `$this->prop instanceof X ? ... : <uses $this->prop>` narrows the PROPERTY in the
/// ternary else-branch (Message|string → string), so `new Message($this->prop)` type-checks.
/// Byte-parity vs PHP 8.5. Exercises property-access flow-narrowing across ternary branches.
#[test]
fn test_property_instanceof_ternary_narrowing() {
    let out = compile_and_run(
        "<?php final class Message { public function __construct(public string $key) {} } final class V { public function __construct(private Message|string $raw) {} public function msg(): Message { return $this->raw instanceof Message ? $this->raw : new Message($this->raw); } } echo (new V('hi'))->msg()->key, ':', (new V(new Message('k')))->msg()->key;",
    );
    assert_eq!(out, "hi:k");
}

/// EC-8 (#491): `if (is_null($x)) { throw; }` narrows ?int → int on the fall-through path — the
/// same complement-stripping as `$x === null` (ward-schema ColumnNode::assertDecimalPrecision).
/// Byte-parity vs PHP 8.5.
#[test]
fn test_is_null_guard_narrowing() {
    let out = compile_and_run(
        "<?php function f(?int $p): int { if (is_null($p)) { throw new \\InvalidArgumentException('null'); } if ($p <= 0) { throw new \\InvalidArgumentException('non-positive'); } return $p; } echo f(5);",
    );
    assert_eq!(out, "5");
}

/// EC-8 (#491): a negated-instanceof throw-guard on a PROPERTY narrows it for the statements
/// after the `if` (ward-forms StoreResult::ref pattern: `?StoredFileRef` → StoredFileRef on the
/// fall-through return). Byte-parity vs PHP 8.5.
#[test]
fn test_property_throw_guard_narrowing() {
    let out = compile_and_run(
        "<?php final class W { public function __construct(public string $v) {} } final class R { public function __construct(private ?W $w) {} public function ref(): W { if (!$this->w instanceof W) { throw new \\LogicException('rejected'); } return $this->w; } } echo (new R(new W('x')))->ref()->v;",
    );
    assert_eq!(out, "x");
}

/// Verifies boxed `mixed` values can cross scalar boundaries handled by the runtime cast funnels.
#[test]
fn test_mixed_value_into_typed_boundary() {
    let out = compile_and_run(
        "<?php function takesStr(string $s): string { return strtoupper($s); } function relay(mixed $m): string { return takesStr($m); } function giveInt(mixed $m): int { return $m; } echo relay('hi'), ':', giveInt(42);",
    );
    assert_eq!(out, "HI:42");
}

/// Verifies `fseek()` has PHP's plain integer result type and can cross an `int` boundary.
#[test]
fn test_fseek_int_result_into_typed_param() {
    let out = compile_and_run(
        "<?php function requireZero(int $value, string $message): int { if ($value !== 0) { throw new \\RuntimeException($message); } return $value; } function main(): void { $f = fopen('php://temp', 'r+b'); fwrite($f, 'abcdef'); $r = requireZero(fseek($f, 2), 'seek failed'); echo $r, ':', fread($f, 3); } main();",
    );
    assert_eq!(out, "0:cde");
}

/// Verifies a boxed associative-array element can cross supported scalar call boundaries.
#[test]
fn test_mixed_assoc_element_into_scalar_typed_params() {
    let out = compile_and_run(
        "<?php function firstMode(string $mode, array $allowed): bool { return in_array($mode, $allowed); } function main(): void { $meta = ['mode' => 'r+', 'seekable' => true]; $mode = $meta['mode']; $ok = firstMode($mode, ['r+', 'w+']) ? 'ok' : 'no'; echo $ok, ':', strtoupper($mode); } main();",
    );
    assert_eq!(out, "ok:R+");
}

/// A typed comparator over an `array`-hinted parameter keeps its declared parameter contract —
/// usort checks the closure against its own declarations (via the element-type binding)
/// instead of a fabricated Int placeholder. Byte-parity vs PHP 8.5.
#[test]
fn test_typed_callback_over_array_hinted_param() {
    let out = compile_and_run(
        "<?php final class Box { public function __construct(public int $n) {} } function sorted(array $items): string { usort($items, static fn (Box $a, Box $b): int => $a->n <=> $b->n); $out = ''; foreach ($items as $b) { $out .= $b->n; } return $out; } function main(): void { echo sorted([new Box(3), new Box(1), new Box(2)]); } main();",
    );
    assert_eq!(out, "123");
}

/// foreach KEYS over an unknown-element array (an `array`-hinted param — elements known only
/// to phpdoc) are Mixed, not Int: the value may be associative at runtime (header-map shape
/// with string keys into a `string $name` parameter). Byte-parity vs PHP 8.5.
#[test]
fn test_foreach_key_over_unknown_element_array() {
    let out = compile_and_run(
        "<?php final class H { private array $headers = []; public function setHeader(string $name, string $value): void { $this->headers[$name] = $value; } public function all(array $headers): string { $out = ''; foreach ($headers as $name => $value) { $this->setHeader($name, $value); $out .= $name . '=' . $value . ';'; } return $out; } } function main(): void { echo (new H())->all(['a' => '1', 'b' => '2']); } main();",
    );
    assert_eq!(out, "a=1;b=2;");
}

/// A LOOP CONDITION THAT READS A BY-REFERENCE OUT-PARAMETER MUST BE RE-EVALUATED EVERY
/// ITERATION. The slot has exactly one store in this function (`$n = 0`) and is written
/// only through the callee's reference alias, so the immutable-local-load analysis used to
/// call the condition's load pure and LICM hoisted the whole `$n > 0` compare into the
/// preheader: the body ran ONCE and the loop exited with `$n == 2`.
///
/// This is the shape PHP's own `curl_multi_exec($mh, $running)` loop is written in, which
/// is how the miscompile was found; the fix (`src/ir_passes/by_ref_alias.rs`) excludes
/// by-reference-aliased slots from the immutability proof, the same exclusion dead store
/// elimination already made. Byte-parity vs PHP 8.5.
#[test]
fn test_loop_condition_reads_by_reference_out_parameter() {
    let out = compile_and_run(
        "<?php final class Counter { public int $left = 3; } function step(Counter $c, int &$n): int { if ($c->left > 0) { $c->left = $c->left - 1; } $n = $c->left; return 0; } function main(): void { $c = new Counter(); $n = 0; $spins = 0; do { step($c, $n); $spins++; } while ($n > 0); echo $spins, ':', $n; } main();",
    );
    assert_eq!(out, "3:0");
}

/// The same guarantee for a `while` loop whose condition is the by-reference out-parameter
/// alone, with the extra statement in the body that makes the hoist profitable.
#[test]
fn test_while_condition_reads_by_reference_out_parameter() {
    let out = compile_and_run(
        "<?php function pull(array $queue, int &$left): string { $left = count($queue) - 1; return (string) $left; } function main(): void { $left = 1; $seen = 0; $queue = [1, 2, 3]; while ($left > 0) { pull($queue, $left); $seen++; array_pop($queue); } echo $seen, ':', $left; } main();",
    );
    assert_eq!(out, "3:0");
}

/// A declared return type with a body that can fall off its end is php's CATCHABLE TypeError,
/// raised at the CALL — not a compile error.
///
/// MEASURED on `php -n` 8.5.6 (`scratchpad/qp/a/retpath2.php`), which is what elephc used to
/// refuse outright. php-src writes exactly this shape in its own filter fixtures
/// (`function filter($in, $out, &$c, $closing): int {}`), so refusing it failed four corpus tests
/// at BUILD.
///
/// ⚠️ The switch row uncovered THREE separate defects once it stopped being refused, every one of
/// them already reachable on `main` through a program that returns on every path:
///
/// * the single-case `switch`→`if` rewrite dropped the nested `break`'s target, so it compiled to
///   `Terminator::Unreachable` and the program died with an illegal instruction;
/// * `switch_terminal_effect` lacked the guard its sibling `switch_guarantees_function_exit` has,
///   so everything after the switch was deleted as dead code;
/// * underneath both, a block keeps only its FIRST non-`FallsThrough` effect, and an `if` with no
///   `else` reports `FallsThrough` — which discards "one branch BREAKS" entirely.
///
/// ⚠️ A CLOSURE still refuses at compile time, deliberately: php names it `{closure:FILE:LINE}` in
/// this message and that cannot be reproduced from the checker, so a wrong name would be worse
/// than a stated narrowing. `tests/error_tests/callables.rs` keeps that assertion.
#[test]
fn test_a_declared_return_type_throws_when_the_fall_through_is_reached() {
    let out = compile_and_run(
        r#"<?php
function a(): int { }
function b(bool $ok): int { if ($ok) { return 1; } }
function c(int $x): int { switch ($x) { case 1: if ($x > 0) { break; } return 1; default: return 2; } }
class Box { public function value(): int { } }
echo "all four declared\n";
foreach ([
    'a'    => fn() => a(),
    'b1'   => fn() => b(true),
    'b0'   => fn() => b(false),
    'c9'   => fn() => c(9),
    'c1'   => fn() => c(1),
    'Box'  => fn() => (new Box)->value(),
] as $label => $call) {
    try {
        // Computed first: with commas php evaluates left to right, so a throw mid-echo would
        // print the label twice.
        $answer = var_export($call(), true);
    } catch (TypeError $e) {
        $answer = $e->getMessage();
    }
    echo $label, " => ", $answer, "\n";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            // the declarations alone say nothing, which is the half php differs on
            "all four declared\n",
            "a => a(): Return value must be of type int, none returned\n",
            // a fall-through only some calls reach throws only for those calls
            "b1 => 1\n",
            "b0 => b(): Return value must be of type int, none returned\n",
            // the switch path that RETURNS still returns, and the one that BREAKS throws
            "c9 => 2\n",
            "c1 => c(): Return value must be of type int, none returned\n",
            // a method names itself Class::method()
            "Box => Box::value(): Return value must be of type int, none returned\n",
        )
    );
}
