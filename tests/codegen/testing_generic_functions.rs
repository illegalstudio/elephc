//! Purpose:
//! Regressions for generic functions declared in files loaded by `elephc test`.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - Calls originate only in discovered test methods, with no top-level warm-up calls.
//! - The CLI cache lives inside each fixture and is removed before checking its result.

use crate::support::*;

/// A required test file retains its generic template and emits the specialization after exit.
#[test]
fn test_cli_test_generic_function_template() {
    let dir = make_cli_test_dir("elephc_test_generic_function");
    fs::write(
        dir.join("IdentityTest.lfc"),
        r#"use Elephc\Testing\TestCase;
function identity<T>(T $value): T { return $value; }
final class IdentityTest extends TestCase {
    public function testIdentity(): void {
        $this->assertSameInt(17, identity(17));
    }
}
"#,
    ).unwrap();
    let output = elephc_cli_command(&dir)
        .args(["test", "IdentityTest.lfc"])
        .output()
        .expect("run native generic function test");
    fs::remove_dir_all(&dir).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert_eq!(stdout, "ok   IdentityTest::testIdentity (1 assertion)\n\nOK (1 test, 1 assertion)\n");
}

/// Namespaced, explicit and transitive specializations work only through native test methods.
#[test]
fn test_cli_test_generic_function_specializations() {
    let dir = make_cli_test_dir("elephc_test_generic_specializations");
    fs::write(
        dir.join("GenericFunctionTest.lfc"),
        r#"namespace Suite;
use Elephc\Testing\TestCase;
function identity<T>(T $value): T { return $value; }
function relay<T>(T $value): T { return identity($value); }
function unused<U>(U $value): U { return $value; }
final class GenericFunctionTest extends TestCase {
    public function testInferred(): void {
        $this->assertSameInt(19, relay(19));
    }
    public function testWritten(): void {
        $this->assertSameString('native', identity<string>('native'));
    }
}
"#,
    ).unwrap();
    let output = elephc_cli_command(&dir)
        .args(["test", "GenericFunctionTest.lfc", "--no-isolation"])
        .output()
        .expect("run namespaced native generic functions");
    fs::remove_dir_all(&dir).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert_eq!(stdout, "ok   Suite\\GenericFunctionTest::testInferred (1 assertion)\nok   Suite\\GenericFunctionTest::testWritten (1 assertion)\n\nOK (2 tests, 2 assertions)\n");
}

/// Each concrete specialization dispatches through the include branch activated at runtime.
#[test]
fn test_cli_test_generic_function_include_variants() {
    let dir = make_cli_test_dir("elephc_generic_function_include_variants");
    for (name, source) in [
        ("main.lfc", r#"
if ($argc > 2) { echo choose(17); exit(0); }
if ($argc > 1) { require 'left.lfc'; } else { require 'right.lfc'; }
echo choose(17), ' ', choose<string>('native');
"#),
        ("left.lfc", "function choose<T>(T $value): T { echo 'left:'; return $value; }"),
        ("right.lfc", "function choose<U>(U $value): U { echo 'right:'; return $value; }"),
    ] {
        fs::write(dir.join(name), source).unwrap();
    }
    let compiled = elephc_cli_command(&dir).arg("main.lfc").output().unwrap();
    let mut outputs = Vec::new();
    if compiled.status.success() {
        for args in [&[][..], &["left"][..], &["before", "include"][..]] {
            outputs.push(Command::new(dir.join("main")).args(args).output().unwrap());
        }
    }
    fs::remove_dir_all(&dir).unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    for (output, expected) in outputs.iter().zip(["right:17 right:native", "left:17 left:native"]) {
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
    }
    assert!(!outputs[2].status.success());
    assert!(outputs[2].stdout.is_empty());
    assert!(String::from_utf8_lossy(&outputs[2].stderr)
        .contains("Call to undefined function choose()"));
}

/// PHPDoc templates in required PHP tests specialize under strict-PHP mode as well.
#[test]
fn test_cli_test_generic_function_phpdoc() {
    let dir = make_cli_test_dir("elephc_test_generic_phpdoc");
    fs::write(dir.join("DocblockTest.php"), r#"<?php
use Elephc\Testing\TestCase;
/**
 * @template T
 * @param T $value
 * @return T
 */
function identity($value) { return $value; }
final class DocblockTest extends TestCase {
    public function testIdentity(): void {
        $this->assertSameString('docblock', identity('docblock'));
    }
}
"#).unwrap();
    let output = elephc_cli_command(&dir)
        .args(["test", "DocblockTest.php", "--strict-php"])
        .output().unwrap();
    fs::remove_dir_all(&dir).unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout),
        "ok   DocblockTest::testIdentity (1 assertion)\n\nOK (1 test, 1 assertion)\n");
}

