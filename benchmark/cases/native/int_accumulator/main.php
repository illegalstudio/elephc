<?php
$n = 0;
for ($i = 0; $i < 4999999 + $argc; $i++) {
    $n += intval("42");
}
echo $n, "\n";
