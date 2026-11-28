<?php
function foreachOriginalKey(mixed $key): mixed { echo 'Q'; return $key; }
function visitForeachKey(mixed $key): void {
    $hash = [0 => [1], 1 => [2], '1' => [3], '' => [4]];
    try {
        foreach ($hash[foreachOriginalKey($key)] as &$value) { $value += 1; echo 'W'; }
    } catch (TypeError $error) { echo 'K'; unset($error); }
    unset($value);
    echo $hash[0][0], ':', $hash[1][0], ':', $hash[''][0], '|';
}
visitForeachKey(false);
visitForeachKey(1.0);
visitForeachKey(null);
$hash = [1 => [2], '1' => [3], '01' => [5]];
foreach ($hash[foreachOriginalKey('1')] as &$wordValue) { $wordValue += 10; }
unset($wordValue);
function foreachPaddedKey(): string { return str_repeat('01', 1); }
foreach ($hash[foreachPaddedKey()] as &$paddedValue) { $paddedValue += 10; }
unset($paddedValue);
echo $hash[1][0], ':', $hash['1'][0], ':', $hash['01'][0], '|';
$nested = ['outer' => [0 => [7], 'node' => [8]]];
$copy = $nested;
try {
    foreach ($nested['outer'][foreachOriginalKey(false)] as &$nestedValue) { $nestedValue += 1; echo 'W'; }
} catch (TypeError $error) { echo 'K'; unset($error); }
unset($nestedValue);
echo $nested['outer'][0][0], ':', $copy['outer'][0][0], '|';
function throwingForeachKey(): mixed { echo 'E'; throw new RuntimeException('key'); }
try { foreach ($hash[throwingForeachKey()] as &$throwValue) { echo 'X'; } }
catch (RuntimeException $error) { echo 'T|'; unset($error); }
unset($throwValue);
function visitReferenceKey(mixed $key): void {
    $hash = [0 => [1], 1 => [2], '1' => [3], '' => [4]];
    try { $reference = &$hash[foreachOriginalKey($key)]; $reference = [9]; echo 'R'; }
    catch (TypeError $error) { echo 'K'; unset($error); }
    unset($reference);
    echo $hash[0][0], ':', $hash[1][0], '|';
}
visitReferenceKey(false);
visitReferenceKey('1');
