<?php
function resetReferenceOrigin(bool $reset, int &$value, mixed $next): void {
    $value = 2;
    $old = function() use (&$value): mixed { return $value; };
    if ($reset) { unset($value); $value = 'new'; }
    try { $value = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, ':', $old(), '|';
}
$first = 1;
resetReferenceOrigin(false, $first, 'bad');
echo $first, '|';
$second = 1;
resetReferenceOrigin(true, $second, 3);
echo $second, '|';

function resetReferenceWithEval(bool $reset, int &$value, mixed $next): void {
    if ($reset) { unset($value); $value = 'new'; }
    eval('');
    try { $value = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|';
}
$third = 1;
resetReferenceWithEval(false, $third, 3);
echo $third, '|';
$fourth = 1;
resetReferenceWithEval(true, $fourth, 3);
echo $fourth, '|';

function resetOpenReference(bool $reset, mixed &$value, mixed $next): void {
    $value = 'early';
    if ($reset) { unset($value); $value = 'new'; }
    try { $value = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|';
}
$fifth = 'open';
resetOpenReference(false, $fifth, 3);
echo gettype($fifth), '|';
$sixth = 'open';
resetOpenReference(true, $sixth, 3);
echo $sixth, '|';

function exerciseOpenReference(bool $reset, mixed $value): void {
    resetOpenReference($reset, $value, 3);
    echo gettype($value), '|';
}
exerciseOpenReference(false, 'open');
exerciseOpenReference(true, 'open');
