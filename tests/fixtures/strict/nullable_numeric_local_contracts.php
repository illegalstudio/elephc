<?php
function showNullableNumber(?int $number): void {
    if ($number === null) { echo 'N|'; }
    else { echo gettype($number), ':', $number, '|'; }
}
function replaceNullableNumber(?int $number, mixed $candidate): void {
    try { $number = $candidate; echo 'W'; }
    catch (TypeError $writeError) { echo 'T'; }
    showNullableNumber($number);
}
function updateNullableNumber(?int $number, int $direction): void {
    if ($number !== null) {
        try {
            if ($direction > 0) { ++$number; } else { --$number; }
            echo 'S';
        } catch (TypeError $valueError) { echo 'T'; }
        catch (ArithmeticError $valueOverflow) { echo 'A'; }
    }
    showNullableNumber($number);
}
function updateNullableReference(?int &$number, int $direction): void {
    if ($number !== null) {
        try {
            if ($direction > 0) { ++$number; } else { --$number; }
            echo 'S';
        } catch (TypeError $referenceError) { echo 'T'; }
        catch (ArithmeticError $referenceOverflow) { echo 'A'; }
    }
}
function invokeNullableReference(?int $number, int $direction): void {
    updateNullableReference($number, $direction);
    showNullableNumber($number);
}
function sequentialNullableUpdates(?int $number): void {
    if ($number !== null) { echo ++$number, ':', $number++, ':', $number; }
    echo '|';
}
replaceNullableNumber($argc, 2.5);
replaceNullableNumber($argc, null);
replaceNullableNumber($argc, 9007199254740993);
updateNullableNumber($argc > 0 ? PHP_INT_MAX : 1, 1);
updateNullableNumber($argc > 0 ? PHP_INT_MIN : 1, -1);
updateNullableNumber(null, 1);
updateNullableNumber($argc, 1);
invokeNullableReference($argc > 0 ? PHP_INT_MAX : 1, 1);
invokeNullableReference($argc > 0 ? PHP_INT_MIN : 1, -1);
invokeNullableReference(null, 1);
invokeNullableReference($argc, 1);
sequentialNullableUpdates($argc);
function compoundNullableResult(?int $number): int {
    if ($number !== null) { $number += 1; return $number; }
    return 0;
}
echo compoundNullableResult($argc), '|', compoundNullableResult(null), '|';
gc_collect_cycles();
