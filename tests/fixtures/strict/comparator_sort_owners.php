<?php
$code = $argc > 0 ? '
function ownedSortValue($left, $right): mixed { return $left <=> $right; }
function ownedSortText($left, $right): mixed { return strcmp($left, $right); }
$items = [2, 1];
usort($items, "ownedSortValue");
echo implode(",", $items); echo ";";
$items = ["b" => 2, "a" => 1];
uasort($items, "ownedSortValue");
echo implode(",", $items); echo ";";
$items = ["b" => 2, "a" => 1];
uksort($items, "ownedSortText");
echo implode(",", $items); echo ";";
$items = ["label-2", "label-1"];
usort($items, "ownedSortText");
echo implode(",", $items);
' : '';
eval($code);
gc_collect_cycles();
