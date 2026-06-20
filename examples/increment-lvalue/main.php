<?php

// Increment and decrement work directly on object properties and array
// elements when used as standalone statements. The prefix (++$x) and postfix
// ($x++) forms are interchangeable here because the produced value is discarded.

class Tally
{
    public int $total = 0;
}

$tally = new Tally();
$byChoice = ["yes" => 0, "no" => 0];

$votes = ["yes", "no", "yes", "yes", "no"];

foreach ($votes as $vote) {
    ++$byChoice[$vote];   // bump the per-choice counter (array element)
    ++$tally->total;      // bump the running total (object property)
}

echo "yes: " . $byChoice["yes"] . "\n";
echo "no: " . $byChoice["no"] . "\n";
echo "total: " . $tally->total . "\n";

// Decrement works the same way; here we retract one "yes" vote.
--$byChoice["yes"];
--$tally->total;

echo "after retract -> yes: " . $byChoice["yes"] . ", total: " . $tally->total . "\n";
