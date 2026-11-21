<?php
$value = 1;
$readValue = function(bool $reset) use ($value): void {
    if ($reset) { unset($value); $value = 'new'; }
    echo gettype($value), ':', $value, '|';
};
$readValue(true);
$readValue(false);
echo $value, '|';
$readReference = function(bool $reset) use (&$value): void {
    if ($reset) { unset($value); $value = 'new'; }
    echo gettype($value), ':', $value, '|';
};
$readReference(true);
$readReference(false);
echo $value, '|';
$resetValue = function() use ($value): void {
    unset($value); $value = 'new'; echo gettype($value), ':', $value, '|';
};
$resetValue(); $resetValue(); echo $value, '|';
$resetReference = function() use (&$value): void {
    unset($value); $value = 'new'; echo gettype($value), ':', $value, '|';
};
$resetReference(); $resetReference(); echo $value, '|';
function openCapture192(mixed $value): void {
    $read = function(bool $reset, mixed $next) use ($value): void {
        if ($reset) { unset($value); $value = 'new'; }
        try { $value = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
        echo gettype($value), '|';
    };
    $read(false, 2); $read(true, 2);
}
openCapture192(1);

$shared = 1;
$keepOld = function(bool $reset, mixed $next) use (&$shared): void {
    $old = function() use (&$shared): mixed { return $shared; };
    if ($reset) { unset($shared); $shared = 'new'; }
    try { $shared = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo $old(), ':', gettype($shared), ':', $shared, '|';
};
$keepOld(false, 2); echo $shared, '|';
$keepOld(true, 3); echo $shared, '|';
