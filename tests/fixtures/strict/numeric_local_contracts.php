<?php
function numericBindings(int $seed, mixed $bad, mixed $good): void {
    $value = $seed;
    try { $value = $bad; echo 'W'; } catch (TypeError $writeError) { echo 'T'; }
    echo gettype($value), '|';
    $value = $good; echo $value, '|';
    $compound = $seed > 0 ? PHP_INT_MAX : 1;
    try { $compound += 1; echo 'C'; } catch (TypeError $compoundError) { echo 'A'; }
    echo gettype($compound), '|';
    $prefix = $seed > 0 ? PHP_INT_MAX : 1;
    try { ++$prefix; echo 'R'; } catch (TypeError $prefixError) { echo 'P'; }
    echo gettype($prefix), '|';
    $postfix = $seed > 0 ? PHP_INT_MAX : 1;
    try { $postfix++; echo 'S'; } catch (TypeError $postfixError) { echo 'Q'; }
    echo gettype($postfix), '|';
    echo gettype(($seed > 0 ? PHP_INT_MAX : 1) + 1), '|';
    echo gettype($seed + '2'), '|', (int) 1.8, '|';
}
function numericWeakArgument(float $number): float { return $number; }
numericBindings($argc, 2.5, 9007199254740993);
echo numericWeakArgument(2), '|', 1 == '1' ? 'E' : 'X';
gc_collect_cycles();
