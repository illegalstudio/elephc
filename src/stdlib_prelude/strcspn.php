<?php
function strcspn(string $string, string $characters, int $offset = 0, ?int $length = null) {
    $len = strlen($string);
    if ($offset < 0) {
        $offset = $offset + $len;
        if ($offset < 0) { $offset = 0; }
    } elseif ($offset > $len) {
        return 0;
    }
    $avail = $len - $offset;
    if ($length === null) {
        $length = $avail;
    } elseif ($length < 0) {
        $length = $avail + $length;
        if ($length < 0) { $length = 0; }
    } elseif ($length > $avail) {
        $length = $avail;
    }
    $n = 0;
    while ($n < $length) {
        if (strpos($characters, $string[$offset + $n]) !== false) { break; }
        $n++;
    }
    return $n;
}
