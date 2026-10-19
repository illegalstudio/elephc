<?php
function random_bytes(int $length) {
    if ($length < 1) {
        throw new ValueError('random_bytes(): Argument #1 ($length) must be greater than 0');
    }
    $out = "";
    $i = 0;
    while ($i < $length) {
        $out .= chr(random_int(0, 255));
        $i++;
    }
    return $out;
}
