# Native test runner for elephc projects (`elephc test`)

A native test runner for projects compiled by elephc, run with one command,
`elephc test`, so suites exercise the compiled binary instead of php-src.

Ground truth for the assertion subset is PHPUnit 11 (method names, argument order,
failure messages where elephc can reproduce them). Everything else is elephc's own
design, and this document is its single source of truth. Code Journal plan
`93ff6343` tracks the same work.

## Tasks

- [x] 0. Design: this document
- [ ] 1. Runtime library: `Elephc\Testing\TestCase`, `Elephc\Testing\Runner`, the
      `PHPUnit\Framework\TestCase` alias, usable from a hand-written entry; Ivory's
      suite ported onto it as the acceptance test
- [x] 2. `elephc test` subcommand: test-file discovery, synthesized entry, project
      root and Composer resolution, compile-time test discovery inside the compiler,
      `--filter`
- [x] 3. Isolation: one forked child per test on the executable hosts, fatals and
      signals reported as failures, per-test timeout, `--no-isolation`
- [x] 4. Lifecycle: `setUp`/`tearDown` ordering guarantees, data providers,
      `expectException`, skipped and incomplete tests, a fresh instance per test
- [x] 5. Differential runs (php-src PHPUnit versus the compiled binary):
      `scripts/test-differential.py` and the `differential-tests` CI job on
      `examples/testing`; the installed PHPUnit is excluded from the suite's autoload
- [x] 6. JUnit XML output, docs page, example; TAP dropped until a consumer asks;
      Ivory migrated to `elephc test`

## Why

- Projects that use elephc extensions (`.lfc`, `packed class`, `buffer<T>`,
  `extern`) cannot run PHPUnit at all: php-src cannot parse them.
- Plain-PHP projects compiled by elephc need to verify the binary's behavior, which
  can differ from Zend's.
- The first consumer is Ivory (`illegalstudio/ivory`). Its hand-rolled harness lists
  every `TestCase` subclass by hand in `tests/main.lfc`, carries its own typed
  assertions in `tests/TestCase.lfc`, and needs a second `composer.json` inside
  `tests/` because elephc reads Composer metadata only from the entry file's
  directory. It works, and it shows exactly the gaps this runner closes.

## Constraints that shape the design

1. **Declaration reachability compiles only what the program reaches.** A test class
   nobody names never reaches the binary. Discovery therefore has to produce real
   `new` expressions inside the compiled program, which is why it happens inside
   the compiler and not in a runtime reflection loop.
2. **No runtime reflection or dynamic dispatch is needed.** `new $name` and
   `$case->$method()` are dynamic forms a static subset does not want on the test
   hot path. The synthesized entry calls every test through a closure:
   `fn() => $case->testFoo()`. That is also what a hand-written entry writes.
3. **`composer.json` is read from the entry file's directory.** The synthesized
   entry is a virtual file placed in the project root, so Composer's `autoload` and
   `autoload-dev` sections (elephc merges both) resolve test and project classes
   without a second `composer.json`.
4. **Native fatals end the process.** Buffer bounds, null pointer dereferences, and
   FFI segfaults cannot be caught in-process. Isolation (task 3) forks one child per
   test and turns an abnormal exit into a reported failure.
5. **`mixed` boxes.** Assertions taking `mixed` box scalars into runtime cells. The
   PHPUnit-named assertions accept `mixed` for compatibility; typed variants
   (`assertSameInt`, `assertSameFloat`, `assertSameString`, `assertSameBool`) avoid
   boxing in hot suites. Generic specialization of the `mixed` forms is a follow-up
   that depends on `synthetic_class` learning generic methods.

## Decisions

### Runtime lives in a compiler prelude

`src/testing_prelude.rs` declares the runtime through `crate::synthetic_class`
builders, exactly like the hash and xml preludes: the PHP form stays in the tree
under `cfg(test)` as the parse-parity oracle, and `build.rs` carries the builders.
A Composer package was rejected for the first version: discovery and entry
synthesis already live in the compiler, so splitting the runtime out would create a
version-coupling problem without removing any compiler work. A thin
`getelephc/testing` package can later re-export the same API for editors and for
php-src runs; it is not needed for the compiled path.

