<?php
// Capture PHP's ordered integer-map conversions, diagnostics, and exceptional input kinds.
// Run: php scripts/mbstring/capture_entity_maps.php

$values = [null, false, true, 0, -1, PHP_INT_MIN, PHP_INT_MAX,
    1.5, -1.5, INF, -INF, NAN, 1e20, -1e20, 9223372036854775808.0,
    -9223372036854775808.0, -9223372036854777856.0,
    '', 'bad', '0', '-1', '  +12 ', '1.5', '-1.5', '12bad', '1.5bad',
    '1e+', '1.e2', '.5', "12\0", "\v42\f", '0xFF', '0b10',
    '1e999', '-1e999', '9223372036854775808', '-9223372036854775809',
    '18446744073709551616', '-18446744073709551616', '1e20', '-1e20',
    [], new stdClass()];
$cases = [];
foreach ($values as $value) {
    foreach ([false, true] as $decode) {
        $trace = [];
        set_error_handler(function (int $level, string $message) use (&$trace): bool {
            $trace[] = ['diagnostic', $level, bin2hex($message)];
            return true;
        });
        $input = match (true) {
            is_float($value) => ['float', bin2hex(pack('E', $value))],
            is_string($value) => ['string', bin2hex($value)],
            is_array($value) => ['array', []],
            is_object($value) => ['object', null],
            default => [get_debug_type($value), $value],
        };
        try {
            $result = $decode ? mb_decode_numericentity('&#65;&#29483;', [0, 0x10FFFF, $value, 0xFFFFFFFF], 'UTF-8')
                : mb_encode_numericentity('A猫', [0, 0x10FFFF, $value, 0xFFFFFFFF], 'UTF-8');
            $result = ['string', bin2hex($result)];
        } catch (Throwable $error) {
            $result = ['error', $error::class, bin2hex($error->getMessage())];
        }
        restore_error_handler();
        $cases[] = ['input' => $input, 'decode' => $decode, 'result' => $result, 'trace' => $trace];
    }
}
file_put_contents(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/entity_maps.json',
    json_encode($cases, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR) . "\n");
