<?php
// Capture stateless legacy multibyte codecs, including invalid units and composites.
// Run: php -d memory_limit=256M scripts/mbstring/capture_doublebyte.php

if (!extension_loaded('mbstring')) {
    fwrite(STDERR, "The mbstring extension is required.\n");
    exit(1);
}
$directory = __DIR__ . '/../../crates/elephc-mbstring/src/encoding/data';
if (!is_dir($directory)) {
    mkdir($directory, 0777, true);
}

// Preserve invalid-unit locations by comparing two different valid substitute values.
function decodedPoints(string $input, string $encoding): array {
    mb_substitute_character(0xFFFE);
    $first = mb_convert_encoding($input, 'UCS-4BE', $encoding);
    mb_substitute_character(0xFFFF);
    $second = mb_convert_encoding($input, 'UCS-4BE', $encoding);
    $points = [];
    for ($offset = 0; $offset < strlen($first); $offset += 4) {
        $a = substr($first, $offset, 4);
        $b = substr($second, $offset, 4);
        $points[] = $a === $b ? unpack('N', $a)[1] : 0xFFFFFFFF;
    }
    return $points;
}

// Intern output sequences so large invalid-byte regions share one compact record.
function internPoints(array $points, string &$pool, array &$offsets): int {
    $record = pack('V*', count($points), ...$points);
    if (!isset($offsets[$record])) {
        $offsets[$record] = strlen($pool);
        $pool .= $record;
    }
    return $offsets[$record];
}

// Generate a separated scalar stream to enumerate every representable codepoint.
$source = '';
for ($code = 1; $code <= 0x10FFFF; ++$code) {
    if ($code < 0xD800 || $code > 0xDFFF) {
        $source .= pack('NN', $code, 0);
    }
}
$manifest = ['php_version' => PHP_VERSION, 'encodings' => []];
foreach ([
    'SJIS', 'CP932', 'SJIS-win', 'SJIS-2004', 'SJIS-mac',
    'EUC-CN', 'CP936', 'BIG-5', 'CP950', 'EUC-KR', 'UHC',
    'SJIS-Mobile#DOCOMO', 'SJIS-Mobile#KDDI', 'SJIS-Mobile#SOFTBANK',
    'EUC-JP', 'eucJP-win', 'EUC-JP-2004', 'CP51932',
] as $encoding) {
    $single = [];
    $singleIndex = $pairIndex = $tripleIndex = $pool = '';
    $poolOffsets = $compositeCandidates = [];
    for ($byte = 0; $byte < 256; ++$byte) {
        $points = decodedPoints(chr($byte), $encoding);
        $single[$byte] = $points;
        $singleIndex .= pack('V', internPoints($points, $pool, $poolOffsets));
        if (count($points) > 1) {
            $compositeCandidates[pack('N*', ...$points)] = $points;
        }
    }
    if (in_array($encoding, ['EUC-JP', 'eucJP-win', 'EUC-JP-2004'], true)) {
        $prefixes = [];
        for ($byte = 0; $byte < 256; ++$byte) {
            $prefixes[$byte] = decodedPoints("\x8f" . chr($byte), $encoding);
        }
        for ($pair = 0; $pair < 65536; ++$pair) {
            $points = decodedPoints("\x8f" . pack('n', $pair), $encoding);
            $separate = array_merge($prefixes[$pair >> 8], $single[$pair & 255]);
            if ($points !== $separate) {
                $tripleIndex .= pack('VV', 0x8F0000 | $pair, internPoints($points, $pool, $poolOffsets));
                if (count($points) > 1 && !in_array(0xFFFFFFFF, $points, true)) {
                    $compositeCandidates[pack('N*', ...$points)] = $points;
                }
            }
        }
    }
    for ($pair = 0; $pair < 65536; ++$pair) {
        $points = decodedPoints(pack('n', $pair), $encoding);
        $separate = array_merge($single[$pair >> 8], $single[$pair & 255]);
        if ($points !== $separate) {
            $pairIndex .= pack('VV', $pair, internPoints($points, $pool, $poolOffsets));
            if (count($points) > 1 && !in_array(0xFFFFFFFF, $points, true)) {
                $compositeCandidates[pack('N*', ...$points)] = $points;
            }
        }
    }
    $escapes = '';
    if ($encoding === 'SJIS-Mobile#SOFTBANK') {
        foreach (str_split('EFGOPQ') as $mode) {
            for ($byte = 0; $byte < 256; ++$byte) {
                $points = decodedPoints("\x1b\x24" . $mode . chr($byte), $encoding);
                $escapes .= pack('V', internPoints($points, $pool, $poolOffsets));
            }
        }
    }
    mb_substitute_character('none');
    $fields = explode("\0", mb_convert_encoding($source, $encoding, 'UTF-32BE'));
    if (count($fields) !== 0x110000 - 0x800) {
        throw new RuntimeException("Unexpected encoder separator count: $encoding");
    }
    $encodeIndex = pack('VV', 0, 0);
    $encodePool = pack('V', 1) . "\0";
    $index = 0;
    for ($code = 1; $code <= 0x10FFFF; ++$code) {
        if ($code >= 0xD800 && $code <= 0xDFFF) {
            continue;
        }
        $bytes = $fields[$index++];
        if ($bytes !== '') {
            $encodeIndex .= pack('VV', $code, strlen($encodePool));
            $encodePool .= pack('V', strlen($bytes)) . $bytes;
        }
    }
    $composites = '';
    foreach ($compositeCandidates as $raw => $points) {
        $bytes = mb_convert_encoding($raw, $encoding, 'UCS-4BE');
        $separate = '';
        foreach ($points as $code) {
            $separate .= mb_convert_encoding(pack('N', $code), $encoding, 'UCS-4BE');
        }
        if ($bytes !== $separate) {
            $composites .= pack('V*', count($points), ...$points) . pack('V', strlen($bytes)) . $bytes;
        }
    }
    $entry = [];
    foreach ([
        'single' => $singleIndex, 'pairs' => $pairIndex, 'triples' => $tripleIndex, 'points' => $pool,
        'encode' => $encodeIndex, 'bytes' => $encodePool, 'composites' => $composites,
        'escapes' => $escapes,
    ] as $direction => $bytes) {
        $name = strtolower(str_replace('#', '-', $encoding)) . "-$direction.bin";
        file_put_contents("$directory/$name", $bytes);
        $entry[$direction] = ['file' => $name, 'sha256' => hash('sha256', $bytes)];
    }
    $manifest['encodings'][$encoding] = $entry;
}
file_put_contents("$directory/doublebyte.json", json_encode($manifest,
    JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
