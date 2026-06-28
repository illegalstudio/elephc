<?php

// var_export() renders a parsable representation of a value, matching PHP's layout.

// Scalars: strings are single-quoted (with \\ and \' escaping), bool/null use
// keywords, and an integer-valued float keeps its decimal point.
var_export(42);
echo "\n";
var_export("it's a \\backslash");
echo "\n";
var_export(true);
echo "\n";
var_export(null);
echo "\n";
var_export(1.0);
echo "\n";

// Floats use serialize_precision = -1: the shortest decimal that round-trips back
// to the same double (so 1/3 keeps 16 significant digits, not 14), with PHP's
// scientific layout for very large or very small magnitudes.
var_export(1.0 / 3.0);
echo "\n";
var_export(1.0e17);
echo "\n";
var_export(0.000001);
echo "\n";

// Arrays render in the indented `array ( ... )` form with `key => value,` entries.
// Integer keys are bare, string keys are quoted, and nested arrays go on their own line.
$config = [
    'name' => 'elephc',
    'version' => 3,
    'targets' => ['macos-aarch64', 'linux-aarch64', 'linux-x86_64'],
    'flags' => ['release' => true, 'lto' => false],
];
var_export($config);
echo "\n";

// With the second argument set to true, the rendering is returned instead of printed.
$dump = var_export([1, 2, 3], true);
echo "captured " . strlen($dump) . " chars:\n";
echo $dump . "\n";
