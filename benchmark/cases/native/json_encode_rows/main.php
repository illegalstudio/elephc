<?php
$rows = [];
for ($i = 0; $i < 100; $i++) {
    $rows[] = ["id" => $i, "name" => "item" . $i, "active" => true];
}
$n = 0;
for ($i = 0; $i < 29999 + $argc; $i++) {
    $n += strlen(json_encode($rows));
}
echo $n, "\n";
