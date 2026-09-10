<?php
// Capture shared kana mappings from PHP, including contextual halfwidth mark composition.
// Run: php scripts/mbstring/capture_kana.php

$root = __DIR__ . '/../../crates/elephc-mbstring';
error_reporting(E_ALL & ~E_DEPRECATED);
$manifest = ['php_version' => PHP_VERSION, 'tables' => []];
$oracle = ['php_version' => PHP_VERSION, 'scalar_hashes' => []];
foreach (['KV', 'HV'] as $mode) {
    $table = '';
    for ($code = 0xFF61; $code <= 0xFF9F; ++$code) {
        foreach ([0, 0xFF9E, 0xFF9F] as $next) {
            $input = pack('N', $code) . ($next ? pack('N', $next) : '');
            $output = mb_convert_kana($input, $mode, 'UCS-4BE');
            $table .= pack('V', strlen($output) === 4 ? unpack('N', $output)[1] : 0xFFFFFFFF);
        }
    }
    $file = 'kana-' . strtolower($mode) . '.bin';
    file_put_contents("$root/src/unicode/data/$file", $table);
    $manifest['tables'][$mode] = ['file' => $file, 'sha256' => hash('sha256', $table)];
}
foreach (str_split('ARNSKHMCarnskhmcV') as $mode) {
    $table = '';
    $hash = hash_init('sha256');
    for ($code = 0; $code <= 0x10FFFF; ++$code) {
        $input = pack('N', $code);
        $output = mb_convert_kana($input, $mode, 'UCS-4BE');
        hash_update($hash, pack('V', strlen($output)) . $output);
        if ($output !== $input) {
            $points = array_values(unpack('N*', $output));
            if ($code > 0xFFFF || count($points) > 2) { throw new RuntimeException('Unexpected kana mapping'); }
            $table .= pack('V3', $code, $points[0], $points[1] ?? 0);
        }
    }
    $file = 'kana-' . (ctype_upper($mode) ? 'upper-' : 'lower-') . strtolower($mode) . '.bin';
    file_put_contents("$root/src/unicode/data/$file", $table);
    $manifest['tables'][$mode] = ['file' => $file, 'sha256' => hash('sha256', $table)];
    $oracle['scalar_hashes'][$mode] = hash_final($hash);
}

$stream = gzopen("$root/tests/fixtures/kana.jsonl.gz", 'wb9');
// Keep invalid flag bytes and exception messages lossless, including embedded NUL.
function captureKana(string $input, string $mode, string $encoding): void {
    global $stream;
    try { $result = ['output' => bin2hex(mb_convert_kana($input, $mode, $encoding))]; }
    catch (Throwable $error) { $result = ['error' => [get_class($error), bin2hex($error->getMessage())]]; }
    gzwrite($stream, json_encode(['input' => bin2hex($input), 'mode' => bin2hex($mode),
        'encoding' => $encoding] + $result, JSON_THROW_ON_ERROR) . "\n");
}

$sample = 'ABC abc 123 !"#$%&\'()~\\ ¥‾ ＡＢＣ ａｂｃ １２３　！＂＃＄％＆＇（）～＼ ￥￣ “”‘’';
$sample .= mb_convert_encoding(pack('N*', ...range(0x3000, 0x30FF), ...range(0xFF61, 0xFF9F)), 'UTF-8', 'UCS-4BE');
for ($code = 0xFF61; $code <= 0xFF9F; ++$code) {
    foreach ([0xFF9E, 0xFF9F] as $next) { $sample .= mb_convert_encoding(pack('N2', $code, $next), 'UTF-8', 'UCS-4BE'); }
}
$flags = str_split('ARNSKHMCarnskhmcV');
foreach ($flags as $first) {
    captureKana($sample, $first, 'UTF-8');
    foreach ($flags as $second) {
        captureKana($sample, $first . $second, 'UTF-8');
        foreach ($flags as $third) { captureKana($sample, $first . $second . $third, 'UTF-8'); }
    }
}
for ($byte = 0; $byte < 256; ++$byte) { captureKana('', chr($byte), 'UTF-8'); }
mb_substitute_character(0xFFFD);
$codecs = json_decode(file_get_contents("$root/tests/fixtures/codecs.json"), true, flags: JSON_THROW_ON_ERROR);
foreach (array_keys($codecs['sha256']) as $encoding) {
    foreach (['', $sample, str_repeat('A', 63) . 'ｶﾞﾊﾟ', str_repeat('ｶ', 64) . 'ﾞ', "\0ｶ\0ﾞ"] as $source) {
        $input = mb_convert_encoding($source, $encoding, 'UTF-8');
        foreach (['', 'KV', 'HV', 'ASKV', 'ask', 'h', 'C', 'c', 'Mm'] as $mode) { captureKana($input, $mode, $encoding); }
    }
    foreach (['', 'KV', 'HV', 'ASKV', 'ask'] as $mode) { captureKana("\xc3\x28\x00\x80", $mode, $encoding); }
}
gzclose($stream);
foreach (["$root/src/unicode/data/kana.json" => $manifest, "$root/tests/fixtures/kana-hashes.json" => $oracle] as $file => $value) {
    file_put_contents($file, json_encode($value, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR) . "\n");
}
