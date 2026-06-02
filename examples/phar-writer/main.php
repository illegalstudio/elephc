<?php

// phar:// write stream (Milestone-1): build a single-entry PHAR archive on disk
// at run time.
//
// The archive path and entry name are resolved at compile time from the literal
// URL, so the phar:// argument must be a string literal (not a runtime
// concatenation). The path resolves against the compiler's working directory,
// so compile this from the repository root:
//   cargo run -- examples/phar-writer/main.php
//   ./examples/phar-writer/main
//
// fwrite() buffers the entry content in memory; fclose() assembles the native
// PHAR (PHP stub + manifest + bytes, with the manifest size/CRC-32 fields filled
// in) and flushes it to disk.

$out = fopen("phar://examples/phar-writer/greeting.phar/message.txt", "w");
fwrite($out, "Hello from a phar entry written by elephc!\n");
fwrite($out, "Milestone-1: one uncompressed entry, signatureless.\n");
fclose($out);

// The writer produced a real file on disk. elephc's phar:// reader parses the
// archive at COMPILE time, so reading this entry back must be done in a separate
// compilation (after the file exists) — that round-trip is exercised by the test
// suite. Here we open the raw archive at run time to confirm it was written.
$raw = fopen("examples/phar-writer/greeting.phar", "r");
$bytes = fread($raw, 4096);
fclose($raw);

echo "wrote " . strlen($bytes) . " bytes\n";
echo "stub: " . substr($bytes, 0, 23) . "\n";
echo "has entry name: " . (strpos($bytes, "message.txt") !== false ? "yes" : "no") . "\n";
echo "has content: " . (strpos($bytes, "written by elephc") !== false ? "yes" : "no") . "\n";
