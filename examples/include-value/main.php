<?php
// Demonstrates `require`/`require_once`/`include`/`include_once` used as general expression
// operands (not just bare statements): comparison operands, assignments, echo values, and call
// arguments. The included file runs in the caller's scope and the expression yields its top-level
// `return` value (or `1` when it has no `return`, `false` for a missing non-required include).

// A pure helper used as a call target below.
function double(int $n): int { return $n * 2; }

// 1. Assignment value: capture a returned config array.
$config = require __DIR__ . '/config.php';
echo $config['host'], ':', $config['port'], "\n";

// 2. Comparison operand inside `||` (the Composer/Symfony autoloader bootstrap pattern).
//    `require_once` yields the loader on first load; the strict `true ===` guard decides whether
//    to run a fallback. Here the loader returns an object, so `true === <object>` is false.
require_once __DIR__ . '/loader.php';
if (true === (require __DIR__ . '/make_loader.php') || false) {
    echo "loader returned true\n";
} else {
    echo "loader returned an object\n";
}

// 3. Object return captured into a variable, then used (non-`_once` so it re-runs and yields the
//    object; the class is already declared by the `require_once` above).
$loader = require __DIR__ . '/make_loader.php';
echo "loader version: ", $loader->version, "\n";

// 4. Echo value: the include's returned value is echoed directly.
echo "echoed: ", require __DIR__ . '/value.php', "\n";

// 5. Call argument: the include is evaluated and passed positionally.
echo "doubled: ", double(require __DIR__ . '/value.php'), "\n";