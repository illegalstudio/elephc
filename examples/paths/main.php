<?php
// Path manipulation — pure string operations, no filesystem access until
// realpath() is reached.

// basename() returns the trailing component of a path. The optional
// second argument trims a known suffix (matched literally).
echo basename("/var/log/system.log") . "\n";          // system.log
echo basename("/var/log/system.log", ".log") . "\n";  // system

// dirname() peels off the trailing component.
echo dirname("/var/log/system.log") . "\n";           // /var/log
echo dirname("/var/log") . "\n";                       // /var

// pathinfo() splits a path into its parts. With no flag it returns an
// associative array.
$parts = pathinfo("/srv/app/index.php");
echo "dirname:   " . $parts["dirname"] . "\n";
echo "basename:  " . $parts["basename"] . "\n";
echo "filename:  " . $parts["filename"] . "\n";
echo "extension: " . $parts["extension"] . "\n";

// Asking for a single component returns it directly.
echo pathinfo("/srv/app/index.php", PATHINFO_EXTENSION) . "\n";

// fnmatch() runs a shell-style glob against a name (no filesystem hit).
echo (fnmatch("*.log", "system.log") ? "y" : "n") . "\n";
echo (fnmatch("*.log", "system.txt") ? "y" : "n") . "\n";

// realpath() resolves the canonical absolute path of an existing file.
// On a missing path PHP returns false; elephc surfaces that as an empty
// string-typed result wrapped as Mixed.
file_put_contents("./local.txt", "");
$resolved = realpath("./local.txt");
echo "resolved: " . $resolved . "\n";

unlink("./local.txt");
echo "done\n";
