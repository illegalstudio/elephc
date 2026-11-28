<?php
function writeAlias(&$target, mixed $candidate): void {
    $target = $candidate;
}

function aliasCaller(int $seed, mixed $candidate): void {
    $owner = $seed;
    try {
        writeAlias($owner, $candidate);
        echo 'W';
    } catch (TypeError $error) {
        echo 'T';
    }
    echo gettype($owner), '|', $owner, '|';
}

aliasCaller($argc, 2.5);
aliasCaller($argc, 3);
aliasCaller($argc, 9007199254740993);
gc_collect_cycles();
