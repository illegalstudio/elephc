<?php
// Capture complete detection results for ordered candidates, malformed inputs, and strictness.
// Run: php scripts/mbstring/capture_detect.php

error_reporting(E_ALL & ~E_DEPRECATED);
mb_language('neutral');
mb_detect_order(['ASCII', 'UTF-8']);
mb_substitute_character(0xFFFD);
$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/detect.jsonl.gz', 'wb9');

/** Stores one independent PHP verdict while retaining list identity's order-weight policy. */
function captureDetect($stream, string $input, array|string|null $list, ?bool $strict, bool $ordered = true, bool $defaultStrict = false): void {
    ini_set('mbstring.strict_detection', $defaultStrict ? '1' : '0');
    $case = ['input' => bin2hex($input), 'list' => is_string($list) ? ['bytes' => bin2hex($list)] : $list,
        'strict' => $strict, 'ordered' => $ordered, 'default_strict' => $defaultStrict];
    try {
        $case['result'] = $strict === null ? mb_detect_encoding($input, $list) : mb_detect_encoding($input, $list, $strict);
    } catch (Throwable $error) {
        $case['error'] = [$error::class, bin2hex($error->getMessage())];
    }
    gzwrite($stream, json_encode($case, JSON_THROW_ON_ERROR) . "\n");
}

$groups = [null, ['UTF-8'], ['ASCII', 'UTF-8'], ['UTF-8', 'ISO-8859-1'], ['ISO-8859-1', 'UTF-8'],
    ['UTF-8', 'ISO-8859-1', 'ISO-8859-5'], ['UTF-7', 'UTF-8'], ['UTF7-IMAP', 'UTF-8'],
    ['JIS', 'ISO-2022-JP', 'UTF-8', 'EUC-JP', 'SJIS'], ['SJIS', 'EUC-JP', 'UTF-8'],
    ['GB18030', 'GB18030-2022', 'BIG-5', 'EUC-CN', 'CP936', 'EUC-TW'],
    ['EUC-KR', 'UHC', 'UTF-8', 'ISO-2022-KR'], ['KOI8-R', 'Windows-1251', 'CP866', 'UTF-8'],
    ['UTF-16', 'UTF-16LE', 'UTF-16BE', 'UCS-2', 'UTF-32', 'UCS-4', 'UTF-8'],
    ['BASE64', 'UUENCODE', '7bit', '8bit', 'Quoted-Printable', 'HTML-ENTITIES'],
    ['BASE64', 'UTF-8', 'UTF-7', 'UTF-8'], ['UTF-8', 'UTF-8', 'ISO-8859-1'],
];
$inputs = ['', 'hello world', '12345', "\xC4\xA2", "\xC3\xA9", "\0", "\xFF", "\x80\x80", "\xED\xA0\x80",
    "\xEF\xBB\xBF", "\xFE\xFF", "\xFF\xFE", "\xEF\xBB\xBF\xFF", "\xFE\xFF\0A", "\xFF\xFEA\0",
    '+AKM-', '&AKM-', '+ABC', "\x1b\x24BAA", "\x1b(IAA\x1b(B", "~{abcd~}", "\x1b\x24)CAA\x0e\x21\x21\x0f"];
foreach (mb_list_encodings() as $encoding) {
    foreach (["caffè Straße", '日本語の文字列', '中文字符', '한국어 문자열', 'Слово текст', 'العربية', 'हिन्दी', '😀🦀🇮🇹'] as $text) {
        $inputs[] = mb_convert_encoding($text, $encoding, 'UTF-8');
    }
}
mt_srand(86510);
foreach (range(0, 255) as $count) {
    $input = '';
    $length = [1, 2, 3, 7, 31, 127, 128, 129, 255, 1024][$count % 10];
    for ($i = 0; $i < $length; ++$i) { $input .= chr(mt_rand(0, 255)); }
    $inputs[] = $input;
}
foreach ($inputs as $input) {
    foreach ($groups as $list) {
        foreach ([false, true] as $strict) { captureDetect($stream, $input, $list, $strict); }
    }
    foreach ([false, true] as $strict) {
        captureDetect($stream, $input, mb_list_encodings(), $strict, false);
        captureDetect($stream, $input, array_map(static fn(string $name): string => $name, mb_list_encodings()), $strict, true);
    }
}
foreach (mb_list_encodings() as $encoding) {
    for ($byte = 0; $byte < 256; ++$byte) {
        foreach ([false, true] as $strict) { captureDetect($stream, chr($byte), [$encoding], $strict); }
    }
}
for ($pair = 0; $pair < 65536; ++$pair) {
    $input = pack('n', $pair);
    foreach ([['UTF-8', 'ISO-8859-1'], ['SJIS', 'EUC-JP'], ['UTF-7', 'ASCII']] as $list) {
        foreach ([false, true] as $strict) { captureDetect($stream, $input, $list, $strict); }
    }
}
foreach ([[], '', ',', 'a', '"auto,UTF-8"', 'auto,unknown', "UTF-8\0", ['8bit'], ['unknown'], null] as $list) {
    foreach (['', "\xFF\x80", 'text'] as $input) {
        foreach ([false, true] as $defaultStrict) { captureDetect($stream, $input, $list, null, true, $defaultStrict); }
    }
}
gzclose($stream);