Injection is pay-for-use: `detect.rs` scans the program for any
`Elephc\Testing\*` or `PHPUnit\Framework\TestCase` reference (class-name
positions, `use` imports, `catch`, `extends`), and `elephc test` forces it. The
prelude carries only declarations, so prepending it is order-independent. Its
declarations are recorded in the `PreludeInventory` group `testing`.

### Namespace: `Elephc\Testing`, with a PHPUnit alias

The implementation is `Elephc\Testing\TestCase`. The prelude also declares
`PHPUnit\Framework\TestCase` as an empty abstract subclass of it, so a plain-PHP
suite written against PHPUnit compiles unchanged under elephc and still runs on
php-src with the real PHPUnit installed. Only the documented subset is provided;
a PHPUnit method outside it is a normal compile error ("undefined method"), never
a silent no-op. The alias is what makes the differential mode (task 5) a plain
double run.

Catalog entries (`crates/elephc-builtin-contract/src/catalog_classes.rs`), all
`route: Prelude`, `module: Elephc`:

| Class | Kind | Notes |
|---|---|---|
| `Elephc\Testing\TestCase` | abstract class | assertions, lifecycle hooks, `__elephc_run_test` |
| `Elephc\Testing\Runner` | final class | collects outcomes, prints the report, computes the exit code |
| `Elephc\Testing\AssertionFailedError` | final class extends `Exception` | thrown by every failing assertion and by `fail()` |
| `Elephc\Testing\SkippedTestError` | final class extends `Exception` | thrown by `markTestSkipped()` (task 4) |
| `PHPUnit\Framework\TestCase` | abstract class | alias, empty body |

### Runtime API (task 1)

```php
namespace Elephc\Testing;

abstract class TestCase
{
    protected function setUp(): void {}
    protected function tearDown(): void {}

    // Internal entry used by the runner: resets the assertion counter, runs
    // setUp, the test closure, then tearDown (also when the test throws).
    public function __elephc_run_test(callable $test): void;
    public function __elephc_assertion_count(): int;

    protected function fail(string $message = ''): void;   // throws AssertionFailedError
    protected function assertTrue(bool $condition, string $message = ''): void;
    protected function assertFalse(bool $condition, string $message = ''): void;
    protected function assertSame(mixed $expected, mixed $actual, string $message = ''): void;
    protected function assertNotSame(mixed $expected, mixed $actual, string $message = ''): void;
    protected function assertEquals(mixed $expected, mixed $actual, string $message = ''): void;
    protected function assertNotEquals(mixed $expected, mixed $actual, string $message = ''): void;
    protected function assertNull(mixed $actual, string $message = ''): void;
    protected function assertNotNull(mixed $actual, string $message = ''): void;
    protected function assertEmpty(mixed $actual, string $message = ''): void;
    protected function assertNotEmpty(mixed $actual, string $message = ''): void;
    protected function assertCount(int $expected, array $haystack, string $message = ''): void;
    protected function assertContains(mixed $needle, array $haystack, string $message = ''): void;
    protected function assertEqualsWithDelta(float $expected, float $actual, float $delta, string $message = ''): void;
    protected function assertGreaterThan(int|float $expected, int|float $actual, string $message = ''): void;
    protected function assertGreaterThanOrEqual(int|float $expected, int|float $actual, string $message = ''): void;
    protected function assertLessThan(int|float $expected, int|float $actual, string $message = ''): void;
    protected function assertLessThanOrEqual(int|float $expected, int|float $actual, string $message = ''): void;
    protected function assertStringContainsString(string $needle, string $haystack, string $message = ''): void;
    protected function assertStringStartsWith(string $prefix, string $string, string $message = ''): void;
    protected function assertStringEndsWith(string $suffix, string $string, string $message = ''): void;
    protected function assertInstanceOf(string $expected, mixed $actual, string $message = ''): void;

    // elephc-only typed variants: no boxing, same failure text.
    protected function assertSameInt(int $expected, int $actual, string $message = ''): void;
    protected function assertSameFloat(float $expected, float $actual, string $message = ''): void;
    protected function assertSameString(string $expected, string $actual, string $message = ''): void;
    protected function assertSameBool(bool $expected, bool $actual, string $message = ''): void;
}

final class Runner
{
    public function __construct() {}
    // Runs one test and records its outcome; prints one line per test.
    public function run(TestCase $case, string $name, callable $test): void;
    // Prints the summary; returns the process exit code (0 only when nothing failed
    // or errored and at least one test ran).
    public function finish(): int;
}
```

