<?php
$code = $argc > 0 ? '
function ownedSortInteger($left, $right): mixed { return $left <=> $right; }
function ownedSortKey($left, $right): mixed { return strcmp($left, $right); }
function ownedSortBoolean($left, $right): mixed { return true; }
class OwnedSortResult { public function __destruct() { echo "D"; } }
function ownedSortObject($left, $right): mixed { return new OwnedSortResult(); }
$items = [2, 1]; usort($items, "ownedSortInteger"); echo implode(",", $items); echo ";";
$items = ["b" => 2, "a" => 1]; uasort($items, "ownedSortInteger"); echo implode(",", $items); echo ";";
$items = ["b" => 2, "a" => 1]; uksort($items, "ownedSortKey"); echo implode(",", $items); echo ";";
$items = [2, 1];
try { usort($items, "ownedSortBoolean"); echo "X"; }
catch (TypeError $booleanError) { echo "B;"; unset($booleanError); }
try { uasort($items, "ownedSortObject"); echo "X"; }
catch (TypeError $valueError) { echo "V;"; unset($valueError); }
try { uksort($items, "ownedSortObject"); echo "X"; }
catch (TypeError $keyError) { echo "K;"; unset($keyError); }
echo implode(",", $items);
' : '';
eval($code);
gc_collect_cycles();
