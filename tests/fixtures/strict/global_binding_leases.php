<?php
$plainGlobal = 1;
function freshGlobalLocal(): void {
    global $plainGlobal;
    unset($plainGlobal); $plainGlobal = 2; echo $plainGlobal, '|';
}
freshGlobalLocal(); echo $plainGlobal, '|';
$conditionalGlobal = 1;
function conditionalGlobalLocal(bool $reset): void {
    global $conditionalGlobal;
    if ($reset) { unset($conditionalGlobal); }
    $conditionalGlobal = 2; echo $conditionalGlobal, ':';
}
conditionalGlobalLocal(true); echo $conditionalGlobal, '|';
conditionalGlobalLocal(false); echo $conditionalGlobal, '|';
$reboundGlobal = 1;
function reboundGlobalLocal(): void {
    global $reboundGlobal;
    $other = 1; $reboundGlobal = &$other; $reboundGlobal = 2; echo $other, '|';
}
reboundGlobalLocal(); echo $reboundGlobal, '|';
$capturedGlobal = 1;
function captureGlobalReference(): callable {
    global $capturedGlobal;
    return function() use (&$capturedGlobal): mixed { return $capturedGlobal; };
}
$readGlobalReference = captureGlobalReference();
$capturedGlobal = 2; echo $readGlobalReference(), ':', $capturedGlobal, '|';
function captureGlobalValue(): callable {
    global $capturedGlobal;
    return function() use ($capturedGlobal): mixed { return $capturedGlobal; };
}
$readGlobalValue = captureGlobalValue();
$capturedGlobal = 3; echo $readGlobalValue(), ':', $readGlobalReference(), '|';
$arrayGlobal = [1];
function captureArrayGlobal(): callable {
    global $arrayGlobal;
    return function() use (&$arrayGlobal): mixed { return $arrayGlobal; };
}
$readArrayGlobal = captureArrayGlobal();
$arrayGlobal = [2]; $arraySnapshot = $readArrayGlobal(); echo $arraySnapshot[0], '|';
function createMissingGlobal(): void {
    global $missingGlobal;
    echo isset($missingGlobal) ? 'present' : 'absent', ':';
    $missingGlobal = 'created'; echo $missingGlobal, '|';
}
createMissingGlobal(); createMissingGlobal();

$textGlobal = 'before';
function captureTextGlobal(): callable {
    global $textGlobal;
    return function() use (&$textGlobal): mixed { return $textGlobal; };
}
$readTextGlobal = captureTextGlobal();
$textGlobal = 'after'; echo $readTextGlobal(), '|';
class GlobalLeaseValue { public function __construct(public int $value) {} }
$objectGlobal = new GlobalLeaseValue(1);
function captureObjectGlobal(): callable {
    global $objectGlobal;
    return function() use (&$objectGlobal): mixed { return $objectGlobal; };
}
$readObjectGlobal = captureObjectGlobal();
$objectGlobal = new GlobalLeaseValue(2);
$objectSnapshot = $readObjectGlobal(); echo $objectSnapshot->value, '|';

$reboundFloat = 1.5;
function reboundFloatGlobal(): void {
    global $reboundFloat;
    $other = 2.5; $reboundFloat = &$other; $reboundFloat = 3.5; echo $other, '|';
}
reboundFloatGlobal(); echo $reboundFloat, '|';
$reboundText = 'before';
function reboundTextGlobal(): void {
    global $reboundText;
    $other = 'old'; $reboundText = &$other; $reboundText = 'after'; echo $other, '|';
}
reboundTextGlobal(); echo $reboundText, '|';
function describeGlobalLease(mixed $value): void { echo gettype($value), ':', $value, '|'; }
function openGlobalProducer(): mixed { return 1; }
$openGlobal = openGlobalProducer();
function replaceOpenGlobal(mixed &$value, mixed $next): void { $value = $next; }
function updateOpenGlobal(mixed $next): void {
    global $openGlobal;
    replaceOpenGlobal($openGlobal, $next);
}
updateOpenGlobal(2.5); describeGlobalLease($openGlobal);

class GlobalLeaseRetired {
    public function __destruct() { echo 'D'; throw new RuntimeException('retired'); }
}
$retiredGlobal = 7;
function replaceGlobalLocal(): void {
    $retiredGlobal = new GlobalLeaseRetired();
    try { global $retiredGlobal; } catch (Throwable $error) { echo 'C'; }
    describeGlobalLease($retiredGlobal);
}
replaceGlobalLocal();

function retiredProgramValue(): mixed { return new GlobalLeaseRetired(); }
$replacedProgramGlobal = retiredProgramValue();
function exposeReplacedProgramGlobal(): void { global $replacedProgramGlobal; }
try { $replacedProgramGlobal = null; } catch (Throwable $error) { echo 'C'; }
describeGlobalLease($replacedProgramGlobal);
