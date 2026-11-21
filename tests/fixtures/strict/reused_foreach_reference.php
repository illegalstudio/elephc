<?php
error_reporting(E_ALL & ~E_DEPRECATED);
function foreachOriginalKey(mixed $key): mixed { echo 'Q'; return $key; }
$hash = [0 => [1], 1 => [2], '1' => [3], '' => [4], '01' => [5]];
foreach ([false, 1.0, null] as $key) {
    try {
        foreach ($hash[foreachOriginalKey($key)] as &$value) { $value += 1; echo 'W'; }
    } catch (TypeError $error) { echo 'K'; unset($error); }
    unset($value);
}
echo '|', $hash[0][0], ':', $hash[1][0], ':', $hash[''][0], '|';
foreach ($hash['1'] as &$value) { $value += 10; }
unset($value);
foreach ($hash['01'] as &$value) { $value += 10; }
unset($value);
echo $hash[1][0], ':', $hash['1'][0], ':', $hash['01'][0], '|';
$nested = ['outer' => [0 => [7], 'node' => [8]]];
$copy = $nested;
try {
    foreach ($nested['outer'][foreachOriginalKey(false)] as &$value) { $value += 1; echo 'W'; }
} catch (TypeError $error) { echo 'K'; unset($error); }
unset($value);
echo $nested['outer'][0][0], ':', $copy['outer'][0][0], '|';
function throwingForeachKey(): mixed { echo 'E'; throw new RuntimeException('key'); }
try { foreach ($hash[throwingForeachKey()] as &$value) { echo 'X'; } }
catch (RuntimeException $error) { echo 'T|'; unset($error); }
unset($value);
