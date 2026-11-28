<?php
function nativeCmpInteger(int $left, int $right): int { return $left <=> $right; }
function nativeCmpBoxedInteger(int $left, int $right): mixed { return $left <=> $right; }
function nativeCmpBoolean(int $left, int $right): bool { return true; }
function nativeCmpFloat(int $left, int $right): float { return 1.5; }
function nativeCmpString(int $left, int $right): string { return '1'; }
class NativeCmpResult { public function __destruct() { echo 'D'; } }
function nativeCmpObject(int $left, int $right): NativeCmpResult { return new NativeCmpResult(); }
function nativeCmpDynamic(callable $callback, string $label): void {
    $items = [2, 1];
    try { usort($items, $callback); echo 'X'; }
    catch (TypeError $dynamicCmpError) { echo $label; unset($dynamicCmpError); }
}
function nativeCmpAccepted(callable $callback): void {
    $items = [2, 1]; usort($items, $callback); echo 'C', implode(',', $items), ';';
}
function nativeCmpValidName(): string { return 'nativeCmpBoxedInteger'; }
function nativeCmpName(): string { return 'nativeCmpBoolean'; }
$items = [2, 1]; usort($items, nativeCmpInteger(...)); echo 'I', implode(',', $items), ';';
$items = [2, 1]; usort($items, nativeCmpBoxedInteger(...)); echo 'M', implode(',', $items), ';';
nativeCmpAccepted(nativeCmpInteger(...));
$validName = nativeCmpValidName();
$items = [2, 1]; usort($items, $validName); echo 'N', implode(',', $items), ';';
echo 'A', count(array_uintersect([1], [1], nativeCmpBoxedInteger(...))),
    count(array_udiff([1], [2], nativeCmpBoxedInteger(...))), ';';
try { usort($items, nativeCmpBoolean(...)); echo 'X'; }
catch (TypeError $boolCmpError) { echo 'B'; unset($boolCmpError); }
try { uasort($items, nativeCmpFloat(...)); echo 'X'; }
catch (TypeError $floatCmpError) { echo 'F'; unset($floatCmpError); }
try { uksort($items, nativeCmpString(...)); echo 'X'; }
catch (TypeError $stringCmpError) { echo 'S'; unset($stringCmpError); }
try { usort($items, nativeCmpObject(...)); echo 'X'; }
catch (TypeError $objectCmpError) { echo 'O'; unset($objectCmpError); }
nativeCmpDynamic(nativeCmpBoolean(...), 'C');
nativeCmpDynamic(nativeCmpObject(...), 'R');
$name = nativeCmpName();
try { usort($items, $name); echo 'X'; }
catch (TypeError $nameCmpError) { echo 'N'; unset($nameCmpError); }
try { array_udiff([1], [2], nativeCmpBoolean(...)); echo 'X'; }
catch (TypeError $diffCmpError) { echo 'U'; unset($diffCmpError); }
try { array_uintersect([1], [2], nativeCmpObject(...)); echo 'X'; }
catch (TypeError $intersectCmpError) { echo 'V'; unset($intersectCmpError); }
gc_collect_cycles();
