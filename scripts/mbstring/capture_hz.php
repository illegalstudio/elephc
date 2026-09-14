<?php
// Capture HZ shifts, every shifted byte pair, streaming cuts, and the full scalar encoder.
// Run: php scripts/mbstring/capture_hz.php

$root = __DIR__ . '/../../crates/elephc-mbstring/tests/fixtures';
$fixture = gzopen("$root/hz.jsonl.gz", 'wb9');
mb_substitute_character(0xFFFD);

// Record independent decoded units, strict validity, and normalized HZ output.
function captureHz($fixture, string $input): void {
    gzwrite($fixture, json_encode([
        'input' => bin2hex($input),
        'decoded' => bin2hex(mb_convert_encoding($input, 'UCS-4BE', 'HZ')),
        'scrub' => bin2hex(mb_scrub($input, 'HZ')),
        'valid' => mb_check_encoding($input, 'HZ'), 'length' => mb_strlen($input, 'HZ'),
    ], JSON_THROW_ON_ERROR) . "\n");
}

for ($pair = 0; $pair < 65536; ++$pair) {
    captureHz($fixture, '~{' . pack('n', $pair));
    captureHz($fixture, '~{' . pack('n', $pair) . '~}ABC');
}
$inputs = ['', '~', '~{', '~{!', '~{!!', '~{!!~', "~{!!~\n!!", '~{~{~}~}',
    '~{~~~}', '~}A', "~{!~}A", "~{\x80\x80~}", "ABC~\nDEF", "~~{ABC", "~{~\r\nA"];
foreach (["你好 中文 日本 Σ\u{2225}\u{2016}", str_repeat('你好', 32), "~\0é\n日本"] as $text) {
    $inputs[] = mb_convert_encoding($text, 'HZ', 'UTF-8');
}
foreach ($inputs as $input) {
    captureHz($fixture, $input);
    for ($from = 0; $from <= min(24, strlen($input)); ++$from) {
        foreach ([1, 2, 3, 4, 5, 6, 7, 8, 12, 19, 20, 21, 40, 80, 1000] as $length) {
            gzwrite($fixture, json_encode([
                'input' => bin2hex($input), 'from' => $from, 'budget' => $length,
                'cut' => bin2hex(mb_strcut($input, $from, $length, 'HZ')),
            ], JSON_THROW_ON_ERROR) . "\n");
        }
    }
}
gzclose($fixture);
$hash = hash_init('sha256');
for ($code = 0; $code <= 0x10FFFF; ++$code) {
    $encoded = mb_convert_encoding(pack('N', $code), 'HZ', 'UCS-4BE');
    hash_update($hash, pack('V', strlen($encoded)) . $encoded);
}
file_put_contents("$root/hz-encode.json", json_encode(['php_version' => PHP_VERSION, 'encode' => hash_final($hash)],
    JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
