<?php
function selectedCapturedReference(bool $numeric, mixed $next): void {
    if ($numeric) { $value = 1; } else { $value = 'old'; }
    $read = function(mixed $next) use (&$value): void {
        try { $value = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
        echo gettype($value), ':', $value, '|';
    };
    $read($next);
    echo gettype($value), ':', $value, '|';
}
selectedCapturedReference(true, 'new');
selectedCapturedReference(false, 2);
selectedCapturedReference(true, 3);
selectedCapturedReference(false, 'ready');
function escapedCapturedReference(bool $numeric): void {
    if ($numeric) { $value = 1; } else { $value = 'old'; }
    $read = function() use (&$value): mixed { return $value; };
    unset($value); $value = 'new';
    echo $read(), ':', $value, '|';
}
escapedCapturedReference(true);
escapedCapturedReference(false);
$root = 1;
$readRoot = function() use (&$root): int { return $root; };
echo $readRoot(), '|';
