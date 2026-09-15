<?php
// PHP's `for` init and update clauses are comma-separated lists of EXPRESSIONS, not just
// the `$i = 0` / `$i++` shapes. Any expression works in either, and the list is evaluated
// left to right.

// An array push in the update clause. It runs after the body, so the value pushed on each
// pass is the one the body just produced.
$squares = [];
for ($i = 0; $i < 4; $squares[] = $i * $i) {
    $i++;
}
echo implode(",", $squares), "\n";

// Comma-separated lists in BOTH clauses: two variables initialised, two updated.
$seen = [];
for ($i = 0, $limit = 3; $i < $limit; $i++, $seen[] = $i) {
    // body intentionally empty — the work is in the clauses
}
echo count($seen), ":", implode(",", $seen), "\n";

// An indexed assignment, and a property assignment, in the update clause.
$slots = [0, 0, 0];
for ($i = 0; $i < 3; $slots[$i] = $i * 2) {
    $i++;
}
echo implode(",", $slots), "\n";

class Box
{
    public int $total = 0;
}

$box = new Box();
for ($i = 0; $i < 4; $box->total = $i) {
    $i++;
}
echo $box->total, "\n";

// A comma inside a call's arguments, an array literal, or a closure body is NOT a clause
// separator — including the `;` inside the closure, which does not end the init clause.
for ($fmt = function (int $n): string { return "#$n"; }, $i = max(0, 1); $i < 4; $i++) {
    echo $fmt($i);
}
echo "\n";

// Empty clauses still work.
$n = 0;
for (;;) {
    $n++;
    if ($n > 2) {
        break;
    }
}
echo $n, "\n";

// `throw` is an expression in PHP 8, so a clause accepts it too. This one ends the loop
// after a single pass, from the update clause.
try {
    for ($i = 0; $i < 4; throw new RuntimeException("one pass only")) {
        echo "pass ", $i, "\n";
    }
} catch (RuntimeException $e) {
    echo $e->getMessage(), "\n";
}
