//! Purpose:
//! The PHP form of the testing prelude, kept as the parse-parity oracle for the built
//! declarations in `crate::testing_prelude::build` and as the readable reference for the
//! runtime's behavior.
//!
//! Called from:
//! - `crate::testing_prelude::oracle_tests` and the transcription driver
//!   (`synthetic_class::transcribe::tests::dump_prelude_on_request`, `ELEPHC_TRANSCRIBE_WHICH=file`).
//!
//! Key details:
//! - NOT compiled into the prelude: `build.rs` carries the same declarations as AST. When
//!   this text changes, re-transcribe and update the builders, or the oracle fails.
//! - The two namespaces are BRACE blocks because the transcriber models `NamespaceBlock`;
//!   the alias extends the runtime class by its fully qualified name on purpose.
//! - `assertInstanceOf` is absent on purpose: `is_a()` with a runtime class name is the
//!   unfinished half of issue #1113.

/// The complete testing prelude as PHP source (with its `<?php` header).
pub(super) const SRC: &str = r##"<?php

namespace Elephc\Testing {

final class AssertionFailedError extends \Exception
{
}

final class SkippedTestError extends \Exception
{
}

final class IncompleteTestError extends \Exception
{
}

abstract class TestCase
{
    private int $__elephc_assertions = 0;
    private string $__elephc_expected_exception = '';
    private string $__elephc_expected_message = '';

    protected function setUp(): void
    {
    }

    protected function tearDown(): void
    {
    }

    public function __elephc_run_test(callable $test): void
    {
        $this->__elephc_assertions = 0;
        $this->__elephc_expected_exception = '';
        $this->__elephc_expected_message = '';
        $this->setUp();
        try {
            $this->__elephc_invoke($test);
        } catch (\Throwable $thrown) {
            if ($thrown instanceof SkippedTestError || $thrown instanceof IncompleteTestError) {
                $this->tearDown();
                throw $thrown;
            }
            $this->__elephc_tear_down_after_failure();
            throw $thrown;
        }
        $this->tearDown();
    }

    private function __elephc_tear_down_after_failure(): void
    {
        try {
            $this->tearDown();
        } catch (\Throwable) {
            return;
        }
    }

    private function __elephc_invoke(callable $test): void
    {
        try {
            $test();
        } catch (\Throwable $thrown) {
            if ($thrown instanceof AssertionFailedError || $thrown instanceof SkippedTestError || $thrown instanceof IncompleteTestError || $this->__elephc_expected_exception === '') {
                throw $thrown;
            }
            $this->__elephc_check_expected_exception($thrown);
            return;
        }
        if ($this->__elephc_expected_exception !== '') {
            $this->__elephc_fail('', 'Failed asserting that exception of type "' . $this->__elephc_expected_exception . '" is thrown.');
        }
    }

    private function __elephc_check_expected_exception(\Throwable $thrown): void
    {
        $expected = $this->__elephc_expected_exception;
        $actual = get_class($thrown);
        $parent = get_parent_class($thrown);
        $matches = strcasecmp($actual, $expected) === 0;
        if (!$matches && $parent !== false) {
            $matches = strcasecmp($parent, $expected) === 0;
        }
        if (!$matches) {
            $this->__elephc_fail('', 'Failed asserting that exception of type "' . $actual . '" matches expected exception "' . $expected . '".');
        }
        $this->__elephc_pass();
        if ($this->__elephc_expected_message === '') {
            return;
        }
        if (!str_contains($thrown->getMessage(), $this->__elephc_expected_message)) {
            $this->__elephc_fail('', "Failed asserting that exception message '" . $thrown->getMessage() . "' contains '" . $this->__elephc_expected_message . "'.");
        }
        $this->__elephc_pass();
    }

    protected function expectException(string $exception): void
    {
        $this->__elephc_expected_exception = ltrim($exception, '\\');
    }

    protected function expectExceptionMessage(string $message): void
    {
        $this->__elephc_expected_message = $message;
    }

    protected function markTestSkipped(string $message = ''): void
    {
        throw new SkippedTestError($message);
    }

    protected function markTestIncomplete(string $message = ''): void
    {
        throw new IncompleteTestError($message);
    }

    public function __elephc_assertion_count(): int
    {
        return $this->__elephc_assertions;
    }

