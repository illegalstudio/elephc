//! Purpose:
//! End-to-end coverage for the `elephc test` subcommand through the real binary: test-file
//! discovery under a project, Composer `autoload` and `autoload-dev` resolution from the
//! project root, compile-time test discovery (`test*`, `#[Test]`, inherited methods, the
//! PHPUnit alias, mixed `.php` and `.lfc` files), `--filter`, the exit code, and the
//! diagnostics for empty runs and refused flags.
//!
//! Called from:
//! - `cargo test --test codegen_tests testing_command` through Rust's test harness.
//!
//! Key details:
//! - Every test builds its own project directory and runs the CLI from inside it, with the
//!   cache redirected under that directory by `elephc_cli_command`, so builds never collide.
//! - The report lines are asserted verbatim: they are the surface a CI log shows.

use crate::support::*;
use std::path::Path;

/// Writes the fixture project: a `src/` class, a plain-PHP PHPUnit-style test, a tagless
/// `.lfc` test, an abstract base with an inherited test, and a nested test directory.
fn write_project(dir: &Path) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("tests/Unit")).unwrap();
    fs::write(
        dir.join("composer.json"),
        r#"{
    "autoload": { "psr-4": { "App\\": "src/" } },
    "autoload-dev": { "psr-4": { "App\\Tests\\": "tests/" } }
}
"#,
    )
    .unwrap();
    fs::write(
        dir.join("src/Counter.php"),
        r#"<?php
namespace App;
final class Counter {
    private int $value = 0;
    public function increment(int $by = 1): void { $this->value += $by; }
    public function value(): int { return $this->value; }
}
"#,
    )
    .unwrap();
    fs::write(
        dir.join("tests/IntegrationTestCase.php"),
        r#"<?php
namespace App\Tests;
use PHPUnit\Framework\TestCase;
abstract class IntegrationTestCase extends TestCase {
    public function testInheritedCheck(): void { $this->assertTrue(true); }
}
"#,
    )
    .unwrap();
    fs::write(
        dir.join("tests/CounterTest.php"),
        r#"<?php
namespace App\Tests;
use App\Counter;
final class CounterTest extends IntegrationTestCase {
    private Counter $counter;
    protected function setUp(): void { $this->counter = new Counter(); }
    public function testStartsAtZero(): void { $this->assertSame(0, $this->counter->value()); }
    public function testIncrements(): void {
        $this->counter->increment(2);
        $this->assertSame(2, $this->counter->value());
        $this->assertGreaterThan(1, $this->counter->value());
    }
    public function helper(): void { $this->fail('never run'); }
}
"#,
    )
    .unwrap();
    fs::write(
        dir.join("tests/Unit/StringsTest.lfc"),
        r#"namespace App\Tests\Unit;
use Elephc\Testing\TestCase;
final class StringsTest extends TestCase {
    #[Test]
    public function typedAssertions(): void {
        $this->assertSameInt(6, strlen('elephc'));
        $this->assertSameString('a', 'a');
    }
    public function testFails(): void { $this->assertSame(1, 2, 'deliberate'); }
}
"#,
    )
    .unwrap();
    // Not a test file: a helper the tests could autoload, never required by the entry.
    fs::write(
        dir.join("tests/Unit/Support.php"),
        "<?php\nnamespace App\\Tests\\Unit;\nfinal class Support { public function testNotATest(): void {} }\n",
    )
    .unwrap();
}

