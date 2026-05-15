<?php
// json-flags — demonstrates the encoding flags currently observed by
// json_encode(): JSON_UNESCAPED_SLASHES and JSON_PRETTY_PRINT, plus
// combinations of the two.

$value = ["url" => "https://example.com/path", "tags" => ["a", "b", "c"]];

echo "compact:\n";
echo json_encode($value) . "\n\n";

echo "JSON_UNESCAPED_SLASHES:\n";
echo json_encode($value, JSON_UNESCAPED_SLASHES) . "\n\n";

echo "JSON_PRETTY_PRINT:\n";
echo json_encode($value, JSON_PRETTY_PRINT) . "\n\n";

echo "JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES:\n";
echo json_encode($value, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES) . "\n\n";

echo "deeply nested with JSON_PRETTY_PRINT:\n";
echo json_encode([
    "user" => [
        "name" => "Alice",
        "addresses" => [
            ["city" => "Paris", "zip" => "75001"],
            ["city" => "Lyon", "zip" => "69001"],
        ],
    ],
], JSON_PRETTY_PRINT) . "\n\n";

// JSON_HEX_* family is useful when embedding JSON inside HTML or XML —
// each flag rewrites a specific character to its \uXXXX escape so the
// resulting string is safe to drop into a script tag, attribute, or body.
$snippet = "Tom & Jerry's <show> says \"hi\"";
echo "HTML-safe (HEX_TAG | HEX_AMP | HEX_APOS | HEX_QUOT):\n";
echo json_encode($snippet, JSON_HEX_TAG | JSON_HEX_AMP | JSON_HEX_APOS | JSON_HEX_QUOT) . "\n\n";

// Non-finite floats trigger JSON_ERROR_INF_OR_NAN. Without
// JSON_THROW_ON_ERROR the encoder substitutes 0 so containers stay valid.
echo "INF without flag:           " . json_encode(INF) . "\n";
echo "json_last_error_msg:       " . json_last_error_msg() . "\n";

try {
    json_encode([1.0, INF, 2.0], JSON_THROW_ON_ERROR);
} catch (JsonException $e) {
    echo "INF with throw flag (caught): " . $e->getMessage() . "\n";
}

// JSON_FORCE_OBJECT: encode an indexed array as a JSON object whose keys
// are the integer indexes "0", "1", "2", ...
echo "\n-- JSON_FORCE_OBJECT --\n";
echo json_encode([10, 20, 30], JSON_FORCE_OBJECT) . "\n";
echo json_encode(["a", "b"], JSON_FORCE_OBJECT) . "\n";
echo json_encode([], JSON_FORCE_OBJECT) . "\n";
echo json_encode([100, 200], JSON_FORCE_OBJECT | JSON_PRETTY_PRINT) . "\n";

// JSON_UNESCAPED_UNICODE: keep multibyte UTF-8 verbatim instead of the
// default \uXXXX escape (with surrogate pairs for codepoints ≥ U+10000).
echo "\n-- JSON_UNESCAPED_UNICODE --\n";
echo "default: " . json_encode("café 你好 😀") . "\n";
echo "unescaped: " . json_encode("café 你好 😀", JSON_UNESCAPED_UNICODE) . "\n";

// JSON_NUMERIC_CHECK: numeric-looking strings encode as raw JSON numbers
// when they match the RFC 8259 number grammar.
echo "\n-- JSON_NUMERIC_CHECK --\n";
echo json_encode(["count" => "42", "ratio" => "3.14", "name" => "Alice"], JSON_NUMERIC_CHECK) . "\n";

// JSON_PRESERVE_ZERO_FRACTION: integer-valued floats stay 1.0 instead of
// collapsing to 1 (so consumers know the value is a float).
echo "\n-- JSON_PRESERVE_ZERO_FRACTION --\n";
echo "default: " . json_encode([1.0, 2.5, 3.0]) . "\n";
echo "preserve: " . json_encode([1.0, 2.5, 3.0], JSON_PRESERVE_ZERO_FRACTION) . "\n";

// JSON_INVALID_UTF8_*: malformed UTF-8 input is detected during encoding.
// Without sanitization flags the encoder records JSON_ERROR_UTF8 and emits
// partial output (the malformed byte dropped). The IGNORE flag silences
// the error code; the SUBSTITUTE flag replaces malformed bytes with the
// U+FFFD REPLACEMENT CHARACTER (� escape, or its UTF-8 bytes when
// JSON_UNESCAPED_UNICODE is also set). Bytes are produced via chr() since
// elephc's lexer does not parse \xHH string escapes.
echo "\n-- JSON_INVALID_UTF8_IGNORE / JSON_INVALID_UTF8_SUBSTITUTE --\n";
$bad = "ok-" . chr(0x80) . "-byte"; // 0x80 alone is a lone continuation
echo "default:    " . json_encode($bad) . " (last_error=" . json_last_error() . ")\n";
echo "ignore:     " . json_encode($bad, JSON_INVALID_UTF8_IGNORE) . " (last_error=" . json_last_error() . ")\n";
echo "substitute: " . json_encode($bad, JSON_INVALID_UTF8_SUBSTITUTE) . " (last_error=" . json_last_error() . ")\n";

try {
    json_encode($bad, JSON_THROW_ON_ERROR);
} catch (JsonException $e) {
    echo "throw flag: " . $e->getMessage() . "\n";
}

// JSON_BIGINT_AS_STRING (decode flag): integer-grammar JSON tokens that
// overflow PHP_INT_MAX (= 9223372036854775807) come back as a string
// preserving the original digits, instead of being coerced to a float
// (PHP's default for overflow). Floats and in-range integers are
// unaffected; the flag threads through nested arrays and objects.
echo "\n-- JSON_BIGINT_AS_STRING --\n";
echo "without flag: " . gettype(json_decode("999999999999999999999")) . "\n";
echo "with flag:    " . gettype(json_decode("999999999999999999999", false, 512, JSON_BIGINT_AS_STRING))
    . " value=" . json_decode("999999999999999999999", false, 512, JSON_BIGINT_AS_STRING) . "\n";
$bigArr = json_decode("[1, 99999999999999999999, 3]", false, 512, JSON_BIGINT_AS_STRING);
echo "in-array element 1: " . gettype($bigArr[1]) . " value=" . $bigArr[1] . "\n";
echo "in-array element 0: " . gettype($bigArr[0]) . " value=" . $bigArr[0] . "\n";
