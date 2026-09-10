<?php
// Capture complete MIME decoding outputs against the pinned PHP mbstring baseline.
// Run: php scripts/mbstring/capture_mime_decode.php

error_reporting(E_ALL & ~E_DEPRECATED);
$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/mime_decode.jsonl.gz', 'wb9');

/** Records an exact MIME request, including internal encoding and ignored replacement state. */
function captureMimeDecode($stream, string $input, string $encoding, int|string $substitute = 'none'): void {
    mb_internal_encoding($encoding);
    mb_substitute_character($substitute);
    $case = ['input' => bin2hex($input), 'encoding' => $encoding, 'substitute' => $substitute,
        'output' => bin2hex(mb_decode_mimeheader($input))];
    gzwrite($stream, json_encode($case, JSON_THROW_ON_ERROR) . "\n");
}

/** Wraps bytes in a Base64 encoded word without normalizing its charset label. */
function mimeWord(string $encoding, string $input): string {
    return '=?' . $encoding . '?B?' . base64_encode($input) . '?=';
}

$syntax = ['', 'ASCII unchanged', " A\tB  C\r\n D\rE\n \t", "\r\nX\xff\0Y", '=', '=?', '=?A?B?',
    '=?UTF-8?Q?caf=C3=A9?=', '=?utf8?q?A_B__C=00?=', '=?UTF-8?B?w6k=?=',
    '=?unknown?B?w6k=?=', '=?UTF-8?Z?w6k=?=', '=?UTF-8??B?w6k=?=',
    '=?UTF-8?Q?=zzX=4qY=0Z!=a?=', '=?UTF-8?Q?=4?=', '=?UTF-8?Q?=?=',
    "=?UTF-8?Q?A=\r\nB=\rC=\nD?=", '=?UTF-8?B?Q!U@J#D$RE==?=',
    '=?UTF-8?B?Q Q==Q?=', '=?UTF-8?B?Q?=', '=?UTF-8?B?QQ', '=?UTF-8?B?QQ?',
    '=?UTF-8?Q?A', '=?UTF-8?Q?A?', '=?UTF-8?Q?A?=  ',
    "=?UTF-8?Q?A?=\t\r\n=?ASCII?Q?B?=  C", "=?UTF-8\0junk?Q?A?=",
    "=?UTF-8?Q?A?=\r\n=?nope?B?QQ==?=", "x=?UTF-8?Q?A?==?UTF-8?Q?B?=y"];
foreach (mb_list_encodings() as $destination) {
    foreach ($syntax as $input) { captureMimeDecode($stream, $input, $destination); }
    foreach (mb_list_encodings() as $source) {
        mb_substitute_character(0xFFFD);
        $inputs = ['', "\x80\xffA", implode('', array_map(chr(...), range(0, 255))),
            mb_convert_encoding("hé日ガ０🇺🇸 & \0", $source, 'UTF-8')];
        foreach ($inputs as $input) {
            $word = mimeWord($source, $input);
            captureMimeDecode($stream, $word, $destination);
            captureMimeDecode($stream, 'A ' . $word . "\r\n\t" . $word . ' Z', $destination);
        }
    }
}

