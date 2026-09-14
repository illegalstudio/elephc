<?php
// Capture canonical JIS/ISO-2022-JP scalar mappings and independent shifted-input oracles.
// Run: php scripts/mbstring/capture_jis.php

$root = __DIR__ . '/../../crates/elephc-mbstring';
$manifest = ['php_version' => PHP_VERSION, 'encodings' => []];
$oracle = ['php_version' => PHP_VERSION, 'encodings' => []];
$fixture = gzopen("$root/tests/fixtures/jis.jsonl.gz", 'wb9');

// Extract one canonical output character's mode and payload, preserving the PHP-selected mode.
function jisMapping(string $bytes): int {
    if ($bytes === '') { return 0xFFFFFFFF; }
    if (strlen($bytes) === 1) { return ord($bytes); }
    if (strlen($bytes) === 3 && $bytes[0] === "\x0e" && $bytes[2] === "\x0f") {
        return (2 << 16) | ord($bytes[1]);
    }
    foreach (["\x1b(J" => 1, "\x1b(I" => 2, "\x1b\x24B" => 3, "\x1b\x24(D" => 4, "\x1b\x24(?" => 5] as $prefix => $mode) {
        if (str_starts_with($bytes, $prefix) && str_ends_with($bytes, "\x1b(B")) {
            $payload = substr($bytes, strlen($prefix), -3);
            $width = $mode >= 3 ? 2 : 1;
            if (strlen($payload) === $width) {
                $value = $width === 2 ? unpack('n', $payload)[1] : ord($payload);
                return ($mode << 16) | $value;
            }
        }
    }
    throw new RuntimeException('Unexpected canonical JIS scalar: ' . bin2hex($bytes));
}

// Frame independently observable decoder, validation, and canonical round-trip results.
function jisResult(string $input, string $encoding): array {
    return [
        'decoded' => bin2hex(mb_convert_encoding($input, 'UCS-4BE', $encoding)),
        'valid' => mb_check_encoding($input, $encoding),
        'length' => mb_strlen($input, $encoding),
        'scrub' => bin2hex(mb_scrub($input, $encoding)),
    ];
}

mb_substitute_character('none');
$cpPlane = '';
for ($first = 0x21; $first <= 0x97; ++$first) {
    for ($second = 0x21; $second <= 0x7E; ++$second) {
        $decoded = mb_convert_encoding("\x1b\x24B" . chr($first) . chr($second), 'UCS-4BE', 'CP50221');
        if ($decoded !== '' && strlen($decoded) !== 4) { throw new RuntimeException('Invalid CP5022x plane mapping'); }
        $cpPlane .= pack('V', $decoded === '' ? 0xFFFFFFFF : unpack('N', $decoded)[1]);
    }
}
file_put_contents("$root/src/encoding/data/cp5022x-plane.bin", $cpPlane);
$manifest['cp5022x_plane'] = ['file' => 'cp5022x-plane.bin', 'sha256' => hash('sha256', $cpPlane)];

