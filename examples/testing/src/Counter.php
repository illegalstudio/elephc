<?php

declare(strict_types=1);

namespace App;

final class Counter
{
    private int $value = 0;

    public function increment(int $by = 1): void
    {
        $this->value += $by;
    }

    public function value(): int
    {
        return $this->value;
    }
}
