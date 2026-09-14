<?php
// Capture MIME header encoding for the shared engine compatibility fixtures.
// Run: php scripts/mbstring/capture_mime_encode.php

error_reporting(E_ALL & ~E_DEPRECATED);
$stream = gzopen(__DIR__ . '/../../crates/elephc-mbstring/tests/fixtures/mime_encode.jsonl.gz', 'wb9');
$count = 0;

/** Records byte-exact output, diagnostics, and omitted versus explicitly supplied options. */
function captureMimeEncode($stream, string $input, string $internal, string $language, array $options = [], int|string $substitute = 'none'): void {
    global $count;
    mb_language($language);
    mb_internal_encoding($internal);
    mb_substitute_character($substitute);
    $warnings = [];
    set_error_handler(static function (int $severity, string $message) use (&$warnings): bool {
        $warnings[] = $message;
        return true;
    });
    $case = ['input' => bin2hex($input), 'internal' => $internal, 'language' => $language,
        'options' => array_map(static fn ($value) => is_string($value) ? ['bytes' => bin2hex($value)] : $value, $options),
        'substitute' => $substitute];
    try {
        $case['output'] = bin2hex(mb_encode_mimeheader($input, ...$options));
    } catch (Throwable $error) {
        $case['error'] = [get_class($error), $error->getMessage()];
    } finally {
        restore_error_handler();
    }
    $case['warnings'] = $warnings;
    gzwrite($stream, json_encode($case, JSON_THROW_ON_ERROR) . "\n");
    $count++;
}

$encodings = mb_list_encodings();
$languages = ['neutral', 'uni', 'English', 'German', 'Japanese', 'Korean',
    'Simplified Chinese', 'Traditional Chinese', 'Russian', 'Armenian', 'Turkish', 'Ukrainian'];

// Cover every internal/output codec pair, including explicitly forbidden MIME charsets.
foreach ($encodings as $internal) {
    mb_substitute_character(0xFFFD);
    $inputs = ['', mb_convert_encoding('ASCII', $internal, 'UTF-8'),
        mb_convert_encoding("Subject: café 猫 🇺🇸 &\0", $internal, 'UTF-8')];
    foreach ($encodings as $output) {
        foreach ($inputs as $input) {
            foreach (['B', 'Q'] as $transfer) {
                captureMimeEncode($stream, $input, $internal, 'neutral', [$output, $transfer]);
            }
        }
    }
}

// Language defaults, explicit-null quirks, encoding aliases, and transfer-prefix selection.
$options = [[], [null], [''], ['missing'], ['UTF-8'], ['utf8'], ["UTF-8\0ignored"],
    ['UTF-8', null], ['UTF-8', ''], ['UTF-8', 'q'], ['UTF-8', 'Qanything'],
    ['UTF-8', 'base64'], ['UTF-8', 'invalid'], ['UTF-8', "\0Q"]];
foreach ($languages as $language) {
    foreach (['', 'safeASCII', 'two words', 'café 猫', str_repeat('word ', 40)] as $input) {
        foreach ($options as $arguments) {
            captureMimeEncode($stream, $input, 'UTF-8', $language, $arguments);
        }
    }
}

// Exercise line folding immediately before and after decoder and MIME-word boundaries.
foreach (range(0, 200) as $length) {
    foreach ([str_repeat('a', $length), str_repeat(' ', $length) . 'A',
        str_repeat('a', $length) . ' ?tail', str_repeat('x ', $length) . 'é',
        str_repeat('é', $length)] as $input) {
        foreach (['B', 'Q'] as $transfer) {
            foreach ([0, 1, 20, 55, 73, 74] as $indent) {
                captureMimeEncode($stream, $input, 'UTF-8', 'neutral', ['UTF-8', $transfer, "\r\n", $indent]);
            }
        }
    }
}

// PHP truncates the line separator to eight bytes and then stops at its first NUL.
foreach (["", "\n", "\r", "\r\n", "123456789abc", "x\0y", "\0\n", "é猫🇺🇸"] as $separator) {
    foreach ([-PHP_INT_MAX - 1, -1, 0, 1, 54, 55, 72, 73, 74, 75, PHP_INT_MAX] as $indent) {
        foreach (['B', 'Q'] as $transfer) {
            captureMimeEncode($stream, str_repeat('a 猫 ', 30), 'UTF-8', 'neutral', ['UTF-8', $transfer, $separator, $indent]);
        }
    }
}