/// Incompatible generic include contracts are diagnosed before dispatch signatures are resolved.
#[test]
fn test_cli_test_generic_function_variant_contract() {
    for (left, right) in [
        ("function choose<T: int>(T $value): T { return $value; }",
         "function choose<U: string>(U $value): U { return $value; }"),
        ("function choose<T>(T $value): T { return $value; }",
         "function choose($value) { return $value; }"),
    ] {
        let dir = make_cli_test_dir("elephc_test_generic_variant_contract");
        for (name, source) in [
            ("main.lfc", "if ($argc > 1) { require 'left.lfc'; } else { require 'right.lfc'; } echo choose(17);"),
            ("left.lfc", left),
            ("right.lfc", right),
        ] {
            fs::write(dir.join(name), source).unwrap();
        }
        let output = elephc_cli_command(&dir).args(["main.lfc", "--check"]).output().unwrap();
        fs::remove_dir_all(&dir).unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(stderr.contains("Generic function variants for 'choose' must declare matching type parameter contracts"), "{stderr}");
        assert_eq!(stderr.matches("Generic function variants").count(), 1, "{stderr}");
        assert!(!stderr.contains("Compiler error:"), "{stderr}");
    }
}

/// Dispatch metadata keeps commas and callable colons inside instantiated type arguments.
#[test]
fn test_cli_test_generic_function_compound_arguments() {
    let dir = make_cli_test_dir("elephc_test_generic_compound_arguments");
    fs::write(dir.join("CompoundTest.lfc"), r#"use Elephc\Testing\TestCase;
function second<A, B>(A $first, B $second): B { return $second; }
function identity<T>(T $value): T { return $value; }
final class CompoundTest extends TestCase {
    public function testArguments(): void {
        $this->assertSameString('native', second(17, 'native'));
        $format = identity<callable(int): string>(fn(int $n): string => strval($n));
        $this->assertSameString('17', $format(17));
    }
}
"#).unwrap();
    let output = elephc_cli_command(&dir).args(["test", "CompoundTest.lfc"]).output().unwrap();
    fs::remove_dir_all(&dir).unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8_lossy(&output.stdout),
        "ok   CompoundTest::testArguments (2 assertions)\n\nOK (1 test, 2 assertions)\n");
}

