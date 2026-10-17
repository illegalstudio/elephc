<?php
// Repeated local append keeps the input string as a borrowed source.
function repeat(string $str, int $times): string {
    $result = "";
    $i = 0;
    while ($i < $times) {
        $result = $result . $str;
        $i++;
    }
    return $result;
}

echo repeat("*", 20) . "\n";
echo "* " . repeat(". ", 8) . " *\n";
echo repeat("*", 20) . "\n";

echo "\n";
echo "Triangle:\n";
for ($row = 1; $row <= 5; $row++) {
    echo repeat("# ", $row) . "\n";
}
