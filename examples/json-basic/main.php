<?php
// json-basic — demonstrates the JSON functions and constants
// currently exposed by elephc.

// json_encode handles every primitive plus indexed and associative arrays.
echo json_encode(42) . "\n";
echo json_encode(3.14) . "\n";
echo json_encode("hello, world") . "\n";
echo json_encode(true) . "\n";
echo json_encode(null) . "\n";
echo json_encode([1, 2, 3]) . "\n";
echo json_encode(["name" => "Alice", "age" => 30]) . "\n";

// Objects are encoded via their public properties (private/protected are
// skipped, mirroring PHP). See examples/json-jsonserializable for opting in
// to a custom shape via JsonSerializable.
class Point { public int $x = 1; public int $y = 2; }
echo json_encode(new Point()) . "\n";

// json_decode is a full structural decoder: every JSON value type
// round-trips through a recursive-descent parser into a boxed Mixed
// cell. Scalars carry their runtime type, strings preserve full escape
// decoding, and arrays/objects nest recursively.
echo json_decode("\"escaped \\\"quote\\\"\"") . "\n";    // string → "escaped \"quote\""
echo json_decode("42") . "\n";                            // int → 42
echo json_decode("3.14") . "\n";                          // float → 3.14
echo json_decode("true") . "\n";                          // bool true → "1" (PHP's bool→string rule)
echo json_decode("false") . "X\n";                        // bool false → "" (PHP's bool→string rule), trailing X for visibility
echo json_decode("null") . "Y\n";                         // null → "" (PHP's null→string rule), trailing Y for visibility
echo gettype(json_decode("100")) . "\n";                  // observable runtime type → "integer"
echo gettype(json_decode("\"hi\"")) . "\n";               // observable runtime type → "string"
echo gettype(json_decode("null")) . "\n";                 // observable runtime type → "NULL"

// Containers decode structurally — arrays become Mixed(array) and
// objects become Mixed(assoc), with each element/value recursively
// decoded. Round-trip through json_encode produces the canonical
// compact form (whitespace dropped, scalars re-encoded).
echo gettype(json_decode("[1, 2, 3]")) . "\n";            // → "array"
echo gettype(json_decode("{\"a\": 1}")) . "\n";           // → "array" (PHP groups indexed and assoc under "array")
echo json_encode(json_decode("[1, 2, 3]")) . "\n";        // → [1,2,3]
echo json_encode(json_decode("{\"a\": 1, \"b\": 2}")) . "\n"; // → {"a":1,"b":2}

// Nested containers work too: the boundary scanner respects nested
// brackets and strings while finding element/pair boundaries.
$payload = "{\"users\": [{\"name\": \"Alice\", \"age\": 30}, {\"name\": \"Bob\", \"age\": 25}], \"count\": 2}";
echo json_encode(json_decode($payload)) . "\n";

// Default mode returns stdClass; properties are accessible directly.
$user = json_decode("{\"name\": \"Alice\", \"age\": 30}");
echo $user->name . " is " . $user->age . "\n";

// Pass true as the second argument to get an associative array instead.
// Mixed[] indexing and count() work directly on the decoded result, so
// the canonical PHP idiom `json_decode($json, true)["k"]` is a one-liner.
$assoc = json_decode("{\"users\":[{\"name\":\"Alice\"},{\"name\":\"Bob\"}],\"count\":2}", true);
echo "decoded type: " . gettype($assoc) . "\n";
echo "users count : " . count($assoc["users"]) . "\n";
echo "first name  : " . $assoc["users"][0]["name"] . "\n";
echo "second name : " . $assoc["users"][1]["name"] . "\n";

// stdClass works standalone too: any property name can be assigned.
$o = new stdClass();
$o->status = "ok";
$o->count = 7;
echo json_encode($o) . "\n";

// Malformed input returns null and sets JSON_ERROR_SYNTAX. Subsequent
// successful calls reset the slot so error state never leaks.
$bad = json_decode("not json");
echo "garbage type: " . gettype($bad) . " (err " . json_last_error() . ", " . json_last_error_msg() . ")\n";

// JSON_THROW_ON_ERROR raises a JsonException on malformed input or
// depth overflow; both fold the runtime error code into a catchable
// exception with the PHP-faithful message.
try {
    json_decode("{", null, 512, JSON_THROW_ON_ERROR);
} catch (JsonException $e) {
    echo "throw-flag caught: " . $e->getMessage() . "\n";
}

// json_validate is the standalone RFC 8259 predicate (PHP 8.3). Returns
// the same JSON_ERROR_* code as json_decode when the input is rejected.
$ok = json_validate("[1, 2, 3]");
echo ($ok ? "[1, 2, 3] is valid JSON" : "invalid") . "\n";

// All JSON constants from the PHP manual are usable, including the bitmask
// flags that future encode/decode paths will respect.
echo "JSON_PRETTY_PRINT          = " . JSON_PRETTY_PRINT . "\n";
echo "JSON_UNESCAPED_SLASHES     = " . JSON_UNESCAPED_SLASHES . "\n";
echo "JSON_UNESCAPED_UNICODE     = " . JSON_UNESCAPED_UNICODE . "\n";
echo "JSON_THROW_ON_ERROR        = " . JSON_THROW_ON_ERROR . "\n";
echo "JSON_OBJECT_AS_ARRAY       = " . JSON_OBJECT_AS_ARRAY . "\n";
echo "JSON_ERROR_SYNTAX          = " . JSON_ERROR_SYNTAX . "\n";

$encode_flags = JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE;
echo "combined encoding flags    = " . $encode_flags . "\n";
