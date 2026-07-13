<?php

$data = [
    "name" => "elephc",
    "version" => 1,
    "features" => ["codegen", "optimizer"],
];

// print_r($value, true) captures the rendered output as a string instead of
// writing it to stdout, so it can be stored, measured, or post-processed.
$rendered = print_r($data, true);

echo "captured " . strlen($rendered) . " bytes:\n";
echo $rendered;

// Echo mode still writes directly to stdout and returns true.
$ok = print_r($data);
echo "echo mode returned: ";
echo $ok ? "true\n" : "false\n";