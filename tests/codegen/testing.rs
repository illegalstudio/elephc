//! Purpose:
//! End-to-end coverage for the testing prelude: a suite written against
//! `Elephc\Testing\TestCase` (or the `PHPUnit\Framework\TestCase` alias) compiles with no
//! package installed, the `Runner` prints the documented report, and its verdict is the
//! process exit code.
//!
//! Called from:
//! - `cargo test --test codegen_tests_core testing` through Rust's test harness.
//!
//! Key details:
//! - The report lines (`ok   `, `FAIL `, `ERROR `, `OK (...)`, `Tests: ...`) are a stable
//!   surface that Ivory's `make test` reads, so they are asserted verbatim.
//! - A failing suite exits non-zero, and its report is on stdout: `run_allowing_failure`
//!   captures both without asserting the exit status, so one compile checks the report and
//!   the verdict together.
//! - Each assertion is exercised once on its passing path and once on its failing path, so
//!   the failure text pinned here is the text a user reads.

use crate::support::*;

/// Compiles and runs `source` once without asserting its exit status, so a deliberately
/// failing suite can be checked on stdout and on its exit code in one compile.
fn run_allowing_failure(source: &str) -> ProgramOutput {
    compile_and_run_with_heap_debug_per_argv(source, &[&[]]).remove(0)
}

/// The driver shape every suite shares: one runner, one closure per test, the verdict as
/// the exit code.
const DRIVER: &str = r#"
$runner = new \Elephc\Testing\Runner();
$case = new App\Tests\SuiteTest();
"#;

/// A passing suite prints one `ok` line per test with its assertion count, then the
/// `OK (...)` summary, and exits zero.
#[test]
fn test_passing_suite_reports_ok_and_exits_zero() {
    let out = compile_and_run(&format!(
        r#"<?php
namespace App\Tests;

use Elephc\Testing\TestCase;

final class SuiteTest extends TestCase
{{
    private int $base = 0;

    protected function setUp(): void
    {{
        $this->base = 40;
    }}

    public function testScalars(): void
    {{
        $this->assertTrue(true);
        $this->assertFalse(false);
        $this->assertSame(42, $this->base + 2);
        $this->assertNotSame('42', 42);
        $this->assertEquals('42', 42);
        $this->assertNotEquals(1, 2);
        $this->assertNull(null);
        $this->assertNotNull(0);
    }}

    public function testContainers(): void
    {{
        $this->assertEmpty([]);
        $this->assertNotEmpty([0]);
        $this->assertCount(2, [1, 2]);
        $this->assertContains(2, [1, 2, 3]);
    }}

    public function testNumbers(): void
    {{
        $this->assertEqualsWithDelta(0.3, 0.1 + 0.2, 1e-9);
        $this->assertGreaterThan(1, 2.5);
        $this->assertGreaterThanOrEqual(2, 2);
        $this->assertLessThan(2, 1);
        $this->assertLessThanOrEqual(1.5, 1.5);
    }}

    public function testStrings(): void
    {{
        $this->assertStringContainsString('eph', 'elephc');
        $this->assertStringStartsWith('ele', 'elephc');
        $this->assertStringEndsWith('phc', 'elephc');
    }}

    public function testTypedVariants(): void
    {{
        $this->assertSameInt(42, $this->base + 2);
        $this->assertSameFloat(1.5, 3.0 / 2.0);
        $this->assertSameString('a', 'a');
        $this->assertSameBool(true, 1 === 1);
    }}
}}

namespace {{
{DRIVER}
$runner->run($case, 'testScalars', fn() => $case->testScalars());
$runner->run($case, 'testContainers', fn() => $case->testContainers());
$runner->run($case, 'testNumbers', fn() => $case->testNumbers());
$runner->run($case, 'testStrings', fn() => $case->testStrings());
$runner->run($case, 'testTypedVariants', fn() => $case->testTypedVariants());
exit($runner->finish());
}}
"#
    ));
    assert_eq!(
        out,
        "ok   App\\Tests\\SuiteTest::testScalars (8 assertions)\n\
         ok   App\\Tests\\SuiteTest::testContainers (4 assertions)\n\
         ok   App\\Tests\\SuiteTest::testNumbers (5 assertions)\n\
         ok   App\\Tests\\SuiteTest::testStrings (3 assertions)\n\
         ok   App\\Tests\\SuiteTest::testTypedVariants (4 assertions)\n\
         \n\
         OK (5 tests, 24 assertions)\n"
    );
}

