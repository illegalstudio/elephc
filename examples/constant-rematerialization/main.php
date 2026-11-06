<?php
// Accumulate eight bounded sensor samples using fixed scaling and a 16-bit checksum.
$seed = $argc & 255;
$checksum = 0;
for ($sample = 0; $sample < 8; $sample++) {
    $reading = ($seed + $sample) & 255;
    $checksum = ($checksum + ($reading * 31) + 4096) & 65535;
}
echo "samples=8; checksum=", $checksum, "\n";
