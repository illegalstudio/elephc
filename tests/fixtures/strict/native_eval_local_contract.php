<?php
function runScope(int $seed): void {
    $owner = $seed;
    $source = $seed > 0
        ? 'try { $owner = 2.5; echo "W"; } catch (TypeError $error) { echo "T"; }'
        : '';
    eval($source);
    echo gettype($owner), '|', $owner, '|';
}

function nullableNumericScope(?int $number, string $source): void {
    eval($source);
    echo gettype($number), '|', $number, '|';
}

function resetNumericScope(int $seed): void {
    $owner = $seed;
    $source = $seed > 0 ? 'unset($owner); $owner = 2.5; echo "R";' : '';
    eval($source);
    echo gettype($owner), '|', $owner, '|';
}

function futureNumericScope(int $seed): void {
    $source = $seed > 0 ? '$future = 2.5; echo "F", gettype($future), "|", $future;' : '';
    eval($source);
    if ($seed < 0) {
        $future = 0;
    }
    echo '|';
}

function unionNumericScope(int $seed): void {
    int|float $owner = $seed;
    $source = $seed > 0
        ? '$owner = 2.5; echo "U"; try { $owner = "bad"; echo "W"; } catch (TypeError $error) { echo "T"; }'
        : '';
    eval($source);
    echo gettype($owner), '|', $owner, '|';
}

function openNumericScope(int $seed): void {
    mixed $open = $seed;
    $source = $seed > 0 ? '$open = 2.5; echo "O";' : '';
    eval($source);
    echo gettype($open), '|', $open, '|';
}

runScope($argc);
nullableNumericScope($argc, $argc > 0 ? '$number = null; echo "N";' : '');
nullableNumericScope($argc, $argc > 0 ? '$number = 9007199254740993; echo "I";' : '');
resetNumericScope($argc);
futureNumericScope($argc);
unionNumericScope($argc);
openNumericScope($argc);
gc_collect_cycles();
