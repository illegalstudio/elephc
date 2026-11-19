<?php
function readOptionalBinding(bool $assign): void {
    if ($assign) { $value = 7; }
    echo isset($value) ? 'P:' : 'A:';
    echo $value ?? 'missing', ':';
    try { $value; echo $value; } catch (Error $error) { echo 'I'; }
    echo '|';
}
readOptionalBinding(true); readOptionalBinding(false);
$initializedNull = null;
echo gettype($initializedNull), ':', $initializedNull ?? 'fallback', '|';
echo isset($neverAssigned) ? 'P' : 'A', empty($neverAssigned) ? 'E' : 'F', ':', $neverAssigned ?? 'fallback', '|';
function readInitializedParameter(?int $value): void { echo gettype($value), '|'; }
readInitializedParameter(null); readInitializedParameter(2);
$capturedInitialized = 3;
$initializedReader = function() use ($capturedInitialized): int { return $capturedInitialized; };
echo $initializedReader(), '|';
function readInitializedVariadic(...$values): void { echo count($values), '|'; }
readInitializedVariadic(); readInitializedVariadic(1, 2);

$initializedLoose = 1; $initializedLoose = 'ready'; echo $initializedLoose, '|';
function probeInitializedLoop(): void {
    for ($index = 0; $index < 2; $index++) {
        echo isset($later) ? 'P:' : 'A:', $later ?? 'none', '|';
        $later = 5;
    }
}
probeInitializedLoop();
function countInitializedFallback(int &$calls): int { $calls++; return $calls; }
function initializedFallback(bool $assign): void {
    $calls = 0;
    if ($assign) { $value = 0; }
    echo $value ?? countInitializedFallback($calls), ':', $calls, '|';
}
initializedFallback(true); initializedFallback(false);
function readDetachedParameter(int &$value, bool $detach): void {
    if ($detach) { unset($value); }
    try { echo $value; } catch (Error $error) { echo 'I'; }
    echo '|';
}
$initializedCaller = 9;
readDetachedParameter($initializedCaller, false); readDetachedParameter($initializedCaller, true);
echo $initializedCaller, '|';
$initializedCapture = 4;
$readInitializedCapture = function(bool $detach) use ($initializedCapture): void {
    if ($detach) { unset($initializedCapture); }
    try { echo $initializedCapture; } catch (Error $error) { echo 'I'; }
    echo '|';
};
$readInitializedCapture(false); $readInitializedCapture(true); echo $initializedCapture, '|';
function ownedBeforeInitializedRead(bool $assign): void {
    if ($assign) { $value = 1; }
    try { $text = str_repeat('x', 2) . $value; } catch (Error $error) { echo 'I|'; }
}
ownedBeforeInitializedRead(false);
function initializedPredicate(bool $assign): void {
    if ($assign) { $value = 1; }
    try { echo is_int($value) ? 'P' : 'F'; } catch (Error $error) { echo 'I'; }
    echo '|';
}
initializedPredicate(false); initializedPredicate(true);
function typedInitializedCoalesce(bool $write): int {
    if ($write) { $number = 6; }
    return $number ?? 8;
}
echo typedInitializedCoalesce(true), ':', typedInitializedCoalesce(false), '|';
$initializedBool = false;
$initializedFloat = 2.5;
$initializedString = 'text';
$initializedArray = [3];
echo gettype($initializedBool ?? true), ':', gettype($initializedFloat ?? 1.0), ':',
    gettype($initializedString ?? 'default'), ':', ($initializedArray ?? [0])[0], '|';
