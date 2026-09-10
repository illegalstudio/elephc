<?php
// Capture ordered mbstring information values, including exact bytes and diagnostics.
// Run: php scripts/mbstring/capture_info.php

$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/info.jsonl.gz', 'wb9');
$count = 0;

/** Preserves scalar types, binary strings, and ordered array keys in the oracle. */
function infoValue(mixed $value): mixed {
    if (is_string($value)) { return ['bytes' => bin2hex($value)]; }
    if (is_array($value)) {
        $entries = [];
        foreach ($value as $key => $element) { $entries[] = [infoValue($key), infoValue($element)]; }
        return ['array' => $entries];
    }
    return $value;
}

/** Records one fully configured request observation after input parsing. */
function captureInfo($stream, array $settings, array $args): void {
    global $count;
    $warnings = [];
    set_error_handler(static function (int $severity, string $message) use (&$warnings): bool {
        $warnings[] = [$severity, $message];
        return true;
    });
    $case = ['settings' => $settings, 'args' => array_map(infoValue(...), $args)];
    try { $case['output'] = infoValue(mb_get_info(...$args)); }
    catch (Throwable $error) { $case['error'] = [get_class($error), $error->getMessage()]; }
    finally { restore_error_handler(); }
    $case['warnings'] = $warnings;
    gzwrite($stream, json_encode($case, JSON_THROW_ON_ERROR) . "\n");
    $count++;
}

$selectors = ['all', 'internal_encoding', 'http_input', 'http_output', 'http_output_conv_mimetypes',
    'mail_charset', 'mail_header_encoding', 'mail_body_encoding', 'illegal_chars', 'encoding_translation',
    'language', 'detect_order', 'substitute_character', 'strict_detection'];
$languages = ['neutral', 'uni', 'English', 'German', 'Japanese', 'Korean',
    'Simplified Chinese', 'Traditional Chinese', 'Russian', 'Armenian', 'Turkish', 'Ukrainian'];
foreach ($languages as $index => $language) {
    foreach ([63, 0, 0x10FFFF, 'none', 'long', 'entity'] as $mode => $substitute) {
        $settings = ['language' => $language, 'internal' => ['UTF-8', 'SJIS', 'UTF-16LE'][$index % 3],
            'output' => ['pass', 'ISO-2022-JP', 'UTF-8'][$mode % 3],
            'detect' => $mode % 2 ? ['UTF-8', 'ASCII', 'UTF-8'] : ['ASCII', 'UTF-8'],
            'substitute' => $substitute, 'strict' => (bool)($mode % 2),
            'mimetypes' => $index % 2 ? '^(text/|application/json)' : '^(text/|application/xhtml\+xml)'];
        mb_language($language);
        mb_internal_encoding($settings['internal']);
        mb_http_output($settings['output']);
        mb_detect_order($settings['detect']);
        mb_substitute_character($substitute);
        ini_set('mbstring.strict_detection', $settings['strict'] ? '1' : '0');
        ini_set('mbstring.http_output_conv_mimetypes', $settings['mimetypes']);
        captureInfo($stream, $settings, []);
        foreach ($selectors as $selector) {
            foreach ([$selector, strtoupper($selector), $selector . "\0", substr($selector, 0, -1)] as $type) {
                captureInfo($stream, $settings, [$type]);
            }
        }
        foreach (['', ' all', 'all ', 'func_overload', 'func_overload_list', null, false, true, 0, 1.5, [], ['all']] as $type) {
            captureInfo($stream, $settings, [$type]);
        }
        captureInfo($stream, $settings, ['all', 'language']);
    }
}
gzclose($stream);
echo "Captured $count information cases on PHP ", PHP_VERSION, "\n";
