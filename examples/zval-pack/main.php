<?php
// Demonstrates the zval bridge: elephc values can be packed into PHP-shaped
// `zval` structs (so a linked PHP extension can read them) and unpacked back.
//
// `zval_pack(value)` returns an opaque pointer to a 16-byte PHP zval; the IS_*
// type byte lives at offset +8. `zval_type` reads that byte (IS_LONG=4, IS_DOUBLE=5,
// IS_STRING=6, IS_ARRAY=7, IS_NULL=1, IS_TRUE=3, IS_FALSE=2). `zval_unpack` turns
// the zval back into a native elephc value, and `zval_free` releases the storage.

function describe(mixed $value): void {
    $z = zval_pack($value);
    echo "type=" . zval_type($z) . " value=" . zval_unpack($z) . "\n";
    zval_free($z);
}

describe(42);
describe(3.5);
describe(true);
describe("hello");
describe(null);

// Associative arrays pack as IS_ARRAY (7) and round-trip back element for element.
$assoc = zval_unpack(zval_pack(["a" => 1, "b" => 2, "c" => 3]));
echo "assoc a=" . $assoc["a"] . " b=" . $assoc["b"] . " c=" . $assoc["c"] . "\n";

// Packed (indexed) arrays likewise round-trip, including nested arrays.
$packed = zval_unpack(zval_pack([10, 20, 30]));
echo "packed=" . implode(",", $packed) . "\n";

$nested = zval_unpack(zval_pack([[1, 2], [3, 4]]));
echo "nested=" . $nested[0][1] . $nested[1][0] . "\n";