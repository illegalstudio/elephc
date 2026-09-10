<?php
// Capture the canonical result of every documented name, MIME charset, and alias.
if (PHP_VERSION !== '8.5.10') { throw new RuntimeException('Requires PHP 8.5.10'); }
$names = ['', 'not-an-encoding', ' UTF-8', 'UTF-8 '];
foreach (mb_list_encodings() as $encoding) {
    $names[] = $encoding;
    $mime = mb_preferred_mime_name($encoding);
    if ($mime !== false) { $names[] = $mime; }
    foreach (mb_encoding_aliases($encoding) as $alias) { $names[] = $alias; }
}
$cases = [];
foreach (array_unique($names) as $name) {
    foreach (array_unique([$name, strtolower($name), strtoupper($name)]) as $input) {
        try { mb_internal_encoding($input); $cases[$input] = mb_internal_encoding(); }
        catch (ValueError) { $cases[$input] = null; }
    }
}
ksort($cases);
echo json_encode($cases, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR), "\n";
