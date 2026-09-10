<?php
// Capture language aliases, default detection lists, and mail encodings from PHP 8.5.
// Run: php scripts/mbstring/capture_languages.php

$languages = [
    'neutral' => [], 'uni' => ['universal'], 'English' => ['en'], 'German' => ['de', 'Deutsch'],
    'Japanese' => ['ja'], 'Korean' => ['ko'], 'Simplified Chinese' => ['zh-cn'],
    'Traditional Chinese' => ['zh-tw'], 'Russian' => ['ru'], 'Ukrainian' => ['ua'],
    'Armenian' => ['hy'], 'Turkish' => ['tr'],
];
$result = ['php_version' => PHP_VERSION, 'languages' => []];
foreach ($languages as $canonical => $aliases) {
    foreach ([$canonical, ...$aliases] as $name) {
        mb_language($name);
        if (mb_language() !== $canonical) { throw new RuntimeException("Unexpected language: $name"); }
    }
    mb_detect_order('auto');
    $result['languages'][$canonical] = ['aliases' => $aliases, 'detect_order' => mb_detect_order()];
    foreach (['mail_charset', 'mail_header_encoding', 'mail_body_encoding'] as $key) {
        $result['languages'][$canonical][$key] = mb_get_info($key);
    }
}
file_put_contents(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/languages.json', json_encode($result, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
