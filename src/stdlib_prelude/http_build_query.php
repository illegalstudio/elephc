<?php
function __elephc_hbq_enc(string $s, int $enc) {
    if ($enc === 2) { return rawurlencode($s); }
    return urlencode($s);
}
function __elephc_hbq_walk(mixed $items, string $prefix, string $numericPrefix, int $enc) {
    $parts = [];
    foreach ($items as $k => $v) {
        if ($v === null) { continue; }
        if (is_string($k) && substr($k, 0, 1) === "\0") { continue; }
        if (is_int($k)) {
            $name = $prefix === "" ? $numericPrefix . $k : $prefix . "%5B" . $k . "%5D";
        } else {
            $ek = __elephc_hbq_enc((string) $k, $enc);
            $name = $prefix === "" ? $ek : $prefix . "%5B" . $ek . "%5D";
        }
        if (is_array($v)) {
            $sub = __elephc_hbq_walk($v, $name, $numericPrefix, $enc);
            foreach ($sub as $piece) { $parts[] = $piece; }
            continue;
        }
        if (is_object($v)) {
            $sub = __elephc_hbq_walk((array) $v, $name, $numericPrefix, $enc);
            foreach ($sub as $piece) { $parts[] = $piece; }
            continue;
        }
        if (is_bool($v)) {
            $s = $v ? "1" : "0";
        } else {
            $s = (string) $v;
        }
        $parts[] = $name . "=" . __elephc_hbq_enc($s, $enc);
    }
    return $parts;
}
function http_build_query(array|object $data, string $numeric_prefix = "", ?string $arg_separator = null, int $encoding_type = PHP_QUERY_RFC1738) {
    // is_array first: the checker narrows `is_array` on both branches but not `is_object`,
    // and `(array)` of the un-narrowed `array|object` is typed `array` (#707).
    if (is_array($data)) {
        $items = $data;
    } else {
        $items = (array) $data;
    }
    $parts = __elephc_hbq_walk($items, "", $numeric_prefix, $encoding_type);
    $sep = $arg_separator === null ? "&" : $arg_separator;
    return implode($sep, $parts);
}
