<?php

$limit = ($argc + 8) & 1023;
$checksum = 0;

for ($i = 0; $i < $limit; $i++) {
    $scaled = ($i * 3) & 65535;
    $remaining = ($limit - $i) & 65535;
    $checksum = ($checksum + $scaled) & 65535;
    $checksum = ($checksum + $remaining) & 65535;
}

echo $checksum;
