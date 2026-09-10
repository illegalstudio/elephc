<?php
// Capture numeric-entity maps, malformed references, substitution, and encoding boundaries.
// Run: php scripts/mbstring/capture_entities.php

error_reporting(E_ALL & ~E_DEPRECATED);
$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/entities.jsonl.gz', 'wb9');

/** Records one complete entity request and its exact result or exception. */
function captureEntity($stream, string $input, array $map, string $encoding, bool $decode, bool $hex, int|string $substitute): void {
    mb_substitute_character(0xFFFD);
    mb_substitute_character($substitute);
    $case = ['input' => bin2hex($input), 'map' => $map, 'encoding' => $encoding,
        'decode' => $decode, 'hex' => $hex, 'substitute' => $substitute];
    try {
        $result = $decode ? mb_decode_numericentity($input, $map, $encoding) : mb_encode_numericentity($input, $map, $encoding, $hex);
        $case['output'] = bin2hex($result);
    } catch (Throwable $error) {
        $case['error'] = [$error::class, $error->getMessage()];
    }
    gzwrite($stream, json_encode($case, JSON_THROW_ON_ERROR) . "\n");
}

$maps = [[], [0, 0x10FFFF, 0, 0xFFFFFFFF], [0, 0xFFFFFFFF, 0, 0xFFFFFFFF],
    [0x80, 0x10FFFF, 0, 0xFFFF], [0, 0xFFFFFFFF, 0xFFFFFFFF, 0xFFFFFFFF],
    [0, 0xFFFFFFFF, 0x100000001, 0x1FFFFFFFF], [0, 0xFFFFFFFF, 100, 255],
    [0, 0xFFFFFFFF, -100, -1], [0, 0xFFFFFFFF, 0, 0], [-1, -1, 0, -1],
    [100, 0, 0, -1], [0xD800, 0xDFFF, 0, -1], [65, 65, 1, -1, 65, 65, 2, -1],
    [65, 65, 0, 0, 66, 66, -1, 0], [0, 100, 0, 255, 0, 0xFFFFFFFF, 0, 0],
    [1], [0, 1, 0], [0, 1, 0, 1, 0]];
$references = [
    'A&#65;B &#x41; &#X41; &#65 &#x41 &&#65;&#0;&#0000000000;',
    '&#;&#x;&#X;&#-1;&#+65;&# 65;&#x0X41;&#12x; &#xFf; &#00000000000;',
    '&#4294967294;&#4294967295;&#4294967296;&#4294967299;&#4294967300;',
    '&#9999999999;&#10000000000;&#xFFFFFFFF;&#x100000000;&#x000000000;',
    '&#xD800;&#xDFFF;&#x10000;&#1114111;&#1114112;&#127482;&#127480;',
    '&&amp;&#38;#65; &#38;&#35;65; &#65&#66; &&&#65;&unfinished',
    "&#65\0; &#x41\xff; &\0#65; \0&", str_repeat('A', 127) . '&#65;' . str_repeat('B', 120) . '&#x41;',
];
foreach (mb_list_encodings() as $encoding) {
    mb_substitute_character(0xFFFD);
    $inputs = ['', "\x80\xffA", implode('', array_map(chr(...), range(0, 255))),
        mb_convert_encoding("hé日ガ０🇺🇸 & \0", $encoding, 'UTF-8')];
    foreach ($references as $reference) { $inputs[] = mb_convert_encoding($reference, $encoding, 'UTF-8'); }
    foreach ($maps as $map) {
        foreach ($inputs as $input) {
            foreach ([0xFFFD, 'none', 'long', 'entity'] as $substitute) {
                captureEntity($stream, $input, $map, $encoding, true, false, $substitute);
                foreach ([false, true] as $hex) { captureEntity($stream, $input, $map, $encoding, false, $hex, $substitute); }
            }
        }
    }
}
$encoding = 'ISO-2022-JP-MOBILE#KDDI';
$map = [0, 0xFFFFFFFF, 0, 0xFFFFFFFF];
foreach (range(0, 270) as $prefix) {
    foreach (['&#127482;&#127480;', '&#x1F1FA;&#x1F1F8;', '&#127482;🇸', '🇺&#127480;', '&#49;&#8419;', '1&#x20E3;'] as $tail) {
        $input = mb_convert_encoding(str_repeat('A', $prefix) . $tail, $encoding, 'UTF-8');
        captureEntity($stream, $input, $map, $encoding, true, false, 0xFFFD);
    }
}
gzclose($stream);
