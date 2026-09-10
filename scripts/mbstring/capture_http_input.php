<?php
// Capture HTTP input selectors independently of request parsing and live text defaults.
// Run: php scripts/mbstring/capture_http_input.php

$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/http_input.jsonl.gz', 'wb9');
$count = 0;

/** Preserves strings as hex and distinguishes arrays from scalar selector inputs. */
function httpInputValue(mixed $value): mixed {
    if (is_string($value)) { return ['bytes' => bin2hex($value)]; }
    if (is_array($value)) { return ['array' => array_map(httpInputValue(...), $value)]; }
    return $value;
}

/** Records PHP parser errors, diagnostics, and identification/list result types. */
function captureHttpInput($stream, array $encodings, array $args): void {
    global $count;
    $warnings = [];
    set_error_handler(static function (int $severity, string $message) use (&$warnings): bool {
        $warnings[] = [$severity, $message];
        return true;
    });
    $case = ['encodings' => $encodings, 'args' => array_map(httpInputValue(...), $args)];
    try { $case['output'] = httpInputValue(mb_http_input(...$args)); }
    catch (Throwable $error) { $case['error'] = [get_class($error), $error->getMessage()]; }
    finally { restore_error_handler(); }
    $case['warnings'] = $warnings;
    gzwrite($stream, json_encode($case, JSON_THROW_ON_ERROR) . "\n");
    $count++;
}

foreach ([['UTF-8'], ['ASCII', 'SJIS'], ['UTF-8', 'ASCII', 'UTF-8'], ['pass'], ['utf8', 'sjis-win'],
    ['BASE64', 'Quoted-Printable'], ['7bit', '8bit'], mb_list_encodings()] as $encodings) {
    @ini_set('mbstring.http_input', implode(',', $encodings));
    // None of these public setters changes the configured HTTP input candidates.
    mb_internal_encoding('UTF-16LE');
    mb_detect_order(['ASCII']);
    mb_language('Japanese');
    captureHttpInput($stream, $encodings, []);
    for ($byte = 0; $byte < 256; $byte++) { captureHttpInput($stream, $encodings, [chr($byte)]); }
    foreach (['', 'Get', "G\0", "\0G", ' G', 'G ', 'GI', null, false, true, 0, 1.5, [], ['G']] as $type) {
        captureHttpInput($stream, $encodings, [$type]);
    }
    captureHttpInput($stream, $encodings, ['G', 'P']);
}
gzclose($stream);
echo "Captured $count HTTP input cases on PHP ", PHP_VERSION, "\n";
