<?php
// Start the platform's standard sort utility and communicate through pipes.

$pipes = [];
$process = proc_open(
    "sort",
    [
        0 => ["pipe", "r"],
        1 => ["pipe", "w"],
        2 => ["pipe", "w"],
    ],
    $pipes,
);

if ($process === false) {
    echo "Could not start sort\n";
    exit(1);
}

fwrite($pipes[0], "pear\napple\norange\n");
fclose($pipes[0]);

$sorted = fread($pipes[1], 4096);
$errors = fread($pipes[2], 4096);
fclose($pipes[1]);
fclose($pipes[2]);

$status = proc_get_status($process);
$exitCode = proc_close($process);

echo $sorted;
echo "running before close: " . ($status["running"] ? "yes" : "no") . "\n";
echo "exit code: " . $exitCode . "\n";

if (strlen($errors) > 0) {
    echo "stderr: " . $errors;
}
