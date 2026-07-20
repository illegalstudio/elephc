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

/// A `mixed` VALUE flowing into a narrower declared boundary is legal PHP (the engine enforces
/// at runtime) — a mixed param passed on to a `string` param and a mixed value returned as a
/// declared `int` both compile and run byte-identically (the boxed-Mixed representation
/// funnels at the boundary).
#[test]
fn test_mixed_value_into_typed_boundary() {
    let out = compile_and_run(
        "<?php function takesStr(string $s): string { return strtoupper($s); } function relay(mixed $m): string { return takesStr($m); } function giveInt(mixed $m): int { return $m; } echo relay('hi'), ':', giveInt(42);",
    );
    assert_eq!(out, "HI:42");
}

/// A union VALUE with a member the declaration accepts crosses the boundary — an
/// `int|false`-typed seek result passed to an `int` param on its success path.
/// Byte-parity vs PHP 8.5.
#[test]
fn test_union_value_into_narrower_param() {
    let out = compile_and_run(
        "<?php function requireZero(int $value, string $message): int { if ($value !== 0) { throw new \\RuntimeException($message); } return $value; } function main(): void { $f = fopen('php://temp', 'r+b'); fwrite($f, 'abcdef'); $r = requireZero(fseek($f, 2), 'seek failed'); echo $r, ':', fread($f, 3); } main();",
    );
    assert_eq!(out, "0:cde");
}

/// An assoc-array element (Mixed) into a `string` param plus an untyped-array element into an
/// object param. Byte-parity vs PHP 8.5.
#[test]
fn test_mixed_array_element_into_typed_params() {
    let out = compile_and_run(
        "<?php final class Item { public function __construct(public string $name) {} } function label(Item $i): string { return $i->name; } function firstMode(string $mode, array $allowed): bool { return in_array($mode, $allowed); } function main(): void { $meta = ['mode' => 'r+', 'seekable' => true]; $mode = $meta['mode']; $ok = firstMode($mode, ['r+', 'w+']) ? 'ok' : 'no'; $items = [new Item('a'), new Item('b')]; $x = $items[1]; echo $ok, ':', label($x), ':', strtoupper($mode); } main();",
    );
    assert_eq!(out, "ok:b:R+");
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
