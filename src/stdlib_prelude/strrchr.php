<?php
function strrchr(string $haystack, string $needle, bool $before_needle = false) {
    $c = $needle === "" ? "\0" : $needle[0];
    $pos = strrpos($haystack, $c);
    if ($pos === false) {
        return false;
    }
    if ($before_needle) {
        return substr($haystack, 0, $pos);
    }
    return substr($haystack, $pos);
}
