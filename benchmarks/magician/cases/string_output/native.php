<?php
$out = "";
$i = 0;
while ($i < 1200) {
    $out = $out . "ab";
    $i += 1;
}
echo strlen($out) . "\n";
