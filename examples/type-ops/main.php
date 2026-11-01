<?php
// Type operations: casting, gettype, empty, unset, settype

// Type casting
echo "--- Casting ---\n";
echo "(int)3.7 = " . (int)3.7 . "\n";
echo "(float)42 = " . (float)42 . "\n";
echo "(string)100 = " . (string)100 . "\n";
echo "(bool)0 = " . (bool)0 . "\n";
echo "(bool)1 = " . (bool)1 . "\n";
$source = ["name" => "array"];
$object = (object) $source;
$object->name = "object";
echo "(object)array = " . $source["name"] . "/" . $object->name . "\n";

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
// `unset` destroys the binding, so the name is undefined afterwards: probe it with
// `isset` rather than reading it, exactly as in PHP.
echo "\n--- unset ---\n";
$x = 42;
$z = 7;
echo "before unset: " . $x . ", " . $z . "\n";
echo "before unset, isset: " . (isset($x) ? "yes" : "no") . ", " . (isset($z) ? "yes" : "no") . "\n";
unset($x, $z);
echo "after unset, isset: " . (isset($x) ? "yes" : "no") . ", " . (isset($z) ? "yes" : "no") . "\n";

// settype
echo "\n--- settype ---\n";
$y = 3.14;
echo "before: " . $y . " (" . gettype($y) . ")\n";
settype($y, "integer");
echo "after settype to integer: " . $y . " (" . gettype($y) . ")\n";
