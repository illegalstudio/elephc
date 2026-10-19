<?php
function substr_compare(string $haystack, string $needle, int $offset, ?int $length = null, bool $case_insensitive = false) {
    $hlen = strlen($haystack);
    if ($length !== null && $length < 0) {
        throw new ValueError('substr_compare(): Argument #4 ($length) must be greater than or equal to 0');
    }
    if ($offset < 0) {
        $offset = $offset + $hlen;
        if ($offset < 0) { $offset = 0; }
    }
    if ($offset > $hlen) {
        throw new ValueError('substr_compare(): Argument #3 ($offset) must be contained in argument #1 ($haystack)');
    }
    if ($length === 0) {
        return 0;
    }
    $rest = $hlen - $offset;
    if ($length === null) {
        $length = $rest > strlen($needle) ? $rest : strlen($needle);
    }
    $a = substr($haystack, $offset, $length);
    $b = substr($needle, 0, $length);
    if ($case_insensitive) {
        $a = strtolower($a);
        $b = strtolower($b);
    }
    $la = strlen($a);
    $lb = strlen($b);
    $m = $la < $lb ? $la : $lb;
    for ($i = 0; $i < $m; $i++) {
        $d = ord($a[$i]) - ord($b[$i]);
        if ($d !== 0) { return $d; }
    }
    if ($la === $lb) { return 0; }
    return $la < $lb ? -1 : 1;
}
