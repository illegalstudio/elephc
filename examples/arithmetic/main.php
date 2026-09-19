<?php
$a = 10;
$b = 32;

echo "a = " . $a . ", b = " . $b . "\n";
echo "a + b = " . ($a + $b) . "\n";
echo "a - b = " . ($a - $b) . "\n";
echo "a * b = " . ($a * $b) . "\n";
echo "b / a = " . intval($b / $a) . "\n";
echo "b % a = " . ($b % $a) . "\n";
echo "2 + 3 * 4 = " . (2 + 3 * 4) . "\n";
echo "(2 + 3) * 4 = " . ((2 + 3) * 4) . "\n";

$overflow = PHP_INT_MAX + $argc;
echo "overflow type = " . gettype($overflow) . "\n";

// PHP 8 coerces numeric strings in arithmetic. An integer-form string stays an int,
// a float-form string (with a "." or exponent) becomes a float, and a leading-numeric
// string uses its numeric prefix.
echo '"123" + 3 = ' . ("123" + 3) . "\n";
echo '"1.5" + 3 = ' . ("1.5" + 3) . "\n";
echo '"100" - 30 = ' . ("100" - 30) . "\n";
echo '"10" / 3 = ' . ("10" / 3) . "\n";

// An integer-form string too wide for a 64-bit int becomes a float, like in PHP,
// so the magnitude survives instead of saturating at PHP_INT_MAX.
$wide = "99999999999999999999" + 1;
echo '"99999999999999999999" + 1 type = ' . gettype($wide) . "\n";

// ++ and -- on a property or an array element read as expressions, not just as statements.
// The postfix form yields what the place held BEFORE the update, the prefix form what it holds
// after.
class Tally
{
    public int $hits = 0;
    public array $per_page = [0, 0];

    public function record(): int
    {
        return $this->hits++;   // the count BEFORE this hit
    }
}

$tally = new Tally();
echo "record() returns " . $tally->record() . ", hits is now " . $tally->hits . "\n";
echo "++per_page[0] = " . ++$tally->per_page[0] . "\n";

$totals = ["a" => 10];
$sum = 0;
$sum += $totals["a"]++;
echo "sum = " . $sum . ", totals[a] = " . $totals["a"] . "\n";

// The place is evaluated ONCE, so an index with side effects runs a single time.
function pick_index(): int
{
    echo "picking ";
    return 0;
}

$scores = [7, 8];
echo "scores[pick_index()]++ = " . $scores[pick_index()]++ . ", now " . $scores[0] . "\n";
