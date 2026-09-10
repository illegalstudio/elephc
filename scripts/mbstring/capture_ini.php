<?php
// Capture raw mbstring INI storage and effective request settings on PHP 8.5.10.
// Run: php scripts/mbstring/capture_ini.php

/** Preserves binary text, scalar types, and nested PHP array insertion order. */
function iniValue(mixed $value): mixed {
    if (is_string($value)) { return ['bytes' => bin2hex($value)]; }
    if (is_array($value)) {
        $entries = [];
        foreach ($value as $key => $entry) { $entries[] = [iniValue($key), iniValue($entry)]; }
        return ['array' => $entries];
    }
    return $value;
}

/** Observes raw metadata and effective state, including the shared Zend quantity conversion. */
function iniSnapshot(): array {
    // PHP 8.5.10 mb_get_info() transfers a borrowed INI MIME string without retaining it.
    // Read selectors separately and use ini_get for that field to avoid corrupting PHP's heap.
    $info = [];
    foreach (['internal_encoding', 'http_output', 'http_output_conv_mimetypes', 'mail_charset', 'mail_header_encoding',
        'mail_body_encoding', 'illegal_chars', 'encoding_translation', 'language', 'detect_order', 'substitute_character', 'strict_detection'] as $selector) {
        $info[$selector] = $selector === 'http_output_conv_mimetypes'
            ? ini_get('mbstring.http_output_conv_mimetypes') : mb_get_info($selector);
    }
    return [iniValue(ini_get_all('mbstring', true)), iniValue(ini_get_all('mbstring', false)),
        iniValue($info), iniValue(mb_http_input('I')),
        [@ini_parse_quantity(ini_get('mbstring.regex_stack_limit')), @ini_parse_quantity(ini_get('mbstring.regex_retry_limit'))]];
}

/** Restores every modified directive and clears public setting changes left by the preceding case. */
function iniBaseline(bool $public): void {
    mb_language('neutral');
    mb_internal_encoding('UTF-8');
    mb_http_output('UTF-8');
    mb_substitute_character(63);
    mb_detect_order(['ASCII', 'UTF-8']);
    foreach (array_keys(ini_get_all('mbstring')) as $key) { @ini_restore($key); }
    if ($public) {
        mb_language('Japanese');
        mb_internal_encoding('SJIS');
        mb_http_output('ISO-8859-1');
        mb_substitute_character(35);
        mb_substitute_character('entity');
        mb_detect_order(['SJIS']);
    }
}

/** Captures each mutation or restore separately so failed-handler effects cannot be hidden. */
function iniStep(string $name, ?string $value): array {
    $warnings = [];
    set_error_handler(static function (int $severity, string $message) use (&$warnings): bool {
        $warnings[] = [$severity, iniValue($message)];
        return true;
    });
    $output = $value === null ? ini_restore($name) : ini_set($name, $value);
    restore_error_handler();
    return ['output' => iniValue($output), 'warnings' => $warnings, 'state' => iniSnapshot()];
}

if (realpath($_SERVER['SCRIPT_FILENAME']) !== __FILE__) { return; }

if (($argv[1] ?? '') === '--startup') {
    echo json_encode(iniSnapshot(), JSON_THROW_ON_ERROR), "\n";
    exit;
}

$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/ini.jsonl.gz', 'wb9');
$count = 0;
$numbers = ['', ' ', "\t\n", '0', '1', '-1', '+1', '42 ', '42garbage', 'foo', '0b10', '-0b10', '0B11',
    '0o17', '0O17', '077', '09', '0x10', '0x', '0x 1', '0x+1', '0x0x1', '0b0b1', '0o0o1', '0x0b1',
    '1K', '1 k', '-2M', '1GiB', '01junkK', '0junk', '0K', '0 K', 'true', 'TRUE', 'on', 'yes', 'off', ' true',
    '4294967296', '4294967297', '9223372036854775807', '9223372036854775808', '-9223372036854775808',
    '-9223372036854775809', '18446744073709551615', '18446744073709551616', '-18446744073709551616',
    '9223372036854775807K', '-9223372036854775808G', '18446744073709551616K', '18446744073709551616badK',
    'none', 'long', 'entity', 'None', 'LONG', 'Entity', "42\0tail", "\0tail", "none\0", '0xffffffff', '0x100000001'];
for ($byte = 0; $byte < 256; $byte++) {
    foreach ([chr($byte), '0' . chr($byte), '42' . chr($byte), '42' . chr($byte) . 'K'] as $value) { $numbers[] = $value; }
}
$encodings = array_merge(mb_list_encodings(), ['', 'bogus', 'auto', 'ASCII,auto,UTF-8', 'ASCII,bogus', 'pass', 'PASS', 'p', 'pa', 'pas',
    '"SJIS,UTF-8"', ' SJIS, UTF-8 ', 'UTF-8,,SJIS', "UTF-8\0bad", "pass\0bad", "\0"]);
$values = [
    'mbstring.language' => ['neutral', 'Japanese', 'ja', 'Korean', 'Russian', 'unknown', '', "Japanese\0tail", "\0Japanese"],
    'mbstring.detect_order' => $encodings,
    'mbstring.http_input' => $encodings,
    'mbstring.http_output' => $encodings,
    'mbstring.internal_encoding' => $encodings,
    'mbstring.strict_detection' => $numbers,
    'mbstring.substitute_character' => $numbers,
    'mbstring.regex_stack_limit' => $numbers,
    'mbstring.regex_retry_limit' => $numbers,
    'mbstring.encoding_translation' => ['0', '1', 'On', ''],
    'mbstring.http_output_conv_mimetypes' => ['', ' ', "\t\0\r\n\v", '^text/', '  ^application/  ', "^text/\0[", "\0^text/\0"],
    'MBSTRING.language' => ['Japanese'],
    "mbstring.language\0" => ['Japanese'],
    'mbstring.missing' => ['1'],
];
foreach ([false, true] as $public) {
    foreach ($values as $name => $inputs) {
        foreach (array_unique($inputs) as $value) {
            if (getenv('ELEPHC_MBSTRING_TRACE_CAPTURE')) { fwrite(STDERR, "$count " . ($public ? 'public' : 'default') . ' ' . bin2hex($name) . ' ' . bin2hex($value) . "\n"); }
            iniBaseline($public);
            $case = ['public' => $public, 'name' => iniValue($name), 'value' => iniValue($value),
                'steps' => [iniStep($name, $value), iniStep($name, null), iniStep($name, null)]];
            gzwrite($stream, json_encode($case, JSON_THROW_ON_ERROR) . "\n");
            $count++;
        }
    }
}
gzclose($stream);
echo "Captured $count INI traces on PHP ", PHP_VERSION, "\n";
