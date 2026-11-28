<?php
// Compile with: elephc build --strict-array-keys main.php
// Numeric strings keep their bytes and their string key kind.
$labels = ["1" => "string", 1 => "integer", "01" => "leading zero"];
foreach ($labels as $key => $label) {
    echo gettype($key), ":", $key, " => ", $label, "\n";
}
echo "Entries: ", count($labels), "\n";
