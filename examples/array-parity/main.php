<?php
// Array builtin parity — a tour of the array helpers added in the parity work.

// --- list shape and edge keys ---
$list = [10, 20, 30];
$hash = ["a" => 1, "b" => 2, "c" => 3];

echo "is_list(list):  " . (array_is_list($list) ? "true" : "false") . "\n";
echo "is_list(hash):  " . (array_is_list($hash) ? "true" : "false") . "\n";
echo "first key:      " . array_key_first($hash) . "\n";
echo "last key:       " . array_key_last($hash) . "\n";

// --- hash set operations (right-wins replace, recursive replace) ---
$base = ["host" => "localhost", "port" => 80];
$over = ["port" => 443, "tls" => 1];
$merged = array_replace($base, $over);
echo "\nreplace:        port=" . $merged["port"] . " tls=" . $merged["tls"] . "\n";

$deepA = ["db" => ["host" => "a", "port" => 1]];
$deepB = ["db" => ["port" => 2]];
$deep = array_replace_recursive($deepA, $deepB);
echo "replace_rec:    host=" . $deep["db"]["host"] . " port=" . $deep["db"]["port"] . "\n";

// --- associative diff / intersect (compare key AND value) ---
$left = ["a" => 1, "b" => 2, "c" => 3];
$right = ["a" => 1, "b" => 99];
echo "diff_assoc:     ";
foreach (array_diff_assoc($left, $right) as $k => $v) {
    echo "$k=$v ";
}
echo "\nintersect_assoc:";
foreach (array_intersect_assoc($left, $right) as $k => $v) {
    echo " $k=$v";
}
echo "\n";

// --- recursive merge (scalar collisions combine into lists) ---
$mr = array_merge_recursive(["tag" => "a"], ["tag" => "b"]);
echo "merge_rec:      tag has " . count($mr["tag"]) . " values\n";

// --- predicate helpers (PHP 8.4): find / any / all ---
function gtTwo($n) { return $n > 2; }
$nums = [1, 2, 3, 4];
echo "\nfind > 2:       " . array_find($nums, "gtTwo") . "\n";
echo "any > 2:        " . (array_any($nums, "gtTwo") ? "true" : "false") . "\n";
echo "all > 2:        " . (array_all($nums, fn($n) => $n > 0) ? "true" : "false") . "\n";

// --- user-comparator set operations ---
function cmp($a, $b) { return $a - $b; }
echo "udiff:          ";
foreach (array_udiff([1, 2, 3, 4], [2, 4], "cmp") as $v) {
    echo $v;
}
echo "\nuintersect:     ";
foreach (array_uintersect([1, 2, 3, 4], [2, 4], "cmp") as $v) {
    echo $v;
}
echo "\n";

// --- recursive walk over nested arrays ---
function visit($leaf) { echo $leaf; echo ","; }
$nested = [[1, 2], [3, 4], [5, 6]];
echo "walk_recursive: ";
array_walk_recursive($nested, "visit");
echo "\n";

// --- multisort: sort one array, reorder a parallel array in tandem ---
$keys = [3, 1, 2];
$vals = [30, 10, 20];
array_multisort($keys, $vals);
echo "multisort keys: ";
foreach ($keys as $v) { echo $v; }
echo "\nmultisort vals: ";
foreach ($vals as $v) { echo $v; }
echo "\n";
