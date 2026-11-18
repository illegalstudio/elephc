<?php
function evalTypedParameter(bool $reset, int $value, mixed $next): void {
    $old = function() use (&$value): mixed { return $value; };
    if ($reset) { unset($value); $value = 'new'; }
    eval('');
    try { $value = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $old(), '|';
}
evalTypedParameter(false, 1, 'bad');
evalTypedParameter(true, 1, 2);

function evalDetachedParameter(int $value): void {
    $old = function() use (&$value): mixed { return $value; };
    unset($value);
    $fresh = function() use (&$value): mixed { return $value; };
    $value = 'new';
    eval('');
    echo $old(), ':', $fresh(), '|';
}
evalDetachedParameter(1);

function evalOpenParameter(bool $reset, $value, mixed $next): void {
    $value = 'early';
    if ($reset) { unset($value); $value = 'new'; }
    eval('');
    try { $value = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|';
}
evalOpenParameter(false, 1, 2);
evalOpenParameter(true, 1, 2);

class EvalParameterMethods {
    public function reset(bool $reset, int $value): void {
        if ($reset) { unset($value); $value = 'new'; }
        eval('');
        echo gettype($value), '|';
    }
}
$methods = new EvalParameterMethods();
$methods->reset(false, 1);
$methods->reset(true, 1);
$closure = function(bool $reset, int $value): void {
    if ($reset) { unset($value); $value = 'new'; }
    eval('');
    echo gettype($value), '|';
};
$closure(false, 1);
$closure(true, 1);

function evalArrayParameter(array $value): void {
    unset($value);
    eval('$value = "new";');
    echo $value, '|';
}
$array = [1];
evalArrayParameter($array);
echo $array[0], '|';