foreach (['JIS', 'ISO-2022-JP', 'ISO-2022-JP-MS', 'CP50220', 'CP50221', 'CP50222', 'ISO-2022-JP-2004', 'ISO-2022-JP-MOBILE#KDDI'] as $encoding) {
    $table = $supplementary = '';
    $encodeHash = hash_init('sha256');
    mb_substitute_character('none');
    for ($code = 0; $code <= 0x10FFFF; ++$code) {
        $encoded = mb_convert_encoding(pack('N', $code), $encoding, 'UCS-4BE');
        hash_update($encodeHash, pack('V', strlen($encoded)) . $encoded);
        if ($encoding === 'ISO-2022-JP-2004') { continue; }
        if ($code <= 0xFFFF) { $table .= pack('V', jisMapping($encoded)); }
        elseif ($encoded !== '') {
            if ($encoding !== 'ISO-2022-JP-MOBILE#KDDI') { throw new RuntimeException("Unexpected supplementary mapping in $encoding"); }
            $supplementary .= pack('V2', $code, jisMapping($encoded));
        }
    }
    $file = strtolower($encoding) . '-encode.bin';
    if ($table !== '') {
        file_put_contents("$root/src/encoding/data/$file", $table);
        $manifest['encodings'][$encoding] = ['encode' => ['file' => $file, 'sha256' => hash('sha256', $table)]];
    }
    if ($encoding === 'ISO-2022-JP-MOBILE#KDDI') {
        $plane = '';
        $composites = [];
        for ($first = 0x21; $first <= 0x7F; ++$first) {
            for ($second = 0x21; $second <= 0x7E; ++$second) {
                $decoded = mb_convert_encoding("\x1b\x24B" . chr($first) . chr($second), 'UCS-4BE', $encoding);
                $points = $decoded === '' ? [0xFFFFFFFF] : array_values(unpack('N*', $decoded));
                if (count($points) > 2) { throw new RuntimeException('Unexpected mobile JIS expansion'); }
                $plane .= pack('V2', $points[0], $points[1] ?? 0);
                if (count($points) === 2) {
                    $encoded = mb_convert_encoding($decoded, $encoding, 'UCS-4BE');
                    $composites[$decoded] = pack('V3', $points[0], $points[1], jisMapping($encoded));
                }
            }
        }
        $candidates = [];
        foreach (str_split('#0123456789') as $digit) { $candidates[] = [ord($digit), 0x20E3]; }
        foreach (range(0x1F1E6, 0x1F1FF) as $first) {
            foreach (range(0x1F1E6, 0x1F1FF) as $second) { $candidates[] = [$first, $second]; }
        }
        foreach ($candidates as [$first, $second]) {
            $input = pack('N2', $first, $second);
            $encoded = mb_convert_encoding($input, $encoding, 'UCS-4BE');
            $separate = mb_convert_encoding(pack('N', $first), $encoding, 'UCS-4BE')
                . mb_convert_encoding(pack('N', $second), $encoding, 'UCS-4BE');
            if ($encoded !== $separate && strlen($encoded) <= 8) {
                $composites[$input] = pack('V3', $first, $second, jisMapping($encoded));
            }
        }
        foreach (['supplementary' => $supplementary, 'composites' => implode('', $composites), 'plane' => $plane] as $field => $data) {
            $file = "jis-kddi-$field.bin";
            file_put_contents("$root/src/encoding/data/$file", $data);
            $manifest['encodings'][$encoding][$field] = ['file' => $file, 'sha256' => hash('sha256', $data)];
        }
    }
    $entry = ['encode' => hash_final($encodeHash), 'prefixes' => []];
    mb_substitute_character(0xFFFD);
    $prefixes = ['', "\x1b\x24B", "\x1b\x24(D", "\x1b\x24(?", "\x1b(J", "\x1b(I", "\x0e"];
    if ($encoding === 'ISO-2022-JP-2004') { $prefixes[] = "\x1b\x24(Q"; $prefixes[] = "\x1b\x24(P"; }
    foreach ($prefixes as $prefix) {
        $hash = hash_init('sha256');
        for ($pair = 0; $pair < 65536; ++$pair) {
            $input = $prefix . pack('n', $pair) . "\x1b(B";
            $result = jisResult($input, $encoding);
            hash_update($hash, chr($result['valid'] ? 1 : 0) . pack('V', $result['length']));
            foreach (['decoded', 'scrub'] as $field) {
                $bytes = hex2bin($result[$field]);
                hash_update($hash, pack('V', strlen($bytes)) . $bytes);
            }
        }
        $entry['prefixes'][bin2hex($prefix)] = hash_final($hash);
    }
    $inputs = [];
    foreach ($prefixes as $prefix) {
        foreach (['', 'A', "\\~", "\x0e", "\x0f", "\xa1\xdf", "\x1b", "\x1b$", "\x1b(", "\x1b\x24(",
            "\x1bA", "\x1b\x24A", "\x1b(A", "\x1b\x24(A", "\x1b(H", "\x1b\x24(B", "\x0e\x1b(B"] as $suffix) {
            $inputs[] = $prefix . $suffix;
            $inputs[] = $prefix . $suffix . 'ABC';
        }
    }
    foreach (["日本語¥‾\\~ｱｲｳ", str_repeat('日本語', 24), "ΟΣΑ ＼∥－￠￡￢",
        "\0日本\0語\0¥\0ｱ\0A", "ｶﾞﾊﾟｳﾞｧﾞ｡ﾟﾝﾞ", "\u{E000}\u{E757}①纊",
        str_repeat('ｶ', 64) . 'ﾞ', "か\u{309a}æ\u{300}日æ", "丂日本\u{20089}あ丂", 'Aæ', 'あæ',
        "#\u{20e3}0\u{20e3}1\u{20e3} 🇯🇵🇺🇸🇬🇧 🇦🇦", "日1", "日#\u{20e3}語0", '©®😀'] as $text) {
        $inputs[] = mb_convert_encoding($text, $encoding, 'UTF-8');
    }
    if (str_starts_with($encoding, 'CP5022')) {
        for ($code = 0xFF61; $code <= 0xFF9F; ++$code) {
            $inputs[] = "\x1b(I" . chr($code - 0xFF40) . "\x5e\x1b(B";
            $inputs[] = "\x1b(I" . chr($code - 0xFF40) . "\x5f\x1b(B";
        }
    }
    foreach ($inputs as $input) {
        gzwrite($fixture, json_encode(['encoding' => $encoding, 'input' => bin2hex($input)] + jisResult($input, $encoding), JSON_THROW_ON_ERROR) . "\n");
        for ($from = 0; $from <= min(16, strlen($input)); ++$from) {
            foreach ([1, 2, 3, 4, 5, 6, 8, 12, 19, 20, 21, 40, 80, 1000] as $length) {
                gzwrite($fixture, json_encode(['encoding' => $encoding, 'input' => bin2hex($input),
                    'from' => $from, 'budget' => $length,
                    'cut' => bin2hex(mb_strcut($input, $from, $length, $encoding))], JSON_THROW_ON_ERROR) . "\n");
            }
        }
    }
    $oracle['encodings'][$encoding] = $entry;
}
gzclose($fixture);
foreach (["$root/src/encoding/data/jis.json" => $manifest,
    "$root/tests/fixtures/jis-hashes.json" => $oracle] as $file => $value) {
    file_put_contents($file, json_encode($value,
        JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
}
