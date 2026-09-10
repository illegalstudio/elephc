<?php
// Capture PHP's older CNS-11643 planes, reverse mappings, and independent corpus hashes.
// Run: php scripts/mbstring/capture_euctw.php

$root = __DIR__ . '/../../crates/elephc-mbstring';
mb_substitute_character('none');
$planes = $encode = '';
foreach ([0xA1, 0xA2, 0xAE] as $plane) {
    for ($row = 0xA1; $row <= 0xFE; ++$row) {
        for ($cell = 0xA1; $cell <= 0xFE; ++$cell) {
            $decoded = mb_convert_encoding("\x8e" . chr($plane) . chr($row) . chr($cell), 'UCS-4BE', 'EUC-TW');
            $planes .= pack('V', $decoded === '' ? 0xFFFFFFFF : unpack('N', $decoded)[1]);
        }
    }
}
for ($code = 0; $code < 65536; ++$code) {
    $encoded = mb_convert_encoding(pack('N', $code), 'EUC-TW', 'UCS-4BE');
    $encode .= pack('V', $encoded === '' ? 0xFFFFFFFF : unpack('N', str_pad($encoded, 4, "\0", STR_PAD_LEFT))[1]);
}
$manifest = ['php_version' => PHP_VERSION, 'encodings' => ['EUC-TW' => []]];
foreach (['planes' => $planes, 'encode' => $encode] as $kind => $bytes) {
    $file = "euc-tw-$kind.bin";
    file_put_contents("$root/src/encoding/data/$file", $bytes);
    $manifest['encodings']['EUC-TW'][$kind] = ['file' => $file, 'sha256' => hash('sha256', $bytes)];
}
mb_substitute_character(0xFFFD);
$decodeHash = hash_init('sha256');
foreach ([0xA1, 0xA2, 0xAE] as $plane) {
    for ($pair = 0; $pair < 65536; ++$pair) {
        $input = "\x8e" . chr($plane) . pack('n', $pair);
        $decoded = mb_convert_encoding($input, 'UCS-4BE', 'EUC-TW');
        hash_update($decodeHash, chr(mb_check_encoding($input, 'EUC-TW') ? 1 : 0));
        hash_update($decodeHash, pack('V', strlen($decoded)) . $decoded);
    }
}
$encodeHash = hash_init('sha256');
for ($code = 0; $code <= 0x10FFFF; ++$code) {
    $encoded = mb_convert_encoding(pack('N', $code), 'EUC-TW', 'UCS-4BE');
    hash_update($encodeHash, pack('V', strlen($encoded)) . $encoded);
}
$oracle = ['php_version' => PHP_VERSION, 'decode' => hash_final($decodeHash), 'encode' => hash_final($encodeHash)];
foreach (["$root/src/encoding/data/euctw.json" => $manifest,
    "$root/tests/fixtures/euctw.json" => $oracle] as $file => $value) {
    file_put_contents($file, json_encode($value,
        JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
}