    protected function fail(string $message = ''): void
    {
        throw new AssertionFailedError($message);
    }

    private function __elephc_pass(): void
    {
        $this->__elephc_assertions++;
    }

    private function __elephc_fail(string $message, string $description): void
    {
        $this->__elephc_assertions++;
        if ($message !== '') {
            throw new AssertionFailedError($message . "\n" . $description);
        }
        throw new AssertionFailedError($description);
    }

    private function __elephc_export(mixed $value): string
    {
        if ($value === null) {
            return 'null';
        }
        if (is_bool($value)) {
            return $value ? 'true' : 'false';
        }
        if (is_int($value) || is_float($value)) {
            return (string) $value;
        }
        if (is_string($value)) {
            return "'" . $value . "'";
        }
        if (is_array($value)) {
            return 'Array (' . count($value) . ')';
        }
        if (is_object($value)) {
            return get_class($value) . ' Object';
        }
        return gettype($value);
    }

    protected function assertTrue(bool $condition, string $message = ''): void
    {
        if ($condition) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that false is true.');
    }

    protected function assertFalse(bool $condition, string $message = ''): void
    {
        if (!$condition) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that true is false.');
    }

    protected function assertSame(mixed $expected, mixed $actual, string $message = ''): void
    {
        if ($expected === $actual) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . $this->__elephc_export($actual) . ' is identical to ' . $this->__elephc_export($expected) . '.');
    }

    protected function assertNotSame(mixed $expected, mixed $actual, string $message = ''): void
    {
        if ($expected !== $actual) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . $this->__elephc_export($actual) . ' is not identical to ' . $this->__elephc_export($expected) . '.');
    }

    protected function assertEquals(mixed $expected, mixed $actual, string $message = ''): void
    {
        if ($expected == $actual) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . $this->__elephc_export($actual) . ' matches expected ' . $this->__elephc_export($expected) . '.');
    }

    protected function assertNotEquals(mixed $expected, mixed $actual, string $message = ''): void
    {
        if ($expected != $actual) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . $this->__elephc_export($actual) . ' is not equal to ' . $this->__elephc_export($expected) . '.');
    }

    protected function assertNull(mixed $actual, string $message = ''): void
    {
        if ($actual === null) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . $this->__elephc_export($actual) . ' is null.');
    }

    protected function assertNotNull(mixed $actual, string $message = ''): void
    {
        if ($actual !== null) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that null is not null.');
    }

    protected function assertEmpty(mixed $actual, string $message = ''): void
    {
        if (empty($actual)) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . $this->__elephc_export($actual) . ' is empty.');
    }

    protected function assertNotEmpty(mixed $actual, string $message = ''): void
    {
        if (!empty($actual)) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . $this->__elephc_export($actual) . ' is not empty.');
    }

    protected function assertCount(int $expected, array $haystack, string $message = ''): void
    {
        $actual = count($haystack);
        if ($expected === $actual) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that actual size ' . $actual . ' matches expected size ' . $expected . '.');
    }

    protected function assertContains(mixed $needle, array $haystack, string $message = ''): void
    {
        if (in_array($needle, $haystack, true)) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that an array contains ' . $this->__elephc_export($needle) . '.');
    }

    protected function assertEqualsWithDelta(float $expected, float $actual, float $delta, string $message = ''): void
    {
        if (abs($expected - $actual) <= $delta) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . $actual . ' matches expected ' . $expected . ' within delta ' . $delta . '.');
    }

    protected function assertGreaterThan(int|float $expected, int|float $actual, string $message = ''): void
    {
        if ($actual > $expected) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . $actual . ' is greater than ' . $expected . '.');
    }

    protected function assertGreaterThanOrEqual(int|float $expected, int|float $actual, string $message = ''): void
    {
        if ($actual >= $expected) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . $actual . ' is equal to or greater than ' . $expected . '.');
    }

    protected function assertLessThan(int|float $expected, int|float $actual, string $message = ''): void
    {
        if ($actual < $expected) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . $actual . ' is less than ' . $expected . '.');
    }

    protected function assertLessThanOrEqual(int|float $expected, int|float $actual, string $message = ''): void
    {
        if ($actual <= $expected) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . $actual . ' is equal to or less than ' . $expected . '.');
    }

    protected function assertStringContainsString(string $needle, string $haystack, string $message = ''): void
    {
        if (str_contains($haystack, $needle)) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, "Failed asserting that '" . $haystack . "' contains \"" . $needle . "\".");
    }

    protected function assertStringStartsWith(string $prefix, string $string, string $message = ''): void
    {
        if (str_starts_with($string, $prefix)) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, "Failed asserting that '" . $string . "' starts with \"" . $prefix . "\".");
    }

    protected function assertStringEndsWith(string $suffix, string $string, string $message = ''): void
    {
        if (str_ends_with($string, $suffix)) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, "Failed asserting that '" . $string . "' ends with \"" . $suffix . "\".");
    }

    protected function assertSameInt(int $expected, int $actual, string $message = ''): void
    {
        if ($expected === $actual) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . $actual . ' is identical to ' . $expected . '.');
    }

    protected function assertSameFloat(float $expected, float $actual, string $message = ''): void
    {
        if ($expected === $actual) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . $actual . ' is identical to ' . $expected . '.');
    }

    protected function assertSameString(string $expected, string $actual, string $message = ''): void
    {
        if ($expected === $actual) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, "Failed asserting that '" . $actual . "' is identical to '" . $expected . "'.");
    }

    protected function assertSameBool(bool $expected, bool $actual, string $message = ''): void
    {
        if ($expected === $actual) {
            $this->__elephc_pass();
            return;
        }
        $this->__elephc_fail($message, 'Failed asserting that ' . ($actual ? 'true' : 'false') . ' is identical to ' . ($expected ? 'true' : 'false') . '.');
    }
}

