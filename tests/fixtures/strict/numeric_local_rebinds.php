<?php
$value = $argc;
$replacement = $argc + 0.5;
$value =& $replacement;
$value = $argc + 1.5;
echo $value, '|', $replacement, '|';
function noNumericDefault(int $seed): void {
    $value = $seed ?? ($temporary = 1);
    $temporary = 2.5;
    echo $value, '|', $temporary;
}
noNumericDefault($argc);
$literalValue = 1 ?? ($literalTemporary = 1);
$literalTemporary = 2.5;
echo '|', $literalValue, '|', $literalTemporary;
function declaredNumericSeed(): int { return 1; }
$callValue = declaredNumericSeed() ?? ($callTemporary = 1);
$callTemporary = 2.5;
echo '|', $callValue, '|', $callTemporary;
gc_collect_cycles();
