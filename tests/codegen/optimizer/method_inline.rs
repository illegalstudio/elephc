//! Purpose:
//! Executes statically bound method splices and the method calls the inliner must keep.
//!
//! Called from:
//! - The focused optimizer codegen integration suite.
//!
//! Key details:
//! - Every case runs with `--ir-opt` on and off under strict heap validation and must print
//!   the same output in both modes; PHP-compatible cases also match real PHP when
//!   `ELEPHC_PHP_CHECK` is set.
//! - The EIR shape (calls removed, receiver checks kept, scopes recorded) is asserted by
//!   `src/ir_passes/tests/method_inline_test.rs`; this file proves the behavior around it:
//!   private-property scope, default arguments, exceptions, destructor timing, static
//!   variables, late static binding, recursion, by-reference parameters and overrides.

use super::*;

/// Runs one mode with strict heap validation and the installed PHP oracle when requested.
fn run(source: &str, optimized: bool, php_compatible: bool) -> String {
    let dir = make_cli_test_dir("elephc_method_inline");
    let path = dir.join("main.php");
    fs::write(&path, source).unwrap();
    let mut command = elephc_cli_command(&dir);
    command.args(["--heap-debug", "--gc-stats"]);
    if !optimized {
        command.arg("--no-ir-opt");
    }
    let compiled = command.arg(&path).output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let output = run_binary(&path.with_extension(""), &dir);
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{stderr}");
    if php_compatible && std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
        let php = Command::new("php").arg("-n").arg(&path).output().unwrap();
        assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
        assert_eq!(output.stdout, php.stdout, "real PHP output");
    }
    fs::remove_dir_all(dir).unwrap();
    String::from_utf8(output.stdout).unwrap()
}

/// Asserts that both optimizer modes print `expected`.
fn assert_both_modes(source: &str, php_compatible: bool, expected: &str) {
    for optimized in [false, true] {
        assert_eq!(run(source, optimized, php_compatible), expected, "--ir-opt={optimized}");
    }
}

/// The method-inlining example filters entities through a spliced private-buffer predicate.
#[test]
fn test_method_inline_example_counts_matching_entities() {
    assert_both_modes(
        include_str!("../../../examples/method-inlining/main.php"),
        false,
        "entities: 1000\nmoving: 750\n",
    );
}

/// Spliced accessors read private properties from a free function and from another class's
/// method, honor omitted default arguments, and let an uninitialized typed property's `Error`
/// reach the caller's `catch`.
#[test]
fn test_method_inline_preserves_scope_defaults_and_exceptions() {
    let source = r#"<?php
final class Account
{
    private int $balance;
    private int $opened;
    public function __construct(int $balance) { $this->balance = $balance; }
    public function balance(): int { return $this->balance; }
    public function scaled(int $x, int $factor = 2): int { return $x * $factor; }
    public function opened(): int { return $this->opened; }
}

final class Bank
{
    public function total(Account $a, Account $b): int { return $a->balance() + $b->balance(); }
}

function audit(Account $a): string
{
    try {
        return (string) $a->opened();
    } catch (Error $e) {
        return get_class($e) . ': ' . $e->getMessage();
    }
}

$a = new Account($argc + 9);
$b = new Account($argc * 20);
echo (new Bank())->total($a, $b), "\n";
echo $a->scaled(3), ' ', $a->scaled(3, 5), "\n";
echo audit($a), "\n";
$sum = 0;
for ($i = 0; $i < 1000; $i++) {
    $sum = $sum + $a->balance();
}
echo $sum, "\n";
"#;
    assert_both_modes(
        source,
        true,
        "30\n6 15\nError: Typed property Account::$opened must not be accessed before initialization\n10000\n",
    );
}

/// An accessor a final class inherits reads the parent's private property, not the subclass's
/// shadowing one, once its parent body is spliced into a free function.
#[test]
fn test_method_inline_inherited_accessor_reads_the_parent_private_property() {
    let source = r#"<?php
class Base
{
    private int $secret;
    public function __construct(int $secret) { $this->secret = $secret; }
    public function secret(): int { return $this->secret; }
}

final class Leaf extends Base
{
    private int $secret = 99;
    public function own(): int { return $this->secret; }
}

function read(Leaf $leaf): int
{
    return $leaf->secret() * 1000 + $leaf->own();
}

echo read(new Leaf($argc + 6)), "\n";
"#;
    assert_both_modes(source, true, "7099\n");
}

/// A spliced call borrows its receiver: a local object and a temporary both reach their
/// destructor exactly where the real call would release them.
#[test]
fn test_method_inline_preserves_destructor_timing() {
    let source = r#"<?php
final class Handle
{
    public function __construct(private int $value) { echo "open {$value}\n"; }
    public function value(): int { return $this->value; }
    public function __destruct() { echo "close {$this->value}\n"; }
}

function useLocal(int $seed): int
{
    $handle = new Handle($seed);
    $value = $handle->value();
    echo "used {$value}\n";
    return $value;
}

echo "local ", useLocal($argc), "\n";
echo "temp ", (new Handle($argc + 1))->value(), "\n";
echo "end\n";
"#;
    assert_both_modes(
        source,
        true,
        "local open 1\nused 1\nclose 1\n1\ntemp open 2\nclose 2\n2\nend\n",
    );
}

/// Methods whose meaning depends on their own frame or on runtime dispatch keep their PHP
/// behavior: late static binding, an override reached through the parent type, a static
/// variable, recursion, and a by-reference parameter.
#[test]
fn test_method_inline_keeps_frame_dependent_methods_correct() {
    let source = r#"<?php
class Base { public function name(): string { return static::class; } }
final class Child extends Base {}
class Shape { public function sides(): int { return 0; } }
final class Square extends Shape { public function sides(): int { return 4; } }
final class Tools
{
    public function tick(): int { static $n = 0; $n = $n + 1; return $n; }
    public function fact(int $n): int { return $n <= 1 ? 1 : $n * $this->fact($n - 1); }
    public function bump(int &$x): void { $x = $x + 1; }
}
function sides(Shape $shape): int { return $shape->sides(); }
$tools = new Tools();
echo (new Child())->name(), ' ', sides(new Square()), ' ', sides(new Shape()), "\n";
echo $tools->tick(), $tools->tick(), $tools->tick(), ' ', $tools->fact($argc + 4), "\n";
$x = $argc;
$tools->bump($x);
$tools->bump($x);
echo $x, "\n";
"#;
    assert_both_modes(source, true, "Child 4 0\n123 120\n3\n");
}

#[path = "method_inline_effects.rs"]
mod effects;
