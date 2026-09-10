<?php
// Capture a cross-encoding oracle for character, slicing, padding, and search operations.
// Run: php scripts/mbstring/capture_operations.php

$root = __DIR__ . '/../../crates/elephc-mbstring/tests/fixtures';
$codecs = json_decode(file_get_contents("$root/codecs.json"), true, flags: JSON_THROW_ON_ERROR);
$stream = gzopen("$root/operations.jsonl.gz", 'wb9');
$excluded = [];
mb_substitute_character(0xFFFD);
error_reporting(E_ALL & ~E_DEPRECATED);

// Encode byte strings independently of JSON's UTF-8 requirements.
function fixtureValue(mixed $value): mixed {
    if (is_string($value)) { return ['bytes' => bin2hex($value)]; }
    if (is_array($value)) { return array_map(fixtureValue(...), $value); }
    return $value;
}

// Record returned values and exact PHP exception class/message, excluding deprecations.
function captureCall(string $function, array $arguments, string $encoding): void {
    global $stream, $excluded;
    // PHP 8.5.10 subtracts this start from byte length before allocating its slow-path
    // buffer, causing an uncatchable allocation-overflow fatal rather than a result.
    if ($function === 'mb_substr' && $encoding === 'SJIS-mac' && $arguments[2] === null
        && $arguments[1] > strlen($arguments[0])) {
        $excluded[] = ['function' => $function, 'encoding' => $encoding,
            'arguments' => fixtureValue($arguments),
            'reason' => 'PHP 8.5.10 mb_get_substr_slow allocation underflow terminates the oracle process'];
        return;
    }
    try {
        $result = fixtureValue(@$function(...[...$arguments, $encoding]));
    } catch (Throwable $error) {
        $result = ['error' => [get_class($error), $error->getMessage()]];
    }
    gzwrite($stream, json_encode([
        'function' => $function, 'encoding' => $encoding,
        'arguments' => fixtureValue($arguments), 'result' => $result,
    ], JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
}

foreach (array_keys($codecs['sha256']) as $encoding) {
    $word = mb_convert_encoding('hélloΣ日本', $encoding, 'UTF-8');
    $spaces = mb_convert_encoding(" \u{3000}ABC\u{180e} ", $encoding, 'UTF-8');
    $pad = mb_convert_encoding('日é', $encoding, 'UTF-8');
    $marker = mb_convert_encoding('…', $encoding, 'UTF-8');
    $needle = mb_convert_encoding('日', $encoding, 'UTF-8');
    foreach (['', $word, "\xc3\x28\x00\x80", $spaces] as $input) {
        foreach ([-10, -1, 0, 1, 5, 100] as $start) {
            foreach ([null, -2, 0, 1, 3] as $length) {
                captureCall('mb_substr', [$input, $start, $length], $encoding);
                captureCall('mb_strcut', [$input, $start, $length], $encoding);
            }
        }
        foreach ([-1, 0, 1, 2, 4, 1073741824] as $length) {
            captureCall('mb_str_split', [$input, $length], $encoding);
        }
        foreach (['mb_trim', 'mb_ltrim', 'mb_rtrim'] as $function) {
            foreach ([null, '', "\x80", mb_convert_encoding(' A.', $encoding, 'UTF-8')] as $characters) {
                captureCall($function, [$input, $characters], $encoding);
            }
        }
        foreach (['mb_ucfirst', 'mb_lcfirst', 'mb_ord'] as $function) {
            captureCall($function, [$input], $encoding);
        }
        foreach ([-1, 1, 10] as $length) {
            foreach ([-1, 0, 1, 2, 3] as $side) {
                captureCall('mb_str_pad', [$input, $length, $pad, $side], $encoding);
            }
            captureCall('mb_str_pad', [$input, $length, '', 1], $encoding);
        }
        foreach ([-1, 0, 1, 100] as $start) {
            foreach ([-1, 0, 1, 5, 100] as $width) {
                foreach (['', $marker] as $trimMarker) {
                    captureCall('mb_strimwidth', [$input, $start, $width, $trimMarker], $encoding);
                }
            }
        }
        foreach (['', $needle, "\xc3", mb_convert_encoding('A', $encoding, 'UTF-8')] as $query) {
            foreach (['mb_strpos', 'mb_stripos', 'mb_strrpos', 'mb_strripos'] as $function) {
                foreach ([-1, 0, 1, 100] as $offset) {
                    captureCall($function, [$input, $query, $offset], $encoding);
                }
            }
            foreach (['mb_strstr', 'mb_stristr', 'mb_strrchr', 'mb_strrichr'] as $function) {
                foreach ([false, true] as $before) {
                    captureCall($function, [$input, $query, $before], $encoding);
                }
            }
            captureCall('mb_substr_count', [$input, $query], $encoding);
        }
    }
    foreach ([-1, 0, 65, 0xD800, 0x1F600, 0xFFFFFFFF] as $code) {
        captureCall('mb_chr', [$code], $encoding);
    }
    captureCall('mb_substr', [$word, PHP_INT_MIN, null], $encoding);
    captureCall('mb_substr', [$word, 0, PHP_INT_MIN], $encoding);
}
foreach (['BASE64', 'UTF-7', 'UTF7-IMAP', 'JIS', 'ISO-2022-JP', 'CP50220'] as $encoding) {
    captureCall('mb_ord', ['a'], $encoding);
    captureCall('mb_chr', [65], $encoding);
}
gzclose($stream);
file_put_contents("$root/operations-excluded.json", json_encode($excluded,
    JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
