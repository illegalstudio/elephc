<?php
function fillBinding(mixed &$target): void { $target = 'bound'; }
fillBinding($direct);
echo $direct, '|';
FILLBINDING(target: $named);
echo $named, '|';
$call = fillBinding(...);
$call($indirect);
echo $indirect, '|';
$closure = function(mixed &$target): void { $target = 'closure'; };
$closure($captured);
echo $captured, '|';

function inspectBinding(mixed &$target, string $kind): void {
    echo $kind, ':', gettype($target), '|';
    $target = 'observed';
}
inspectBinding(target: $ordered, kind: gettype($ordered));
echo $ordered, '|';
inspectBinding(target: $visible, kind: array_key_exists('visible', get_defined_vars()) ? 'present' : 'missing');
echo $visible, '|';
function formReferenceOnly(mixed &$target): void {}
formReferenceOnly($formed);
echo array_key_exists('formed', get_defined_vars()) ? 'formed|' : 'lost|';
$formClosure = function(mixed &$target): void {};
$formClosure($closureFormed);
echo array_key_exists('closureFormed', get_defined_vars()) ? 'closure-formed|' : 'closure-lost|';
function setLateBinding(mixed &$target): string { $target = 'late'; return 'later'; }
inspectBinding(target: $late, kind: setLateBinding($late));
echo $late, '|';

function fillRest(mixed &...$targets): void {
    $targets[0] = 'positional';
    $targets['tail'] = 'named';
}
fillRest($positional, tail: $variadic);
echo $positional, ':', $variadic, '|';

function privateDefault(mixed &$defaultTarget = null): void { echo gettype($defaultTarget), '|'; }
privateDefault();
echo array_key_exists('defaultTarget', get_defined_vars()) ? 'bad|' : 'private|';

function failReference(mixed &$target): void { throw new RuntimeException('failed'); }
try { failReference($failed); } catch (RuntimeException $error) { echo gettype($failed), '|'; }

function replaceArgument(mixed &$target, mixed $candidate): void { $target = $candidate; }
$fixed = 1;
try { replaceArgument($fixed, 'changed'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
echo gettype($fixed), '|';
