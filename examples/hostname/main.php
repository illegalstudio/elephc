<?php
// gethostname() returns the name of the host running the program.
// gethostbyname() resolves a host name to its IPv4 address, and
// gethostbyaddr() reverse-resolves an address back to a host name.

$host = gethostname();
echo "running on host: " . $host . "\n";
echo "host name length: " . strlen($host) . "\n";

// Resolve the loopback host name to its IPv4 address. An unresolvable
// name is returned unchanged.
$loopback = gethostbyname("localhost");
echo "localhost resolves to: " . $loopback . "\n";

// Reverse-resolve that address back to a host name.
echo "address resolves back to: " . gethostbyaddr($loopback) . "\n";
