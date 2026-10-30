<?php

#[Export]
function native_add(int $left, int $right): int
{
    return $left + $right;
}

#[Export]
function native_greet(string $name): string
{
    return "Hello, " . strtoupper($name) . "!";
}

#[Export]
function native_array(array $values): array
{
    $values["compiled"] = true;
    return $values;
}

#[Export]
function native_throw(): void
{
    throw new RuntimeException("example failure", 17);
}
