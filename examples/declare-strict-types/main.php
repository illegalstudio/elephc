<?php
// This closure is defined in weak mode and keeps that mode after the next directive.
declare(ticks=1) $before = function (): string {
    return label(42);
};
// Ordinary lowered array groups keep autovivification inside a declare body.
declare(ticks=1) {
    $groups = [];
    $groups["numbers"][] = 7;
}
declare(strict_types=1);
; // An empty statement is harmless after the directive.

function label(string $value): string
{
    return "item:" . $value;
}

echo $before(), "\n";
echo label("ready"), "\n";
echo count($groups["numbers"]), ":", $groups["numbers"][0], "\n";
