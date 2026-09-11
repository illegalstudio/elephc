<?php
#[Export]
function mb_size(string $text): int { return mb_strlen($text); }

#[Export]
function mb_weight(string $text): float { return (float)mb_strlen($text) + 0.5; }

#[Export]
function mb_switch(string $encoding): bool { return (bool)mb_internal_encoding($encoding); }

#[Export]
function mb_read(): string { return (string)mb_internal_encoding(); }

#[Export]
function mb_combine(string $left, int $count, float $ratio, bool $enabled,
    string $right, int $a, int $b, int $c): string {
    if ($count !== 7 || $ratio !== 1.5 || !$enabled || $a !== 11 || $b !== 13 || $c !== 17) {
        return "bad arguments";
    }
    return mb_strtoupper($left, "UTF-8") . $right . ":" . mb_internal_encoding();
}
