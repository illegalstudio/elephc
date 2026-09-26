<?php
// Capture transfer-specific malformed syntax, line boundaries, legacy cuts, and fast conversion.
// Run: php scripts/mbstring/capture_transfer.php

error_reporting(E_ALL & ~E_DEPRECATED);
$root = __DIR__ . '/../../crates/elephc-mbstring/tests/fixtures';
$stream = gzopen("$root/transfer.jsonl.gz", 'wb9');
$encodings = ['BASE64', 'Quoted-Printable', 'UUENCODE', 'HTML-ENTITIES'];
$binary = implode('', array_map(chr(...), range(0, 255)));
$inputs = ['', 'a', 'abc', 'YQ==', '=A', "=\r", "=\rA", "=\r=41", '=AX', '=4=41',
    " \t\n\r\0", "A\rB\nC\r\nD\0E", 'Y?Q==', 'YW?Jj', 'Y=Q=Q', '===', "YW\x80Jj",
    '&amp; &AMP; &apos; &quot; &lt; &gt; &eacute; &nbsp; &NotEqualTilde;',
    'A&#0;&#65;&#x41;&#X41;&#xD800;&#1114111;&#1114112;&#4294967296;&#x100000041;',
    '&#;&#x;&#X;&#12x;&#-1;&missing;&unfinished&&amp;&#&amp;',
    "&amp\0; &amp\0tail;", str_repeat('&', 40), $binary];
foreach ([1, 2, 3, 44, 45, 46, 56, 57, 58, 71, 72, 73, 75, 76, 77, 127, 128, 129, 255, 256] as $count) {
    $inputs[] = str_repeat('A', $count);
    $inputs[] = str_repeat('=', $count) . "\rB\n";
    $inputs[] = str_repeat('A', $count) . '&eacute;';
    $inputs[] = str_repeat('A', $count) . '&#00000000000000000065;';
}
foreach (['abc', $binary, str_repeat('test', 70)] as $raw) {
    $inputs[] = base64_encode($raw);
    $uu = "begin 0644 filename\n" . convert_uuencode($raw);
    foreach ([$uu, rtrim($uu), 'prefix' . $uu, str_replace("\n", "\r\n", $uu), str_replace('begin ', 'beginX', $uu)] as $input) {
        $inputs[] = $input;
    }
}
mb_substitute_character(0xFFFD);
foreach ($encodings as $encoding) {
    foreach ($inputs as $input) {
        $result = ['encoding' => $encoding, 'input' => bin2hex($input), 'kind' => 'text',
            'length' => mb_strlen($input, $encoding), 'valid' => mb_check_encoding($input, $encoding),
            'width' => mb_strwidth($input, $encoding), 'scrub' => bin2hex(mb_scrub($input, $encoding)),
            'upper' => bin2hex(mb_strtoupper($input, $encoding)), 'lower' => bin2hex(mb_strtolower($input, $encoding)),
            'kana' => bin2hex(mb_convert_kana($input, 'KV', $encoding))];
        gzwrite($stream, json_encode($result, JSON_THROW_ON_ERROR) . "\n");
        for ($from = 0; $from <= min(18, strlen($input)); ++$from) {
            foreach ([1, 2, 3, 4, 5, 8, 12, 19, 20, 21, 40, 72, 76, 128, 1000] as $length) {
                gzwrite($stream, json_encode(['kind' => 'cut', 'encoding' => $encoding, 'input' => bin2hex($input),
                    'from' => $from, 'length' => $length, 'output' => bin2hex(mb_strcut($input, $from, $length, $encoding))], JSON_THROW_ON_ERROR) . "\n");
            }
        }
    }
}
foreach (['UTF-8', 'UTF-16', 'UTF-7', 'GB18030', 'JIS', 'UCS-4BE', '8bit', ...$encodings] as $from) {
    foreach ($encodings as $to) {
        foreach (["\0\x80\xff", "hé日本", "A=E9&amp;", 'YQ==', $binary,
            pack('N*', 0, 65, 0x100, 0x3BC, 0xD800, 0x10000, 0xFFFFFFFF), ...array_slice($inputs, -18)] as $input) {
            foreach ([0xFFFD, 'none', 'long', 'entity'] as $substitute) {
                mb_substitute_character(0xFFFD);
                mb_substitute_character($substitute);
                gzwrite($stream, json_encode(['kind' => 'convert', 'encoding' => $from, 'to' => $to,
                    'input' => bin2hex($input), 'substitute' => $substitute,
                    'output' => bin2hex(mb_convert_encoding($input, $to, $from))], JSON_THROW_ON_ERROR) . "\n");
            }
        }
    }
}
foreach (mb_list_encodings() as $to) {
    foreach ([63, 0, 0x80, 0x3BC, 0xFFFD, 0x1F600] as $character) {
        foreach ([$character, 'none', 'long', 'entity'] as $substitute) {
            mb_substitute_character($character);
            mb_substitute_character($substitute);
            foreach (['UTF-8' => "\x80\xffA", 'UCS-4BE' => pack('N*', 0x80, 0x100, 0x1F600, 0xD800, 0xFFFFFFFF)] as $from => $input) {
                gzwrite($stream, json_encode(['kind' => 'convert', 'encoding' => $from, 'to' => $to,
                    'input' => bin2hex($input), 'substitute' => $substitute, 'character' => $character,
                    'output' => bin2hex(mb_convert_encoding($input, $to, $from))], JSON_THROW_ON_ERROR) . "\n");
            }
        }
    }
}
// Mobile encoders can defer the final digit of a recursively encoded replacement marker.
foreach (['SJIS-Mobile#DOCOMO', 'SJIS-Mobile#KDDI', 'SJIS-Mobile#SOFTBANK'] as $to) {
    foreach ([35, 49, 63, 0xFFFD, 0x1F1FA] as $character) {
        foreach ([$character, 'none', 'long', 'entity'] as $substitute) {
            mb_substitute_character($character);
            mb_substitute_character($substitute);
            foreach ([[0x10000], [0x10000, 0x10000], [0x10000, 65, 0x10000], [0x10000, 49, 0x20E3],
                [0x10000, 49], [0x10000, 0x10001, 0x10002], [0x1F1FA, 0x1F1F8], [0x1F1F8, 65],
                [0xFFFFFFFF, 0xFFFFFFFF], [49, 0x20E3, 0xFFFFFFFF]] as $points) {
                foreach ([0, 125, 126, 127, 128] as $prefix) {
                    $input = pack('N*', ...[...array_fill(0, $prefix, 65), ...$points]);
                    gzwrite($stream, json_encode(['kind' => 'convert', 'encoding' => 'UCS-4BE', 'to' => $to,
                        'input' => bin2hex($input), 'substitute' => $substitute, 'character' => $character,
                        'output' => bin2hex(mb_convert_encoding($input, $to, 'UCS-4BE'))], JSON_THROW_ON_ERROR) . "\n");
                }
            }
        }
    }
}
gzclose($stream);
