<?php
// Capture UTF-7 code units, malformed shifts, streaming cuts, and scalar encoder hashes.
// Run: php scripts/mbstring/capture_utf7.php

$root = __DIR__ . '/../../crates/elephc-mbstring/tests/fixtures';
$fixture = gzopen("$root/utf7.jsonl.gz", 'wb9');
$hashes = ['php_version' => PHP_VERSION, 'encodings' => []];
mb_substitute_character(0xFFFD);

// Capture independently observable conversion results for one original encoded byte string.
function captureUtf7($fixture, string $input, string $encoding): void {
    gzwrite($fixture, json_encode([
        'encoding' => $encoding, 'input' => bin2hex($input),
        'decoded' => bin2hex(mb_convert_encoding($input, 'UCS-4BE', $encoding)),
        'scrub' => bin2hex(mb_scrub($input, $encoding)),
        'valid' => mb_check_encoding($input, $encoding), 'length' => mb_strlen($input, $encoding),
    ], JSON_THROW_ON_ERROR) . "\n");
}

foreach (['UTF-7', 'UTF7-IMAP'] as $encoding) {
    $shift = $encoding === 'UTF-7' ? '+' : '&';
    for ($word = 0; $word < 65536; ++$word) {
        $encoded = rtrim(base64_encode(pack('n', $word)), '=');
        if ($encoding === 'UTF7-IMAP') { $encoded = str_replace('/', ',', $encoded); }
        captureUtf7($fixture, $shift . $encoded . '-', $encoding);
        captureUtf7($fixture, $shift . $encoded, $encoding);
    }
    $inputs = ['', $shift];
    foreach (['', 'A', 'AA', 'AAA', 'AAB', 'AAAA', 'AAAAA', 'AAAAAA', 'AAAAAB', 'AAAAAAA', 'AAAAAAAA',
        '2AA', '3AA', '2AAAQQ', '2ADcAA', '2ADYAA', '2ADcANgA', '2AAAQQBB', '3ADcAA'] as $encoded) {
        $inputs[] = $shift . $encoded;
        foreach (['', '-', ' ', '!', '~', "\x80", "\0"] as $end) {
            $inputs[] = $shift . $encoded . $end . 'ABC';
        }
    }
    foreach (['日本語é😃', str_repeat('日本語', 20), "\0ABCΣ", "AΣ\u{0301} " . str_repeat('é', 70)] as $text) {
        $inputs[] = mb_convert_encoding($text, $encoding, 'UTF-8');
    }
    foreach ($inputs as $input) {
        captureUtf7($fixture, $input, $encoding);
        for ($from = 0; $from <= min(16, strlen($input)); ++$from) {
            foreach ([1, 2, 3, 4, 5, 6, 8, 12, 19, 20, 21, 40, 80, 1000] as $length) {
                gzwrite($fixture, json_encode([
                    'encoding' => $encoding, 'input' => bin2hex($input),
                    'from' => $from, 'budget' => $length,
                    'cut' => bin2hex(mb_strcut($input, $from, $length, $encoding)),
                ], JSON_THROW_ON_ERROR) . "\n");
            }
        }
    }
    $hash = hash_init('sha256');
    for ($code = 0; $code <= 0x10FFFF; ++$code) {
        $encoded = mb_convert_encoding(pack('N', $code), $encoding, 'UCS-4BE');
        hash_update($hash, pack('V', strlen($encoded)) . $encoded);
    }
    $hashes['encodings'][$encoding] = hash_final($hash);
}
gzclose($fixture);
file_put_contents("$root/utf7-encode.json", json_encode($hashes,
    JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