final class Runner
{
    private int $tests = 0;
    private int $assertions = 0;
    private int $failures = 0;
    private int $errors = 0;
    private int $skipped = 0;
    private int $incomplete = 0;
    private bool $isolate;
    private int $timeout;
    private string $filter;
    private string $junit;
    private bool $reportFailed = false;
    private array $results = [];
    private float $started;

    public function __construct(bool $isolate = true, int $timeout = 60, string $filter = '', string $junit = '')
    {
        $this->isolate = $isolate;
        $this->timeout = $timeout;
        $this->filter = $filter;
        $this->junit = $junit;
        $this->started = microtime(true);
    }

    public function run(TestCase $case, string $name, callable $test): void
    {
        $this->runLabelled($case, $name, $test);
    }

    public function rows(string $class, string $name, callable $provider): array
    {
        try {
            $rows = $provider();
            if (is_array($rows)) {
                return (array) $rows;
            }
            throw new \UnexpectedValueException('it returned ' . gettype($rows) . ', not an array');
        } catch (\Throwable $error) {
            $label = $class . '::' . $name;
            $this->tests++;
            $this->record($class, $name, $label, "error\n0\nThe data provider failed: " . get_class($error) . ': ' . $error->getMessage(), 0.0);
            return [];
        }
    }

    public function runWithData(TestCase $case, string $name, callable $test, mixed $dataSet): void
    {
        if (is_int($dataSet)) {
            $this->runLabelled($case, $name . ' with data set #' . $dataSet, $test);
            return;
        }
        $this->runLabelled($case, $name . ' with data set "' . $dataSet . '"', $test);
    }

    private function runLabelled(TestCase $case, string $name, callable $test): void
    {
        $class = get_class($case);
        $label = $class . '::' . $name;
        if ($this->filter !== '' && !str_contains($label, $this->filter)) {
            return;
        }
        $this->tests++;
        $began = microtime(true);
        if ($this->isolate) {
            $record = $this->runIsolated($case, $test);
        } else {
            $record = $this->execute($case, $test);
        }
        $this->record($class, $name, $label, $record, microtime(true) - $began);
    }

    private function execute(TestCase $case, callable $test): string
    {
        try {
            $case->__elephc_run_test($test);
            return "passed\n" . $case->__elephc_assertion_count() . "\n";
        } catch (AssertionFailedError $failure) {
            return "failed\n" . $case->__elephc_assertion_count() . "\n" . $failure->getMessage();
        } catch (SkippedTestError $skipped) {
            return "skipped\n" . $case->__elephc_assertion_count() . "\n" . $skipped->getMessage();
        } catch (IncompleteTestError $incomplete) {
            return "incomplete\n" . $case->__elephc_assertion_count() . "\n" . $incomplete->getMessage();
        } catch (\Throwable $error) {
            return "error\n" . $case->__elephc_assertion_count() . "\n" . get_class($error) . ': ' . $error->getMessage();
        }
    }