/// A single test with a single assertion is reported in the singular, as PHPUnit does.
#[test]
fn test_singular_counts_are_pluralized_correctly() {
    let out = compile_and_run(&format!(
        r#"<?php
namespace App\Tests;

use Elephc\Testing\TestCase;

final class SuiteTest extends TestCase
{{
    public function testOne(): void
    {{
        $this->assertTrue(true);
    }}
}}

namespace {{
{DRIVER}
$runner->run($case, 'testOne', fn() => $case->testOne());
exit($runner->finish());
}}
"#
    ));
    assert_eq!(
        out,
        "ok   App\\Tests\\SuiteTest::testOne (1 assertion)\n\nOK (1 test, 1 assertion)\n"
    );
}

/// A failing assertion is a FAILURE with PHPUnit's wording, a user message precedes it on its
/// own line, an escaping exception is an ERROR naming its class, `tearDown()` still runs,
/// assertions made before the failure are counted, and the exit code is non-zero.
#[test]
fn test_failures_and_errors_are_reported_and_exit_non_zero() {
    let source = format!(
        r#"<?php
namespace App\Tests;

use Elephc\Testing\TestCase;

final class SuiteTest extends TestCase
{{
    protected function tearDown(): void
    {{
        echo "tearDown\n";
    }}

    public function testFails(): void
    {{
        $this->assertTrue(true);
        $this->assertSame(1, 2, 'custom message');
        $this->assertTrue(false);
    }}

    public function testErrors(): void
    {{
        throw new \RuntimeException('boom');
    }}

    public function testExplicitFail(): void
    {{
        $this->fail('not implemented');
    }}

    public function testPasses(): void
    {{
        $this->assertNull(null);
    }}
}}

namespace {{
{DRIVER}
$runner->run($case, 'testFails', fn() => $case->testFails());
$runner->run($case, 'testErrors', fn() => $case->testErrors());
$runner->run($case, 'testExplicitFail', fn() => $case->testExplicitFail());
$runner->run($case, 'testPasses', fn() => $case->testPasses());
exit($runner->finish());
}}
"#
    );
    let out = run_allowing_failure(&source);
    assert!(!out.success, "a failing suite must exit non-zero\n{}", out.stdout);
    assert_eq!(
        out.stdout,
        "tearDown\n\
         FAIL App\\Tests\\SuiteTest::testFails\n\
         \x20    custom message\n\
         \x20    Failed asserting that 2 is identical to 1.\n\
         tearDown\n\
         ERROR App\\Tests\\SuiteTest::testErrors\n\
         \x20    RuntimeException: boom\n\
         tearDown\n\
         FAIL App\\Tests\\SuiteTest::testExplicitFail\n\
         \x20    not implemented\n\
         tearDown\n\
         ok   App\\Tests\\SuiteTest::testPasses (1 assertion)\n\
         \n\
         Tests: 4, Assertions: 3, Failures: 2, Errors: 1.\n"
    );
}

