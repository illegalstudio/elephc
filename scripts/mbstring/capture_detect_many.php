<?php
// Capture multi-string detection through PHP's independent public conversion API.
// Run with PHP 8.5: php scripts/mbstring/capture_detect_many.php

$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/detect_many.jsonl.gz', 'wb9');
$count = 0;

/** Captures only detection, retaining original argument boundaries before PHP mutates values. */
function captureDetectMany($stream, array $inputs, array $candidates, bool $strict, bool $ordered): void {
    global $count;
    ini_set('mbstring.strict_detection', $strict ? '1' : '0');
    $values = $inputs;
    // Passing one array lets an empty input list reach the detector with zero strings.
    $result = @mb_convert_variables('UTF-8', $candidates, $values);
    gzwrite($stream, json_encode([
        'inputs' => array_map(bin2hex(...), $inputs), 'candidates' => $candidates,
        'strict' => $strict, 'ordered' => $ordered, 'result' => $result,
    ], JSON_THROW_ON_ERROR) . "\n");
    $count++;
}

$all = mb_list_encodings();
$lists = [
    ['UTF-8', 'SJIS', 'EUC-JP'], ['SJIS', 'UTF-8', 'EUC-JP'],
    ['UTF-16BE', 'UTF-16LE', 'UTF-8'], ['ASCII', 'UTF-8'],
    ['ISO-2022-JP', 'JIS', 'UTF-8'], ['HZ', 'UTF-8', 'GB18030'],
    ['UTF-7', 'UTF7-IMAP', 'UTF-8'], ['ISO-2022-KR', 'ASCII', 'EUC-KR'],
    ['UTF-8', 'UTF-8', 'SJIS'], $all,
];
$inputs = ['', 'a', '!?', "\x80", "\xff", "\xc3\xa9", "\x82\xa0", "\xa4\xa2",
    "\xef\xbb\xbfhello", "\xfe\xff\0a", "\xff\xfea\0", "\x1b\x24\x42\x24\x22",
    "\x24\x24", '+ZeVnLIqe-', '&ZeVnLIqe-', '~{Dc:C', '~}', "\x1b\x24\x29\x43\x0e\x30\x21",
    str_repeat('a', 130), str_repeat("\x82\xa0", 129), str_repeat('?', 150)];
foreach ($lists as $index => $list) {
    foreach ([false, true] as $strict) {
        captureDetectMany($stream, [], $list, $strict, $index !== count($lists) - 1);
        foreach ($inputs as $left) {
            foreach ($inputs as $right) {
                captureDetectMany($stream, [$left, $right], $list, $strict, $index !== count($lists) - 1);
            }
        }
        foreach ($inputs as $value) {
            captureDetectMany($stream, [$value, '', 'name', $value, '123'], $list, $strict, $index !== count($lists) - 1);
        }
    }
}
gzclose($stream);
echo "Captured $count multi-string detection cases on PHP ", PHP_VERSION, "\n";