    private function runIsolated(TestCase $case, callable $test): string
    {
        $path = tempnam(sys_get_temp_dir(), 'elephc-test-');
        if ($path === false || $path === '') {
            return "error\n0\nCould not create the isolated test's result file";
        }
        $pid = pcntl_fork();
        if ($pid === -1) {
            unlink($path);
            return "error\n0\nCould not fork an isolated test process";
        }
        if ($pid === 0) {
            if ($this->timeout > 0) {
                pcntl_alarm($this->timeout);
            }
            $childRecord = $this->execute($case, $test);
            $written = file_put_contents($path, $childRecord);
            if (!is_int($written) || $written !== strlen($childRecord)) {
                exit(1);
            }
            exit(0);
        }
        $status = 0;
        pcntl_waitpid($pid, $status);
        $lines = file($path);
        unlink($path);
        $record = $lines === false ? '' : implode('', $lines);
        if ($record !== '' && pcntl_wifexited($status) && pcntl_wexitstatus($status) === 0) {
            return $record;
        }
        if (pcntl_wifsignaled($status)) {
            $signal = pcntl_wtermsig($status);
            if ($signal === 14) {
                return "error\n0\nTest exceeded the timeout of " . $this->timeout . ($this->timeout === 1 ? ' second' : ' seconds');
            }
            return "error\n0\nTest process killed by signal " . $signal . $this->signalName($signal);
        }
        return "error\n0\nTest process exited with code " . pcntl_wexitstatus($status) . " before reporting a result";
    }

    private function signalName(int $signal): string
    {
        if ($signal === 6) {
            return ' (SIGABRT)';
        }
        if ($signal === 8) {
            return ' (SIGFPE)';
        }
        if ($signal === 9) {
            return ' (SIGKILL)';
        }
        if ($signal === 11) {
            return ' (SIGSEGV)';
        }
        if ($signal === 15) {
            return ' (SIGTERM)';
        }
        return '';
    }

    private function record(string $class, string $name, string $label, string $record, float $seconds): void
    {
        $parts = explode("\n", $record, 3);
        $status = $parts[0];
        $count = count($parts) > 1 ? (int) $parts[1] : 0;
        $message = count($parts) > 2 ? $parts[2] : '';
        $this->assertions += $count;
        if ($status === 'passed') {
            echo 'ok   ' . $label . ' (' . $count . ($count === 1 ? ' assertion)' : ' assertions)') . "\n";
        } elseif ($status === 'failed') {
            $this->failures++;
            echo 'FAIL ' . $label . "\n";
            $this->echoIndented($message);
        } elseif ($status === 'skipped') {
            $this->skipped++;
            echo 'SKIP ' . $label . "\n";
            $this->echoIndented($message);
        } elseif ($status === 'incomplete') {
            $this->incomplete++;
            echo 'INCOMPLETE ' . $label . "\n";
            $this->echoIndented($message);
        } else {
            $this->errors++;
            echo 'ERROR ' . $label . "\n";
            $this->echoIndented($message);
        }
        $this->results[] = [$class, $name, $status, $message, number_format($seconds, 6, '.', '')];
    }

    private function echoIndented(string $text): void
    {
        if ($text === '') {
            return;
        }
        $lines = explode("\n", $text);
        foreach ($lines as $line) {
            echo '     ' . $line . "\n";
        }
    }

    public function finish(): int
    {
        echo "\n";
        if ($this->junit !== '') {
            $this->writeJunit();
        }
        if ($this->tests === 0) {
            echo "No tests executed!\n";
            return 1;
        }
        if ($this->failures === 0 && $this->errors === 0) {
            if ($this->skipped === 0 && $this->incomplete === 0) {
                echo 'OK (' . $this->tests . ($this->tests === 1 ? ' test, ' : ' tests, ') . $this->assertions . ($this->assertions === 1 ? ' assertion)' : ' assertions)') . "\n";
                return $this->reportFailed ? 1 : 0;
            }
            echo "OK, but there were issues!\n";
            echo 'Tests: ' . $this->tests . ', Assertions: ' . $this->assertions . $this->issueCounts() . '.' . "\n";
            return $this->reportFailed ? 1 : 0;
        }
        echo 'Tests: ' . $this->tests . ', Assertions: ' . $this->assertions . ', Failures: ' . $this->failures . ', Errors: ' . $this->errors . $this->issueCounts() . '.' . "\n";
        return 1;
    }

