<?php
// Multi-file example: include functions from other files, including through a
// loader function.

function load_libraries() {
    require_once 'math.php';
    require_once 'greet.php';
}

load_libraries();

for ($i = 0; $i < 2; $i = $i + 1) {
    require_once 'bootstrap.php';
}

hello("World");

echo "3 + 4 = " . add(3, 4) . "\n";
echo "5 * 6 = " . multiply(5, 6) . "\n";
echo "10! = " . factorial(10) . "\n";

// `require` used as a value: the file is included and the expression yields 1.
function load_version(): int {
    return require_once 'version.php';
}
$ok = load_version();
echo "version loaded (" . $ok . "): " . library_version() . "\n";

// `require` of a config file yields the file's `return` value.
$settings = require 'settings.php';
echo "settings: " . $settings["name"] . " / " . $settings["answer"] . "\n";
