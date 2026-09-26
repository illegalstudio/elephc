<?php
// Capture sparse mobile UTF-8 overrides and full scalar conversion oracle hashes.
// Run: php scripts/mbstring/capture_mobile_utf8.php

$root = __DIR__ . '/../../crates/elephc-mbstring';
$manifest = ['php_version' => PHP_VERSION, 'encodings' => []];
$oracle = ['php_version' => PHP_VERSION, 'encodings' => []];

// Intern a variable-size record, retaining the PHP-chosen mapping instead of inverting it.
function internMobile(string $record, string &$pool, array &$offsets): int {
    if (!isset($offsets[$record])) {
        $offsets[$record] = strlen($pool);
        $pool .= $record;
    }
    return $offsets[$record];
}

foreach (['DOCOMO', 'KDDI-A', 'KDDI-B', 'SOFTBANK'] as $carrier) {
    $encoding = "UTF-8-Mobile#$carrier";
    $decodeIndex = $pointPool = $encodeIndex = $bytePool = $composites = '';
    $pointOffsets = $byteOffsets = $candidates = [];
    $decodeHash = hash_init('sha256');
    $encodeHash = hash_init('sha256');
    mb_substitute_character('none');
    for ($code = 0; $code <= 0x10FFFF; ++$code) {
        if ($code >= 0xD800 && $code <= 0xDFFF) { continue; }
        $raw = pack('N', $code);
        $utf8 = mb_convert_encoding($raw, 'UTF-8', 'UCS-4BE');
        $decoded = mb_convert_encoding($utf8, 'UCS-4BE', $encoding);
        hash_update($decodeHash, pack('V', strlen($decoded)) . $decoded);
        if ($decoded !== $raw) {
            $points = array_values(unpack('N*', $decoded));
            $record = pack('V*', count($points), ...$points);
            $decodeIndex .= pack('VV', $code, internMobile($record, $pointPool, $pointOffsets));
            if (count($points) > 1) { $candidates[$decoded] = $points; }
        }
        $encoded = mb_convert_encoding($raw, $encoding, 'UCS-4BE');
        hash_update($encodeHash, pack('V', strlen($encoded)) . $encoded);
        if ($encoded !== $utf8) {
            $record = pack('V', strlen($encoded)) . $encoded;
            $encodeIndex .= pack('VV', $code, internMobile($record, $bytePool, $byteOffsets));
        }
    }
    foreach ($candidates as $raw => $points) {
        $encoded = mb_convert_encoding($raw, $encoding, 'UCS-4BE');
        $composites .= pack('V*', count($points), ...$points) . pack('V', strlen($encoded)) . $encoded;
    }
    $entry = [];
    foreach (['decode' => $decodeIndex, 'points' => $pointPool, 'encode' => $encodeIndex,
        'bytes' => $bytePool, 'composites' => $composites] as $kind => $data) {
        $file = 'utf8-mobile-' . strtolower($carrier) . "-$kind.bin";
        file_put_contents("$root/src/encoding/data/$file", $data);
        $entry[$kind] = ['file' => $file, 'sha256' => hash('sha256', $data)];
    }
    $manifest['encodings'][$encoding] = $entry;
    $oracle['encodings'][$encoding] = ['decode' => hash_final($decodeHash), 'encode' => hash_final($encodeHash)];
}
foreach (["$root/src/encoding/data/mobile_utf8.json" => $manifest,
    "$root/tests/fixtures/mobile_utf8.json" => $oracle] as $file => $value) {
    file_put_contents($file, json_encode($value,
        JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
}
