<?php

// The long-form `array(...)` construct is exactly equivalent to the short `[...]` form.
// Both produce the same array; they differ only in spelling. The long form is common in
// older code and in libraries such as Composer's autoloader, so it must parse identically.

// Indexed (positional) entries.
$primes = array(2, 3, 5, 7, 11);
echo "primes: " . count($primes) . " (last " . $primes[4] . ")\n";

// Associative entries with `key => value`, including a runtime-valued key.
$env = "prod";
$config = array(
    "debug"   => false,
    "env"     => $env,
    "secret"  => 1234,
);
echo "env: " . $config["env"] . ", debug: " . ($config["debug"] ? "on" : "off") . "\n";

// Nesting and mixing with the short form freely.
$routes = array(
    "home"  => ["path" => "/", "method" => "GET"],
    "about" => array("path" => "/about", "method" => "GET"),
);
foreach ($routes as $name => $route) {
    echo $name . " => " . $route["method"] . " " . $route["path"] . "\n";
}

// Spreading one array into another, long form.
$base = array(1, 2);
$extended = array(...$base, 3, 4);
echo "extended: " . implode(",", $extended) . "\n";

// The long form interoperates with array builtins just like `[...]`.
$merged = array_merge(array(1, 2), [3, 4]);
echo "merged: " . implode(",", $merged) . "\n";
