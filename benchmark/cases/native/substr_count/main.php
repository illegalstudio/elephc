<?php
$haystack = str_repeat("abc", 699 + $argc);
$total = 0;
for ($i = 0; $i < 19999 + $argc; $i++) {
    $total += substr_count($haystack, "abc");
}
echo $total, "\n";
