<?php
// A seeded Mt19937 engine makes every draw reproducible.
use Random\Engine\Mt19937;
use Random\Engine\Xoshiro256StarStar;
use Random\IntervalBoundary;
use Random\Randomizer;

$randomizer = new Randomizer(new Mt19937(2024));

$rolls = [];
for ($i = 0; $i < 10; $i++) {
    $rolls[] = $randomizer->getInt(1, 6);
}
echo "dice: ", implode(" ", $rolls), "\n";

echo "bytes: ", bin2hex($randomizer->getBytes(8)), "\n";
echo "hex:   ", $randomizer->getBytesFromString("0123456789abcdef", 16), "\n";

$deck = range(1, 10);
echo "shuffle: ", implode(",", $randomizer->shuffleArray($deck)), "\n";

$secure = new Randomizer(new Xoshiro256StarStar(7));
echo "float: ", $secure->getFloat(0.0, 1.0, IntervalBoundary::ClosedClosed), "\n";
