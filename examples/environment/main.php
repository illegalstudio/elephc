<?php
// putenv() sets a variable for this process, and getenv() reads it back.
putenv("ELEPHC_DEMO=on");
echo "getenv: " . getenv("ELEPHC_DEMO") . "\n";

// A bare name without `=` removes the variable, exactly as PHP does.
putenv("ELEPHC_DEMO");
var_dump(getenv("ELEPHC_DEMO"));

// The second argument is accepted; in the CLI SAPI there is no environment
// separate from the process's, so it answers the same value.
putenv("ELEPHC_LOCAL=1");
var_dump(getenv("ELEPHC_LOCAL", true));

// getenv() with no argument answers the whole environment as an array.
$env = getenv();
echo "PATH is set: " . (isset($env["PATH"]) ? "yes" : "no") . "\n";
echo "putenv value visible: " . $env["ELEPHC_LOCAL"] . "\n";

// $_ENV and $_SERVER are snapshots taken before the program ran, so they do
// not carry the putenv() addition — PHP has the same asymmetry.
echo '$_ENV has PATH: ' . (isset($_ENV["PATH"]) ? "yes" : "no") . "\n";
echo '$_ENV has the putenv value: ' . (isset($_ENV["ELEPHC_LOCAL"]) ? "yes" : "no") . "\n";

// $_SERVER carries PHP's own CLI keys on top of the environment.
echo '$_SERVER has argv: ' . (isset($_SERVER["argv"]) ? "yes" : "no") . "\n";
echo '$_SERVER argc: ' . $_SERVER["argc"] . "\n";
echo "SCRIPT_NAME: " . ($_SERVER["SCRIPT_NAME"] === $argv[0] ? "the invoked program" : "?") . "\n";
