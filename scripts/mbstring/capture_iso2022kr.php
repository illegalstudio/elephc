<?php
// Capture ISO-2022-KR shifts, escapes, streaming cuts, and the full scalar encoder.
// Run: php scripts/mbstring/capture_iso2022kr.php

$root = __DIR__ . '/../../crates/elephc-mbstring/tests/fixtures';
$fixture = gzopen("$root/iso2022kr.jsonl.gz", 'wb9');
mb_substitute_character(0xFFFD);

// Record independent decoded units, strict validity, and normalized ISO-2022-KR output.
function captureKr($fixture, string $input): void {
    gzwrite($fixture, json_encode([
        'input' => bin2hex($input),
        'decoded' => bin2hex(mb_convert_encoding($input, 'UCS-4BE', 'ISO-2022-KR')),
        'scrub' => bin2hex(mb_scrub($input, 'ISO-2022-KR')),
        'valid' => mb_check_encoding($input, 'ISO-2022-KR'), 'length' => mb_strlen($input, 'ISO-2022-KR'),
    ], JSON_THROW_ON_ERROR) . "\n");
}

for ($pair = 0; $pair < 65536; ++$pair) {
    captureKr($fixture, "\x0e" . pack('n', $pair));
    captureKr($fixture, "\x0e" . pack('n', $pair) . "\x0fABC");
}
$inputs = ['', "\x1b", "\x1b$", "\x1b$)", "\x1b$)C", "\x0e", "\x0e!", "\x0e!!", "\x0e!!\x0fABC"];
foreach (['', '$', ')', 'A', '$)', 'A)', '$A', 'AA'] as $prefix) {
    foreach (['', 'C', 'A', "\x0e", "\x0f", "\x1b", "\0", "\x80"] as $suffix) {
        $inputs[] = "\x1b" . $prefix . $suffix . 'ABC';
        $inputs[] = "\x0e\x1b" . $prefix . $suffix . "!!\x0f";
    }
}
foreach (["한국어 中文 Σ\u{2123}", str_repeat('한국어', 24), "\0é\nABC"] as $text) {
    $inputs[] = mb_convert_encoding($text, 'ISO-2022-KR', 'UTF-8');
}
foreach ($inputs as $input) {
    captureKr($fixture, $input);
    for ($from = 0; $from <= min(24, strlen($input)); ++$from) {
        foreach ([1, 2, 3, 4, 5, 6, 7, 8, 12, 19, 20, 21, 40, 80, 1000] as $length) {
            gzwrite($fixture, json_encode([
                'input' => bin2hex($input), 'from' => $from, 'budget' => $length,
                'cut' => bin2hex(mb_strcut($input, $from, $length, 'ISO-2022-KR')),
            ], JSON_THROW_ON_ERROR) . "\n");
        }
    }
}
gzclose($fixture);
$hash = hash_init('sha256');
for ($code = 0; $code <= 0x10FFFF; ++$code) {
    $encoded = mb_convert_encoding(pack('N', $code), 'ISO-2022-KR', 'UCS-4BE');
    hash_update($hash, pack('V', strlen($encoded)) . $encoded);
}
file_put_contents("$root/iso2022kr-encode.json", json_encode(['php_version' => PHP_VERSION, 'encode' => hash_final($hash)],
    JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
