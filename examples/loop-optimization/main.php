<?php

// Two cursors track the same position while a checksum accumulates independently.
$limit = ($argc + 7) & 255;
$left = 0;
$right = 0;
$checksum = 0;

for (; $limit > $left; $left++, $right++) {
    if ($left > $right) {
        echo "cursor mismatch\n";
        break;
    }
    $checksum = ($checksum + $left * 3) & 65535;
}

echo "items=", $limit, "; checksum=", $checksum, "\n";
