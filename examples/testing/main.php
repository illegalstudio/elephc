<?php

declare(strict_types=1);

// The same suite `elephc test` discovers, driven by hand: one Runner, one closure
// per test, the runner's verdict as the exit code. `elephc test` synthesizes
// exactly this entry from the classes it finds under tests/.

require_once __DIR__ . '/src/Counter.php';
require_once __DIR__ . '/tests/CounterTest.php';
require_once __DIR__ . '/tests/StringsTest.lfc';

use App\Tests\CounterTest;
use App\Tests\StringsTest;
use Elephc\Testing\Runner;

$runner = new Runner();

$counter = new CounterTest();
$runner->run($counter, 'testStartsAtZero', fn() => $counter->testStartsAtZero());
$runner->run($counter, 'testIncrements', fn() => $counter->testIncrements());
$runner->run($counter, 'testSetUpRunsBeforeEveryTest', fn() => $counter->testSetUpRunsBeforeEveryTest());

$strings = new StringsTest();
$runner->run($strings, 'testHaystacks', fn() => $strings->testHaystacks());
$runner->run($strings, 'typedAssertionsDoNotBox', fn() => $strings->typedAssertionsDoNotBox());

exit($runner->finish());
