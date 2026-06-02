<?php
// phar:// reads a single entry out of a PHAR archive. elephc parses the archive
// and embeds the entry's bytes at compile time (like data://), so the compiled
// binary carries the data and needs no archive file at run time.
//
// The archive path is resolved at compile time relative to the compiler's
// working directory, so compile this from the repository root:
//   cargo run -- examples/phar-reader/main.php
//   ./examples/phar-reader/main
//
// Regenerate app.phar with: php -d phar.readonly=0 build-phar.php

// A top-level entry.
$f = fopen("phar://examples/phar-reader/app.phar/greeting.txt", "r");
echo fread($f, 1024);
fclose($f);

// A nested entry (a sub-path inside the archive).
$g = fopen("phar://examples/phar-reader/app.phar/data/info.txt", "r");
echo fread($g, 1024);
fclose($g);

// A gzip-compressed entry (stored as raw DEFLATE in the archive). elephc
// inflates it at compile time, so reading it is no different from a plain entry.
$z = fopen("phar://examples/phar-reader/app.phar/notes.txt", "r");
$notes = fread($z, 4096);
fclose($z);
echo "notes.txt: " . strlen($notes) . " bytes, starts \"" . substr($notes, 0, 26) . "...\"\n";

// A missing entry returns false, like any failed fopen().
$missing = @fopen("phar://examples/phar-reader/app.phar/does-not-exist.txt", "r");
echo $missing === false ? "missing entry -> false\n" : "unexpected\n";
