<?php

declare(strict_types=1);

namespace App\Tests;

use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

// Data providers, expected exceptions and skipped tests, written the PHPUnit way:
// the attributes and methods below are the ones PHPUnit itself defines.
final class ParserTest extends TestCase
{
    public static function integers(): array
    {
        return [
            'plain' => ['42', 42],
            'negative' => ['-7', -7],
            'spaces' => [' 3 ', 3],
        ];
    }

    #[DataProvider('integers')]
    public function testParsesIntegers(string $text, int $expected): void
    {
        $this->assertSame($expected, (int) trim($text));
    }

    public function testRejectsEmptyInput(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessage('empty');
        throw new \InvalidArgumentException('empty input');
    }

    public function testNeedsADatabase(): void
    {
        $this->markTestSkipped('no database in this example');
    }
}
