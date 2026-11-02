<?php
// Purpose: writes a differential corpus for elephc-iconv's mb_strtoupper()/mb_strtolower()
// port. Each line is `mode<TAB>encoding-hex<TAB>input-hex<TAB>expected-hex` (`-` for a
// null encoding); the expected value is php's result, or the ValueError message.
//
// Usage: php scripts/mbstring/gen_case_oracle.php <cases> [seed] > corpus.tsv
// Replay: ELEPHC_MB_CASE_ORACLE=corpus.tsv cargo test -p elephc-iconv --lib \
//         differential_corpus_matches_php -- --ignored

error_reporting(E_ALL & ~E_DEPRECATED);
$cases = (int) ($argv[1] ?? 20000);
mt_srand((int) ($argv[2] ?? 1));

$utf8Pool = [
    'a', 'z', 'A', 'Z', 'i', 'I', '0', ' ', '!', '?', "'", '.', ':', '^', '`', "\0",
    'Σ', 'σ', 'ς', 'Α', 'α', 'Ω', 'ΐ', 'ΰ', 'ͅ', 'ʰ', 'ˀ', "\u{300}", "\u{AD}", '·', "\u{2019}",
    'ß', 'ŉ', 'ǰ', 'ﬁ', 'ﬀ', 'İ', 'ı', 'ǅ', 'ǆ', 'Ǆ', 'ẞ', 'µ', 'ÿ', 'é', 'É',
    'ა', 'Ა', 'ꭰ', 'Ꭰ', "\u{10400}", "\u{10428}", "\u{1E900}", '😀', 'Ⅻ', 'ⓐ', 'Ａ', 'ａ',
    "\u{A7CB}", "\u{A7DC}", "\u{1C89}", "\u{1C8A}", 'ǈ', 'ᾈ', 'ᾀ', 'ῼ',
    "\x80", "\xff", "\xc3", "\xe2\x82", "\xf0\x9f", "\xf0\x9f\x98", "\xed\xa0\x80", "\xc0\xaf",
    "\xf4\x90\x80\x80", "\xe0\x80", "\xf8\x88\x80\x80\x80", "\xc2",
];

$exact = [
    'UTF-8', 'utf8', 'ASCII', '7bit', '8bit', 'ISO-8859-1', 'ISO-8859-2', 'ISO-8859-5',
    'ISO-8859-7', 'ISO-8859-9', 'ISO-8859-15', 'Windows-1251', 'Windows-1252', 'Windows-1254',
    'CP866', 'KOI8-R', 'KOI8-U', 'ArmSCII-8', 'CP850',
    'UTF-16', 'UTF-16BE', 'UTF-16LE', 'UTF-32', 'UTF-32BE', 'UTF-32LE',
    'UCS-2', 'UCS-2BE', 'UCS-2LE', 'UCS-4', 'UCS-4BE', 'UCS-4LE',
];
$invalid = ['nope', '', 'pass', 'auto', 'UTF_8', "utf-8\0junk", "bad\0x", "\xff"];

/** Builds a random UTF-8-ish string from the interesting-token pool. */
function utf8_sample(array $pool): string
{
    $length = mt_rand(0, 3) === 0 ? mt_rand(60, 140) : mt_rand(0, 12);
    $out = '';
    for ($i = 0; $i < $length; $i++) {
        $out .= $pool[mt_rand(0, count($pool) - 1)];
    }
    return $out;
}

/** Builds random bytes, sometimes long enough to cross a 64-code-point chunk. */
function byte_sample(): string
{
    $length = mt_rand(0, 3) === 0 ? mt_rand(60, 300) : mt_rand(0, 16);
    $out = '';
    for ($i = 0; $i < $length; $i++) {
        $out .= chr(mt_rand(0, 255));
    }
    return $out;
}

/** Builds a sample for a wide encoding by transcoding UTF-8 text, then damaging it. */
function wide_sample(array $pool, string $encoding): string
{
    $text = mb_convert_encoding(utf8_sample($pool), $encoding, 'UTF-8');
    switch (mt_rand(0, 5)) {
        case 0:
            return byte_sample();
        case 1:
            return "\xff\xfe" . $text;
        case 2:
            return "\xfe\xff" . $text;
        case 3:
            return $text . chr(mt_rand(0, 255));
        default:
            return $text;
    }
}

/** Runs one conversion in php and returns the result or the ValueError message. */
function oracle(string $mode, string $input, ?string $encoding): string
{
    try {
        return $mode === 'upper' ? mb_strtoupper($input, $encoding) : mb_strtolower($input, $encoding);
    } catch (ValueError $error) {
        return $error->getMessage();
    }
}

for ($n = 0; $n < $cases; $n++) {
    $mode = mt_rand(0, 1) ? 'upper' : 'lower';
    $pick = mt_rand(0, 99);
    if ($pick < 35) {
        $encoding = mt_rand(0, 2) === 0 ? null : 'UTF-8';
        $input = utf8_sample($utf8Pool);
    } elseif ($pick < 40) {
        $encoding = $invalid[mt_rand(0, count($invalid) - 1)];
        $input = utf8_sample($utf8Pool);
    } else {
        $encoding = $exact[mt_rand(0, count($exact) - 1)];
        if (preg_match('/^(UTF-(16|32)|UCS-)/', $encoding)) {
            $input = wide_sample($utf8Pool, $encoding);
        } elseif (stripos($encoding, 'utf') === 0) {
            $input = utf8_sample($utf8Pool);
        } else {
            $input = mt_rand(0, 1) ? byte_sample() : utf8_sample($utf8Pool);
        }
    }
    $expected = oracle($mode, $input, $encoding);
    echo $mode, "\t", $encoding === null ? '-' : bin2hex($encoding), "\t", bin2hex($input), "\t", bin2hex($expected), "\n";
}