Failure messages follow PHPUnit's wording where elephc can reproduce it:
`Failed asserting that 2 is identical to 1.`, `Failed asserting that false is true.`,
`Failed asserting that actual size 3 matches expected size 2.`. Values are rendered
by a prelude-private exporter (`null`, `true`, `1`, `1.5`, `'text'`, `Array (3)`,
`ClassName Object`) so the prelude does not depend on the `var_export` prelude's
detection order. A user message, when given, is printed on its own line before the
assertion text, as PHPUnit does.

A test outcome is one of `passed`, `failed` (an `AssertionFailedError` escaped),
`error` (any other `Throwable` escaped), and later `skipped`. `tearDown` always
runs after `setUp` succeeded, whatever the test did.

Report format (stable, consumed by Ivory's `make test` and by the CLI tests):

```text
ok   Ivory\Tests\Time\ManualClockTest::testAdvance (3 assertions)
FAIL Ivory\Tests\Loop\FixedTimestepTest::testStep
     Failed asserting that 2 is identical to 1.
ERROR Ivory\Tests\Loop\GameLoopTest::testQuit
     RuntimeException: boom

Tests: 12, Assertions: 40, Failures: 1, Errors: 1.
```

The last line is `OK (12 tests, 40 assertions)` when everything passed.

Hand-written entry (task 1, and what Ivory uses until task 2 lands):

```php
$runner = new \Elephc\Testing\Runner();
$case = new ManualClockTest();
$runner->run($case, 'testAdvance', fn() => $case->testAdvance());
exit($runner->finish());
```

### `elephc test` (task 2)

```text
elephc test [PATH] [--filter PATTERN] [compile options...]
```

Flow, all inside the compiler process:

1. **Project root.** `PATH` defaults to `tests`. The root is the nearest ancestor
   of `PATH` (starting at `PATH` itself when it is a directory) that contains a
   `composer.json`; without one, the current directory. The synthesized entry is
   the virtual file `<root>/.elephc-test.lfc`: it is never written to disk, and
   its directory is what the Composer autoload registry reads, so `autoload` and
   `autoload-dev` from the root `composer.json` resolve test and project classes.
2. **Test files.** Every regular file under `PATH` whose name ends in `Test.php` or
   `Test.lfc` (PHPUnit's default suffix), sorted by path; a `PATH` that is a file is
   taken as is. The entry is one `require_once` per file, with absolute paths.
   `.php` files keep their tagged profile and `--strict-php` semantics, `.lfc` files
   are extension-enabled, through the normal per-file source profile.
3. **Compile-time discovery** runs as a pipeline pass after `autoload::run`, when
   every class the tests reference, including base classes autoloaded from `src/`,
   is in the AST with resolved names. It collects every non-abstract class whose
   ancestry reaches `Elephc\Testing\TestCase` (the PHPUnit alias included), in
   declaration order, and for each one its public, non-static, non-abstract methods
   that either start with `test` or carry a `#[Test]` attribute (the PHPUnit
   attribute, matched on the unqualified name), in declaration order.
4. **Synthesized driver** appended to the program's top level, as synthetic AST:

   ```php
   $__elephc_runner = new \Elephc\Testing\Runner();
   $__elephc_case_0 = new \Ivory\Tests\Time\ManualClockTest();
   $__elephc_runner->run($__elephc_case_0, 'testAdvance', fn() => $__elephc_case_0->testAdvance());
   ...
   exit($__elephc_runner->finish());
   ```

   One instance per class per test, matching PHPUnit (`new` per test method), is
   deferred to task 4 together with data providers; task 2 uses one instance per
   class because the lifecycle hooks already reset state.
5. **`--filter PATTERN`** keeps the `Class::method` pairs whose fully qualified
   `Class::method` string contains `PATTERN` (case-sensitive substring). It is
   applied at discovery time, so filtered-out tests are not compiled at all.
6. **Build and run.** The binary is written to a private directory under
   `std::env::temp_dir()` and removed after the run. The child runs with the
   caller's working directory and inherited stdio; its exit code is the command's
   exit code. A child killed by a signal exits with `128 + signal` and a one-line
   diagnostic. Zero discovered tests is a compile-time error (`no tests found under
   PATH`), exit code 1, matching PHPUnit's "No tests executed".
7. **Compile options** after the path are the ordinary compile flags (`--strict-php`,
   `--with-<crate>`, `--heap-size`, `--php-version`, `--define`, `--ini`,
   `--quiet`, ...). Output-mode flags (`--emit-*`, `--check`, `--emit-asm`,
   `--target` other than the host) are rejected: the runner always builds and runs
   a host executable.

Pipeline changes: `pipeline::compile_inner` currently keys three behaviors on
"this is the REPL host build" (`generated_source`, empty autoload registry, output
path override). Task 2 replaces that `Option<&str>` with an explicit `EntryKind`
describing generated source, autoload policy, output override, and test
discovery, so the REPL path is unchanged and the test path composes the pieces it
needs.

### Isolation (task 3)

`pcntl_fork`, `pcntl_waitpid`, `pcntl_wifexited`, `pcntl_wexitstatus`,
`pcntl_wifsignaled`, `pcntl_wtermsig`, and `pcntl_alarm` are compiled builtins on
every executable host, so isolation is PHP code inside the prelude, not new
runtime: the runner forks, the child runs the test and exits with a status that
encodes the outcome, and the parent maps an abnormal status to `ERROR` with the
signal name. Output and assertion counts cross the fork boundary through a
temporary file the parent reads after `pcntl_waitpid` (a pipe was the first design;
a process the test forks and leaves running inherits a pipe's write end and stalls
the parent's read, so the file won). A failed fork is an `ERROR`. Timeouts use
`pcntl_alarm` in the child. `--no-isolation` disables it for speed and for
debuggers. iOS targets are library-only and never run the runner.

### Differential mode (task 5)

Same plain-PHP suite, two runs: `vendor/bin/phpunit` on php-src and `elephc test`.
A CI job compares test-level outcomes, which is the compiler's semantic parity
signal for everything PHPUnit can express. The alias namespace is what makes this
a plain double run; elephc-only assertions (`assertSameInt`, ...) are not available
in that mode by construction.

### Output and docs (task 6)

`--junit FILE` (JUnit XML, the format every CI reader consumes) and `--tap`.
Docs: `docs/beyond-php/testing.md` (the runtime API and the subcommand),
`docs/compiling/cli-reference.md` (the subcommand synopsis and flags), `README.md`
mention, `examples/testing/` with a two-class suite and a `Makefile`-free
`elephc test` invocation. Ivory deletes `tests/main.lfc`, `tests/TestCase.lfc` and
`tests/composer.json`, moves `Ivory\Tests\` to `autoload-dev` in its root
`composer.json`, and `make test` becomes `elephc test`.

## Open items

- Generic specialization of the `mixed` assertions once `synthetic_class` can
  express generic methods; until then the typed variants carry the hot path.
- Benchmarks (paired C references with checksums, `.elephc` budgets) are related
  and out of scope for every task above.