// A single C decoder-state word survives charset switches and ordinary ASCII runs.
$stateful = ['UTF-16' => ["\xff\xfeA\0", "\xfe\xff\0B", "A\0", "\0B"],
    'UTF-32' => ["\xff\xfe\0\0A\0\0\0", "\0\0\0B"],
    'UCS-2' => ["\xff\xfeA\0", "\0B"], 'UCS-4' => ["\xff\xfe\0\0A\0\0\0", "\0\0\0B"],
    'UTF-7' => ['+AOk', '+2AA', '+AOk-', 'AOk-', '+AAAA', '+AAAAAAA', '+AAAAAAAA2AA', '+3AA-', '+3AA'],
    'UTF7-IMAP' => ['&AOk', '&2AA', '&AOk-', 'AOk-', '&3AA-', '&3AA'],
    'JIS' => ["\x1b\$B\x24\x22", "\x1b(J\\~", "\x0e!", "\x24\x24"],
    'ISO-2022-JP-2004' => ["\x1b\$(Q\x24\x22", "\x24\x24"],
    'ISO-2022-JP-MOBILE#KDDI' => ["\x1b\$B\x24\x22", "\x24\x24"],
    'ISO-2022-KR' => ["\x1b\$)C\x0e0!", '0!', "\x0fA"], 'HZ' => ['~{VP', 'VP', '~}A'],
    'ISO-2022-JP-MS' => ["\x1b\$B\x24\x22", "\x24\x24"],
    'CP50220' => ["\x1b\$B\x24\x22", "\x24\x24"],
    'SJIS-Mobile#SOFTBANK' => ["\x1b\$G!", "!\x0fA", "\x1b\$Q!"],
    'BASE64' => ['QQ', str_repeat('!', 125) . 'Q!!Q', 'QQ=='],
    'UUENCODE' => ["begin 0644 filename\n!00``\n", '!00``' . "\n", 'ABC']];
foreach ($stateful as $firstEncoding => $firstInputs) {
    foreach ($stateful as $secondEncoding => $secondInputs) {
        foreach ($firstInputs as $first) {
            foreach ($secondInputs as $second) {
                foreach (['', "\r\n ", ' X '] as $separator) {
                    $input = mimeWord($firstEncoding, $first) . $separator . mimeWord($secondEncoding, $second);
                    captureMimeDecode($stream, $input, 'UTF-8');
                }
            }
        }
    }
}
// Output lookahead ends at a MIME word or decoder scratch-buffer boundary for mobile codecs.
foreach (['SJIS-Mobile#KDDI', 'SJIS-Mobile#DOCOMO', 'ISO-2022-JP-MOBILE#KDDI', 'ISO-2022-JP-2004', 'EUC-JP-2004', 'UTF-7'] as $destination) {
    foreach (range(0, 260) as $prefix) {
        foreach ([['🇺', '🇸'], ['1', "\u{20E3}"], ['か', "\u{309A}"]] as [$first, $second]) {
            captureMimeDecode($stream, mimeWord('UTF-8', str_repeat('A', $prefix) . $first) . ' ' . mimeWord('UTF-8', $second), $destination);
        }
    }
}
// Exercise malformed partial units and codec state with deterministic arbitrary input.
mt_srand(2047);
foreach (array_keys($stateful) as $source) {
    foreach (range(0, 255) as $byte) {
        $input = chr($byte) . chr(($byte * 73 + 19) % 256) . chr(($byte * 31 + 11) % 256);
        foreach (['UTF-8', 'UUENCODE', 'ISO-2022-JP-MOBILE#KDDI'] as $destination) {
            captureMimeDecode($stream, mimeWord($source, $input) . ' ' . mimeWord($source, $input), $destination);
        }
    }
    for ($case = 0; $case < 256; $case++) {
        $input = '';
        for ($byte = 0, $length = mt_rand(0, 280); $byte < $length; $byte++) { $input .= chr(mt_rand(0, 255)); }
        captureMimeDecode($stream, mimeWord($source, $input) . ' ' . mimeWord($source, $input), 'UTF-8');
    }
}
foreach (['JIS', 'UTF-7', 'UTF7-IMAP', 'ISO-2022-JP-2004', 'SJIS-Mobile#SOFTBANK', 'UUENCODE'] as $source) {
    foreach (range(120, 135) as $prefix) {
        foreach (["\x1b\$B", '+2AA', '&2AA', "\x1b\$G", "\x1b(B"] as $tail) {
            $word = mimeWord($source, str_repeat('A', $prefix) . $tail);
            foreach (['UTF-8', 'UUENCODE'] as $destination) { captureMimeDecode($stream, $word . ' ' . $word, $destination); }
        }
    }
}
gzclose($stream);
