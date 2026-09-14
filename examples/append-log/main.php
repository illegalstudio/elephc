<?php
// Writing an append-only log under a directory tree that may not exist yet — the two
// arguments this example exists for: mkdir()'s $recursive, and file_put_contents()'s
// FILE_APPEND.

$root = sys_get_temp_dir() . "/elephc-append-log";
$logDir = $root . "/2026/09/14";
$logFile = $logDir . "/app.log";

// $recursive creates every missing level in one call. Without it, a missing parent makes
// mkdir() fail, so the whole tree has to be walked by hand.
if (!is_dir($logDir)) {
    mkdir($logDir, 0755, true);
}
echo "log dir: ", is_dir($logDir) ? "ready" : "missing", "\n";

function append(string $file, string $line): void
{
    // FILE_APPEND extends the file instead of truncating it; LOCK_EX holds an exclusive
    // lock for the write, so two processes appending at once cannot interleave a line.
    file_put_contents($file, $line . "\n", FILE_APPEND | LOCK_EX);
}

// The first write creates the file. Without FILE_APPEND each call would replace it, which is
// what makes the count below the point of the example.
@unlink($logFile);
append($logFile, "boot");
append($logFile, "request /index.php");
append($logFile, "request /about.php");
append($logFile, "shutdown");

echo file_get_contents($logFile);
echo "lines: ", count(file($logFile, FILE_IGNORE_NEW_LINES)), "\n";

// file_put_contents() returns the number of bytes it wrote, not the file's total size.
$written = file_put_contents($logFile, "rotated\n", FILE_APPEND);
echo "appended ", $written, " bytes\n";

// Without FILE_APPEND the same call truncates, which is the default and stays the default.
file_put_contents($logFile, "fresh\n");
echo file_get_contents($logFile);

unlink($logFile);
rmdir($logDir);
rmdir($root . "/2026/09");
rmdir($root . "/2026");
rmdir($root);
