<?php
function resetStatic197(bool $reset): void {
    static $value = 1;
    if ($reset) { unset($value); $value = 'new'; }
    echo gettype($value), ':', $value, '|';
}
resetStatic197(true); resetStatic197(false);
function parameterStatic197(bool $reset, mixed $value): void {
    $value = 'early';
    static $value = 1;
    if ($reset) { unset($value); $value = 'new'; }
    echo gettype($value), ':', $value, '|';
}
$caller197 = [1];
parameterStatic197(true, $caller197); parameterStatic197(false, $caller197);
echo $caller197[0], '|';
function arrayStatic197(bool $reset): void {
    static $value = [1];
    if ($reset) { unset($value); $value = 'new'; }
    echo gettype($value), ':';
    if ($reset) { echo $value, '|'; } else { echo $value[0], '|'; }
}
arrayStatic197(true); arrayStatic197(false);
function aliasStatic197(bool $reset): void {
    static $value = 1;
    $old = function() use (&$value): mixed { return $value; };
    if ($reset) { unset($value); $value = 'new'; }
    echo $old(), ':', gettype($value), ':', $value, '|';
}
aliasStatic197(true); aliasStatic197(false);
function guardStatic197(bool $reset, mixed $next): void {
    static $value = 1;
    if ($reset) { unset($value); $value = 'new'; }
    try { $value = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
guardStatic197(false, 'bad'); guardStatic197(true, 2);
function evalStatic197(bool $reset): void {
    static $value = 1;
    if ($reset) { unset($value); $value = 'new'; }
    eval('');
    echo gettype($value), ':', $value, '|';
}
evalStatic197(true); evalStatic197(false);
