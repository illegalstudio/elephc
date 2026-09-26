<?php
// Capture GB18030's BMP mappings and independent full four-byte/scalar corpus hashes.
// Run: php scripts/mbstring/capture_gb18030.php

$root = __DIR__ . '/../../crates/elephc-mbstring';
$manifest = ['php_version' => PHP_VERSION, 'encodings' => []];
$oracle = ['php_version' => PHP_VERSION, 'encodings' => []];

// Converts a linear four-byte address into GB18030's decimal and base-126 byte positions.
function gbBytes(int $pointer): string {
    $fourth = $pointer % 10 + 0x30;
    $pointer = intdiv($pointer, 10);
    $third = $pointer % 126 + 0x81;
    $pointer = intdiv($pointer, 126);
    return chr(intdiv($pointer, 10) + 0x81) . chr($pointer % 10 + 0x30) . chr($third) . chr($fourth);
}

foreach (['GB18030', 'GB18030-2022'] as $encoding) {
    $pairs = $bmp = $encode = '';
    mb_substitute_character('none');
    for ($lead = 0x81; $lead <= 0xFE; ++$lead) {
        for ($trail = 0; $trail <= 255; ++$trail) {
            $decoded = mb_convert_encoding(chr($lead) . chr($trail), 'UCS-4BE', $encoding);
            if (strlen($decoded) !== 0 && strlen($decoded) !== 4) {
                throw new RuntimeException("Unexpected GB18030 pair expansion: $encoding");
            }
            $pairs .= pack('V', $decoded === '' ? 0xFFFFFFFF : unpack('N', $decoded)[1]);
        }
    }
    for ($pointer = 0; $pointer <= 39419; ++$pointer) {
        $decoded = mb_convert_encoding(gbBytes($pointer), 'UCS-4BE', $encoding);
        $bmp .= pack('V', $decoded === '' ? 0xFFFFFFFF : unpack('N', $decoded)[1]);
    }
    for ($code = 0; $code < 65536; ++$code) {
        $encoded = mb_convert_encoding(pack('N', $code), $encoding, 'UCS-4BE');
        $encode .= pack('V', $encoded === '' ? 0xFFFFFFFF : unpack('N', str_pad($encoded, 4, "\0", STR_PAD_LEFT))[1]);
    }
    $entry = [];
    foreach (['pairs' => $pairs, 'bmp' => $bmp, 'encode' => $encode] as $kind => $bytes) {
        $file = strtolower($encoding) . "-$kind.bin";
        file_put_contents("$root/src/encoding/data/$file", $bytes);
        $entry[$kind] = ['file' => $file, 'sha256' => hash('sha256', $bytes)];
    }
    $manifest['encodings'][$encoding] = $entry;
    mb_substitute_character(0xFFFD);
    $decodeHash = hash_init('sha256');
    for ($pointer = 0; $pointer < 126 * 10 * 126 * 10; ++$pointer) {
        $input = gbBytes($pointer);
        $decoded = mb_convert_encoding($input, 'UCS-4BE', $encoding);
        hash_update($decodeHash, chr(mb_check_encoding($input, $encoding) ? 1 : 0));
        hash_update($decodeHash, pack('V', strlen($decoded)) . $decoded);
    }
    $encodeHash = hash_init('sha256');
    for ($code = 0; $code <= 0x10FFFF; ++$code) {
        $encoded = mb_convert_encoding(pack('N', $code), $encoding, 'UCS-4BE');
        hash_update($encodeHash, pack('V', strlen($encoded)) . $encoded);
    }
    $oracle['encodings'][$encoding] = ['decode' => hash_final($decodeHash), 'encode' => hash_final($encodeHash)];
}
foreach (["$root/src/encoding/data/gb18030.json" => $manifest,
    "$root/tests/fixtures/gb18030.json" => $oracle] as $file => $value) {
    file_put_contents($file, json_encode($value,
        JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
}
