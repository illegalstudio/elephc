<?php

echo @file_get_contents("missing.txt");
echo "after missing file\n";

echo strpos("elephc", "php") === false ? "strpos miss\n" : "strpos hit\n";

$names = ["Ada", "Grace"];
echo array_search("Linus", $names) === false ? "array_search miss\n" : "array_search hit\n";

echo define("ERROR_CONTROL_EXAMPLE", true) ? "defined\n" : "duplicate\n";
echo @define("ERROR_CONTROL_EXAMPLE", false) ? "defined again\n" : "duplicate suppressed\n";
