<?php

#[Export]
function phpext_counter(): int
{
    static $counter = 0;
    ++$counter;
    return $counter;
}

#[Export]
function phpext_label(string $value): string
{
    static $label = 'label:';
    return $label . $value;
}

#[Export]
function phpext_mirror(array $value): array
{
    return $value;
}

#[Export]
function phpext_failure(): void
{
    throw new RuntimeException('test failure', 42);
}

#[Export]
function phpext_recurse(int $depth): int
{
    return $depth > 0 ? phpext_recurse($depth - 1) + 1 : 0;
}

#[Export]
function phpext_output(string $value): string
{
    echo $value;
    return $value;
}

#[Export]
function phpext_pause(string $message): void
{
    $values = [];
    $missing = $values['missing'];
    if ($missing !== null) { throw new RuntimeException('unexpected array value'); }
    throw new RuntimeException($message, 42);
}
