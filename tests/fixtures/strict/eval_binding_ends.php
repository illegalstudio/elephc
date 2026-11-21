<?php
function evalFreshCapture(): void {
    $value = 1;
    $old = function() use (&$value): mixed { return $value; };
    unset($value);
    $fresh = function() use (&$value): mixed { return $value; };
    $value = 'after';
    eval('');
    echo $old(), ':', gettype($fresh()), ':', $fresh(), '|';
}
evalFreshCapture();

function evalMaybeEnd(bool $reset, mixed $candidate): void {
    $value = 1;
    $old = function() use (&$value): mixed { return $value; };
    if ($reset) { unset($value); }
    $fresh = function(mixed $next) use (&$value): void { $value = $next; };
    eval('');
    try { $fresh($candidate); echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $old(), '|';
}
evalMaybeEnd(false, 'after');
evalMaybeEnd(true, 'after');

function evalOnlyReferenceBirth(): void {
    $old = function() use (&$value): mixed { return $value; };
    unset($value);
    eval('$value = "new";');
    echo gettype($old()), ':', $value, '|';
}
evalOnlyReferenceBirth();

function evalSingleProducer(): void {
    $value = null;
    $old = function() use (&$value): mixed { return $value; };
    unset($value);
    eval('$value = "new";');
    echo gettype($old()), ':', $value, '|';
}
evalSingleProducer();
