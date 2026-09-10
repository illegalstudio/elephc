<?php
// Capture conversion output and illegal-character deltas for every source/destination pair.
// Run: php scripts/mbstring/capture_conversion_errors.php

$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/conversion-errors.jsonl.gz', 'wb9');
error_reporting(E_ALL & ~E_DEPRECATED);
foreach (mb_list_encodings() as $from) {
    mb_substitute_character(0xFFFD);
    $encoded = mb_convert_encoding("Aé日😀#\u{20E3}🇯🇵", $from, 'UTF-8');
    foreach (mb_list_encodings() as $to) {
        foreach (['', $encoded, "\xFF\x80\0", $encoded . "\xFF"] as $input) {
            foreach ([63, 233, 35, 49, 'none', 'long', 'entity'] as $substitute) {
                mb_substitute_character(233);
                mb_substitute_character($substitute);
                $before = mb_get_info('illegal_chars');
                $output = mb_convert_encoding($input, $to, $from);
                gzwrite($stream, json_encode(['from' => $from, 'to' => $to,
                    'input' => bin2hex($input), 'substitute' => $substitute,
                    'output' => bin2hex($output), 'errors' => mb_get_info('illegal_chars') - $before], JSON_THROW_ON_ERROR) . "\n");
            }
        }
    }
}
gzclose($stream);
