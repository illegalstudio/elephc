<?php
// Capture ordered setting mutations, list parsing, failures, and explicit-encoding diagnostics.
// Run: php scripts/mbstring/capture_state.php

error_reporting(E_ALL & ~E_DEPRECATED);
$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/state.jsonl.gz', 'wb9');
mb_language('neutral');
mb_internal_encoding('UTF-8');
mb_http_output('UTF-8');
mb_detect_order(['ASCII', 'UTF-8']);
mb_substitute_character(63);

/** Stores arbitrary PHP strings, including exception messages, without UTF-8 loss. */
function lossless(mixed $value): mixed {
    if (is_string($value)) { return ['bytes' => bin2hex($value)]; }
    if (is_array($value)) { return array_map(lossless(...), $value); }
    return $value;
}

/** Records one operation together with the settings and diagnostics it leaves behind. */
function captureState($stream, string $function, mixed $argument): void {
    $warnings = [];
    set_error_handler(static function (int $level, string $message) use (&$warnings): bool {
        $warnings[] = [$level, $message];
        return true;
    });
    try {
        $result = $function === 'mb_strlen' ? mb_strlen('', $argument) : $function($argument);
    } catch (Throwable $error) {
        $result = ['error' => [$error::class, $error->getMessage()]];
    }
    restore_error_handler();
    $state = [mb_language(), mb_internal_encoding(), mb_http_output(), mb_detect_order(), mb_substitute_character()];
    gzwrite($stream, json_encode(['function' => $function, 'argument' => lossless($argument),
        'result' => lossless($result), 'warnings' => lossless($warnings), 'state' => lossless($state)], JSON_THROW_ON_ERROR) . "\n");
}

$languages = json_decode(file_get_contents(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/languages.json'), true, flags: JSON_THROW_ON_ERROR);
foreach ($languages['languages'] as $name => $info) {
    foreach ([$name, strtoupper($name), ...$info['aliases'], $name . "\0extra", $name . ' ', "bad\xFF", ''] as $alias) {
        captureState($stream, 'mb_language', $alias);
        captureState($stream, 'mb_detect_order', 'auto');
    }
    captureState($stream, 'mb_language', $name);
    foreach (['', ' ', '\t', "\t", ',', ',,', 'a', 'au', 'aut', 'auto', 'AUTO', 'auto,auto',
        'UTF-8,auto,ASCII,auto,UTF-8', '"auto,UTF-8"', '""', " ASCII,\tUTF-8 \t", "UTF-8\n", 'pass',
        "UTF-8\0", "UTF-8\0junk", 'UTF-8,,SJIS', 'auto,unknown', "\xff", [], ['auto'], ['auto', 'auto'],
        ['a'], [''], [' UTF-8'], ["UTF-8\0bad"], ['UTF-8', 'ASCII', 'UTF-8'], ['UTF-8', "\xff"], null] as $list) {
        captureState($stream, 'mb_detect_order', $list);
    }
}
foreach (mb_list_encodings() as $encoding) {
    foreach ([$encoding, ...mb_encoding_aliases($encoding), $encoding . "\0extra"] as $name) {
        foreach (['mb_internal_encoding', 'mb_http_output'] as $function) { captureState($stream, $function, $name); }
    }
}
foreach (['', 'p', 'pa', 'pas', 'pass', 'PASS', 'Pass', 'auto', 'unknown', "\xff", null] as $name) {
    foreach (['mb_internal_encoding', 'mb_http_output'] as $function) { captureState($stream, $function, $name); }
}
foreach ([63, 0, 0xD7FF, 0xE000, 0x10FFFF, -1, PHP_INT_MIN, PHP_INT_MAX, 0xD800, 0xDFFF, 0x110000] as $code) {
    captureState($stream, 'mb_substitute_character', $code);
    foreach (['none', 'NoNe', 'long', 'entity', '63', '0', 'unknown', ' long', "long\0", '', null] as $mode) {
        captureState($stream, 'mb_substitute_character', $mode);
    }
}
captureState($stream, 'mb_internal_encoding', 'UTF-8');
foreach (['BASE64', 'base64', null, 'BASE64', 'bad', 'BASE64', 'UTF-8', 'BASE64',
    "BASE64\0extra", "BASE64\0extra", 'HTML', 'html', 'HTML-ENTITIES', 'Quoted-Printable', 'qprint',
    'UUENCODE', 'UUENCODE', 'UTF-8', 'UUENCODE', null] as $name) {
    captureState($stream, 'mb_strlen', $name);
}
gzclose($stream);
