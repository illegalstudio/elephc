<?php
// Generate an independent PHP oracle over every Unicode scalar and contextual cases.
// Run: php scripts/mbstring/capture_unicode.php > crates/elephc-mbstring/tests/fixtures/unicode.json

if (!extension_loaded('mbstring')) {
    fwrite(STDERR, "The mbstring extension is required.\n");
    exit(1);
}

$caseHashes = [];
for ($mode = 0; $mode < 8; ++$mode) {
    $caseHashes[$mode] = hash_init('sha256');
}
$widthHash = hash_init('sha256');
for ($code = 0; $code <= 0x10FFFF; ++$code) {
    if ($code >= 0xD800 && $code <= 0xDFFF) {
        continue;
    }
    $char = mb_chr($code, 'UTF-8');
    foreach ($caseHashes as $mode => $hash) {
        $mapped = mb_convert_case($char, $mode, 'UTF-8');
        hash_update($hash, pack('V', strlen($mapped)) . $mapped);
    }
    hash_update($widthHash, chr(mb_strwidth($char, 'UTF-8')));
}

$contexts = [];
foreach ([
    "Straße İSTANBUL don't o'NEILL",
    "ΟΣ ΟΣΑ Σ ΑΣ' ΑΣ'Α",
    "AΣ\u{0345} A\u{0345}Σ \u{0345}Σ",
    "AΣ\u{0301} B AΣ\u{0301}C",
    "ǳUNGLE ǆUNGLE ﬃANCÉ",
    str_repeat('A', 63) . "Σ\u{0301}B",
    str_repeat('A', 63) . "Σ\u{0301} ",
    'A' . str_repeat("\u{0301}", 256) . 'Σ',
    'AΣ' . str_repeat("\u{0301}", 256) . 'A',
] as $input) {
    $expected = [];
    for ($mode = 0; $mode < 8; ++$mode) {
        $expected[] = mb_convert_case($input, $mode, 'UTF-8');
    }
    $contexts[] = ['input' => $input, 'expected' => $expected];
}
echo json_encode([
    'php_version' => PHP_VERSION,
    'scalar_count' => 0x110000 - 0x800,
    'case_sha256' => array_map(hash_final(...), $caseHashes),
    'width_sha256' => hash_final($widthHash),
    'contexts' => $contexts,
], JSON_PRETTY_PRINT | JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR), "\n";