/// Deferred includes activate generic helpers in retained methods and their nested closures.
#[test]
fn test_cli_test_generic_function_method_includes() {
    let dir = make_cli_test_dir("elephc_test_generic_method_includes");
    for (name, source) in [
        ("MethodHelperTest.lfc", r#"use Elephc\Testing\TestCase;
trait HelperLoader {
    public function fromTrait(): int {
        require 'trait-helper.lfc';
        return keepTrait(11);
    }
}
trait EnumHelperLoader {
    public function fromEnum(): string {
        require_once 'enum-helper.lfc';
        return keepEnum('enum');
    }
}
enum HelperState {
    use EnumHelperLoader;
    case Ready;
}
final class MethodHelperTest extends TestCase {
    use HelperLoader;
    public function testHelper(): void {
        require_once 'helpers.lfc';
        $this->assertSameInt(9, keep(9));
        $this->assertSameString('method', keep('method'));
    }
    public function testClosure(): void {
        $load = static function (): string {
            require_once 'closure-helper.lfc';
            return keepClosure('closure');
        };
        $this->assertSameString('closure', $load());
    }
    public function testTrait(): void { $this->assertSameInt(11, $this->fromTrait()); }
    public function testEnum(): void { $this->assertSameString('enum', HelperState::Ready->fromEnum()); }
}
"#),
        ("helpers.lfc", "function keep<T>(T $value): T { return $value; }"),
        ("closure-helper.lfc", "function keepClosure<T>(T $value): T { return $value; }"),
        ("trait-helper.lfc", "function keepTrait<T>(T $value): T { return $value; }"),
        ("enum-helper.lfc", "function keepEnum<T>(T $value): T { return $value; }"),
    ] {
        fs::write(dir.join(name), source).unwrap();
    }
    let output = elephc_cli_command(&dir).args(["test", "MethodHelperTest.lfc"]).output().unwrap();
    fs::remove_dir_all(&dir).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert_eq!(stdout, "ok   MethodHelperTest::testHelper (2 assertions)\nok   MethodHelperTest::testClosure (1 assertion)\nok   MethodHelperTest::testTrait (1 assertion)\nok   MethodHelperTest::testEnum (1 assertion)\n\nOK (4 tests, 5 assertions)\n");
}

/// Strict-PHP PHPDoc dispatch failures retain the source function name and namespace.
#[test]
fn test_cli_test_generic_function_phpdoc_undefined_name() {
    let dir = make_cli_test_dir("elephc_test_generic_phpdoc_undefined_name");
    for (name, source) in [
        ("main.php", "<?php namespace Suite; if ($argc > 1) { require 'helpers.php'; } echo identity(17);"),
        ("helpers.php", r#"<?php
namespace Suite;
/**
 * @template T
 * @param T $value
 * @return T
 */
function identity($value) { return $value; }
"#),
    ] {
        fs::write(dir.join(name), source).unwrap();
    }
    let compiled = elephc_cli_command(&dir).args(["main.php", "--strict-php"]).output().unwrap();
    let mut outputs = Vec::new();
    if compiled.status.success() {
        for args in [&[][..], &["include"][..]] {
            outputs.push(Command::new(dir.join("main")).args(args).output().unwrap());
        }
    }
    fs::remove_dir_all(&dir).unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    assert!(!outputs[0].status.success());
    assert!(outputs[0].stdout.is_empty());
    assert_eq!(String::from_utf8_lossy(&outputs[0].stderr),
        "Fatal error: Call to undefined function Suite\\identity()\n");
    assert!(outputs[1].status.success(), "{}", String::from_utf8_lossy(&outputs[1].stderr));
    assert_eq!(String::from_utf8_lossy(&outputs[1].stdout), "17");
}

/// The instantiation report names the public template once across alternative include bodies.
#[test]
fn test_cli_test_generic_function_report() {
    let dir = make_cli_test_dir("elephc_test_generic_function_report");
    for (name, source) in [
        ("main.lfc", "if ($argc > 1) { require 'left.lfc'; } else { require 'right.lfc'; } echo choose(17), choose('x');"),
        ("left.lfc", "function choose<T>(T $value): T { return $value; }"),
        ("right.lfc", "function choose<U>(U $value): U { return $value; }"),
    ] {
        fs::write(dir.join(name), source).unwrap();
    }
    let output = elephc_cli_command(&dir)
        .args(["main.lfc", "--check", "--report-generics"]).output().unwrap();
    fs::remove_dir_all(&dir).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("generics: 2 instantiations across 1 template"), "{stderr}");
    assert!(stderr.contains("  function x2   choose  [choose<int>, choose<string>]"), "{stderr}");
    assert!(!stderr.contains("__elephc_include_variant_"), "{stderr}");
}
