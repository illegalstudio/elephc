<?php
// Capture complete split arrays around decoder boundaries, including malformed units.
// Run: php scripts/mbstring/capture_split_batches.php

$root = __DIR__ . '/../../crates/elephc-mbstring/tests/fixtures';
$stream = gzopen("$root/split-batches.jsonl.gz", 'wb9');
error_reporting(E_ALL & ~E_DEPRECATED);

// Preserve binary input and every output element independently of JSON encoding.
function captureSplit(string $encoding, string $input, int $length, int|string $substitute): void {
    global $stream;
    mb_substitute_character(0xFFFD);
    mb_substitute_character($substitute);
    $output = array_map(bin2hex(...), mb_str_split($input, $length, $encoding));
    gzwrite($stream, json_encode(['encoding' => $encoding, 'input' => bin2hex($input),
        'length' => $length, 'substitute' => $substitute, 'output' => $output], JSON_THROW_ON_ERROR) . "\n");
}

foreach (mb_list_encodings() as $encoding) {
    foreach (['A', '日'] as $fill) {
        foreach ([0, 1, 125, 126, 127, 128, 129] as $count) {
            foreach (["#\u{20E3}Z", '🇯🇵日', '😀Z'] as $tail) {
                mb_substitute_character(0xFFFD);
                $input = mb_convert_encoding(str_repeat($fill, $count) . $tail . str_repeat('Z', 132), $encoding, 'UTF-8');
                foreach ([1, 2, 127, 128, 129, 255] as $length) {
                    captureSplit($encoding, $input, $length, 0xFFFD);
                }
            }
        }
    }
    foreach (["A\0\xFF", "\x1B\$B!!\xFF\x1B(B", "\x80\xD8\0\xDC\xFF", "\xF0\x9F"] as $tail) {
        foreach ([0, 126, 127, 128] as $count) {
            $input = str_repeat('A', $count) . $tail;
            foreach ([1, 2, 129] as $length) {
                foreach ([35, 'none', 'long', 'entity'] as $substitute) {
                    captureSplit($encoding, $input, $length, $substitute);
                }
            }
        }
    }
}
gzclose($stream);
