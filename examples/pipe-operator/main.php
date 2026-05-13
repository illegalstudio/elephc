<?php

// PHP 8.5 pipe operator: `$value |> $callable` invokes the right-hand callable
// with the left-hand value as the single positional argument. Pipes chain
// left-to-right and read top-to-bottom, replacing nested calls and throwaway
// variables in functional pipelines.

function double(int $n): int { return $n * 2; }
function increment(int $n): int { return $n + 1; }

// 1. Chained user functions.
$result = 5
    |> double(...)
    |> increment(...);
echo "5 -> double -> increment = " . $result . "\n";   // 11

// 2. Pipe into a built-in via first-class callable syntax.
echo ("hello"
    |> strtoupper(...)
    |> strrev(...)) . "\n";                            // OLLEH

// 3. Pipe into a closure literal.
$result = 7 |> (fn($v) => $v * $v);
echo "7 squared = " . $result . "\n";                  // 49

// 4. Pipe into a variable holding a callable.
$shout = fn($s) => strtoupper($s) . "!";
echo ("ready" |> $shout) . "\n";                       // READY!

// 5. Static method as the pipe target.
class Calc {
    public static function quad(int $n): int { return $n * 4; }
}
echo "3 quadrupled = " . (3 |> Calc::quad(...)) . "\n"; // 12

// 6. Instance method as the pipe target.
class Tagger {
    public function __construct(private string $tag) {}
    public function wrap(string $s): string { return "<" . $this->tag . ">" . $s . "</" . $this->tag . ">"; }
}
$em = new Tagger("em");
echo ("important" |> $em->wrap(...)) . "\n";           // <em>important</em>