/// Every failure text the subset can print, pinned verbatim so a rewording is a visible
/// change rather than a surprise in a user's CI log.
#[test]
fn test_failure_messages_follow_phpunit_wording() {
    let source = format!(
        r#"<?php
namespace App\Tests;

use Elephc\Testing\TestCase;

final class SuiteTest extends TestCase
{{
    public function testFalse(): void {{ $this->assertFalse(true); }}
    public function testNotSame(): void {{ $this->assertNotSame('a', 'a'); }}
    public function testEquals(): void {{ $this->assertEquals([1], 2.5); }}
    public function testNotEquals(): void {{ $this->assertNotEquals(null, null); }}
    public function testNull(): void {{ $this->assertNull(false); }}
    public function testNotNull(): void {{ $this->assertNotNull(null); }}
    public function testEmpty(): void {{ $this->assertEmpty([1, 2]); }}
    public function testNotEmpty(): void {{ $this->assertNotEmpty(''); }}
    public function testCount(): void {{ $this->assertCount(2, [1, 2, 3]); }}
    public function testContains(): void {{ $this->assertContains('x', ['y']); }}
    public function testDelta(): void {{ $this->assertEqualsWithDelta(1.0, 1.5, 0.1); }}
    public function testGreater(): void {{ $this->assertGreaterThan(2, 1); }}
    public function testGreaterOrEqual(): void {{ $this->assertGreaterThanOrEqual(2, 1); }}
    public function testLess(): void {{ $this->assertLessThan(1, 2); }}
    public function testLessOrEqual(): void {{ $this->assertLessThanOrEqual(1, 2); }}
    public function testContainsString(): void {{ $this->assertStringContainsString('z', 'abc'); }}
    public function testStartsWith(): void {{ $this->assertStringStartsWith('z', 'abc'); }}
    public function testEndsWith(): void {{ $this->assertStringEndsWith('z', 'abc'); }}
    public function testSameInt(): void {{ $this->assertSameInt(1, 2); }}
    public function testSameFloat(): void {{ $this->assertSameFloat(1.5, 2.5); }}
    public function testSameString(): void {{ $this->assertSameString('a', 'b'); }}
    public function testSameBool(): void {{ $this->assertSameBool(true, false); }}
    public function testObject(): void {{ $this->assertSame(null, $this); }}
}}

namespace {{
{DRIVER}
$runner->run($case, 'testFalse', fn() => $case->testFalse());
$runner->run($case, 'testNotSame', fn() => $case->testNotSame());
$runner->run($case, 'testEquals', fn() => $case->testEquals());
$runner->run($case, 'testNotEquals', fn() => $case->testNotEquals());
$runner->run($case, 'testNull', fn() => $case->testNull());
$runner->run($case, 'testNotNull', fn() => $case->testNotNull());
$runner->run($case, 'testEmpty', fn() => $case->testEmpty());
$runner->run($case, 'testNotEmpty', fn() => $case->testNotEmpty());
$runner->run($case, 'testCount', fn() => $case->testCount());
$runner->run($case, 'testContains', fn() => $case->testContains());
$runner->run($case, 'testDelta', fn() => $case->testDelta());
$runner->run($case, 'testGreater', fn() => $case->testGreater());
$runner->run($case, 'testGreaterOrEqual', fn() => $case->testGreaterOrEqual());
$runner->run($case, 'testLess', fn() => $case->testLess());
$runner->run($case, 'testLessOrEqual', fn() => $case->testLessOrEqual());
$runner->run($case, 'testContainsString', fn() => $case->testContainsString());
$runner->run($case, 'testStartsWith', fn() => $case->testStartsWith());
$runner->run($case, 'testEndsWith', fn() => $case->testEndsWith());
$runner->run($case, 'testSameInt', fn() => $case->testSameInt());
$runner->run($case, 'testSameFloat', fn() => $case->testSameFloat());
$runner->run($case, 'testSameString', fn() => $case->testSameString());
$runner->run($case, 'testSameBool', fn() => $case->testSameBool());
$runner->run($case, 'testObject', fn() => $case->testObject());
exit($runner->finish());
}}
"#
    );
    let out = run_allowing_failure(&source);
    assert!(!out.success, "{}", out.stdout);
    let expected = [
        ("testFalse", "Failed asserting that true is false."),
        ("testNotSame", "Failed asserting that 'a' is not identical to 'a'."),
        ("testEquals", "Failed asserting that 2.5 matches expected Array (1)."),
        ("testNotEquals", "Failed asserting that null is not equal to null."),
        ("testNull", "Failed asserting that false is null."),
        ("testNotNull", "Failed asserting that null is not null."),
        ("testEmpty", "Failed asserting that Array (2) is empty."),
        ("testNotEmpty", "Failed asserting that '' is not empty."),
        ("testCount", "Failed asserting that actual size 3 matches expected size 2."),
        ("testContains", "Failed asserting that an array contains 'x'."),
        ("testDelta", "Failed asserting that 1.5 matches expected 1 within delta 0.1."),
        ("testGreater", "Failed asserting that 1 is greater than 2."),
        ("testGreaterOrEqual", "Failed asserting that 1 is equal to or greater than 2."),
        ("testLess", "Failed asserting that 2 is less than 1."),
        ("testLessOrEqual", "Failed asserting that 2 is equal to or less than 1."),
        ("testContainsString", "Failed asserting that 'abc' contains \"z\"."),
        ("testStartsWith", "Failed asserting that 'abc' starts with \"z\"."),
        ("testEndsWith", "Failed asserting that 'abc' ends with \"z\"."),
        ("testSameInt", "Failed asserting that 2 is identical to 1."),
        ("testSameFloat", "Failed asserting that 2.5 is identical to 1.5."),
        ("testSameString", "Failed asserting that 'b' is identical to 'a'."),
        ("testSameBool", "Failed asserting that false is identical to true."),
        (
            "testObject",
            "Failed asserting that App\\Tests\\SuiteTest Object is identical to null.",
        ),
    ];
    let mut report = String::new();
    for (name, message) in expected {
        report.push_str(&format!("FAIL App\\Tests\\SuiteTest::{name}\n     {message}\n"));
    }
    report.push_str("\nTests: 23, Assertions: 23, Failures: 23, Errors: 0.\n");
    assert_eq!(out.stdout, report);
}

/// A suite written against PHPUnit's namespace compiles unchanged, including under
/// `--strict-php` for a tagged `.php` file, and passes.
#[test]
fn test_phpunit_alias_compiles_under_strict_php() {
    let out = compile_cli_file_and_run_with_flags(
        r#"<?php

declare(strict_types=1);

namespace App\Tests;

use PHPUnit\Framework\TestCase;

final class StackTest extends TestCase
{
    private array $stack = [];

    protected function setUp(): void
    {
        $this->stack = [];
    }

    public function testPushAndPop(): void
    {
        $this->assertEmpty($this->stack);
        $this->stack[] = 'foo';
        $this->assertCount(1, $this->stack);
        $this->assertSame('foo', array_pop($this->stack));
        $this->assertEmpty($this->stack);
    }
}

$runner = new \Elephc\Testing\Runner();
$case = new StackTest();
$runner->run($case, 'testPushAndPop', fn() => $case->testPushAndPop());
$runner->finish();
"#,
        &["--strict-php"],
    );
    assert_eq!(
        out,
        "ok   App\\Tests\\StackTest::testPushAndPop (4 assertions)\n\nOK (1 test, 4 assertions)\n"
    );
}

