<?php
// Capture exact one-byte decoder tables and every representable Unicode encoder mapping.
// Run: php scripts/mbstring/capture_singlebyte.php

if (!extension_loaded('mbstring')) {
    fwrite(STDERR, "The mbstring extension is required.\n");
    exit(1);
}
$directory = __DIR__ . '/../../crates/elephc-mbstring/src/encoding/data';
if (!is_dir($directory)) {
    mkdir($directory, 0777, true);
}
$encodings = array_values(array_filter(mb_list_encodings(), static fn($name) =>
    str_starts_with($name, 'ISO-8859-') || str_starts_with($name, 'Windows-') ||
    in_array($name, ['CP866', 'CP850', 'KOI8-R', 'KOI8-U', 'ArmSCII-8'], true)
));

// A NUL separator survives each codec. With substitution disabled, unsupported
// scalars produce an empty field; supported ones produce exactly one non-NUL byte.
$source = '';
for ($code = 1; $code <= 0x10FFFF; ++$code) {
    if ($code < 0xD800 || $code > 0xDFFF) {
        $source .= pack('NN', $code, 0);
    }
}
$manifest = ['php_version' => PHP_VERSION, 'encodings' => []];
foreach ($encodings as $encoding) {
    $decode = '';
    mb_substitute_character(0xFFFD);
    for ($byte = 0; $byte < 256; ++$byte) {
        $code = 0xFFFFFFFF;
        if (mb_check_encoding(chr($byte), $encoding)) {
            $bytes = mb_convert_encoding(chr($byte), 'UCS-4BE', $encoding);
            if (strlen($bytes) !== 4) {
                throw new RuntimeException("Unexpected decoder shape: $encoding");
            }
            $code = unpack('N', $bytes)[1];
        }
        $decode .= pack('V', $code);
    }
    mb_substitute_character('none');
    $converted = mb_convert_encoding($source, $encoding, 'UTF-32BE');
    $fields = explode("\0", $converted);
    if (count($fields) !== 0x110000 - 0x800) {
        throw new RuntimeException("Unexpected encoder separator count: $encoding");
    }
    $encode = pack('VV', 0, 0);
    $index = 0;
    for ($code = 1; $code <= 0x10FFFF; ++$code) {
        if ($code >= 0xD800 && $code <= 0xDFFF) {
            continue;
        }
        $bytes = $fields[$index++];
        if ($bytes === '') {
            continue;
        }
        if (strlen($bytes) !== 1) {
            throw new RuntimeException("Unexpected encoder shape: $encoding U+$code");
        }
        $encode .= pack('VV', $code, ord($bytes));
    }
    $entry = [];
    foreach (['decode' => $decode, 'encode' => $encode] as $direction => $bytes) {
        $name = strtolower($encoding) . "-$direction.bin";
        file_put_contents("$directory/$name", $bytes);
        $entry[$direction] = ['file' => $name, 'sha256' => hash('sha256', $bytes)];
    }
    $manifest['encodings'][$encoding] = $entry;
}
file_put_contents("$directory/singlebyte.json", json_encode($manifest,
    JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
