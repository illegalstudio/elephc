<?php
function resetParameterOrigin(bool $reset, int $value, mixed $replacement): void {
    if ($reset) { unset($value); $value = 'after'; }
    try { $value = $replacement; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
resetParameterOrigin(true, 1, 2);
resetParameterOrigin(false, 1, 'bad');
resetParameterOrigin(true, 1, 'ready');
resetParameterOrigin(false, 1, 3);

function resetNullableParameter(bool $reset, ?int $value, mixed $replacement): void {
    if ($reset) { unset($value); $value = 'after'; }
    try { $value = $replacement; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|';
}
resetNullableParameter(false, null, 3);
resetNullableParameter(false, null, 2.5);
resetNullableParameter(true, null, 3);
function resetUnionParameter(bool $reset, int|float $value, mixed $replacement): void {
    if ($reset) { unset($value); $value = 'after'; }
    try { $value = $replacement; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|';
}
resetUnionParameter(false, 1, 2.5);
resetUnionParameter(true, 1, 2.5);
function resetMixedParameter(bool $reset, mixed $value, mixed $replacement): void {
    if ($reset) { unset($value); $value = 'after'; }
    try { $value = $replacement; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|';
}
resetMixedParameter(false, 1, 'open');
resetMixedParameter(true, 1, 2);
class ParameterOriginBase {}
class ParameterOriginFirst extends ParameterOriginBase {}
class ParameterOriginSecond extends ParameterOriginBase {}
class ParameterOriginOther {}
function resetObjectParameter(bool $reset, ParameterOriginBase $value, mixed $replacement): void {
    if ($reset) { unset($value); $value = 'after'; }
    try { $value = $replacement; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|';
}
resetObjectParameter(false, new ParameterOriginFirst(), new ParameterOriginSecond());
resetObjectParameter(false, new ParameterOriginFirst(), new ParameterOriginOther());
resetObjectParameter(true, new ParameterOriginFirst(), new ParameterOriginSecond());
function resetArrayParameter(bool $reset, array $value, mixed $replacement): void {
    if ($reset) { unset($value); $value = 'after'; }
    try { $value = $replacement; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|';
}
$originalArray = [1];
resetArrayParameter(false, $originalArray, ['changed']);
resetArrayParameter(true, $originalArray, ['changed']);
echo $originalArray[0], '|';
function resetStreamParameter(bool $reset, $value, mixed $replacement): void {
    if ($reset) { unset($value); $value = 'after'; }
    try { $value = $replacement; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|';
}
$stream = tmpfile();
resetStreamParameter(false, $stream, 1);
echo fwrite($stream, 'x'), '|';
resetStreamParameter(true, $stream, 1);
echo fclose($stream) ? 'closed|' : 'bad-close|';

class ConditionalParameterMethods {
    public function run(bool $reset, int $value, mixed $replacement): void {
        if ($reset) { unset($value); $value = 'after'; }
        try { $value = $replacement; echo 'W'; } catch (TypeError $error) { echo 'T'; }
        echo gettype($value), '|';
    }
    public static function runStatic(bool $reset, int $value, mixed $replacement): void {
        if ($reset) { unset($value); $value = 'after'; }
        try { $value = $replacement; echo 'W'; } catch (TypeError $error) { echo 'T'; }
        echo gettype($value), '|';
    }
}
$methods = new ConditionalParameterMethods();
$methods->run(true, 1, 2);
ConditionalParameterMethods::runStatic(false, 1, 'bad');
class ConditionalParameterConstructor {
    public function __construct(bool $reset, int $value, mixed $replacement) {
        if ($reset) { unset($value); $value = 'after'; }
        try { $value = $replacement; echo 'W'; } catch (TypeError $error) { echo 'T'; }
        echo gettype($value), '|';
    }
}
$constructed = new ConditionalParameterConstructor(true, 1, 2);
$value = 'outer';
$closure = function(bool $reset, int $value, mixed $replacement): void {
    if ($reset) { unset($value); $value = 'after'; }
    try { $value = $replacement; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|';
};
$closure(false, 1, 'bad');
echo $value, '|';
function parameterScopePresence(bool $reset, int $value): void {
    if ($reset) { unset($value); }
    echo array_key_exists('value', get_defined_vars()) ? 'present|' : 'absent|';
}
parameterScopePresence(false, 1);
parameterScopePresence(true, 1);

function resetVariadicParameter(bool $reset, mixed $replacement, int ...$values): void {
    if ($reset) { unset($values); $values = 'after'; }
    try { $values = $replacement; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($values), '|';
}
resetVariadicParameter(false, ['changed'], 1, 2);
resetVariadicParameter(true, ['changed'], 1, 2);
