<?php
// Capture recursive mbstring array output, warnings, and illegal-character deltas.
// Run: php scripts/mbstring/capture_arrays.php

$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/arrays.jsonl.gz', 'wb9');
$warnings = [];
set_error_handler(function (int $level, string $message) use (&$warnings): bool {
    if ($level === E_WARNING) { $warnings[] = $message; }
    return true;
});

// Materialize graph references without passing through JSON's array/key coercion rules.
function graphValue(array $graph): array {
    $arrays = array_fill(0, count($graph), []);
    foreach ($graph as $index => $entries) {
        foreach ($entries as [$key, $value]) {
            $key = $key[0] === 'i' ? $key[1] : hex2bin($key[1]);
            if ($value[0] === 'a') {
                $arrays[$index][$key] =& $arrays[$value[1]];
            } else {
                $arrays[$index][$key] = match ($value[0]) {
                    's' => hex2bin($value[1]), 'i', 'b' => $value[1], 'n' => null,
                    'f' => unpack('E', hex2bin($value[1]))[1],
                    'o' => new stdClass(), 'r' => fopen('php://memory', 'r+'),
                };
            }
        }
    }
    return $arrays[0];
}

// Retain output key identity, scalar types, float bits, and arbitrary string bytes.
function normalizeValue(mixed $value): array {
    if (is_string($value)) { return ['s', bin2hex($value)]; }
    if (is_int($value)) { return ['i', $value]; }
    if (is_bool($value)) { return ['b', $value]; }
    if (is_null($value)) { return ['n']; }
    if (is_float($value)) { return ['f', bin2hex(pack('E', $value))]; }
    $entries = [];
    foreach ($value as $key => $entry) {
        $entries[] = [is_int($key) ? ['i', $key] : ['s', bin2hex($key)], normalizeValue($entry)];
    }
    return ['a', $entries];
}

// Execute one operation from a fresh graph and record only operation-local diagnostics/counts.
function captureArray(string $kind, array $graph, string $to, string|array $from = 'UTF-8', bool $strict = false, int|string $substitute = 63): void {
    global $stream, $warnings;
    ini_set('mbstring.strict_detection', $strict ? '1' : '0');
    mb_substitute_character(233);
    mb_substitute_character($substitute);
    $input = graphValue($graph);
    $warnings = [];
    $before = mb_get_info('illegal_chars');
    $result = $kind === 'check' ? mb_check_encoding($input, $to) : mb_convert_encoding($input, $to, $from);
    gzwrite($stream, json_encode(['kind' => $kind, 'graph' => $graph, 'to' => $to,
        'from' => $from, 'strict' => $strict, 'substitute' => $substitute,
        'result' => normalizeValue($result), 'warnings' => $warnings,
        'errors' => mb_get_info('illegal_chars') - $before], JSON_THROW_ON_ERROR) . "\n");
}

// Build readable descriptor keys/values from byte strings.
function strValue(string $value): array { return ['s', bin2hex($value)]; }

$scalars = [
    [['i', -7], ['n']], [['i', 0], ['b', true]], [['i', 3], ['b', false]],
    [strValue('integer'), ['i', PHP_INT_MIN]],
    [strValue('float'), ['f', '8000000000000000']],
    [strValue('nan'), ['f', '7ff8000000000001']],
    [strValue('infinity'), ['f', '7ff0000000000000']],
];
$cases = [
    [[]], [$scalars],
    [[[strValue('first'), ['a', 1]], [strValue('second'), ['a', 1]]], [[strValue('text'), strValue("é\xFF")]]],
    [[[strValue('self'), ['a', 0]]]],
    [[[strValue('nested'), ['a', 1]]], [[strValue('back'), ['a', 0]]]],
    [[[strValue('self'), ['a', 0]], [strValue('nested'), ['a', 1]]], [[strValue('back'), ['a', 0]]]],
    [[[strValue("bad\xFF"), ['a', 0]], [strValue('later'), ['a', 0]]]],
    [[[strValue('bad-value'), strValue("\xFF")], [strValue('self'), ['a', 0]]]],
    [[[strValue('object'), ['o']], [strValue('resource'), ['r']], [strValue('last'), strValue('ok')]]],
    [[[strValue('object'), ['o']], [strValue('self'), ['a', 0]]]],
    [[[strValue('é'), strValue("first\xFF")], [strValue('è'), strValue("second\xFF")], [strValue('éé'), ['a', 1]]], [[strValue('leaf'), strValue('日')]]],
];
foreach ($cases as $caseIndex => $graph) {
    foreach (mb_list_encodings() as $encoding) { captureArray('check', $graph, $encoding); }
    foreach (['UTF-8', 'ASCII', 'UTF-16LE', 'ISO-2022-JP-MOBILE#KDDI'] as $to) {
        foreach (['UTF-8', ['UTF-8', 'ASCII'], ['UTF-8', 'ISO-8859-1']] as $from) {
            foreach ([false, true] as $strict) {
                // Two traversed self references make PHP 8.5.10 recurse until process failure.
                // Strict detection skips the malformed first key, leaving a finite single cycle.
                if ($caseIndex === 6 && !($strict && $from === ['UTF-8', 'ASCII'])) { continue; }
                foreach ([63, 233, 'none', 'long', 'entity'] as $substitute) {
                    captureArray('convert', $graph, $to, $from, $strict, $substitute);
                }
            }
        }
    }
}
foreach (mb_list_encodings() as $from) {
    mb_substitute_character(233);
    $word = mb_convert_encoding('Aé日😀', $from, 'UTF-8');
    $key = mb_convert_encoding('keyé', $from, 'UTF-8');
    $graph = [[[strValue($key), strValue($word)], [['i', 2], ['a', 1]]], [[strValue($key), strValue($word . "\xFF")]]];
    foreach (['UTF-8', 'ASCII', 'UTF-16LE', 'ISO-2022-JP-MOBILE#KDDI'] as $to) {
        foreach ([63, 233, 'none', 'long', 'entity'] as $substitute) {
            captureArray('convert', $graph, $to, $from, false, $substitute);
        }
    }
}
// Converted numeric-looking string keys remain distinct from an existing integer key.
captureArray('convert', [[[strValue('NDI='), strValue('YQ==')], [['i', 42], strValue('Yg==')]]], '8bit', 'BASE64');
gzclose($stream);