    private function issueCounts(): string
    {
        $text = '';
        if ($this->skipped > 0) {
            $text .= ', Skipped: ' . $this->skipped;
        }
        if ($this->incomplete > 0) {
            $text .= ', Incomplete: ' . $this->incomplete;
        }
        return $text;
    }

    private function writeJunit(): void
    {
        $xml = '<?xml version="1.0" encoding="UTF-8"?>' . "\n";
        $xml .= '<testsuites>' . "\n";
        $xml .= '  <testsuite name="elephc test" tests="' . $this->tests . '" assertions="' . $this->assertions . '" failures="' . $this->failures . '" errors="' . $this->errors . '" skipped="' . ($this->skipped + $this->incomplete) . '" time="' . number_format(microtime(true) - $this->started, 6, '.', '') . '">' . "\n";
        foreach ($this->results as $result) {
            $status = $result[2];
            $xml .= '    <testcase classname="' . $this->xmlEscape($result[0]) . '" name="' . $this->xmlEscape($result[1]) . '" time="' . $result[4] . '"';
            if ($status === 'passed') {
                $xml .= ' />' . "\n";
            } elseif ($status === 'failed') {
                $xml .= '>' . "\n" . '      <failure type="failure">' . $this->xmlEscape($result[3]) . '</failure>' . "\n" . '    </testcase>' . "\n";
            } elseif ($status === 'skipped' || $status === 'incomplete') {
                $xml .= '>' . "\n" . '      <skipped message="' . $this->xmlEscape($result[3]) . '" />' . "\n" . '    </testcase>' . "\n";
            } else {
                $xml .= '>' . "\n" . '      <error type="error">' . $this->xmlEscape($result[3]) . '</error>' . "\n" . '    </testcase>' . "\n";
            }
        }
        $xml .= '  </testsuite>' . "\n";
        $xml .= '</testsuites>' . "\n";
        $written = file_put_contents($this->junit, $xml);
        if (!is_int($written) || $written !== strlen($xml)) {
            $this->reportFailed = true;
            fwrite(STDERR, 'Could not write the JUnit report to ' . $this->junit . "\n");
        }
    }

    private function xmlEscape(string $text): string
    {
        $text = strtr($text, "\x00\x01\x02\x03\x04\x05\x06\x07\x08\x0b\x0c\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f", '?????????????????????????????');
        return htmlspecialchars($this->scrubUtf8($text), ENT_XML1 | ENT_QUOTES, 'UTF-8');
    }

    private function scrubUtf8(string $text): string
    {
        $clean = '';
        $length = strlen($text);
        $i = 0;
        while ($i < $length) {
            $byte = ord($text[$i]);
            if ($byte < 128) {
                $clean = $clean . $text[$i];
                $i++;
                continue;
            }
            $width = 0;
            $minimum = 0;
            if ($byte >= 194 && $byte <= 223) {
                $width = 2;
                $minimum = 128;
            } elseif ($byte >= 224 && $byte <= 239) {
                $width = 3;
                $minimum = 2048;
            } elseif ($byte >= 240 && $byte <= 244) {
                $width = 4;
                $minimum = 65536;
            }
            if ($width === 0 || $i + $width > $length) {
                $clean = $clean . '?';
                $i++;
                continue;
            }
            $code = $byte & (255 >> ($width + 1));
            $valid = true;
            $k = 1;
            while ($k < $width) {
                $next = ord($text[$i + $k]);
                if (($next & 192) !== 128) {
                    $valid = false;
                }
                $code = ($code << 6) | ($next & 63);
                $k++;
            }
            if (!$valid || $code < $minimum || $code > 1114111 || ($code >= 55296 && $code <= 57343) || $code === 65534 || $code === 65535) {
                $clean = $clean . '?';
                $i++;
                continue;
            }
            $clean = $clean . substr($text, $i, $width);
            $i = $i + $width;
        }
        return $clean;
    }
}

}

namespace PHPUnit\Framework {

abstract class TestCase extends \Elephc\Testing\TestCase
{
}

}
"##;
