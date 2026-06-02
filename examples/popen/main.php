<?php
// popen() opens a pipe to a child process. With mode "r" the program reads
// the process's output; with mode "w" it writes to the process's input.

$read = popen("printf 'line one\nline two\n'", "r");
echo "child output:\n" . fread($read, 256);
$status = pclose($read);
echo "child exited with status " . $status . "\n";

$write = popen("cat", "w");
fwrite($write, "this text is piped through cat\n");
pclose($write);
