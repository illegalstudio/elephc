<?php
$out = "";
$i = 0;
while ($i < 5000) {
    $out = $out . "abc";
    $i += 1;
}
echo strlen($out) . "\n";
