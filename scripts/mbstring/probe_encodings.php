<?php
// Purpose: dumps, as JSON, the mbstring facts `gen_case_tables.py` bakes into
// crates/elephc-iconv/src/case/tables.rs: every encoding name mbstring accepts (including
// the prefixes its perfect-hash lookup lets through) and the exact byte <-> code point
// tables of the single-byte encodings. Run it with the php whose mbstring is the oracle.

error_reporting(E_ALL & ~E_DEPRECATED & ~E_WARNING);
mb_substitute_character('none');

$singleByte = [
    'ASCII', '7bit', '8bit',
    'ISO-8859-1', 'ISO-8859-2', 'ISO-8859-3', 'ISO-8859-4', 'ISO-8859-5', 'ISO-8859-6',
    'ISO-8859-7', 'ISO-8859-8', 'ISO-8859-9', 'ISO-8859-10', 'ISO-8859-13', 'ISO-8859-14',
    'ISO-8859-15', 'ISO-8859-16',
    'Windows-1251', 'Windows-1252', 'Windows-1254', 'CP866', 'CP850', 'KOI8-R', 'KOI8-U',
    'ArmSCII-8',
];

/** Reports whether mb_strtoupper() accepts one encoding name. */
function accepted(string $name): bool
{
    try {
        mb_strtoupper('', $name);
        return true;
    } catch (ValueError $e) {
        return false;
    }
}

/** Fingerprints the encoding one accepted name resolves to. */
function fingerprint(string $name): string
{
    return json_encode([@mb_encoding_aliases($name), @mb_preferred_mime_name($name)]);
}

$encodings = mb_list_encodings();
$byFingerprint = [];
foreach ($encodings as $encoding) {
    $byFingerprint[fingerprint($encoding)][] = $encoding;
}

$names = [];
foreach ($encodings as $encoding) {
    $candidates = [$encoding];
    $mime = @mb_preferred_mime_name($encoding);
    if ($mime) {
        $candidates[] = $mime;
    }
    foreach (@mb_encoding_aliases($encoding) as $alias) {
        $candidates[] = $alias;
    }
    for ($length = 1; $length < strlen($encoding); $length++) {
        $candidates[] = substr($encoding, 0, $length);
    }
    foreach ($candidates as $candidate) {
        $key = strtolower($candidate);
        if (isset($names[$key]) || !accepted($candidate)) {
            continue;
        }
        $exact = null;
        foreach ($encodings as $known) {
            if (strcasecmp($known, $candidate) === 0) {
                $exact = $known;
                break;
            }
        }
        if ($exact !== null) {
            $names[$key] = $exact;
            continue;
        }
        $targets = $byFingerprint[fingerprint($candidate)] ?? [];
        if (count($targets) !== 1) {
            fwrite(STDERR, "ambiguous encoding name: $candidate\n");
            exit(1);
        }
        $names[$key] = $targets[0];
    }
}
ksort($names, SORT_STRING);

$tables = [];
foreach ($singleByte as $encoding) {
    $decode = [];
    for ($byte = 0; $byte < 256; $byte++) {
        $wide = mb_convert_encoding(chr($byte), 'UTF-32BE', $encoding);
        $decode[] = $wide === '' ? -1 : unpack('N', $wide)[1];
    }
    $encode = [];
    for ($cp = 0; $cp <= 0xFFFF; $cp++) {
        if ($cp >= 0xD800 && $cp <= 0xDFFF) {
            continue;
        }
        $bytes = mb_convert_encoding(mb_chr($cp, 'UTF-8'), $encoding, 'UTF-8');
        if ($bytes === '') {
            continue;
        }
        if (strlen($bytes) !== 1) {
            fwrite(STDERR, "multi-byte output for $encoding U+" . dechex($cp) . "\n");
            exit(1);
        }
        $encode[] = [$cp, ord($bytes)];
    }
    $tables[$encoding] = ['decode' => $decode, 'encode' => $encode];
}

echo json_encode([
    'php_version' => PHP_VERSION,
    'encodings' => $encodings,
    'names' => $names,
    'single_byte' => $tables,
], JSON_PRETTY_PRINT), "\n";
