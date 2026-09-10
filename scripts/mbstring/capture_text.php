<?php
// Capture cross-encoding contextual text behavior and SoftBank escape streams.
// Run: php scripts/mbstring/capture_text.php > crates/elephc-mbstring/tests/fixtures/text.json

$codecs = json_decode(file_get_contents(__DIR__ .
    '/../../crates/elephc-mbstring/tests/fixtures/codecs.json'), true, flags: JSON_THROW_ON_ERROR);
mb_substitute_character(0xFFFD);
error_reporting(E_ALL & ~E_DEPRECATED);
$cases = [];
foreach (array_keys($codecs['sha256']) as $encoding) {
    $inputs = [];
    foreach ([
        "Straße İSTANBUL don't o'NEILL",
        "ΟΣ ΟΣΑ Σ ΑΣ' ΑΣ'Α",
        "AΣ\u{0345} A\u{0345}Σ \u{0345}Σ",
        str_repeat('A', 62) . "Σ\u{0301}B",
        str_repeat('A', 63) . "Σ\u{0301} ",
        'A' . str_repeat("\u{0301}", 256) . 'Σ',
        "か\u{309a} エレファント 一二三 中文 한국어 🇯🇵 1️⃣",
        "Σ\u{00a0}ABC\u{0301}\u{0000}",
    ] as $source) {
        $inputs[] = mb_convert_encoding($source, $encoding, 'UTF-8');
    }
    if (str_starts_with($encoding, 'UCS-4')) {
        foreach ([0x110000, 0xFFFFFF, 0x1000000, 0xFFFFFFFF] as $code) {
            $inputs[] = $encoding === 'UCS-4LE' ? pack('V*', 0x61, $code, 0x61)
                : pack('N*', 0x61, $code, 0x61);
        }
    }
    if ($encoding === 'SJIS-Mobile#SOFTBANK') {
        foreach (str_split('EFGOPQ') as $mode) {
            $prefix = "\x1b\x24" . $mode;
            $stream = $prefix;
            $inputs[] = $stream;
            for ($byte = 0; $byte < 256; ++$byte) {
                $inputs[] = $prefix . chr($byte) . "\x0fAB";
                $stream .= chr($byte);
            }
            $inputs[] = $stream;
        }
    }
    foreach ($inputs as $input) {
        $converted = [];
        for ($mode = 0; $mode < 8; ++$mode) {
            $converted[] = bin2hex(mb_convert_case($input, $mode, $encoding));
        }
        $cases[] = [
            'encoding' => $encoding,
            'input' => bin2hex($input),
            'length' => mb_strlen($input, $encoding),
            'width' => mb_strwidth($input, $encoding),
            'valid' => mb_check_encoding($input, $encoding),
            'scrub' => bin2hex(mb_scrub($input, $encoding)),
            'case' => $converted,
        ];
    }
}
echo json_encode(['php_version' => PHP_VERSION, 'cases' => $cases],
    JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR), "\n";
