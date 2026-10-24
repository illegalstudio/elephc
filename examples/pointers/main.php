<?php
// Pointer operations in elephc
// Pointers provide low-level memory access for systems programming

// Creating pointers
$x = 42;
$p = ptr($x);           // take address of $x
$null = ptr_null();      // null pointer

echo "Address of x: ";
echo $p;
echo "\n";

echo "Null pointer: ";
echo $null;
echo "\n";

// Pointers convert to the same "0x..." text in concatenation and interpolation
$label = "p = " . $p;
echo $label === "p = $p" ? "Concatenation matches interpolation\n" : "mismatch\n";
echo "Null as string: " . (string)$null . "\n";

// Checking for null
echo "Is null? ";
echo ptr_is_null($null) ? "yes" : "no";
echo "\n";

echo "Is p null? ";
echo ptr_is_null($p) ? "yes" : "no";
echo "\n";

// Reading and writing through pointers
echo "Value at p: ";
echo ptr_get($p);
echo "\n";

ptr_set($p, 100);
echo "After ptr_set: x = ";
echo $x;
echo "\n";

// Pointer comparison
$q = ptr($x);
echo "Same address? ";
echo $p === $q ? "yes" : "no";
echo "\n";

// Type sizes
echo "sizeof(int) = ";
echo ptr_sizeof("int");
echo "\n";
echo "sizeof(string) = ";
echo ptr_sizeof("string");
echo "\n";
echo "sizeof(ptr) = ";
echo ptr_sizeof("ptr");
echo "\n";

// Type introspection
echo "gettype(p) = ";
echo gettype($p);
echo "\n";

// Pointer casting keeps the same address and only changes the static tag
$typed = ptr_cast<int>($p);
echo "Typed ptr: ";
echo $typed;
echo "\n";

echo "Same address after cast? ";
echo $p === $typed ? "yes" : "no";
echo "\n";

// Modifying variables via pointer in a function
function increment($p) {
    ptr_set($p, ptr_get($p) + 1);
}

$counter = 0;
$cp = ptr($counter);
for ($i = 0; $i < 5; $i++) {
    increment($cp);
}
echo "Counter after 5 increments: ";
echo $counter;
echo "\n";

// Arrays and hashes of pointers hold the raw addresses: here, records of a packed buffer
packed class Point { public int $x; public int $y; }

$points = buffer_new<Point>(3);
$picked = [$points[0], $points[2]];
$picked[] = null;                       // a null element is the null pointer
foreach ($picked as $point) {
    if ($point !== null) {
        $point->x += 10;                // writes through to the buffer
    }
}
echo "Point x values: ", $points[0]->x, " ", $points[1]->x, " ", $points[2]->x, "\n";
echo "Third element is null? ", $picked[2] === null ? "yes" : "no", "\n";

$byName = ["origin" => $points[0], "far" => $points[2]];
$byName["far"]->y = 7;
echo "far.y = ", $points[2]->y, ", missing key is null? ", ($byName["nope"] ?? null) === null ? "yes" : "no", "\n";

// A local that starts as null and later holds a record is one pointer word (null is address 0)
$points[1]->x = 25;
$widest = null;
for ($i = 0; $i < buffer_len($points); $i++) {
    if ($widest === null || $points[$i]->x > $widest->x) {
        $widest = $points[$i];
    }
}
if ($widest !== null) {
    echo "Largest x = ", $widest->x, "\n";
}