// Stateful source decoding keeps its state when the ASCII fast path restarts the input.
$stateful = ['UTF-16', 'UTF-32', 'UCS-2', 'UCS-4', 'UTF-7', 'UTF7-IMAP', 'JIS',
    'ISO-2022-JP-2004', 'ISO-2022-JP-MS', 'ISO-2022-JP-MOBILE#KDDI', 'ISO-2022-KR',
    'HZ', 'SJIS-Mobile#SOFTBANK', 'BASE64', 'UUENCODE'];
foreach ($stateful as $internal) {
    foreach ([0, 1, 73, 74, 75, 78, 79, 80, 88, 89, 90, 91, 127, 128, 179, 180, 181] as $length) {
        foreach (['é', '猫', '🇺🇸', "か\u{309A}", '?', "\0"] as $tail) {
            mb_substitute_character(0xFFFD);
            $input = mb_convert_encoding(str_repeat('A', $length) . $tail . ' end', $internal, 'UTF-8');
            foreach (['UTF-8', 'ISO-2022-JP', 'SJIS-Mobile#KDDI'] as $output) {
                foreach (['B', 'Q'] as $transfer) {
                    captureMimeEncode($stream, $input, $internal, 'neutral', [$output, $transfer]);
                }
            }
        }
    }
}

// Replacement policy is fixed by MIME encoding, independently of request substitution settings.
foreach ($encodings as $internal) {
    foreach (["\xff\x80A", "\0?_=\r\n\t", implode('', array_map(chr(...), range(0, 255)))] as $input) {
        foreach (['none', 'long', 'entity', 0xFFFD] as $substitute) {
            foreach (['B', 'Q'] as $transfer) {
                captureMimeEncode($stream, $input, $internal, 'neutral', ['UTF-8', $transfer], $substitute);
            }
        }
    }
}

// Keep composable pairs and deferred output next to trial-chunk and MIME line boundaries.
$composing = ['SJIS-2004', 'EUC-JP-2004', 'ISO-2022-JP-2004', 'CP50220',
    'SJIS-Mobile#DOCOMO', 'SJIS-Mobile#KDDI', 'SJIS-Mobile#SOFTBANK', 'ISO-2022-JP-MOBILE#KDDI'];
foreach ($composing as $output) {
    foreach (["か\u{309A}", "カ\u{309A}", 'ｶﾞ', '🇺🇸', "1\u{20E3}", "#\u{20E3}"] as $pair) {
        foreach (range(0, 30) as $length) {
            foreach (['B', 'Q'] as $transfer) {
                foreach ([0, 55] as $indent) {
                    captureMimeEncode($stream, '?' . str_repeat('A', $length) . $pair . $pair . '1',
                        'UTF-8', 'neutral', [$output, $transfer, "\r\n", $indent]);
                }
            }
        }
    }
}

// Retain every legacy composition across trial chunks, including compact Apple hint sequences.
foreach (['sjis-mac' => 'SJIS-mac', 'sjis-2004' => 'SJIS-2004', 'euc-jp-2004' => 'EUC-JP-2004'] as $table => $output) {
    $data = file_get_contents(__DIR__ . '/../../crates/elephc-mbstring/src/encoding/data/' . $table . '-composites.bin');
    for ($position = 0; $position < strlen($data);) {
        $countPoints = unpack('V', $data, $position)[1];
        $position += 4;
        $points = array_values(unpack('V' . $countPoints, $data, $position));
        $position += 4 * $countPoints;
        $length = unpack('V', $data, $position)[1];
        $position += 4 + $length;
        $sequence = implode('', array_map(static fn ($point) => mb_chr($point, 'UTF-8'), $points));
        foreach (range(0, 15) as $prefix) {
            foreach (['B', 'Q'] as $transfer) {
                captureMimeEncode($stream, '?' . str_repeat('A', $prefix) . $sequence . $sequence,
                    'UTF-8', 'neutral', [$output, $transfer]);
            }
        }
        foreach ([20, 100] as $repeats) {
            captureMimeEncode($stream, str_repeat($sequence, $repeats), 'UTF-8', 'neutral', [$output, 'B']);
        }
    }
}

gzclose($stream);
fwrite(STDERR, "Captured {$count} MIME encoding cases with PHP " . PHP_VERSION . ".\n");
