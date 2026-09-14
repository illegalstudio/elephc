<?php
// Capture observable decoder partitions for mobile JIS conversion and UTF-16 contextual casing.
// Run: php scripts/mbstring/capture_batches.php

$root = __DIR__ . '/../../crates/elephc-mbstring/tests/fixtures';
$codecs = json_decode(file_get_contents("$root/codecs.json"), true, flags: JSON_THROW_ON_ERROR);
$stream = gzopen("$root/batches.jsonl.gz", 'wb9');
mb_substitute_character(0xFFFD);
error_reporting(E_ALL & ~E_DEPRECATED);
foreach (array_keys($codecs['sha256']) as $encoding) {
    foreach (['A', '日', '😀', 'A😀'] as $fill) {
        foreach ([62, 63, 64, 125, 126, 127, 128, 129, 254, 255, 256] as $count) {
            foreach (["#\u{20e3}", '🇯🇵', '🇦🇦'] as $tail) {
                $input = mb_convert_encoding(str_repeat($fill, $count) . $tail . '日', $encoding, 'UTF-8');
                gzwrite($stream, json_encode(['kind' => 'convert', 'encoding' => $encoding,
                    'input' => bin2hex($input),
                    'output' => bin2hex(mb_convert_encoding($input, 'ISO-2022-JP-MOBILE#KDDI', $encoding))], JSON_THROW_ON_ERROR) . "\n");
            }
        }
    }
}
foreach (['UTF-16', 'UTF-16BE', 'UTF-16LE'] as $encoding) {
    foreach ([[], [0x1F600], [0x41, 0x1F600], [0xD800], [0xDC00], [0xD800, 0x41], [0xD800, 0xD800]] as $fill) {
        foreach ([0, 1, 15, 16, 31, 32, 62, 63, 64, 126, 127, 128] as $count) {
            foreach ([0, 1, 63, 64, 127, 128, 191, 192, 255, 256] as $marks) {
                $points = [...array_merge(...array_fill(0, $count, $fill)), 0x41, ...array_fill(0, $marks, 0x301), 0x3A3, 0x20];
                $input = mb_convert_encoding(pack('N*', ...$points), $encoding, 'UCS-4BE');
                gzwrite($stream, json_encode(['kind' => 'lower', 'encoding' => $encoding,
                    'input' => bin2hex($input), 'output' => bin2hex(mb_strtolower($input, $encoding))], JSON_THROW_ON_ERROR) . "\n");
            }
        }
    }
}
foreach (['SJIS-Mobile#DOCOMO', 'SJIS-Mobile#KDDI', 'SJIS-Mobile#SOFTBANK', 'ISO-2022-JP-MOBILE#KDDI'] as $encoding) {
    foreach ([0, 60, 61, 62, 63, 64, 65, 124, 125, 126, 127, 128] as $count) {
        foreach (["ΑΣ ", "🇺🇸Z", "1\u{20E3}Z", "ｶﾞZ", "ABC"] as $tail) {
            mb_substitute_character(0xFFFD);
            $input = mb_convert_encoding(str_repeat('A', $count) . $tail . str_repeat('Z', 130), $encoding, 'UTF-8');
            foreach ([35, 49, 0xFFFD, 'none', 'long', 'entity'] as $substitute) {
                mb_substitute_character(0xFFFD);
                mb_substitute_character($substitute);
                foreach (['lower', 'kana'] as $kind) {
                    $output = $kind === 'lower' ? mb_strtolower($input, $encoding) : mb_convert_kana($input, 'KV', $encoding);
                    gzwrite($stream, json_encode(['kind' => $kind, 'encoding' => $encoding, 'input' => bin2hex($input),
                        'substitute' => $substitute, 'output' => bin2hex($output)], JSON_THROW_ON_ERROR) . "\n");
                }
            }
        }
    }
}
gzclose($stream);
