<?php

declare(strict_types=1);

namespace App\Tests;

use App\Counter;
use PHPUnit\Framework\TestCase;

// A plain-PHP test class: the same file runs under php-src with PHPUnit installed
// and under elephc with nothing installed, because the alias namespace is what
// PHPUnit itself declares.
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
        $this->counter->increment();
        $this->counter->increment(2);
        $this->assertSame(3, $this->counter->value());
        $this->assertGreaterThan(2, $this->counter->value());
    }

    public function testSetUpRunsBeforeEveryTest(): void
    {
        // A fresh Counter: the increments of the previous test are gone.
        $this->assertSame(0, $this->counter->value());
    }
}
