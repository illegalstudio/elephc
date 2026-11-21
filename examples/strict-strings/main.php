<?php
function rowLabel(int $row, string $name): string {
    $prefix = "Row $row: ";
    return $prefix . $name;
}

echo rowLabel(3, 'notebook'), "\n";
$total = 7;
echo "Total: $total\n";
echo 'Next row: ' . (string) 4 . "\n";