/// A runner that ran nothing reports it and returns a failing exit code, like PHPUnit's
/// "No tests executed!".
#[test]
fn test_empty_run_is_a_failure() {
    let source = r#"<?php
$runner = new \Elephc\Testing\Runner();
exit($runner->finish());
"#;
    let out = run_allowing_failure(source);
    assert!(!out.success, "{}", out.stdout);
    assert_eq!(out.stdout, "\nNo tests executed!\n");
}

/// A program that never mentions the runtime does not carry it: the classes are absent, so
/// `class_exists` is false and the binary pays nothing for the prelude.
#[test]
fn test_prelude_is_not_injected_without_a_reference() {
    let out = compile_and_run(
        r#"<?php
var_dump(class_exists('Elephc\\Testing\\TestCase'), class_exists('PHPUnit\\Framework\\TestCase'));
"#,
    );
    assert_eq!(out, "bool(false)\nbool(false)\n");
}

/// A PHPUnit method outside the supported subset is a compile error, never a silent no-op.
#[test]
fn test_unsupported_assertion_is_a_compile_error() {
    let error = compile_expect_type_error(
        r#"<?php
use Elephc\Testing\TestCase;
final class T extends TestCase {
    public function testIt(): void { $this->assertInstanceOf(self::class, $this); }
}
$t = new T();
$t->testIt();
"#,
    );
    assert!(
        error.contains("Undefined method: T::assertInstanceOf"),
        "unexpected diagnostic: {error}"
    );
}

/// The whole report path (boxed `mixed` operands, failure strings, caught exceptions, the
/// closure per test) leaves a clean heap under heap debugging. Isolation is off: the
/// isolated parent reads each outcome file with `file()`, which leaks one block per call
/// (journal task 93012677), and `file_get_contents()` is not an option there because a non-literal
/// path makes every test binary link bzip2. That is the builtin's bug, not the runner's.
#[test]
fn test_suite_leaves_a_clean_heap() {
    let out = compile_and_run_with_heap_debug(&format!(
        r#"<?php
namespace App\Tests;

use Elephc\Testing\TestCase;

final class SuiteTest extends TestCase
{{
    public function testPasses(): void
    {{
        $this->assertSame('a', 'a');
        $this->assertEquals([1, 2], [1, 2]);
        $this->assertContains('x', ['x', 'y']);
    }}

    public function testFails(): void
    {{
        $this->assertSame([1], 'text', 'with a message');
    }}

    public function testErrors(): void
    {{
        throw new \LogicException('nope');
    }}
}}

namespace {{
$runner = new \Elephc\Testing\Runner(false);
$case = new App\Tests\SuiteTest();

$runner->run($case, 'testPasses', fn() => $case->testPasses());
$runner->run($case, 'testFails', fn() => $case->testFails());
$runner->run($case, 'testErrors', fn() => $case->testErrors());
$runner->finish();
}}
"#
    ));
    assert!(
        out.stdout.starts_with("ok   App\\Tests\\SuiteTest::testPasses (3 assertions)\n"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("Tests: 3, Assertions: 4, Failures: 1, Errors: 1.\n"), "{}", out.stdout);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
    assert!(
        !out.stderr.contains("leak summary: live_blocks"),
        "the runner must be leak-free: {}",
        out.stderr
    );
}

/// A program that uses the runtime links no compression or TLS library: the isolated runner
/// reads its outcome file with `file()`, not `file_get_contents()`, whose non-literal path
/// conservatively requires the TLS and PHAR bridges plus zlib and bzip2. Requiring them made
/// every `elephc test` binary fail to link on a host without libbz2.
#[test]
fn test_runtime_requires_no_compression_or_tls_libraries() {
    let dir = make_cli_test_dir("elephc_testing_link_requirements");
    let (_, _, requirements) = compile_source_to_asm_with_options(
        r#"<?php
final class T extends \Elephc\Testing\TestCase {
    public function testIt(): void { $this->assertTrue(true); }
}
$runner = new \Elephc\Testing\Runner(true, 60, '', 'report.xml');
$case = new T();
$runner->run($case, 'testIt', fn() => $case->testIt());
exit($runner->finish());
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    for library in ["bz2", "z", "elephc_tls"] {
        assert!(
            !requirements.iter().any(|name| name == library),
            "the testing runtime must not require {library}: {:?}",
            &*requirements
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
