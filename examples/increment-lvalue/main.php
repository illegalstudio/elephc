<?php

// Increment and decrement work directly on object properties and array
// elements. As standalone statements the prefix (++$x) and postfix ($x++) forms
// are interchangeable because the produced value is discarded.

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

// In expression position the produced value matters, exactly like PHP: a prefix
// increment yields the NEW value, a postfix increment yields the OLD value. This
// is the idiom Twig's lexer and Symfony's kernel use to walk a cursor.

class Cursor
{
    public int $pos = 0;

    // Consume the current position and advance — `$this->pos++` yields the OLD index.
    public function next(): int
    {
        return $this->pos++;
    }
}

$cursor = new Cursor();
$tokens = ["a", "b", "c"];
echo "walk: " . $tokens[$cursor->next()] . $tokens[$cursor->next()] . "\n";   // "ab"
echo "pos after two reads: " . $cursor->pos . "\n";                            // 2

$counter = ["hits" => 41];
$next = ++$counter["hits"];   // prefix: store 42, then read it back
echo "next hit id: " . $next . " (stored " . $counter["hits"] . ")\n";
