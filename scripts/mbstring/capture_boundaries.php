<?php
// Capture the byte-width tables used by PHP's raw substring/split/cut fast paths.
// Run: php scripts/mbstring/capture_boundaries.php

$root = __DIR__ . '/../../crates/elephc-mbstring/src/encoding/data';
$double = json_decode(file_get_contents("$root/doublebyte.json"), true, flags: JSON_THROW_ON_ERROR);
$manifest = ['php_version' => PHP_VERSION, 'encodings' => []];
foreach (array_merge(['EUC-TW', 'UTF-8', 'UTF-8-Mobile#DOCOMO', 'UTF-8-Mobile#KDDI-A', 'UTF-8-Mobile#KDDI-B', 'UTF-8-Mobile#SOFTBANK'], array_keys($double['encodings'])) as $encoding) {
    $widths = '';
    for ($byte = 0; $byte < 256; ++$byte) {
        $parts = mb_str_split(chr($byte) . 'AAAAAAAA', 1, $encoding);
        $width = strlen($parts[0]);
        if ($width < 1 || $width > 8) {
            throw new RuntimeException("Unexpected character byte width: $encoding");
        }
        $widths .= chr($width);
    }
    $file = strtolower(str_replace('#', '-', $encoding)) . '-boundaries.bin';
    file_put_contents("$root/$file", $widths);
    $manifest['encodings'][$encoding] = ['file' => $file, 'sha256' => hash('sha256', $widths)];
}
file_put_contents("$root/boundaries.json", json_encode($manifest,
    JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
