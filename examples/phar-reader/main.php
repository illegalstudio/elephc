<?php
// phar:// reads a single entry out of a PHAR archive. Literal URLs are parsed
// at compile time (like data://) and embedded in the binary. Runtime-built URLs
// parse the archive while the program runs, including compressed entries.
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

// A gzip-compressed entry (stored as raw DEFLATE in the archive). Literal reads
// inflate it at compile time, so reading it is no different from a plain entry.
$z = fopen("phar://examples/phar-reader/app.phar/notes.txt", "r");
$notes = fread($z, 4096);
fclose($z);
echo "notes.txt: " . strlen($notes) . " bytes, starts \"" . substr($notes, 0, 26) . "...\"\n";

// The same gzip-compressed entry through a runtime-built URL. This path parses
// the PHAR and inflates the entry at run time, so app.phar must exist when the
// compiled binary runs.
$archive = "examples/phar-reader/app.phar";
$runtime_notes = file_get_contents("phar://" . $archive . "/notes.txt");
echo "runtime notes: " . strlen($runtime_notes) . " bytes\n";

// Runtime-built phar:// URLs can also write native PHAR archives. Existing
// entries are preserved when later writes update another path in the same
// archive.
$out_archive = "examples/phar-reader/runtime-write.phar";
@unlink($out_archive);
echo file_put_contents("phar://" . $out_archive . "/generated.txt", "created at runtime\n") . " bytes written\n";
echo file_put_contents("phar://" . $out_archive . "/nested/info.txt", "nested payload\n") . " bytes written\n";
$stream = fopen("phar://" . $out_archive . "/streamed.txt", "w");
echo fwrite($stream, "stream payload\n") . " bytes streamed\n";
fclose($stream);
echo file_get_contents("phar://" . $out_archive . "/generated.txt");
echo file_get_contents("phar://" . $out_archive . "/nested/info.txt");
echo file_get_contents("phar://" . $out_archive . "/streamed.txt");

// The same phar:// write bridge can produce zip-based containers.
$zip_archive = "examples/phar-reader/runtime-write.zip";
@unlink($zip_archive);
echo file_put_contents("phar://" . $zip_archive . "/zip-entry.txt", "zip payload\n") . " zip bytes written\n";
echo file_get_contents("phar://" . $zip_archive . "/zip-entry.txt");

// A missing entry returns false, like any failed fopen().
$missing = @fopen("phar://examples/phar-reader/app.phar/does-not-exist.txt", "r");
echo $missing === false ? "missing entry -> false\n" : "unexpected\n";
