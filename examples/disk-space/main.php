<?php
// disk_free_space() and disk_total_space() report filesystem capacity in bytes.

$total = disk_total_space("/");
$free = disk_free_space("/");

echo "total bytes: " . $total . "\n";
echo "free bytes:  " . $free . "\n";

$used_percent = (int) (($total - $free) / $total * 100);
echo "used:        " . $used_percent . "%\n";
