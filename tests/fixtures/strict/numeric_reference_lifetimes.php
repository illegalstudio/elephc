<?php
function replaceUntyped(&$target, mixed $candidate): void {
    $target = $candidate;
}

function detachNumeric(int $seed): void {
    $value = $seed;
    replaceUntyped($value, $seed);
    unset($value);
    $value = 2.5;
    replaceUntyped($value, 3.5);
    echo $value, '|';
}

function replaceParameter(int $number): void {
    try {
        replaceUntyped($number, 2.5);
        echo 'W';
    } catch (TypeError $error) {
        echo 'T';
    }
    echo gettype($number), '|', $number, '|';
}

function escapeNumeric(int $seed, mixed $candidate): void {
    $number = $seed;
    replaceUntyped($number, $candidate);
}

function nullableUntyped(?int $value, mixed $candidate): void {
    try {
        replaceUntyped($value, $candidate);
        echo 'W';
    } catch (TypeError $error) {
        echo 'T';
    }
    echo gettype($value), '|', $value, '|';
}

function alternateUntyped(&$target, mixed $candidate): void {
    $target = $candidate;
}

function descriptorNumeric(int $seed, mixed $replacement): void {
    $name = $seed > 0 ? 'replaceUntyped' : 'alternateUntyped';
    $number = $seed;
    try {
        $name($number, $replacement);
        echo 'W';
    } catch (TypeError $error) {
        echo 'T';
    }
    echo gettype($number), '|', $number, '|';
}

detachNumeric($argc);
replaceParameter($argc);
try {
    escapeNumeric($argc, 2.5);
    echo 'W';
} catch (TypeError $error) {
    echo 'T';
}
escapeNumeric($argc, 3);
echo 'E|';
int|float $both = 1;
replaceUntyped($both, 2.5);
echo $both, '|';
try {
    replaceUntyped($both, 'bad');
    echo 'W';
} catch (TypeError $error) {
    echo 'T';
}
echo gettype($both), '|', $both, '|';
mixed $open = 1;
replaceUntyped($open, 2.5);
echo $open, '|';
nullableUntyped($argc, 2.5);
nullableUntyped($argc, null);
descriptorNumeric($argc, 2.5);
gc_collect_cycles();
