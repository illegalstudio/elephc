<?php
// Type operations: casting, gettype, empty, unset, settype

// Type casting
echo "--- Casting ---\n";
echo "(int)3.7 = " . (int)3.7 . "\n";
echo "(float)42 = " . (float)42 . "\n";
echo "(string)100 = " . (string)100 . "\n";
echo "(bool)0 = " . (bool)0 . "\n";
echo "(bool)1 = " . (bool)1 . "\n";

// gettype
echo "\n--- gettype ---\n";
echo "gettype(42) = " . gettype(42) . "\n";
echo "gettype(3.14) = " . gettype(3.14) . "\n";
echo "gettype(\"hi\") = " . gettype("hi") . "\n";
echo "gettype(true) = " . gettype(true) . "\n";
echo "gettype(null) = " . gettype(null) . "\n";

// empty
echo "\n--- empty ---\n";
echo "empty(0) = " . empty(0) . "\n";
echo "empty(1) = " . empty(1) . "\n";
echo "empty(\"\") = " . empty("") . "\n";
echo "empty(null) = " . empty(null) . "\n";
echo "empty(false) = " . empty(false) . "\n";

// unset
echo "\n--- unset ---\n";
$x = 42;
echo "before unset: " . $x . "\n";
unset($x);
echo "after unset, is_null: " . is_null($x) . "\n";

// settype
echo "\n--- settype ---\n";
$y = 3.14;
echo "before: " . $y . " (" . gettype($y) . ")\n";
settype($y, "integer");
echo "after settype to integer: " . $y . " (" . gettype($y) . ")\n";
