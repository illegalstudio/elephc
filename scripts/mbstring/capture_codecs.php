<?php
// Capture exhaustive short inputs and deterministic longer inputs for Unicode codecs.
// Run: php scripts/mbstring/capture_codecs.php > crates/elephc-mbstring/tests/fixtures/codecs.json

if (!extension_loaded('mbstring')) {
    fwrite(STDERR, "The mbstring extension is required.\n");
    exit(1);
}

mb_substitute_character(0xFFFD);
error_reporting(E_ALL & ~E_DEPRECATED);
$inputs = [''];
for ($byte = 0; $byte < 256; ++$byte) {
    $inputs[] = chr($byte);
}
for ($pair = 0; $pair < 65536; ++$pair) {
    $inputs[] = pack('n', $pair);
}
$state = 42;
for ($sample = 0; $sample < 1024; ++$sample) {
    $input = '';
    for ($index = 0; $index < 3 + $sample % 33; ++$index) {
        $state = ($state * 1664525 + 1013904223) & 0xFFFFFFFF;
        $input .= chr($state >> 24);
    }
    $inputs[] = $input;
}
foreach ([
    'fffe4100', 'feff0041', '0000feff00000041', 'fffe000041000000',
    '0000d800', '00110000', 'ffffffff', 'd8004100', 'd8000041',
    'eda080', 'f0908080', 'f4908080', 'f0808080', 'e08080',
] as $hex) {
    $inputs[] = hex2bin($hex);
}

$expected = [];
$counts = [];
$singlebyte = json_decode(file_get_contents(__DIR__ .
    '/../../crates/elephc-mbstring/src/encoding/data/singlebyte.json'), true, flags: JSON_THROW_ON_ERROR);
$doublebyte = json_decode(file_get_contents(__DIR__ .
    '/../../crates/elephc-mbstring/src/encoding/data/doublebyte.json'), true, flags: JSON_THROW_ON_ERROR);
foreach (array_merge([
    'BASE64', 'Quoted-Printable', 'UUENCODE', 'HTML-ENTITIES',
    'ASCII', '8bit', 'JIS', 'ISO-2022-JP', 'ISO-2022-JP-MS', 'CP50220', 'CP50221', 'CP50222',
    'ISO-2022-JP-2004', 'ISO-2022-JP-MOBILE#KDDI',
    'ISO-2022-KR', 'HZ', 'EUC-TW', 'GB18030', 'GB18030-2022', 'UTF-7', 'UTF7-IMAP', 'UTF-8', 'UTF-8-Mobile#DOCOMO', 'UTF-8-Mobile#KDDI-A',
    'UTF-8-Mobile#KDDI-B', 'UTF-8-Mobile#SOFTBANK', 'UTF-16', 'UTF-16BE', 'UTF-16LE',
    'UTF-32', 'UTF-32BE', 'UTF-32LE', 'UCS-2', 'UCS-2BE', 'UCS-2LE',
    'UCS-4', 'UCS-4BE', 'UCS-4LE',
], array_keys($singlebyte['encodings']), array_keys($doublebyte['encodings'])) as $encoding) {
    $encodingInputs = $inputs;
    if (in_array($encoding, ['EUC-JP', 'eucJP-win', 'EUC-JP-2004', 'CP51932'], true)) {
        for ($pair = 0; $pair < 65536; ++$pair) {
            $encodingInputs[] = "\x8f" . pack('n', $pair);
        }
    }
    $counts[$encoding] = count($encodingInputs);
    $hash = hash_init('sha256');
    foreach ($encodingInputs as $input) {
        hash_update($hash, pack('V', mb_strlen($input, $encoding)));
        hash_update($hash, chr(mb_check_encoding($input, $encoding) ? 1 : 0));
        foreach ([
            mb_convert_encoding($input, 'UCS-4BE', $encoding),
            mb_scrub($input, $encoding),
            mb_strtoupper($input, $encoding),
        ] as $output) {
            hash_update($hash, pack('V', strlen($output)) . $output);
        }
    }
    $expected[$encoding] = hash_final($hash);
}
echo json_encode([
    'php_version' => PHP_VERSION,
    'input_count' => count($inputs),
    'input_counts' => $counts,
    'sha256' => $expected,
], JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR), "\n";
