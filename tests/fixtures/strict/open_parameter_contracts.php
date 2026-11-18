<?php
function openIntegerInput($value): void {
    $value = 'new';
    $value = 2;
    echo gettype($value), ':', $value, '|';
}
openIntegerInput(1);
function openFloatInput($value): void {
    $value = 'ready';
    echo gettype($value), ':', $value, '|';
    $value = 2.5;
    echo gettype($value), ':', $value, '|';
}
openFloatInput(1.5);
function explicitMixedInput(mixed $value): void {
    $value = 'new';
    $value = 3.5;
    echo gettype($value), ':', $value, '|';
}
explicitMixedInput(1);
function sameKindOpenInput($value): void {
    $value = 2;
    echo gettype($value), ':', $value, '|';
}
sameKindOpenInput(1);
function closedDeclaredInput(int $value, mixed $next): void {
    try { $value = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
closedDeclaredInput(1, 'bad');
function closedOrdinaryLocal(mixed $next): void {
    $value = 1;
    try { $value = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
closedOrdinaryLocal('bad');

function replaceInferredReference(&$value): void { $value = 'new'; }
function openInferredReferenceCaller($value): void {
    replaceInferredReference($value);
    echo gettype($value), ':', $value, '|';
}
openInferredReferenceCaller(1);
$closed = 1;
try { replaceInferredReference($closed); echo 'W'; } catch (TypeError $error) { echo 'T'; }
echo gettype($closed), ':', $closed, '|';

class OpenParameterMethods {
    public function update($value): void { $value = 'method'; echo $value, '|'; }
    public static function updateStatic($value): void { $value = 'static'; echo $value, '|'; }
}
$methods = new OpenParameterMethods();
$methods->update(1);
OpenParameterMethods::updateStatic(1);
$closure = function($value): void { $value = 'closure'; echo $value, '|'; };
$closure(1);
class OpenPromotedParameter {
    public function __construct(public $value) {
        $value = 'local';
        echo $value, ':', $this->value, '|';
    }
}
$promoted = new OpenPromotedParameter(1);

function replaceStreamInput($value): void { $value = 'stream'; echo $value, '|'; }
$stream = tmpfile();
replaceStreamInput($stream);
echo fwrite($stream, 'x'), '|';
echo fclose($stream) ? 'closed|' : 'bad-close|';

function mergedOpenInput($value): void { $value = 'merged'; echo $value, '|'; }
mergedOpenInput(1);
mergedOpenInput('initial');
