---
title: "Testing"
description: "The native test runtime: Elephc\\Testing\\TestCase, the supported PHPUnit-compatible assertion subset, the Runner and its report, and the PHPUnit\\Framework\\TestCase alias."
sidebar:
  order: 13
---

elephc ships a test runtime inside the compiler, so a suite tests the compiled binary
instead of php-src. It matters twice: a project that uses elephc extensions (`.lfc`,
`packed class`, `buffer<T>`, `extern`) cannot run PHPUnit at all, and a plain-PHP project
compiled by elephc wants to verify what the binary does, which can differ from Zend.

The runtime is a prelude: it is declared only when a program references it, costs nothing
otherwise, and needs no package to install. The design and the roadmap of the runner live
in `.plans/elephc-test.md`.

## Writing a test

A test class extends `Elephc\Testing\TestCase`. Public methods whose name starts with
`test`, or that carry `#[Test]`, are the tests; every test runs on a fresh instance, and
`setUp()` and `tearDown()` run around each one:

```php
<?php

namespace App\Tests;

use App\Counter;
use Elephc\Testing\TestCase;

final class CounterTest extends TestCase
{
    private Counter $counter;

    protected function setUp(): void
    {
        $this->counter = new Counter();
    }

    public function testStartsAtZero(): void
    {
        $this->assertSame(0, $this->counter->value());
    }

    public function testIncrements(): void
    {
        $this->counter->increment(2);
        $this->assertSameInt(2, $this->counter->value());
    }
}
```

A suite written against PHPUnit compiles unchanged: `PHPUnit\Framework\TestCase` is
declared as an alias of the elephc class, so the same file runs under php-src with the real
PHPUnit installed and under elephc with nothing installed. Only the subset documented here
exists; a PHPUnit method outside it is a compile error (`Undefined method`), never a silent
no-op.

## Running a suite

`elephc test` compiles the suite into a native binary and runs it:

```text
$ elephc test
ok   App\Tests\CounterTest::testStartsAtZero (1 assertion)
ok   App\Tests\CounterTest::testIncrements (1 assertion)

OK (2 tests, 2 assertions)
```

What it does, in order:

1. Collects every `*Test.php` and `*Test.lfc` file under the path (`tests` by default),
   recursively and in sorted order; a file path is taken as is.
2. Finds the project root: the nearest directory, starting at the path itself, with a
   `composer.json`. Its `autoload` and `autoload-dev` sections resolve project and test
   classes, so `Ivory\Tests\` mapped to `tests/` under `autoload-dev` is all a project
   declares. An installed `phpunit/phpunit`, and the packages only it requires (read from
   `vendor/composer/installed.json`), are left out of the index: the compiler provides
   that namespace itself, and PHPUnit's own `autoload.files` entry would otherwise pull
   the real runner into the compile. A package PHPUnit shares with the project or with
   another dependency stays, as does every other dev dependency.
3. Compiles a virtual entry that requires those files, with the testing runtime forced in.
   After autoloading, when every class the tests extend is known, it discovers the test
   classes: every concrete class whose ancestry reaches `Elephc\Testing\TestCase` (the
   PHPUnit alias included), in declaration order. A test is a public, non-static method
   whose name starts with `test` or that carries a `#[Test]` attribute; methods supplied
   by traits count (with `insteadof`, `as` aliases and visibility changes applied), and
   inherited test methods run on the concrete class after its own.
4. Appends the driver below to the program, builds the binary under the per-user cache
   in a directory of its own (so concurrent runs never share a binary), runs it with your
   working directory, and exits with the suite's exit code.

