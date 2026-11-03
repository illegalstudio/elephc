<?php

// What the wasm32-wasi target reproduces from php-src, byte for byte.
// Compile:  elephc main.php --target wasm32-wasi
// Run:      node --no-warnings run.mjs main.wasm main.php
//
// Pass the script path as the module's FIRST WASI argument: php-src puts it in
// $argv[0] and counts it in $argc, so a host that starts the module with an
// empty argument vector makes both differ for reasons unrelated to the backend.

echo "Hello from WASI\n";
echo "argc=", $argc, "\n";
echo "script=", $argv[0], "\n";

// Objects with untyped and declared properties. An untyped property is a boxed
// cell, and whether reading one borrows or takes a reference follows what the
// EIR says the value is — which is what keeps `$this->count` alive across a read
// that the epilogue then releases.
class Counter
{
    public $count;
    public string $label;

    public function __construct(string $label)
    {
        $this->count = 0;
        $this->label = $label;
    }

    public function inc(): void
    {
        $this->count += 1;
    }

    public function dec(): void
    {
        if ($this->count > 0) {
            $this->count -= 1;
        }
    }

    public function __toString(): string
    {
        return $this->label . "=" . $this->count;
    }
}

$c = new Counter("hits");
$c->inc();
$c->inc();
$c->inc();
$c->dec();
echo $c, "\n";

// `round($v, $p)` is not `round($v)` with a default argument. Scaling is
// inexact, so php-src extracts the integral part and then repairs it — which is
// why 1.005 rounds up where a naive scale-round-unscale answers 1.0.
echo round(1.005, 2), " ", round(9.995, 2), " ", round(0.285, 2), " ", round(-1.005, 2), "\n";
echo round(1234.5678, -2), " ", round(2.5), " ", round(-2.5), "\n";

// Casts, integer division and the string form of an int.
function describe(int $n): string
{
    if ($n % 15 === 0) {
        return "FizzBuzz";
    }
    if ($n % 3 === 0) {
        return "Fizz";
    }
    if ($n % 5 === 0) {
        return "Buzz";
    }
    return (string) $n;
}

$i = 1;
while ($i <= 15) {
    echo describe($i), " ";
    $i = $i + 1;
}
echo "\n";

// A hash walks in INSERTION order, never sorted, and `array_keys` /
// `array_values` project that same order. Writing the container after the loop
// is fine: the iterator is dead by then.
$scores = ["zoe" => 26, "amir" => 41, "mia" => 13];
foreach ($scores as $name => $score) {
    echo $name, ":", $score, " ";
}
echo "\n";
$scores["kai"] = 7;
echo implode(",", array_keys($scores)), "\n";
echo implode(",", array_values($scores)), "\n";
echo count($scores), " entries\n";

// Every float reaching a string shares one renderer, so all three spellings agree.
$ratio = 2 / 3;
echo $ratio, " ", (string) $ratio, " ", "$ratio", "\n";
echo 1.0E+20, " ", 1.0E-7, " ", 100.0, " ", -0.0, "\n";
