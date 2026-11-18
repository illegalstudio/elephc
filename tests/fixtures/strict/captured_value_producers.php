<?php
function selectedCapturedValue(bool $numeric, mixed $next): void {
    if ($numeric) { $value = 1; } else { $value = 'old'; }
    $read = function(mixed $next) use ($value): void {
        echo count(get_defined_vars()), '|';
        try { $value = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
        echo gettype($value), ':', $value, '|';
    };
    $read($next);
    echo gettype($value), ':', $value, '|';
}
selectedCapturedValue(true, 'new');
selectedCapturedValue(false, 2);
selectedCapturedValue(true, 3);
selectedCapturedValue(false, 'ready');

function makeCapturedValue(bool $numeric): callable {
    if ($numeric) { $value = 1; } else { $value = 'old'; }
    return function(mixed $next) use ($value): void {
        try { $value = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
        echo gettype($value), ':', $value, '|';
    };
}
$read = makeCapturedValue(true);
call_user_func($read, 'bad');
call_user_func_array($read, [2]);
$read('again');

function makeNestedCapturedValue(bool $numeric): callable {
    if ($numeric) { $value = 1; } else { $value = 'old'; }
    return function() use ($value): callable {
        return function(mixed $next) use ($value): void {
            try { $value = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
            echo gettype($value), ':', $value, '|';
        };
    };
}
$factory = makeNestedCapturedValue(false);
$nested = $factory();
$nested(2);
$nested('ready');

function captureOpenProducer(bool $open, mixed $seed, mixed $next): void {
    if ($open) { $value = $seed; } else { $value = 1; }
    $read = function(mixed $next) use ($value): void {
        try { $value = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
        echo gettype($value), ':', $value, '|';
    };
    $read($next);
}
captureOpenProducer(true, 1, 'new');
captureOpenProducer(false, 1, 'new');