`--filter PATTERN` keeps the tests whose `Class::method` contains the pattern, before
compilation. Compile options after the path (`--strict-php`, `--with-<crate>`,
`--heap-size`, `--php-version`, `--define`, `--ini`) reach the compiler; output modes
and `--target` are refused. An empty run (no test files, no test classes, a filter that
matches nothing) is an error, not a green build. See the
[CLI reference](../compiling/cli-reference.md#test-runner-command) for the full table.

### Isolation

Every test runs in a forked child by default. The runner forks, the child runs
`setUp()`, the test and `tearDown()`, writes its outcome to a temporary file and exits;
the parent waits for the child, reads the outcome and prints the line. What this buys:

- A native fatal (a buffer index out of bounds, a null pointer dereference, a segfault in
  FFI code) ends only that child. The parent reports `ERROR` with the exit code or the
  signal (`Test process exited with code 70 before reporting a result`, `Test process
  killed by signal 11 (SIGSEGV)`) and runs the next test.
- A test that never returns is ended by `pcntl_alarm` after `--timeout` seconds (default
  60, `0` disables) and reported as `Test exceeded the timeout of 60 seconds`.
- Every test starts from a fresh process state, on top of its fresh instance.
- A process the test forks and leaves running cannot stall the run: the parent waits for
  its direct child only, and nothing the descendant inherits keeps it waiting.
- When the fork itself fails (a host at its process limit), the test is reported as an
  `ERROR` rather than run unprotected in the runner's process.

`--no-isolation` runs every test in the runner's own process: faster, and what a debugger
wants, but a native fatal then ends the whole run with the fatal's exit code. The
isolation is plain PHP in the runtime over the compiled `pcntl_*` builtins, so it exists
on every executable host (macOS ARM64, Linux ARM64, Linux x86_64) and nowhere else.

### Data providers

A `#[DataProvider('name')]` test runs once per row of the public static method `name()`,
which returns an array of argument rows. The row key names the run: `with data set "small"`
for a string key, `with data set #0` for an integer key, and `--filter` matches that label.

```php
<?php

use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

final class ParserTest extends TestCase
{
    public static function integers(): array
    {
        return ['plain' => ['42', 42], 'negative' => ['-7', -7]];
    }

    #[DataProvider('integers')]
    public function testParsesIntegers(string $text, int $expected): void
    {
        $this->assertSame($expected, (int) $text);
    }
}
```

A test may carry several `#[DataProvider]` attributes; it then runs once per row of each,
in attribute order. A provider that throws, or returns something other than an array, is
reported as that test's `ERROR` (`The data provider failed: ...`) and the run continues.
Providers run whenever a `--filter` is given, because the filter may name one of their data
sets, so a failing provider is reported, and fails the run, whatever the filter.

The row's elements are passed by position, one per declared parameter, so a row must
supply every parameter: the compiler cannot spread a run-time array into a typed call. For
the same reason a test with a data provider cannot end in a variadic parameter; discovery
reports that as a compile error rather than calling the test without the row's tail. The
provider may be named positionally, `#[DataProvider('rows')]`, or with PHPUnit's named
argument, `#[DataProvider(methodName: 'rows')]`.

### Expected exceptions, skipped and incomplete tests

`expectException(Foo::class)` makes the test pass when a `Foo`, or a direct subclass of
`Foo`, escapes it, and fail otherwise; `expectExceptionMessage('text')` additionally
requires the message to contain `text`. Deeper subclasses are not matched: that needs
`is_a()` with a run-time class name, the unfinished half of
[issue #1113](https://github.com/illegalstudio/elephc/issues/1113), so a test expecting
`\Exception` and throwing `\InvalidArgumentException` (two levels down) fails with
`Failed asserting that exception of type "InvalidArgumentException" matches expected
exception "Exception".` rather than passing.

`markTestSkipped('why')` and `markTestIncomplete('why')` stop the test and report it as
`SKIP` or `INCOMPLETE`. Neither fails the run: the verdict becomes `OK, but there were
issues!` with the counts, and the exit code stays `0`.

### Differential runs against PHPUnit

Because the alias namespace is what PHPUnit itself declares, a plain-PHP suite runs
unchanged under both. `scripts/test-differential.py PROJECT` runs `vendor/bin/phpunit`
and `elephc test` on the same project with a JUnit report each, and fails when a test
PHPUnit reports is missing from the compiled run or ends differently. CI runs it on
`examples/testing` with PHPUnit installed by Composer, which is the compiler's semantic
parity signal for everything a PHPUnit suite can express. Tests that only elephc runs,
from `.lfc` files PHPUnit cannot parse, are listed and never fail the comparison.

### JUnit report

`--junit FILE` writes a JUnit XML report: one `<testcase>` per run test with its class,
name and duration, `<failure>`, `<error>` and `<skipped>` children for the other outcomes,
and the totals on the `<testsuite>`. The file is resolved against the working directory.
Control characters XML forbids and bytes that are not valid UTF-8 are replaced with `?`,
so a binary failure message cannot corrupt the report. A report that cannot be written (a
missing directory, no permission) is reported on stderr and makes the run exit `1`.

### Driving a suite by hand

The driver `elephc test` synthesizes is ordinary PHP, and a suite can be an ordinary
program with that driver written out: one `Runner`, one call per test, the runner's
verdict as the exit code. This is what `examples/testing/main.php` does.

```php
<?php

use App\Tests\CounterTest;
use Elephc\Testing\Runner;

$runner = new Runner();

$counter = new CounterTest();
$runner->run($counter, 'testStartsAtZero', fn() => $counter->testStartsAtZero());
$runner->run($counter, 'testIncrements', fn() => $counter->testIncrements());

exit($runner->finish());
```

```text
$ elephc tests/main.php && ./tests/main
ok   App\Tests\CounterTest::testStartsAtZero (1 assertion)
ok   App\Tests\CounterTest::testIncrements (1 assertion)

OK (2 tests, 2 assertions)
```

The test is passed as a closure rather than by name on purpose: declaration reachability
compiles only what the program reaches, and `$case->testIncrements()` inside the closure is
what keeps the method in the binary. `elephc test` synthesizes exactly this driver from
the classes it discovers.

`Runner::run()` records one of five outcomes per test and prints one line for it:

| Line | Meaning |
|---|---|
| `ok   Class::method (n assertions)` | Every assertion passed. |
| `FAIL Class::method` | An assertion failed, `fail()` was called, or an expected exception was wrong or missing. The message follows, indented. |
| `ERROR Class::method` | The test threw anything else, died of a native fatal or a signal, or ran past the timeout. The reason follows. |
| `SKIP Class::method` | `markTestSkipped()` was called. |
| `INCOMPLETE Class::method` | `markTestIncomplete()` was called. |

`Runner::finish()` prints the totals and returns the exit code: `0` when no test failed
or errored and at least one ran; skipped and incomplete tests are counted but do not
fail the run.

```text
Tests: 4, Assertions: 12, Failures: 1, Errors: 1, Skipped: 1.
```

`tearDown()` always runs after a `setUp()` that succeeded, whatever the test did. When
the test failed or errored, an exception thrown by `tearDown()` is discarded, so cleanup
can never turn a failure into a pass or a skip; when the test passed, was skipped or was
incomplete, that exception is the test's outcome, so a broken cleanup cannot hide behind
a skip. A user
message given to an assertion is printed on its own line above the assertion text, as
PHPUnit prints it.

## Assertions

Every assertion is `protected`, returns `void`, takes an optional trailing
`string $message = ''`, counts itself, and throws `Elephc\Testing\AssertionFailedError`
when it fails. Failure text follows PHPUnit's wording where elephc can reproduce it.

| Assertion | Passes when |
|---|---|
| `assertTrue(bool $condition)` | `$condition` is `true` |
| `assertFalse(bool $condition)` | `$condition` is `false` |
| `assertSame(mixed $expected, mixed $actual)` | `$expected === $actual` |
| `assertNotSame(mixed $expected, mixed $actual)` | `$expected !== $actual` |
| `assertEquals(mixed $expected, mixed $actual)` | `$expected == $actual` |
| `assertNotEquals(mixed $expected, mixed $actual)` | `$expected != $actual` |
| `assertNull(mixed $actual)` | `$actual === null` |
| `assertNotNull(mixed $actual)` | `$actual !== null` |
| `assertEmpty(mixed $actual)` | `empty($actual)` |
| `assertNotEmpty(mixed $actual)` | `!empty($actual)` |
| `assertCount(int $expected, array $haystack)` | `count($haystack) === $expected` |
| `assertContains(mixed $needle, array $haystack)` | `in_array($needle, $haystack, true)` |
| `assertEqualsWithDelta(float $expected, float $actual, float $delta)` | `abs($expected - $actual) <= $delta` |
| `assertGreaterThan(int\|float $expected, int\|float $actual)` | `$actual > $expected` |
| `assertGreaterThanOrEqual(int\|float $expected, int\|float $actual)` | `$actual >= $expected` |
| `assertLessThan(int\|float $expected, int\|float $actual)` | `$actual < $expected` |
| `assertLessThanOrEqual(int\|float $expected, int\|float $actual)` | `$actual <= $expected` |
| `assertStringContainsString(string $needle, string $haystack)` | `str_contains($haystack, $needle)` |
| `assertStringStartsWith(string $prefix, string $string)` | `str_starts_with($string, $prefix)` |
| `assertStringEndsWith(string $suffix, string $string)` | `str_ends_with($string, $suffix)` |
| `fail(string $message = '')` | never: throws immediately |
| `expectException(string $class)` | the test ends with an exception of that class or a direct subclass |
| `expectExceptionMessage(string $text)` | the expected exception's message contains `$text` |
| `markTestSkipped(string $message = '')` | never: reports the test as skipped |
| `markTestIncomplete(string $message = '')` | never: reports the test as incomplete |

The `mixed` parameters box a scalar into a runtime cell. Four elephc-only variants take the
typed value directly and print the same failure text, for suites that assert in hot loops:

| Assertion | Passes when |
|---|---|
| `assertSameInt(int $expected, int $actual)` | `$expected === $actual` |
| `assertSameFloat(float $expected, float $actual)` | `$expected === $actual` |
| `assertSameString(string $expected, string $actual)` | `$expected === $actual` |
| `assertSameBool(bool $expected, bool $actual)` | `$expected === $actual` |

These four do not exist in PHPUnit: a suite that must also run under php-src uses the
`mixed` forms.

`assertInstanceOf` is deliberately absent. `is_a()` with a class name that is only known at
run time is the unfinished half of [issue #1113](https://github.com/illegalstudio/elephc/issues/1113)
and currently answers `false`, so the assertion would fail every time; write
`$this->assertTrue($value instanceof Foo)` instead, which the compiler resolves statically.

## Classes

| Class | Role |
|---|---|
| `Elephc\Testing\TestCase` | Abstract base of every test class: lifecycle hooks and the assertions above. |
| `Elephc\Testing\Runner` | `__construct(bool $isolate = true, int $timeout = 60, string $filter = '', string $junit = '')`, `run(TestCase $case, string $name, callable $test)`, `runWithData(TestCase $case, string $name, callable $test, mixed $dataSet)` and `finish(): int`. |
| `Elephc\Testing\AssertionFailedError` | Thrown by a failing assertion and by `fail()`; extends `Exception`. |
| `Elephc\Testing\SkippedTestError` | Thrown by `markTestSkipped()`. |
| `Elephc\Testing\IncompleteTestError` | Thrown by `markTestIncomplete()`. |
| `PHPUnit\Framework\TestCase` | Empty abstract subclass of `Elephc\Testing\TestCase`: the compatibility alias. |

## Limits

- `assertInstanceOf` and deeper-than-one-level `expectException()` matching wait on
  `is_a()` with a run-time class name (issue #1113).
- A data-provider row must supply every declared parameter; the compiler does not spread
  a run-time array into a typed call.
- `expectExceptionCode()`, `#[Depends]`, `#[Group]`, test doubles and the rest of
  PHPUnit's surface are not provided; a call to them is a compile error.
- TAP output is not provided; JUnit XML is the CI format.