/// Runs `elephc test` with `args` inside `dir`, returning stdout, stderr and the exit code.
fn run_test_command(dir: &Path, args: &[&str]) -> (String, String, i32) {
    let output = elephc_cli_command(dir)
        .arg("test")
        .args(args)
        .output()
        .expect("failed to run elephc test");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

/// The whole project: files found in sorted order, `test*`, `#[Test]` and inherited methods
/// discovered, autoload from `src/` and `autoload-dev` resolved, the deliberate failure
/// reported, and the exit code non-zero because of it.
#[test]
fn test_cli_test_runs_the_discovered_suite() {
    let dir = make_cli_test_dir("elephc_cli_test_suite");
    write_project(&dir);
    let (stdout, stderr, code) = run_test_command(&dir, &[]);
    assert_eq!(
        stdout,
        "ok   App\\Tests\\CounterTest::testStartsAtZero (1 assertion)\n\
         ok   App\\Tests\\CounterTest::testIncrements (2 assertions)\n\
         ok   App\\Tests\\CounterTest::testInheritedCheck (1 assertion)\n\
         ok   App\\Tests\\Unit\\StringsTest::typedAssertions (2 assertions)\n\
         FAIL App\\Tests\\Unit\\StringsTest::testFails\n\
         \x20    deliberate\n\
         \x20    Failed asserting that 2 is identical to 1.\n\
         \n\
         Tests: 5, Assertions: 7, Failures: 1, Errors: 0.\n",
        "stderr: {stderr}"
    );
    assert_eq!(code, 1, "a failing suite exits 1\nstderr: {stderr}");
    assert!(
        !stderr.contains("Compiled '"),
        "the test build must not print the compile success line: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `--filter` keeps only the matching `Class::method` pairs, so a green subset exits 0,
/// and the filter is applied before compilation (the failing test never runs).
#[test]
fn test_cli_test_filter_selects_tests_and_exits_zero_when_green() {
    let dir = make_cli_test_dir("elephc_cli_test_filter");
    write_project(&dir);
    let (stdout, stderr, code) = run_test_command(&dir, &["--filter", "CounterTest"]);
    assert_eq!(
        stdout,
        "ok   App\\Tests\\CounterTest::testStartsAtZero (1 assertion)\n\
         ok   App\\Tests\\CounterTest::testIncrements (2 assertions)\n\
         ok   App\\Tests\\CounterTest::testInheritedCheck (1 assertion)\n\
         \n\
         OK (3 tests, 4 assertions)\n",
        "stderr: {stderr}"
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    let (stdout, _, code) = run_test_command(&dir, &["--filter=typedAssertions"]);
    assert!(stdout.ends_with("OK (1 test, 2 assertions)\n"), "{stdout}");
    assert_eq!(code, 0);
    let _ = fs::remove_dir_all(&dir);
}

/// A path argument narrows the files: a subdirectory, or a single file, while the project
/// root (and so autoload) is still found from the nearest `composer.json` above it.
#[test]
fn test_cli_test_accepts_a_directory_or_a_file_path() {
    let dir = make_cli_test_dir("elephc_cli_test_path");
    write_project(&dir);
    let (stdout, _, code) = run_test_command(&dir, &["tests/Unit", "--filter", "typed"]);
    assert!(stdout.starts_with("ok   App\\Tests\\Unit\\StringsTest::typedAssertions"), "{stdout}");
    assert_eq!(code, 0);
    let (stdout, stderr, code) = run_test_command(&dir, &["tests/CounterTest.php"]);
    assert!(stdout.ends_with("OK (3 tests, 4 assertions)\n"), "{stdout}\n{stderr}");
    assert_eq!(code, 0);
    let _ = fs::remove_dir_all(&dir);
}

/// Running from a subdirectory of the project still resolves the root through the test
/// path's ancestors, not the working directory.
#[test]
fn test_cli_test_resolves_the_root_from_the_test_path() {
    let dir = make_cli_test_dir("elephc_cli_test_root");
    write_project(&dir);
    let output = elephc_cli_command(&dir.join("tests/Unit"))
        .args(["test", "../../tests", "--filter", "testStartsAtZero"])
        .output()
        .expect("failed to run elephc test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.ends_with("OK (1 test, 1 assertion)\n"), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
    let _ = fs::remove_dir_all(&dir);
}

/// Empty runs are errors with a message naming the rule: no test files, no test classes,
/// no matching filter, a missing path.
#[test]
fn test_cli_test_diagnoses_empty_runs() {
    let dir = make_cli_test_dir("elephc_cli_test_empty");
    fs::create_dir_all(dir.join("tests")).unwrap();
    let (_, stderr, code) = run_test_command(&dir, &[]);
    assert!(stderr.contains("no test files found under 'tests'"), "{stderr}");
    assert_eq!(code, 1);
    let (_, stderr, code) = run_test_command(&dir, &["missing"]);
    assert!(stderr.contains("no such path 'missing'"), "{stderr}");
    assert_eq!(code, 1);

    fs::write(dir.join("tests/PlainTest.php"), "<?php\nclass PlainTest { public function testNothing(): void {} }\n").unwrap();
    let (_, stderr, code) = run_test_command(&dir, &[]);
    assert!(stderr.contains("no test classes found"), "{stderr}");
    assert_eq!(code, 1);

    write_project(&dir);
    let (_, stderr, code) = run_test_command(&dir, &["--filter", "nothing-matches"]);
    assert!(stderr.contains("no tests match --filter 'nothing-matches'"), "{stderr}");
    assert_eq!(code, 1);
    let _ = fs::remove_dir_all(&dir);
}

/// Output-mode and target flags are refused with the usage text; `--help` prints it and
/// exits 0; ordinary compile options such as `--strict-php` are forwarded.
#[test]
fn test_cli_test_refuses_output_modes_and_forwards_compile_options() {
    let dir = make_cli_test_dir("elephc_cli_test_flags");
    write_project(&dir);
    let (_, stderr, code) = run_test_command(&dir, &["--emit-ir"]);
    assert!(stderr.contains("--emit-ir is not accepted by `elephc test`"), "{stderr}");
    assert!(stderr.contains("Usage: elephc test"), "{stderr}");
    assert_eq!(code, 1);

    let (stdout, _, code) = run_test_command(&dir, &["--help"]);
    assert!(stdout.starts_with("Usage: elephc test [PATH]"), "{stdout}");
    assert_eq!(code, 0);

    // `--strict-php` reaches the compiler: the `.lfc` file keeps its extensions, the
    // tagged `.php` files are audited, and the suite still runs.
    let (stdout, stderr, code) = run_test_command(&dir, &["--strict-php", "--filter", "CounterTest"]);
    assert!(stdout.ends_with("OK (3 tests, 4 assertions)\n"), "{stdout}\n{stderr}");
    assert_eq!(code, 0);
    let _ = fs::remove_dir_all(&dir);
}

/// Writes a project whose tests exercise the lifecycle surface: a data provider, expected
/// exceptions, skipped and incomplete tests, inline property state, and a native fatal.
fn write_lifecycle_project(dir: &Path) {
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::write(dir.join("composer.json"), r#"{ "autoload-dev": { "psr-4": { "App\\Tests\\": "tests/" } } }"#).unwrap();
    fs::write(
        dir.join("tests/LifecycleTest.lfc"),
        r#"namespace App\Tests;
use Elephc\Testing\TestCase;
final class LifecycleTest extends TestCase {
    private array $seen = [];

    public static function sums(): array {
        return ['small' => [1, 2, 3], 'big' => [10, 20, 30], [0, 0, 0]];
    }

    #[DataProvider('sums')]
    public function testAdds(int $a, int $b, int $sum): void {
        $this->assertSame($sum, $a + $b);
    }

    public function testFreshInstanceOne(): void {
        $this->assertCount(0, $this->seen);
        $this->seen[] = 'one';
    }

    public function testFreshInstanceTwo(): void {
        $this->assertCount(0, $this->seen);
    }

    public function testExpectsException(): void {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('bad');
        throw new \InvalidArgumentException('very bad');
    }

    public function testExpectedButNotThrown(): void {
        $this->expectException(\RuntimeException::class);
    }

    public function testSkipped(): void {
        $this->markTestSkipped('no database here');
    }

    public function testIncomplete(): void {
        $this->markTestIncomplete();
    }

    public function testFatal(): void {
        $values = buffer_new<int>(1);
        $this->assertSame(0, $values[7]);
    }

    public function testAfterFatal(): void {
        $this->assertTrue(true);
    }
}
"#,
    )
    .unwrap();
}

/// Data-provider rows become labelled tests, every test gets a fresh instance, expected
/// exceptions pass or fail with PHPUnit's wording, skipped and incomplete tests are reported
/// without failing, and a native fatal in one test is an ERROR that does not stop the run.
#[test]
fn test_cli_test_lifecycle_surface_with_isolation() {
    let dir = make_cli_test_dir("elephc_cli_test_lifecycle");
    write_lifecycle_project(&dir);
    let (stdout, stderr, code) = run_test_command(&dir, &[]);
    assert_eq!(
        stdout,
        "ok   App\\Tests\\LifecycleTest::testAdds with data set \"small\" (1 assertion)\n\
         ok   App\\Tests\\LifecycleTest::testAdds with data set \"big\" (1 assertion)\n\
         ok   App\\Tests\\LifecycleTest::testAdds with data set #0 (1 assertion)\n\
         ok   App\\Tests\\LifecycleTest::testFreshInstanceOne (1 assertion)\n\
         ok   App\\Tests\\LifecycleTest::testFreshInstanceTwo (1 assertion)\n\
         ok   App\\Tests\\LifecycleTest::testExpectsException (2 assertions)\n\
         FAIL App\\Tests\\LifecycleTest::testExpectedButNotThrown\n\
         \x20    Failed asserting that exception of type \"RuntimeException\" is thrown.\n\
         SKIP App\\Tests\\LifecycleTest::testSkipped\n\
         \x20    no database here\n\
         INCOMPLETE App\\Tests\\LifecycleTest::testIncomplete\n\
         ERROR App\\Tests\\LifecycleTest::testFatal\n\
         \x20    Test process exited with code 70 before reporting a result\n\
         ok   App\\Tests\\LifecycleTest::testAfterFatal (1 assertion)\n\
         \n\
         Tests: 11, Assertions: 9, Failures: 1, Errors: 1, Skipped: 1, Incomplete: 1.\n",
        "stderr: {stderr}"
    );
    assert!(stderr.contains("Fatal error: buffer index out of bounds"), "{stderr}");
    assert_eq!(code, 1);
    let _ = fs::remove_dir_all(&dir);
}

/// Without isolation the native fatal ends the whole run with the fatal's own exit code:
/// everything before it is reported, nothing after it.
#[test]
fn test_cli_test_no_isolation_lets_a_fatal_end_the_run() {
    let dir = make_cli_test_dir("elephc_cli_test_no_isolation");
    write_lifecycle_project(&dir);
    let (stdout, stderr, code) = run_test_command(&dir, &["--no-isolation"]);
    assert!(stdout.contains("INCOMPLETE App\\Tests\\LifecycleTest::testIncomplete\n"), "{stdout}");
    assert!(!stdout.contains("testAfterFatal"), "the run must stop at the fatal: {stdout}");
    assert!(stderr.contains("Fatal error: buffer index out of bounds"), "{stderr}");
    assert_eq!(code, 70);
    let _ = fs::remove_dir_all(&dir);
}

/// A test running past `--timeout` is ended by the alarm and reported as an error with the
/// configured limit; the suite continues and the exit code reflects the error.
#[test]
fn test_cli_test_timeout_ends_a_slow_test() {
    let dir = make_cli_test_dir("elephc_cli_test_timeout");
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::write(
        dir.join("tests/SlowTest.php"),
        r#"<?php
use Elephc\Testing\TestCase;
final class SlowTest extends TestCase {
    public function testSlow(): void { usleep(3000000); $this->assertTrue(true); }
    public function testQuick(): void { $this->assertTrue(true); }
}
"#,
    )
    .unwrap();
    let (stdout, stderr, code) = run_test_command(&dir, &["--timeout", "1"]);
    assert_eq!(
        stdout,
        "ERROR SlowTest::testSlow\n\
         \x20    Test exceeded the timeout of 1 second\n\
         ok   SlowTest::testQuick (1 assertion)\n\
         \n\
         Tests: 2, Assertions: 1, Failures: 0, Errors: 1.\n",
        "stderr: {stderr}"
    );
    assert_eq!(code, 1);
    let _ = fs::remove_dir_all(&dir);
}

/// `--junit FILE` writes a JUnit report next to the caller, with one testcase per run test,
/// the totals on the suite, and failures, errors and skips as their JUnit elements.
#[test]
fn test_cli_test_writes_a_junit_report() {
    let dir = make_cli_test_dir("elephc_cli_test_junit");
    write_lifecycle_project(&dir);
    fs::create_dir_all(dir.join("out")).unwrap();
    let (_, stderr, code) = run_test_command(&dir, &["--junit", "out/report.xml", "--filter", "Fresh"]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let (_, stderr, code) = run_test_command(&dir, &["--junit", "out/report.xml"]);
    assert_eq!(code, 1, "stderr: {stderr}");
    let report = fs::read_to_string(dir.join("out/report.xml")).expect("the report is written where the caller pointed");
    assert!(report.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites>\n"), "{report}");
    assert!(
        report.contains("<testsuite name=\"elephc test\" tests=\"11\" assertions=\"9\" failures=\"1\" errors=\"1\" skipped=\"2\" time=\""),
        "{report}"
    );
    assert!(report.contains("<testcase classname=\"App\\Tests\\LifecycleTest\" name=\"testAdds with data set &quot;small&quot;\" time=\""), "{report}");
    assert!(report.contains("<failure type=\"failure\">Failed asserting that exception of type &quot;RuntimeException&quot; is thrown.</failure>"), "{report}");
    assert!(report.contains("<skipped message=\"no database here\" />"), "{report}");
    assert!(report.contains("<error type=\"error\">Test process exited with code 70 before reporting a result</error>"), "{report}");
    assert!(report.ends_with("  </testsuite>\n</testsuites>\n"), "{report}");
    let _ = fs::remove_dir_all(&dir);
}

/// Skipped and incomplete tests alone do not fail the run: the verdict is `OK, but there
/// were issues!` and the exit code is zero.
#[test]
fn test_cli_test_skipped_tests_do_not_fail_the_run() {
    let dir = make_cli_test_dir("elephc_cli_test_skipped");
    write_lifecycle_project(&dir);
    let (stdout, stderr, code) = run_test_command(&dir, &["--filter", "testSkipped"]);
    assert_eq!(
        stdout,
        "SKIP App\\Tests\\LifecycleTest::testSkipped\n\
         \x20    no database here\n\
         \n\
         OK, but there were issues!\n\
         Tests: 1, Assertions: 0, Skipped: 1.\n",
        "stderr: {stderr}"
    );
    assert_eq!(code, 0);
    let _ = fs::remove_dir_all(&dir);
}

/// A filter that names a provider row selects that row at run time.
#[test]
fn test_cli_test_filter_matches_provider_labels() {
    let dir = make_cli_test_dir("elephc_cli_test_provider_filter");
    write_lifecycle_project(&dir);
    let (stdout, stderr, code) = run_test_command(&dir, &["--filter", "data set \"big\""]);
    assert_eq!(
        stdout,
        "ok   App\\Tests\\LifecycleTest::testAdds with data set \"big\" (1 assertion)\n\
         \n\
         OK (1 test, 1 assertion)\n",
        "stderr: {stderr}"
    );
    assert_eq!(code, 0);
    let _ = fs::remove_dir_all(&dir);
}

/// An installed PHPUnit (and the packages only it requires) is left out of the suite's
/// autoload index: its `autoload.files` entry would otherwise be inlined and drag the
/// runner's own classes, with their run-time `require`s, into the compile. Other dev
/// packages stay available.
#[test]
fn test_cli_test_ignores_an_installed_phpunit() {
    let dir = make_cli_test_dir("elephc_cli_test_vendor_phpunit");
    write_project(&dir);
    for package in ["phpunit/phpunit", "sebastian/exporter", "acme/helpers"] {
        let package_dir = dir.join("vendor").join(package);
        fs::create_dir_all(package_dir.join("src")).unwrap();
        fs::write(
            package_dir.join("composer.json"),
            format!(r#"{{"name": "{package}", "autoload": {{"files": ["src/functions.php"]}}}}"#),
        )
        .unwrap();
    }
    // PHPUnit's real `Functions.php` shape: a file-level helper that reaches into the
    // package, here with the kind of dynamic include the compiler refuses.
    fs::write(
        dir.join("vendor/phpunit/phpunit/src/functions.php"),
        "<?php\n$loader = __DIR__ . '/Loader.php';\nrequire $loader;\n",
    )
    .unwrap();
    fs::write(
        dir.join("vendor/sebastian/exporter/src/functions.php"),
        "<?php\n$helper = __DIR__ . '/Helper.php';\nrequire $helper;\n",
    )
    .unwrap();
    fs::write(
        dir.join("vendor/acme/helpers/src/functions.php"),
        "<?php\nfunction acme_double(int $n): int { return $n * 2; }\n",
    )
    .unwrap();
    fs::create_dir_all(dir.join("vendor/composer")).unwrap();
    fs::write(
        dir.join("vendor/composer/installed.json"),
        r#"{"packages": [
            {"name": "phpunit/phpunit", "require": {"php": ">=8.3", "sebastian/exporter": "^6"}},
            {"name": "sebastian/exporter", "require": {}},
            {"name": "acme/helpers", "require": {}}
        ]}"#,
    )
    .unwrap();
    fs::write(
        dir.join("tests/HelpersTest.php"),
        "<?php\nnamespace App\\Tests;\nuse PHPUnit\\Framework\\TestCase;\nfinal class HelpersTest extends TestCase {\n    public function testVendorHelper(): void { $this->assertSame(4, \\acme_double(2)); }\n}\n",
    )
    .unwrap();
    let (stdout, stderr, code) = run_test_command(&dir, &["--filter", "HelpersTest"]);
    assert_eq!(
        stdout,
        "ok   App\\Tests\\HelpersTest::testVendorHelper (1 assertion)\n\nOK (1 test, 1 assertion)\n",
        "stderr: {stderr}"
    );
    assert_eq!(code, 0);
    let _ = fs::remove_dir_all(&dir);
}

/// Writes a project exercising the review fixes: trait-supplied tests, several providers on
/// one test, a provider that throws, a tearDown that throws after a failure, a binary failure
/// message, and a test that leaves a forked descendant running.
fn write_review_project(dir: &Path) {
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::write(dir.join("composer.json"), r#"{ "autoload-dev": { "psr-4": { "App\\Tests\\": "tests/" } } }"#).unwrap();
    fs::write(
        dir.join("tests/SharedChecks.lfc"),
        r#"namespace App\Tests;
trait SharedChecks {
    public function testFromTrait(): void { $this->assertTrue(true); }
    public function helper(): void { $this->assertTrue(true); }
}
"#,
    )
    .unwrap();
    fs::write(
        dir.join("tests/ReviewTest.lfc"),
        r#"namespace App\Tests;
use Elephc\Testing\TestCase;
final class ReviewTest extends TestCase {
    use SharedChecks { helper as public testAliasedHelper; }

    private bool $skipInTearDown = false;

    protected function tearDown(): void {
        if ($this->skipInTearDown) {
            $this->markTestSkipped('cleanup skip');
        }
    }

    public static function small(): array { return ['one' => [1]]; }
    public static function large(): array { return ['hundred' => [100]]; }
    public static function broken(): array { throw new \RuntimeException('provider broke'); }

    #[DataProvider('small')]
    #[DataProvider('large')]
    public function testPositive(int $n): void { $this->assertGreaterThan(0, $n); }

    #[DataProvider('broken')]
    public function testFromBrokenProvider(int $n): void { $this->assertSame(1, $n); }

    public function testFailureSurvivesTearDown(): void {
        $this->skipInTearDown = true;
        $this->assertSame(1, 2);
    }

    public function testBinaryMessage(): void {
        $this->fail("bad\0data");
    }

    public function testInvalidUtf8Message(): void {
        $this->fail("bad\xFFbyte \xC3\xA9t\xC3\xA9");
    }

    public function testLeavesADescendant(): void {
        file_put_contents(__DIR__ . '/descendant-started', (string) microtime(true));
        $pid = pcntl_fork();
        if ($pid === 0) {
            usleep(3000000);
            exit(0);
        }
        $this->assertTrue(true);
    }

    public function testRunsWhileTheDescendantLives(): void {
        $started = (float) file_get_contents(__DIR__ . '/descendant-started');
        $this->assertLessThan(2.0, microtime(true) - $started);
    }
}
"#,
    )
    .unwrap();
}

/// Trait-supplied tests (and aliases) are discovered, every provider of a test runs, a
/// provider that throws is that test's ERROR and the suite continues, a tearDown that throws
/// after a failure does not hide it, and a forked descendant does not block the runner.
#[test]
fn test_cli_test_review_fixes() {
    let dir = make_cli_test_dir("elephc_cli_test_review_fixes");
    write_review_project(&dir);
    let (stdout, stderr, code) = run_test_command(&dir, &["--junit", "report.xml"]);
    assert_eq!(
        stdout,
        "ok   App\\Tests\\ReviewTest::testPositive with data set \"one\" (1 assertion)\n\
         ok   App\\Tests\\ReviewTest::testPositive with data set \"hundred\" (1 assertion)\n\
         ERROR App\\Tests\\ReviewTest::testFromBrokenProvider\n\
         \x20    The data provider failed: RuntimeException: provider broke\n\
         FAIL App\\Tests\\ReviewTest::testFailureSurvivesTearDown\n\
         \x20    Failed asserting that 2 is identical to 1.\n\
         FAIL App\\Tests\\ReviewTest::testBinaryMessage\n\
         \x20    bad\0data\n\
         FAIL App\\Tests\\ReviewTest::testInvalidUtf8Message\n\
         \x20    bad\u{FFFD}byte \u{e9}t\u{e9}\n\
         ok   App\\Tests\\ReviewTest::testLeavesADescendant (1 assertion)\n\
         ok   App\\Tests\\ReviewTest::testRunsWhileTheDescendantLives (1 assertion)\n\
         ok   App\\Tests\\ReviewTest::testFromTrait (1 assertion)\n\
         ok   App\\Tests\\ReviewTest::testAliasedHelper (1 assertion)\n\
         \n\
         Tests: 10, Assertions: 7, Failures: 3, Errors: 1.\n",
        "stderr: {stderr}"
    );
    assert_eq!(code, 1);
    let bytes = fs::read(dir.join("report.xml")).expect("the report is written");
    let report = String::from_utf8(bytes).expect("the report must be valid UTF-8");
    assert!(!report.contains('\0'), "XML 1.0 forbids NUL: {report:?}");
    assert!(report.contains("<failure type=\"failure\">bad?data</failure>"), "{report}");
    assert!(
        report.contains("<failure type=\"failure\">bad?byte \u{e9}t\u{e9}</failure>"),
        "an invalid byte becomes `?` while valid UTF-8 is kept: {report}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A JUnit report that cannot be written is reported on stderr and fails an otherwise
/// green run, instead of disappearing silently.
#[test]
fn test_cli_test_unwritable_junit_report_fails_the_run() {
    let dir = make_cli_test_dir("elephc_cli_test_unwritable_junit");
    write_project(&dir);
    let (stdout, stderr, code) = run_test_command(
        &dir,
        &["--filter", "CounterTest::testStartsAtZero", "--junit", "missing/dir/report.xml"],
    );
    assert!(stdout.ends_with("OK (1 test, 1 assertion)\n"), "{stdout}");
    assert!(stderr.contains("Could not write the JUnit report to"), "{stderr}");
    assert_eq!(code, 1);
    let _ = fs::remove_dir_all(&dir);
}

/// Two concurrent runs of one project with different filters each run their own binary.
#[test]
fn test_cli_test_concurrent_runs_do_not_share_a_binary() {
    let dir = make_cli_test_dir("elephc_cli_test_concurrent");
    write_project(&dir);
    let spawn = |filter: &str| {
        elephc_cli_command(&dir)
            .args(["test", "--filter", filter])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to start elephc test")
    };
    let counter = spawn("CounterTest::testStartsAtZero");
    let strings = spawn("typedAssertions");
    let counter = counter.wait_with_output().expect("counter run");
    let strings = strings.wait_with_output().expect("strings run");
    let counter_out = String::from_utf8_lossy(&counter.stdout);
    let strings_out = String::from_utf8_lossy(&strings.stdout);
    assert_eq!(counter_out, "ok   App\\Tests\\CounterTest::testStartsAtZero (1 assertion)\n\nOK (1 test, 1 assertion)\n");
    assert_eq!(strings_out, "ok   App\\Tests\\Unit\\StringsTest::typedAssertions (2 assertions)\n\nOK (1 test, 2 assertions)\n");
    let _ = fs::remove_dir_all(&dir);
}

/// A hand-written entry whose only reference to the runtime is inside an autoloaded class
/// still gets the runtime: the post-autoload pass injects it.
#[test]
fn test_runtime_referenced_only_from_an_autoloaded_class() {
    let dir = make_cli_test_dir("elephc_cli_test_autoloaded_runner");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("composer.json"), r#"{ "autoload": { "psr-4": { "App\\": "src/" } } }"#).unwrap();
    fs::write(
        dir.join("src/Suite.php"),
        "<?php\nnamespace App;\nfinal class Suite {\n    public function run(): int {\n        $runner = new \\Elephc\\Testing\\Runner(false);\n        return $runner->finish();\n    }\n}\n",
    )
    .unwrap();
    fs::write(dir.join("main.php"), "<?php\n$suite = new App\\Suite();\nexit($suite->run() === 1 ? 0 : 1);\n").unwrap();
    let compile = elephc_cli_command(&dir).args(["main.php", "--quiet"]).output().expect("compile");
    assert!(compile.status.success(), "{}", String::from_utf8_lossy(&compile.stderr));
    let run = std::process::Command::new(dir.join("main")).output().expect("run");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "\nNo tests executed!\n");
    assert!(run.status.success());
    let _ = fs::remove_dir_all(&dir);
}

/// A provider that throws is reported whatever the filter: when the filter names only a data
/// set of that test, and when it names another test entirely. Discovery keeps provider tests
/// whenever a filter is set, so dropping the failure would hide it or turn the run green.
#[test]
fn test_cli_test_filter_never_hides_a_failing_provider() {
    let dir = make_cli_test_dir("elephc_cli_test_filter_provider_error");
    write_review_project(&dir);
    let (stdout, stderr, code) = run_test_command(&dir, &["--filter", "data set \"big\""]);
    assert!(
        stdout.contains("ERROR App\\Tests\\ReviewTest::testFromBrokenProvider\n     The data provider failed: RuntimeException: provider broke\n"),
        "{stdout}\n{stderr}"
    );
    assert_eq!(code, 1);
    let (stdout, stderr, code) = run_test_command(&dir, &["--filter", "testFromTrait"]);
    assert!(stdout.contains("ok   App\\Tests\\ReviewTest::testFromTrait (1 assertion)\n"), "{stdout}");
    assert!(stdout.contains("The data provider failed: RuntimeException: provider broke"), "{stdout}\n{stderr}");
    assert_eq!(code, 1, "a failing provider fails the run even when the filter names another test");
    let _ = fs::remove_dir_all(&dir);
}

/// A cleanup failure after a skipped or incomplete test is the test's outcome: the skip
/// cannot hide a broken tearDown() and turn the run green. A cleanup failure after a test
/// failure is still discarded in favor of that failure.
#[test]
fn test_cli_test_cleanup_failure_is_not_hidden_by_a_skip() {
    let dir = make_cli_test_dir("elephc_cli_test_cleanup_after_skip");
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::write(
        dir.join("tests/CleanupTest.php"),
        r#"<?php
use Elephc\Testing\TestCase;
final class CleanupTest extends TestCase {
    protected function tearDown(): void {
        $this->assertTrue(false, 'cleanup broke');
    }
    public function testSkipped(): void { $this->markTestSkipped('not here'); }
    public function testIncomplete(): void { $this->markTestIncomplete('later'); }
    public function testFails(): void { $this->assertSame(1, 2); }
}
"#,
    )
    .unwrap();
    let (stdout, stderr, code) = run_test_command(&dir, &[]);
    assert_eq!(
        stdout,
        "FAIL CleanupTest::testSkipped\n\
         \x20    cleanup broke\n\
         \x20    Failed asserting that false is true.\n\
         FAIL CleanupTest::testIncomplete\n\
         \x20    cleanup broke\n\
         \x20    Failed asserting that false is true.\n\
         FAIL CleanupTest::testFails\n\
         \x20    Failed asserting that 2 is identical to 1.\n\
         \n\
         Tests: 3, Assertions: 4, Failures: 3, Errors: 0.\n",
        "the tearDown() assertion after testFails is discarded but still counted: {stderr}"
    );
    assert_eq!(code, 1);
    let _ = fs::remove_dir_all(&dir);
}

/// `--coverage-format html` prints a self-contained line/branch report and
/// `--coverage-out coverage.html` writes it while keeping the text summary.
#[test]
fn test_cli_test_coverage_writes_html() {
    let dir = make_cli_test_dir("elephc_cli_test_coverage_html");
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::write(
        dir.join("tests/BranchTest.php"),
        "<?php\nuse Elephc\\Testing\\TestCase;\nfinal class BranchTest extends TestCase {\n    public function testBranches(): void {\n        $value = strlen(\"abc\");\n        if ($value > 0) {\n            $positive = 1;\n        } else {\n            $negative = 1;\n        }\n        if ($value < 0) {\n            $never = 1;\n        }\n        $this->assertTrue(true);\n    }\n}\n",
    )
    .expect("write HTML coverage fixture");

    let (stdout, stderr, code) = run_test_command(
        &dir,
        &["--filter", "testBranches", "--coverage-format", "html"],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert!(stdout.contains("<!doctype html>"), "{stdout}");
    assert!(
        stdout.contains("<title>elephc coverage report</title>"),
        "{stdout}"
    );
    assert!(stdout.contains("BranchTest.php"), "{stdout}");
    assert!(stdout.contains("if ($value &gt; 0)"), "{stdout}");
    assert!(stdout.contains("class=\"edge hit\""), "{stdout}");
    assert!(stdout.contains("class=\"edge miss\""), "{stdout}");
    assert!(!stdout.contains("Coverage:"), "{stdout}");

    let (stdout, stderr, code) = run_test_command(
        &dir,
        &[
            "--filter",
            "testBranches",
            "--coverage-out",
            "coverage.html",
        ],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert!(stdout.contains("Coverage:"), "{stdout}");
    assert!(stdout.contains("Branches:"), "{stdout}");
    assert!(
        stderr.contains("coverage report written to"),
        "{stderr}"
    );
    let html = fs::read_to_string(dir.join("coverage.html")).expect("coverage.html");
    assert!(html.starts_with("<!doctype html>"), "{html}");
    assert!(html.contains("class=\"covered\""), "{html}");
    assert!(html.contains("class=\"uncovered\""), "{html}");
    assert!(html.contains("if ($value &gt; 0)"), "{html}");
    assert!(html.contains("T 1"), "{html}");
    assert!(html.contains("E 0"), "{html}");

    let _ = fs::remove_dir_all(&dir);
}

/// `--coverage` records both edges of source-level conditionals and emits
/// LCOV `BRDA`/`BRF`/`BRH` plus Cobertura `branch`/`conditions` records.
#[test]
fn test_cli_test_branch_coverage_lcov_and_cobertura() {
    let dir = make_cli_test_dir("elephc_cli_test_branch_coverage");
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::write(
        dir.join("tests/BranchTest.php"),
        "<?php\nuse Elephc\\Testing\\TestCase;\nfinal class BranchTest extends TestCase {\n    public function testBranches(): void {\n        $value = strlen(\"abc\");\n        if ($value > 0) {\n            $positive = 1;\n        } else {\n            $negative = 1;\n        }\n        if ($value < 0) {\n            $never = 1;\n        }\n        $this->assertTrue(true);\n    }\n}\n",
    )
    .expect("write branch coverage fixture");

    let (stdout, stderr, code) = run_test_command(
        &dir,
        &[
            "--filter",
            "testBranches",
            "--coverage-format",
            "lcov",
        ],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let record = stdout
        .split("end_of_record")
        .find(|record| record.contains("BranchTest.php"))
        .unwrap_or_else(|| panic!("BranchTest LCOV record missing: {stdout}"));
    assert!(record.contains("BRDA:6,0,0,1"), "{record}");
    assert!(record.contains("BRDA:6,0,1,0"), "{record}");
    assert!(record.contains("BRDA:11,0,0,0"), "{record}");
    assert!(record.contains("BRDA:11,0,1,1"), "{record}");
    assert!(record.contains("BRF:4"), "{record}");
    assert!(record.contains("BRH:2"), "{record}");

    let (stdout, stderr, code) = run_test_command(
        &dir,
        &[
            "--filter",
            "testBranches",
            "--coverage-out",
            "coverage.xml",
        ],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert!(stdout.contains("Branches: 2/4 (50.0%)"), "{stdout}");
    let xml = fs::read_to_string(dir.join("coverage.xml")).expect("coverage.xml");
    let start = xml
        .find("<class name=\"BranchTest\"")
        .unwrap_or_else(|| panic!("BranchTest class missing: {xml}"));
    let end = xml[start..]
        .find("</class>")
        .map(|offset| start + offset)
        .expect("BranchTest class end");
    let class = &xml[start..end];
    assert!(class.contains("branch-rate=\"0.5000\""), "{class}");
    assert!(class.contains("condition-coverage=\"50% (1/2)\""), "{class}");
    assert!(
        class.contains("<condition number=\"0\" type=\"jump\" coverage=\"50%\"/>"),
        "{class}"
    );
    assert!(
        xml.contains("branches-covered=\"2\" branches-valid=\"4\""),
        "{xml}"
    );
    assert!(
        !xml.contains("<condition number=\"1\""),
        "each if has one branch: {xml}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `--coverage` attributes `&&`/`||`/`!` leaves and switch cases as branch
/// edges, and both LCOV and Cobertura report the same multi-edge counts.
#[test]
fn test_cli_test_logical_and_switch_branch_coverage() {
    let dir = make_cli_test_dir("elephc_cli_test_logical_switch_branch");
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::write(
        dir.join("tests/BranchTest.php"),
        "<?php\nuse Elephc\\Testing\\TestCase;\nfinal class BranchTest extends TestCase {\n    public function testBranches(): void {\n        $value = strlen(\"abc\");\n        $zero = strlen(\"\");\n        if ($value > 0 && $zero > 0) {\n            $never = 1;\n        }\n        if ($value > 0 || $zero > 0) {\n            $either = 1;\n        }\n        if (!$zero) {\n            $notZero = 1;\n        }\n        switch ($value) {\n            case 1:\n                $one = 1;\n                break;\n            case 3:\n                $three = 1;\n                break;\n            default:\n                $other = 1;\n        }\n        $this->assertTrue(true);\n    }\n}\n",
    )
    .expect("write logical/switch branch fixture");

    let (stdout, stderr, code) = run_test_command(
        &dir,
        &[
            "--filter",
            "testBranches",
            "--coverage-format",
            "lcov",
        ],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let record = stdout
        .split("end_of_record")
        .find(|record| record.contains("BranchTest.php"))
        .unwrap_or_else(|| panic!("BranchTest LCOV record missing: {stdout}"));
    // First `&&`: both leaves evaluated (left true, right false).
    assert!(record.contains("BRDA:7,0,0,1"), "{record}");
    assert!(record.contains("BRDA:7,0,1,0"), "{record}");
    assert!(record.contains("BRDA:7,1,0,0"), "{record}");
    assert!(record.contains("BRDA:7,1,1,1"), "{record}");
    // First `||` short-circuits: the right leaf is never evaluated.
    assert!(record.contains("BRDA:10,0,0,1"), "{record}");
    assert!(record.contains("BRDA:10,0,1,0"), "{record}");
    assert!(record.contains("BRDA:10,1,0,-"), "{record}");
    assert!(record.contains("BRDA:10,1,1,-"), "{record}");
    // `!$zero` is a single leaf; the false edge takes the outer then arm.
    assert!(record.contains("BRDA:13,0,0,0"), "{record}");
    assert!(record.contains("BRDA:13,0,1,1"), "{record}");
    // `switch`: one edge per case label plus default; case 3 matched.
    assert!(record.contains("BRDA:16,0,0,0"), "{record}");
    assert!(record.contains("BRDA:16,0,1,1"), "{record}");
    assert!(record.contains("BRDA:16,0,2,0"), "{record}");
    assert!(record.contains("BRF:13"), "{record}");
    assert!(record.contains("BRH:5"), "{record}");

    let (stdout, stderr, code) = run_test_command(
        &dir,
        &[
            "--filter",
            "testBranches",
            "--coverage-out",
            "coverage.xml",
        ],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert!(stdout.contains("Branches: 5/13 (38.5%)"), "{stdout}");
    let xml = fs::read_to_string(dir.join("coverage.xml")).expect("coverage.xml");
    let start = xml
        .find("<class name=\"BranchTest\"")
        .unwrap_or_else(|| panic!("BranchTest class missing: {xml}"));
    let end = xml[start..]
        .find("</class>")
        .map(|offset| start + offset)
        .expect("BranchTest class end");
    let class = &xml[start..end];
    assert!(class.contains("branch-rate=\"0.3846\""), "{class}");
    assert!(
        class.contains("line number=\"7\" hits=\"1\" branch=\"true\" condition-coverage=\"50% (2/4)\""),
        "{class}"
    );
    assert!(
        class.contains("line number=\"10\" hits=\"1\" branch=\"true\" condition-coverage=\"25% (1/4)\""),
        "{class}"
    );
    assert!(
        class.contains("line number=\"13\" hits=\"1\" branch=\"true\" condition-coverage=\"50% (1/2)\""),
        "{class}"
    );
    assert!(
        class.contains("line number=\"16\" hits=\"1\" branch=\"true\" condition-coverage=\"33% (1/3)\""),
        "{class}"
    );
    assert!(
        class.contains("<condition number=\"1\" type=\"jump\" coverage=\"0%\"/>"),
        "{class}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// `--coverage` records every full logical prefix combination and every switch
/// dispatch route, then renders the path metric in text, LCOV, Cobertura and HTML.
#[test]
fn test_cli_test_condition_and_path_coverage() {
    let dir = make_cli_test_dir("elephc_cli_test_path_coverage");
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::write(
        dir.join("tests/NestedTest.php"),
        "<?php\nuse Elephc\\Testing\\TestCase;\nfinal class NestedTest extends TestCase {\n    public function testNested(): void {\n        $t = strlen(\"t\") > 0;\n        $f = strlen(\"\") > 0;\n        $this->check($t, $f, $t, $f);\n        $this->check($f, $t, $t, $f);\n        $value = strlen(\"a\");\n        switch ($value) {\n            case 2:\n                $two = 1;\n                break;\n            case 3:\n                $three = 1;\n                break;\n            default:\n                $other = 1;\n        }\n        $this->assertTrue(true);\n    }\n    private function check(bool $a, bool $b, bool $c, bool $d): void {\n        if (($a || $b) && ($c || $d)) {\n            $hit = 1;\n        }\n    }\n}\n",
    )
    .expect("write nested path coverage fixture");

    let compile = elephc_cli_command(&dir)
        .args(["--debug", "tests/NestedTest.php"])
        .output()
        .expect("compile nested path coverage fixture");
    assert!(
        compile.status.success(),
        "path coverage compilation failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("tests/NestedTest.edbg.json")).expect("path metadata"),
    )
    .expect("valid path metadata");
    let groups = metadata["paths"].as_array().expect("paths table");
    assert_eq!(groups.len(), 2, "{metadata}");
    let logical = groups
        .iter()
        .find(|group| group["kind"] == "logical")
        .unwrap_or_else(|| panic!("missing logical path group: {metadata}"));
    assert_eq!(logical["line"], 23, "{metadata}");
    assert_eq!(logical["truncated"], false, "{metadata}");
    let logical_labels: Vec<&str> = logical["paths"]
        .as_array()
        .expect("logical paths")
        .iter()
        .filter_map(|path| path["label"].as_str())
        .collect();
    assert_eq!(logical_labels.len(), 7, "{metadata}");
    assert!(
        logical_labels.iter().any(|label| label.starts_with("#0=T,#2=T@")),
        "{metadata}"
    );
    assert!(
        logical_labels.iter().any(|label| label.starts_with("#0=T,#2=F,#3=T@")),
        "{metadata}"
    );
    assert!(
        logical_labels.iter().any(|label| label.starts_with("#0=F,#1=F@")),
        "{metadata}"
    );
    let switch_group = groups
        .iter()
        .find(|group| group["kind"] == "switch")
        .unwrap_or_else(|| panic!("missing switch path group: {metadata}"));
    assert_eq!(switch_group["line"], 10, "{metadata}");
    let switch_labels: Vec<&str> = switch_group["paths"]
        .as_array()
        .expect("switch paths")
        .iter()
        .filter_map(|path| path["label"].as_str())
        .collect();
    assert_eq!(switch_labels, ["case0", "case1", "default"], "{metadata}");

    let (stdout, stderr, code) = run_test_command(
        &dir,
        &[
            "--filter",
            "testNested",
            "--coverage-format",
            "lcov",
        ],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let record = stdout
        .split("end_of_record")
        .find(|record| record.contains("NestedTest.php"))
        .unwrap_or_else(|| panic!("NestedTest LCOV record missing: {stdout}"));
    assert!(record.contains("BRF:11"), "{record}");
    assert!(record.contains("BRH:5"), "{record}");
    assert!(record.contains("PATH:10,0,0\n"), "{record}");
    assert!(record.contains("PATH:10,1,0\n"), "{record}");
    assert!(record.contains("PATH:10,2,1\n"), "{record}");
    assert!(record.contains("PATH:23,3,1\n"), "{record}");
    assert!(record.contains("PATH:23,4,0\n"), "{record}");
    assert!(record.contains("PATH:23,5,0\n"), "{record}");
    assert!(record.contains("PATH:23,6,1\n"), "{record}");
    assert!(record.contains("PATH:23,7,0\n"), "{record}");
    assert!(record.contains("PATH:23,8,0\n"), "{record}");
    assert!(record.contains("PATH:23,9,0\n"), "{record}");
    assert!(record.contains("PTF:10"), "{record}");
    assert!(record.contains("PTH:3"), "{record}");

    let (stdout, stderr, code) = run_test_command(
        &dir,
        &[
            "--filter",
            "testNested",
            "--coverage-out",
            "coverage.xml",
        ],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert!(stdout.contains("Paths: 3/10 (30.0%)"), "{stdout}");
    assert!(stdout.contains("uncovered paths:"), "{stdout}");
    assert!(stdout.contains("#0=T,#2=F,#3=T"), "{stdout}");
    let xml = fs::read_to_string(dir.join("coverage.xml")).expect("coverage.xml");
    assert!(xml.contains("paths-covered=\"3\" paths-valid=\"10\""), "{xml}");
    assert!(xml.contains("path-rate=\"0.3000\""), "{xml}");
    assert!(
        xml.contains("line number=\"23\" hits=\"2\" branch=\"true\" condition-coverage=\"50% (4/8)\" path-coverage=\"28% (2/7)\""),
        "{xml}"
    );
    assert!(
        xml.contains("line number=\"10\" hits=\"1\" branch=\"true\" condition-coverage=\"33% (1/3)\" path-coverage=\"33% (1/3)\""),
        "{xml}"
    );
    assert!(xml.contains("type=\"path\""), "{xml}");

    let (stdout, stderr, code) = run_test_command(
        &dir,
        &[
            "--filter",
            "testNested",
            "--coverage-out",
            "coverage.html",
        ],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let html = fs::read_to_string(dir.join("coverage.html")).expect("coverage.html");
    assert!(
        html.contains("Path coverage: <strong>3/10 (30.0%)</strong>"),
        "{html}"
    );
    assert!(html.contains("paths 3/10"), "{html}");
    assert!(html.contains("<h3>Path coverage</h3>"), "{html}");
    assert!(html.contains("class=\"path hit\""), "{html}");
    assert!(html.contains("class=\"path miss\""), "{html}");
    assert!(html.contains("#0=T,#2=F,#3=T"), "{html}");
    assert!(html.contains("default"), "{html}");

    let _ = fs::remove_dir_all(&dir);
}

/// `--coverage-format lcov` prints LCOV records and `--coverage-out coverage.xml`
/// writes Cobertura XML while keeping the text summary on stdout.
#[test]
fn test_cli_test_coverage_writes_lcov_and_cobertura() {
    let dir = make_cli_test_dir("elephc_cli_test_coverage_formats");
    write_project(&dir);

    let (stdout, stderr, code) = run_test_command(
        &dir,
        &[
            "--filter",
            "testStartsAtZero",
            "--coverage-format",
            "lcov",
        ],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert!(stdout.contains("SF:"), "{stdout}");
    assert!(stdout.contains("DA:"), "{stdout}");
    assert!(stdout.contains("end_of_record"), "{stdout}");
    assert!(stdout.contains("tests/CounterTest.php"), "{stdout}");
    assert!(!stdout.contains("Coverage:"), "{stdout}");

    let (stdout, stderr, code) = run_test_command(
        &dir,
        &[
            "--filter",
            "testStartsAtZero",
            "--coverage-out",
            "coverage.xml",
        ],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert!(stdout.contains("Coverage:"), "{stdout}");
    assert!(
        stderr.contains("coverage report written to"),
        "{stderr}"
    );
    let xml = fs::read_to_string(dir.join("coverage.xml")).expect("coverage.xml");
    assert!(xml.starts_with("<?xml version=\"1.0\" ?>"), "{xml}");
    assert!(xml.contains("<coverage line-rate="), "{xml}");
    assert!(xml.contains("<line number="), "{xml}");
    assert!(xml.contains("filename=\"tests/CounterTest.php\""), "{xml}");
    assert!(!stdout.contains("<coverage"), "{stdout}");

    let _ = fs::remove_dir_all(&dir);
}

/// `--coverage` compiles the suite with the debug site guards, runs it in one
/// process, filters the profiler lines out of stderr, and reports line coverage
/// for the executed test file.
#[test]
fn test_cli_test_coverage_reports_executed_lines() {
    let dir = make_cli_test_dir("elephc_cli_test_coverage");
    write_project(&dir);
    let (stdout, stderr, code) = run_test_command(
        &dir,
        &["--filter", "testStartsAtZero", "--coverage"],
    );
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert!(stdout.contains("OK (1 test, 1 assertion)"), "{stdout}");
    assert!(stdout.contains("Coverage:"), "{stdout}");
    assert!(
        stdout.contains("tests/CounterTest.php"),
        "test source missing from coverage report: {stdout}"
    );
    assert!(
        !stderr.contains("elephc-debug-site:"),
        "profiler site lines leaked into stderr: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}
