<?php
// json-exception — demonstrates the PHP-compatible exception hierarchy
// surfaced by elephc: Throwable (interface), Exception, RuntimeException,
// JsonException. Each level is catchable as itself or any of its parents.

// JsonException extends RuntimeException extends Exception.
$e = new JsonException("decode failed");
echo "JsonException::getMessage = " . $e->getMessage() . "\n";

// Catch as the most specific class.
try {
    throw new JsonException("syntax");
} catch (JsonException $err) {
    echo "caught JsonException: " . $err->getMessage() . "\n";
}

// Catch as the parent class, RuntimeException.
try {
    throw new JsonException("recursion");
} catch (RuntimeException $err) {
    echo "caught RuntimeException: " . $err->getMessage() . "\n";
}

// Catch as the grandparent, Exception.
try {
    throw new JsonException("utf8");
} catch (Exception $err) {
    echo "caught Exception: " . $err->getMessage() . "\n";
}

// instanceof verifies the inheritance chain.
$e = new JsonException("x");
echo "JsonException is RuntimeException: "
    . ($e instanceof RuntimeException ? "yes" : "no") . "\n";
echo "JsonException is Exception: "
    . ($e instanceof Exception ? "yes" : "no") . "\n";
echo "JsonException is Throwable: "
    . ($e instanceof Throwable ? "yes" : "no") . "\n";

// RuntimeException can stand on its own — it's a concrete class too.
$r = new RuntimeException("plain");
echo "RuntimeException::getMessage = " . $r->getMessage() . "\n";
echo "RuntimeException is Exception: "
    . ($r instanceof Exception ? "yes" : "no") . "\n";

// JSON_THROW_ON_ERROR makes json_decode throw a JsonException on failure
// instead of returning null. Without the flag, json_last_error reports the
// failure as JSON_ERROR_SYNTAX while json_decode returns null.

echo "\n-- JSON_THROW_ON_ERROR demo --\n";
echo "valid input ([1,2,3]):    "
    . (json_validate("[1,2,3]") ? "true" : "false") . "\n";
echo "invalid input (garbage): "
    . (json_validate("garbage") ? "true" : "false") . "\n";
echo "json_last_error after invalid: " . json_last_error()
    . " (" . json_last_error_msg() . ")\n";

try {
    json_decode("garbage", null, 512, JSON_THROW_ON_ERROR);
    echo "did not throw\n";
} catch (JsonException $err) {
    echo "JSON_THROW_ON_ERROR caught: " . $err->getMessage() . "\n";
}

// The JsonException raised by JSON_THROW_ON_ERROR is the same class as one
// the user can throw manually, so it can be caught at any parent level.
try {
    json_decode("", null, 512, JSON_THROW_ON_ERROR);
} catch (RuntimeException $err) {
    echo "caught at RuntimeException level: " . $err->getMessage() . "\n";
}
