<?php
$out = "";
$i = 0;
$fragment = '$out = $out . "ab";';
while ($i < 1200) {
    eval($fragment);
    $i += 1;
}
echo strlen($out) . "\n";
