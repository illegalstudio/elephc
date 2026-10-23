<?php

// Comparing release versions and minting random tokens.
//
// version_compare() follows PHP's ordering of development and release tags
// (dev < alpha < beta < RC < stable < pl), and random_bytes() reads the
// operating system's cryptographically secure generator.

$releases = ["1.10.0", "1.2.0", "1.10.0-RC1", "1.10.0beta2", "1.9.9"];
usort($releases, fn($a, $b) => version_compare($a, $b));
echo "sorted: ", implode(", ", $releases), "\n";

$installed = "8.1.4";
foreach (["8.0.0", "8.1.4", "8.2.0"] as $required) {
    $ok = version_compare($installed, $required, ">=") ? "yes" : "no";
    echo "installed $installed satisfies >= $required: $ok\n";
}

// A token is raw bytes; hex-encode it for printing or URLs.
$token = random_bytes(16);
echo "token bytes: ", strlen($token), "\n";
echo "hex length: ", strlen(bin2hex($token)), "\n";

try {
    random_bytes(0);
} catch (ValueError $e) {
    echo "caught: ", $e->getMessage(), "\n";
}
