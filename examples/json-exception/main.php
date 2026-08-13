<?php
// json-exception — demonstrates the PHP-compatible exception hierarchy
// surfaced by elephc: Throwable (interface), Exception, JsonException.
// Each level is catchable as itself or any of its parents.

// JsonException extends Exception, DIRECTLY. It is not a RuntimeException —
// `php -r 'var_dump(class_parents("JsonException"));'` answers ["Exception"].
$e = new JsonException("decode failed");
echo "JsonException::getMessage = " . $e->getMessage() . "\n";

// Catch as the most specific class.
try {
    throw new JsonException("syntax");
} catch (JsonException $err) {
    echo "caught JsonException: " . $err->getMessage() . "\n";
}

// Catch as the parent, Exception.
try {
    throw new JsonException("utf8");
} catch (Exception $err) {
    echo "caught Exception: " . $err->getMessage() . "\n";
}

// Catch as the interface every throwable implements.
try {
    throw new JsonException("interface");
} catch (Throwable $err) {
    echo "caught Throwable: " . $err->getMessage() . "\n";
}

// instanceof verifies the inheritance chain — including what is NOT in it.
$e = new JsonException("x");
echo "JsonException is Exception: "
    . ($e instanceof Exception ? "yes" : "no") . "\n";
echo "JsonException is Throwable: "
    . ($e instanceof Throwable ? "yes" : "no") . "\n";
echo "JsonException is RuntimeException: "
    . ($e instanceof RuntimeException ? "yes" : "no") . "\n";

// So a handler written for RuntimeException does not see a JSON error. This is
// worth spelling out: catching too wide a type is a common way to swallow one.
try {
    throw new JsonException("not a runtime exception");
} catch (RuntimeException $err) {
    echo "unreachable\n";
} catch (Exception $err) {
    echo "RuntimeException did not match, Exception did: " . $err->getMessage() . "\n";
}

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

// The JsonException raised by JSON_THROW_ON_ERROR is the same class as one the
// user can throw manually, so it can be caught at any level it really has.
try {
    json_decode("", null, 512, JSON_THROW_ON_ERROR);
} catch (Exception $err) {
    echo "caught at Exception level: " . $err->getMessage() . "\n";
}
