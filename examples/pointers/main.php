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

// Pointer casting
$typed = ptr_cast<int>($p);
echo "Typed ptr: ";
echo $typed;
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
